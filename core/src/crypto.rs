// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Device keys (security-critical). They are not stored in the database: the core asks the
//! platform to keep them in its keystore through `SecretStore`.

use ed25519_dalek::SigningKey;
use rand::RngCore;
use rand::rngs::OsRng;
use secrecy::ExposeSecret;
use zeroize::Zeroizing;

use crate::error::CoreError;

pub const SECRET_SIGNING: &str = "device_ed25519";
pub const SECRET_AGE: &str = "device_age";
pub const SECRET_DB: &str = "db_key";
/// Network key (QUIC/TLS identity between peers). Separate from the signing key: a key is never
/// reused across protocols.
pub const SECRET_NET: &str = "device_net";
/// New database key staged by "delete all my data" before re-encrypting the file, so an
/// interrupted erase can be finished on the next open.
pub const SECRET_DB_NEXT: &str = "db_key_next";

/// Each platform implements it with its keystore (Keystore, DPAPI, Keychain…).
/// SECURITY: The implementation must protect the values with a hardware-bound key when available.
#[uniffi::export(with_foreign)]
pub trait SecretStore: Send + Sync {
    fn load(&self, name: String) -> Result<Option<Vec<u8>>, CoreError>;
    fn save(&self, name: String, value: Vec<u8>) -> Result<(), CoreError>;
}

pub struct DeviceKeys {
    pub signing: SigningKey,
    pub age: age::x25519::Identity,
    pub net: Zeroizing<[u8; 32]>,
}

pub fn random_32() -> Zeroizing<[u8; 32]> {
    let mut k = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(k.as_mut());
    k
}

/// Loads a 32-byte secret or creates it the first time.
fn load_or_create_32(store: &dyn SecretStore, name: &str) -> Result<Zeroizing<[u8; 32]>, CoreError> {
    if let Some(bytes) = store.load(name.into())? {
        let bytes = Zeroizing::new(bytes);
        return Ok(Zeroizing::new(bytes.as_slice().try_into().map_err(|_| CoreError::crypto("corrupted key"))?));
    }
    let k = random_32();
    store.save(name.into(), k.to_vec())?;
    Ok(k)
}

impl DeviceKeys {
    /// Loads the keys or creates them the first time.
    pub fn load_or_create(store: &dyn SecretStore) -> Result<Self, CoreError> {
        let signing = SigningKey::from_bytes(&*load_or_create_32(store, SECRET_SIGNING)?);
        let age = match store.load(SECRET_AGE.into())? {
            Some(bytes) => {
                let text =
                    Zeroizing::new(String::from_utf8(bytes).map_err(|_| CoreError::crypto("corrupted age identity"))?);
                text.parse::<age::x25519::Identity>().map_err(|_| CoreError::crypto("corrupted age identity"))?
            }
            None => {
                let id = age::x25519::Identity::generate();
                store.save(SECRET_AGE.into(), id.to_string().expose_secret().as_bytes().to_vec())?;
                id
            }
        };
        Ok(Self { signing, age, net: load_or_create_32(store, SECRET_NET)? })
    }

    /// A new identity saved over the old one ("delete all my data"): afterwards the old keys no
    /// longer exist in the keystore.
    pub fn replace(store: &dyn SecretStore) -> Result<Self, CoreError> {
        let (signing, net, age) = (random_32(), random_32(), age::x25519::Identity::generate());
        store.save(SECRET_SIGNING.into(), signing.to_vec())?;
        store.save(SECRET_NET.into(), net.to_vec())?;
        store.save(SECRET_AGE.into(), age.to_string().expose_secret().as_bytes().to_vec())?;
        Ok(Self { signing: SigningKey::from_bytes(&signing), age, net })
    }

    pub fn device_id(&self) -> String {
        hex::encode(self.signing.verifying_key().to_bytes())
    }

    pub fn age_recipient(&self) -> String {
        self.age.to_public().to_string()
    }

    /// Public network key (hex): what peers check at connection time (`Member::node_id`).
    pub fn node_id(&self) -> String {
        hex::encode(SigningKey::from_bytes(&self.net).verifying_key().to_bytes())
    }
}

/// 32-byte key of the SQLCipher database.
pub fn db_key(store: &dyn SecretStore) -> Result<Zeroizing<Vec<u8>>, CoreError> {
    if let Some(k) = store.load(SECRET_DB.into())? {
        if k.len() == 32 {
            return Ok(Zeroizing::new(k));
        }
        return Err(CoreError::crypto("corrupted database key"));
    }
    let k = Zeroizing::new(random_32().to_vec());
    store.save(SECRET_DB.into(), k.to_vec())?;
    Ok(k)
}

/// In-memory implementation ONLY for tests. Never use it in production.
#[cfg(test)]
pub mod testing {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct MemStore(pub Mutex<BTreeMap<String, Vec<u8>>>);

    impl SecretStore for MemStore {
        fn load(&self, name: String) -> Result<Option<Vec<u8>>, CoreError> {
            Ok(self.0.lock().unwrap().get(&name).cloned())
        }
        fn save(&self, name: String, value: Vec<u8>) -> Result<(), CoreError> {
            self.0.lock().unwrap().insert(name, value);
            Ok(())
        }
    }
}
