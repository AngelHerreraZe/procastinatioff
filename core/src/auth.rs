// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local secrets (security-critical): PIN, password and pattern hashed with Argon2id, attempts
//! limited with a growing wait. A local gate the app applies; biometrics is checked by the OS.
//!
//! SECURITY: secrets are never logged. Kotlin/Swift pass them as `String` (cannot be zeroed):
//! accepted and documented in ADR 0013.

use std::collections::BTreeMap;

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use rand::rngs::OsRng;
use zeroize::Zeroizing;

use crate::error::CoreError;
use crate::model::{EventKind, Fact, LocalFactor, SecretHolder};
use crate::rules::Denied;

/// After the recovery code is used, the owner's secrets stop counting after this long.
pub const RESET_DELAY_MS: i64 = 72 * 3_600_000;
/// Crockford base32: no I, L, O, U (dictated over the phone).
pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// VERIFY: measure on the slowest supported phone (target 0.3–0.8 s). The parameters travel in
/// the PHC string: raising them later does not break older hashes.
fn argon() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(64 * 1024, 3, 1, None).expect("fixed valid parameters"),
    )
}

/// Domain separation: a PIN "123456" and a pattern with the same digits do not hash alike.
fn input(domain: &str, secret: &str) -> Zeroizing<String> {
    Zeroizing::new(format!("pco/{domain}/v1:{secret}"))
}

fn domain(kind: LocalFactor) -> &'static str {
    match kind {
        LocalFactor::Pin => "pin",
        LocalFactor::Password => "password",
        LocalFactor::Pattern => "pattern",
        LocalFactor::Biometric => "biometric", // never hashed: check_new_secret rejects it first
    }
}

fn hash(domain: &str, secret: &str) -> Result<String, CoreError> {
    Ok(argon().hash_password(input(domain, secret).as_bytes()).map_err(CoreError::crypto)?.to_string())
}

fn verify(domain: &str, secret: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|h| argon().verify_password(input(domain, secret).as_bytes(), &h).is_ok())
}

pub fn hash_secret(kind: LocalFactor, secret: &str) -> Result<String, CoreError> {
    hash(domain(kind), secret)
}

pub fn verify_secret(kind: LocalFactor, secret: &str, phc: &str) -> bool {
    kind != LocalFactor::Biometric && verify(domain(kind), secret, phc)
}

/// Rules for a new secret. Patterns are "0-4-8-5": cells 0..=8 of a 3×3 grid.
pub fn check_new_secret(kind: LocalFactor, s: &str) -> Result<(), Denied> {
    match kind {
        LocalFactor::Pin => {
            if s.len() < 6 || !s.bytes().all(|b| b.is_ascii_digit()) {
                return Err(Denied::WeakPin);
            }
            // 000000, 123456, 654321…: every step the same and at most 1
            let steps: std::collections::BTreeSet<i16> =
                s.as_bytes().windows(2).map(|w| i16::from(w[1]) - i16::from(w[0])).collect();
            if steps.len() == 1 && steps.iter().all(|x| x.abs() <= 1) {
                return Err(Denied::EasyPin);
            }
        }
        LocalFactor::Password if s.chars().count() < 8 => return Err(Denied::WeakPassword),
        LocalFactor::Password => {}
        LocalFactor::Pattern => {
            let parts: Vec<&str> = s.split('-').collect();
            let cells: Vec<u8> = parts.iter().filter_map(|c| c.parse().ok()).filter(|&c| c <= 8).collect();
            let unique: std::collections::BTreeSet<u8> = cells.iter().copied().collect();
            if cells.len() != parts.len() || unique.len() != cells.len() || !(4..=9).contains(&cells.len()) {
                return Err(Denied::WeakPattern);
            }
        }
        LocalFactor::Biometric => return Err(Denied::MethodNotSet), // configured in the OS
    }
    Ok(())
}

/// Wait after `failures` failed attempts in a row: none up to 4; 30 s at the 5th; doubles; 1 h max.
pub fn lockout_ms(failures: u32) -> i64 {
    if failures < 5 { 0 } else { (30_000i64 << (failures - 5).min(7)).min(3_600_000) }
}

/// Upper case, no separators, O→0 and I/L→1: codes are dictated and typed by hand.
pub fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect()
}

/// Recovery code: 20 Crockford characters (100 bits), shown once in groups of 5.
pub fn new_recovery_code() -> String {
    let mut b = [0u8; 13];
    OsRng.fill_bytes(&mut b);
    let n = u128::from_be_bytes([[0u8; 3].as_slice(), &b].concat().try_into().expect("16 bytes"));
    let chars: Vec<char> = (0..20).rev().map(|i| ALPHABET[((n >> (5 * i)) & 31) as usize] as char).collect();
    chars.chunks(5).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-")
}

pub fn hash_recovery(code: &str) -> Result<String, CoreError> {
    hash("recovery", &normalize_code(code))
}

pub fn verify_recovery(code: &str, phc: &str) -> bool {
    verify("recovery", &normalize_code(code), phc)
}

