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
}

impl DeviceKeys {
    /// Loads the keys or creates them the first time.
    pub fn load_or_create(store: &dyn SecretStore) -> Result<Self, CoreError> {
        let signing = match store.load(SECRET_SIGNING.into())? {
            Some(bytes) => {
                let bytes = Zeroizing::new(bytes);
                let arr: Zeroizing<[u8; 32]> = Zeroizing::new(
                    bytes.as_slice().try_into().map_err(|_| CoreError::crypto("corrupted signing key"))?,
                );
                SigningKey::from_bytes(&arr)
            }
            None => {
                let key = SigningKey::generate(&mut OsRng);
                store.save(SECRET_SIGNING.into(), Zeroizing::new(key.to_bytes()).to_vec())?;
                key
            }
        };
        let age = match store.load(SECRET_AGE.into())? {
            Some(bytes) => {
                let text =
                    Zeroizing::new(String::from_utf8(bytes).map_err(|_| CoreError::crypto("identidad age corrupta"))?);
                text.parse::<age::x25519::Identity>().map_err(|_| CoreError::crypto("identidad age corrupta"))?
            }
            None => {
                let id = age::x25519::Identity::generate();
                store.save(SECRET_AGE.into(), id.to_string().expose_secret().as_bytes().to_vec())?;
                id
            }
        };
        Ok(Self { signing, age })
    }

    pub fn device_id(&self) -> String {
        hex::encode(self.signing.verifying_key().to_bytes())
    }

    pub fn age_recipient(&self) -> String {
        self.age.to_public().to_string()
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
    let mut k = Zeroizing::new(vec![0u8; 32]);
    OsRng.fill_bytes(&mut k);
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
