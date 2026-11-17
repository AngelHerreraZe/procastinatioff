// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Encrypted backup (security-critical). Without a cloud, losing every device means starting over:
//! the app offers a file with this device's keys and events, encrypted with `age` and a
//! passphrase the app generates. The app never uploads it anywhere.

use std::io::{Read, Write};
use std::iter;

use rand::{Rng, rngs::OsRng};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::error::CoreError;
use crate::events::SignedEvent;

const VERSION: u8 = 1;
/// The largest backup accepted (90 days of events are a few MB).
const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Generated passphrases are 25 characters; shorter ones are not accepted.
const MIN_PASSPHRASE: usize = 20;
/// scrypt work factor accepted when decrypting (age encrypts with ~1 s of work, about 2^18).
const MAX_WORK_FACTOR: u8 = 22;

#[derive(Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct Contents {
    pub v: u8,
    /// Device keys (hex / age secret): the restored device IS this device.
    pub signing: String,
    pub age: String,
    pub net: String,
    pub genesis: Option<String>,
    pub kind: Option<String>,
    pub time_source: bool,
    #[zeroize(skip)]
    pub events: Vec<SignedEvent>,
}

/// 5 groups of 5 characters (Crockford base32, no I/L/O/U): about 125 bits.
/// ponytail: groups instead of the 6 words in the plan (no word list to ship); switch if users
/// struggle to copy it.
pub fn new_passphrase() -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
    let mut rng = OsRng;
    (0..5)
        .map(|_| (0..5).map(|_| ALPHABET[rng.gen_range(0..32)] as char).collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn seal(c: &Contents, passphrase: &str) -> Result<Vec<u8>, CoreError> {
    if passphrase.chars().count() < MIN_PASSPHRASE {
        return Err(CoreError::protocol("passphrase too short"));
    }
    let plain = Zeroizing::new(serde_json::to_vec(c).map_err(CoreError::crypto)?);
    let enc = age::Encryptor::with_user_passphrase(SecretString::from(passphrase.to_owned()));
    let mut out = Vec::new();
    let mut w = enc.wrap_output(&mut out).map_err(CoreError::crypto)?;
    w.write_all(&plain).map_err(CoreError::crypto)?;
    w.finish().map_err(CoreError::crypto)?;
    Ok(out)
}

/// A wrong passphrase and a damaged file give the same error: no hints.
pub fn open(data: &[u8], passphrase: &str) -> Result<Contents, CoreError> {
    let wrong = || CoreError::crypto("wrong passphrase or damaged backup");
    let dec = age::Decryptor::new(data).map_err(|_| wrong())?;
    let mut id = age::scrypt::Identity::new(SecretString::from(passphrase.to_owned()));
    id.set_max_work_factor(MAX_WORK_FACTOR);
    let mut r = dec.decrypt(iter::once(&id as &dyn age::Identity)).map_err(|_| wrong())?;
    let mut plain = Zeroizing::new(Vec::new());
    r.by_ref().take(MAX_BYTES).read_to_end(&mut plain).map_err(|_| wrong())?;
    let c: Contents = serde_json::from_slice(&plain).map_err(|_| wrong())?;
    if c.v != VERSION {
        return Err(CoreError::protocol("unsupported backup version"));
    }
    Ok(c)
}

pub fn contents(
    keys: &crate::crypto::DeviceKeys,
    genesis: Option<String>,
    kind: Option<String>,
    time_source: bool,
    events: Vec<SignedEvent>,
) -> Contents {
    use secrecy::ExposeSecret;
    Contents {
        v: VERSION,
        signing: hex::encode(keys.signing.to_bytes()),
        age: keys.age.to_string().expose_secret().to_owned(),
        net: hex::encode(*keys.net),
        genesis,
        kind,
        time_source,
        events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrases_are_random_and_well_formed() {
        let (a, b) = (new_passphrase(), new_passphrase());
        assert_ne!(a, b);
        assert_eq!(a.len(), 29);
        assert_eq!(a.split('-').count(), 5);
        assert!(a.chars().all(|c| c == '-' || c.is_ascii_digit() || c.is_ascii_lowercase()));
    }

    #[test]
    fn short_passphrases_and_wrong_ones_fail_without_hints() {
        let keys = crate::crypto::DeviceKeys::load_or_create(&crate::crypto::testing::MemStore::default()).unwrap();
        let c = contents(&keys, None, None, false, vec![]);
        assert!(seal(&c, "short").is_err());
        let pass = new_passphrase();
        let sealed = seal(&c, &pass).unwrap();
        assert_eq!(open(&sealed, &pass).unwrap().signing, c.signing);
        let wrong = open(&sealed, &new_passphrase()).err().unwrap().to_string();
        let damaged = open(b"not a backup", &pass).err().unwrap().to_string();
        assert_eq!(wrong, damaged);
    }
}