/// The secrets and recovery code in force at `now`, from the profile's facts.
#[derive(Debug, Default)]
pub struct Secrets {
    /// Latest hash per (method, holder), without those voided by a finished reset.
    pub phc: BTreeMap<(LocalFactor, SecretHolder), String>,
    pub recovery: Option<String>,
    /// A reset in progress: when the owner's secrets stop counting.
    pub reset_at_ms: Option<i64>,
}

impl Secrets {
    pub fn from_facts(facts: &[Fact], now_ms: i64) -> Self {
        let mut s = Self::default();
        for f in facts.iter().filter(|f| f.at_ms <= now_ms) {
            match &f.kind {
                EventKind::SecretSet { factor, holder, phc } => {
                    s.phc.insert((*factor, *holder), phc.clone());
                }
                EventKind::RecoveryCodeSet { phc } => s.recovery = Some(phc.clone()),
                EventKind::LocalResetStarted => {
                    let done = f.at_ms + RESET_DELAY_MS;
                    if now_ms >= done {
                        // The owner's secrets set before the reset no longer count; the guardian's stay.
                        s.phc.retain(|(_, holder), _| *holder != SecretHolder::Owner);
                        s.reset_at_ms = None;
                    } else {
                        s.reset_at_ms = Some(done);
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// Owner methods that can be used now: in the policy and, except biometrics, with a secret.
    pub fn usable(&self, policy: &[LocalFactor]) -> Vec<LocalFactor> {
        policy
            .iter()
            .copied()
            .filter(|f| *f == LocalFactor::Biometric || self.phc.contains_key(&(*f, SecretHolder::Owner)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_verifies_only_the_same_secret_and_kind() {
        let phc = hash_secret(LocalFactor::Pin, "482916").unwrap();
        assert!(phc.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"));
        assert!(verify_secret(LocalFactor::Pin, "482916", &phc));
        assert!(!verify_secret(LocalFactor::Pin, "482917", &phc));
        assert!(!verify_secret(LocalFactor::Password, "482916", &phc), "domain separation");
        assert!(!verify_secret(LocalFactor::Pin, "482916", "not a phc"));
    }

    #[test]
    fn weak_secrets_are_rejected() {
        for pin in ["000000", "123456", "654321", "12345", "12a456"] {
            assert!(check_new_secret(LocalFactor::Pin, pin).is_err(), "{pin}");
        }
        assert!(check_new_secret(LocalFactor::Pin, "482916").is_ok());
        assert!(check_new_secret(LocalFactor::Pin, "135791").is_ok(), "steps of 2 are fine");
        assert!(check_new_secret(LocalFactor::Password, "abc").is_err());
        assert!(check_new_secret(LocalFactor::Password, "correct horse").is_ok());
        for p in ["0-1-2", "0-0-1-2", "0-1-2-9", "0-1-x-3", ""] {
            assert!(check_new_secret(LocalFactor::Pattern, p).is_err(), "{p}");
        }
        assert!(check_new_secret(LocalFactor::Pattern, "0-4-8-5").is_ok());
        assert!(check_new_secret(LocalFactor::Biometric, "x").is_err());
    }

    #[test]
    fn lockout_grows_and_caps() {
        assert_eq!(lockout_ms(4), 0);
        assert_eq!(lockout_ms(5), 30_000);
        assert_eq!(lockout_ms(6), 60_000);
        assert_eq!(lockout_ms(40), 3_600_000);
    }

    #[test]
    fn recovery_code_has_100_bits_and_survives_typing_mistakes() {
        let code = new_recovery_code();
        assert_eq!(code.len(), 23); // 20 characters + 3 dashes
        assert_ne!(code, new_recovery_code());
        let phc = hash_recovery(&code).unwrap();
        let typed = code.to_lowercase().replace('-', " ").replace('0', "o").replace('1', "l");
        assert!(verify_recovery(&typed, &phc));
        assert!(!verify_recovery("AAAAA-AAAAA-AAAAA-AAAAA", &phc));
    }

    #[test]
    fn reset_voids_owner_secrets_after_72_hours() {
        let set = |holder| Fact::new(0, EventKind::SecretSet { factor: LocalFactor::Pin, holder, phc: "h".into() });
        let facts =
            vec![set(SecretHolder::Owner), set(SecretHolder::Guardian), Fact::new(10, EventKind::LocalResetStarted)];
        let before = Secrets::from_facts(&facts, 10 + RESET_DELAY_MS - 1);
        assert_eq!(before.usable(&[LocalFactor::Pin]), vec![LocalFactor::Pin], "71 h: still valid");
        assert_eq!(before.reset_at_ms, Some(10 + RESET_DELAY_MS));
        let after = Secrets::from_facts(&facts, 10 + RESET_DELAY_MS);
        assert!(after.usable(&[LocalFactor::Pin]).is_empty(), "72 h: gone");
        assert!(after.phc.contains_key(&(LocalFactor::Pin, SecretHolder::Guardian)), "the guardian's stays");
        assert_eq!(after.usable(&[LocalFactor::Biometric]), vec![LocalFactor::Biometric]);
    }
}
