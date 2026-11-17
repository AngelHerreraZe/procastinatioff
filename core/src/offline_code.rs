// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Offline approval codes (security-critical). The device that asks shows a 10-character request
//! code; the approver's app answers with 8 digits = HMAC(approver key, profile ‖ request).
//!
//! Honest limit (ADR 0013): the asking device can also compute the answer (it has to check it), so
//! a rooted device could forge one. That is why codes only cover small concessions (`OfflineAsk`),
//! at most 3 a day, and every one shows up in the mirror panel and the contact's alerts.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::auth::{ALPHABET, normalize_code};
use crate::model::OfflineAsk;

const DOMAIN: &[u8] = b"procastinatioff/offline/v1\0";
const KEY_DOMAIN: &[u8] = b"procastinatioff/offline-key/v1\0";
/// A request expires after this long; after `MAX_ATTEMPTS` wrong answers it needs a new nonce.
pub const REQUEST_TTL_MS: i64 = 24 * 3_600_000;
pub const MAX_ATTEMPTS: u32 = 3;
/// Concessions without network per day (codes and the guardian in person, together).
pub const MAX_PER_DAY: usize = 3;

/// Approver key: derived from the discovery key of the profile (or channel) where the approver is
/// a member, so every member has it without one more secret to distribute. `ponytail:` it lives in
/// the encrypted database like the discovery key; the platform keystore would add nothing while
/// the discovery key sits next to it.
pub fn approver_key(discovery_key: &[u8; 32], approver: &str) -> [u8; 32] {
    let mut mac = <Hmac<Sha256>>::new_from_slice(discovery_key).expect("HMAC accepts any key length");
    mac.update(KEY_DOMAIN);
    mac.update(approver.as_bytes());
    mac.finalize().into_bytes().into()
}

/// Is `ask` within the limits of an offline concession?
pub fn ask_in_range(ask: &OfflineAsk) -> bool {
    match ask {
        OfflineAsk::ExtraPermit { minutes } => (5..=120).contains(minutes),
        OfflineAsk::RestDay => true,
        OfflineAsk::LaterBedtime { minutes } => (1..=120).contains(minutes),
    }
}

/// 50 bits = version (2) + kind (4) + parameter (12) + nonce (32), as "K7Q2-M9XA-3B".
pub fn request_code(ask: &OfflineAsk, nonce: u32) -> String {
    let (kind, param): (u64, u64) = match ask {
        OfflineAsk::ExtraPermit { minutes } => (0, u64::from(*minutes)),
        OfflineAsk::RestDay => (1, 0),
        OfflineAsk::LaterBedtime { minutes } => (2, u64::from(*minutes)),
    };
    let n = (1u64 << 48) | (kind << 44) | ((param & 0xFFF) << 32) | u64::from(nonce); // version 1
    let c: String = (0..10).rev().map(|i| ALPHABET[((n >> (5 * i)) & 31) as usize] as char).collect();
    format!("{}-{}-{}", &c[..4], &c[4..8], &c[8..])
}

/// Accepts lower case, dashes and spaces, and fixes O→0, I/L→1. Returns the canonical code too.
pub fn parse_request(code: &str) -> Option<(OfflineAsk, u32, String)> {
    let clean = normalize_code(code);
    if clean.len() != 10 {
        return None;
    }
    let mut n = 0u64;
    for c in clean.bytes() {
        n = (n << 5) | ALPHABET.iter().position(|&a| a == c)? as u64;
    }
    if n >> 48 != 1 {
        return None;
    }
    let (kind, param, nonce) = ((n >> 44) & 0xF, ((n >> 32) & 0xFFF) as u16, n as u32);
    let ask = match kind {
        0 => OfflineAsk::ExtraPermit { minutes: param },
        1 if param == 0 => OfflineAsk::RestDay,
        2 => OfflineAsk::LaterBedtime { minutes: param },
        _ => return None,
    };
    ask_in_range(&ask).then(|| (ask.clone(), nonce, request_code(&ask, nonce)))
}

/// 8 digits. `profile`: genesis author of the profile (or channel) the key comes from.
pub fn response_code(key: &[u8; 32], profile: &str, request: &str) -> String {
    let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(DOMAIN);
    mac.update(profile.as_bytes());
    mac.update(b"\0");
    mac.update(request.as_bytes());
    let t = mac.finalize().into_bytes();
    format!("{:08}", u32::from_be_bytes([t[0], t[1], t[2], t[3]]) % 100_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_codes_round_trip_and_survive_dictation() {
        for ask in
            [OfflineAsk::ExtraPermit { minutes: 20 }, OfflineAsk::RestDay, OfflineAsk::LaterBedtime { minutes: 120 }]
        {
            let code = request_code(&ask, 0xDEAD_BEEF);
            assert_eq!(code.len(), 12);
            let (back, nonce, canonical) = parse_request(&code).unwrap();
            assert_eq!((back, nonce, canonical.as_str()), (ask, 0xDEAD_BEEF, code.as_str()));
            let sloppy = code.to_lowercase().replace('-', " ").replace('0', "o").replace('1', "i");
            assert_eq!(parse_request(&sloppy).unwrap().2, code);
        }
    }

    #[test]
    fn malformed_requests_are_rejected() {
        assert!(parse_request("K7Q2-M9XA-3").is_none(), "length");
        assert!(parse_request("K7Q2-M9XA-3BB").is_none(), "length");
        assert!(parse_request("K7Q2-M9XA-3U").is_none(), "U is not in the alphabet");
        let raw =
            |n: u64| -> String { (0..10).rev().map(|i| ALPHABET[((n >> (5 * i)) & 31) as usize] as char).collect() };
        assert!(parse_request(&raw(2 << 48)).is_none(), "version");
        assert!(parse_request(&raw((1 << 48) | (3 << 44))).is_none(), "kind");
        assert!(parse_request(&raw((1 << 48) | (500 << 32))).is_none(), "500 extra minutes");
        assert!(parse_request(&raw((1 << 48) | (1 << 44) | (5 << 32))).is_none(), "rest day with a parameter");
    }

    #[test]
    fn response_is_a_fixed_function_of_key_profile_and_request() {
        // Fixed vector: any change to the format breaks codes already handed out.
        assert_eq!(response_code(&[7; 32], "p", "0123-4567-89"), response_code(&[7; 32], "p", "0123-4567-89"));
        let r = response_code(&[7; 32], "p", "0123-4567-89");
        assert_eq!(r.len(), 8);
        assert_eq!(r, "59196638"); // since the ProcastinatiOFF rename (domain procastinatioff/offline/v1)
        assert_ne!(r, response_code(&[8; 32], "p", "0123-4567-89"));
        assert_ne!(r, response_code(&[7; 32], "q", "0123-4567-89"));
        assert_ne!(approver_key(&[1; 32], "a"), approver_key(&[1; 32], "b"));
    }
}
