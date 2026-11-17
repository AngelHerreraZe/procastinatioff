// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Linux keystore (security-critical): a 32-byte master key that systemd decrypts for the service
//! only (`LoadCredentialEncrypted`, bound to the TPM2 if there is one) and ChaCha20-Poly1305 per
//! secret in /var/lib/procastinacioff/secrets (0700, root).
//! File = random 12-byte nonce ‖ ciphertext; the secret's name is the associated data, so files
//! cannot be swapped for one another.
//! ponytail: root can read the master key while the service runs; acceptable because root is the
//! owner or the guardian (same as DPAPI and SYSTEM on Windows).

use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use procastinacioff_core::crypto::SecretStore;
use procastinacioff_core::error::CoreError;
use zeroize::Zeroizing;

/// Name of the credential in the unit (`LoadCredentialEncrypted=procastinacioff.master:…`).
pub const MASTER: &str = "procastinacioff.master";
const NONCE_LEN: usize = 12;

fn err(e: impl std::fmt::Display) -> CoreError {
    CoreError::Secret { msg: e.to_string() }
}

pub struct CredsStore {
    dir: PathBuf,
    cipher: ChaCha20Poly1305,
}

impl CredsStore {
    /// The master key from systemd (`$CREDENTIALS_DIRECTORY`). Without it, the service does not start.
    pub fn open(dir: PathBuf) -> Result<Self, CoreError> {
        let creds = std::env::var_os("CREDENTIALS_DIRECTORY").ok_or_else(|| err("no systemd credentials"))?;
        let key = Zeroizing::new(std::fs::read(Path::new(&creds).join(MASTER)).map_err(err)?);
        let key: &[u8; 32] = key.as_slice().try_into().map_err(|_| err("the master key must be 32 bytes"))?;
        Self::with_key(dir, key)
    }

    pub fn with_key(dir: PathBuf, key: &[u8; 32]) -> Result<Self, CoreError> {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(err)?;
        Ok(Self { dir, cipher: ChaCha20Poly1305::new(Key::from_slice(key)) })
    }

    fn path(&self, name: &str) -> Result<PathBuf, CoreError> {
        // Avoids paths like "../../x": only letters, digits and underscore.
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(err("invalid secret name"));
        }
        Ok(self.dir.join(format!("{name}.bin")))
    }
}

impl SecretStore for CredsStore {
    fn load(&self, name: String) -> Result<Option<Vec<u8>>, CoreError> {
        let blob = match std::fs::read(self.path(&name)?) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(err(e)),
        };
        if blob.len() < NONCE_LEN {
            return Err(err("damaged secret"));
        }
        let (nonce, ct) = blob.split_at(NONCE_LEN);
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad: name.as_bytes() })
            .map_err(|_| err("secret cannot be decrypted (other machine or tampered)"))?;
        Ok(Some(plain))
    }

    fn save(&self, name: String, value: Vec<u8>) -> Result<(), CoreError> {
        let value = Zeroizing::new(value);
        let path = self.path(&name)?;
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ct =
            self.cipher.encrypt(&nonce, Payload { msg: &value, aad: name.as_bytes() }).map_err(|_| err("encrypt"))?;
        let tmp = path.with_extension("tmp");
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(err)?;
            f.write_all(&nonce).and_then(|_| f.write_all(&ct)).and_then(|_| f.sync_all()).map_err(err)?;
        }
        std::fs::rename(&tmp, &path).map_err(err) // atomic write
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_names_and_tampering() {
        let dir = std::env::temp_dir().join(format!("pco-creds-{}", std::process::id()));
        let store = CredsStore::with_key(dir.clone(), &[7u8; 32]).unwrap();
        store.save("db_key".into(), vec![9u8; 32]).unwrap();
        assert_eq!(store.load("db_key".into()).unwrap(), Some(vec![9u8; 32]));
        assert!(!std::fs::read(dir.join("db_key.bin")).unwrap().windows(32).any(|w| w == [9u8; 32]));
        assert_eq!(store.load("nothing".into()).unwrap(), None);
        assert!(store.load("../x".into()).is_err());
        // Swapping files: the name is authenticated
        std::fs::copy(dir.join("db_key.bin"), dir.join("device_age.bin")).unwrap();
        assert!(store.load("device_age".into()).is_err());
        // Another machine's key
        assert!(CredsStore::with_key(dir.clone(), &[8u8; 32]).unwrap().load("db_key".into()).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
