// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Windows keystore with DPAPI (security-critical). The service runs as SYSTEM: only SYSTEM can decrypt.
//! ponytail: an administrator can obtain SYSTEM privileges and read the secrets; acceptable
//! because the administrator is the owner or the guardian. Improvement: TPM-wrapped key (NCrypt).

use std::path::PathBuf;

use procastinatioff_core::crypto::SecretStore;
use procastinatioff_core::error::CoreError;
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::core::PCWSTR;

pub struct DpapiStore {
    dir: PathBuf,
}

impl DpapiStore {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, name: &str) -> Result<PathBuf, CoreError> {
        // Avoids paths like "..\\..\\x": only letters, digits and underscore.
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(CoreError::Secret { msg: "invalid secret name".into() });
        }
        Ok(self.dir.join(format!("{name}.bin")))
    }
}

fn dpapi(data: &[u8], protect: bool) -> Result<Vec<u8>, CoreError> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        let r = if protect {
            CryptProtectData(&input, PCWSTR::null(), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        r.map_err(|e| CoreError::Secret { msg: format!("DPAPI: {e}") })?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(out.pbData as _));
        Ok(v)
    }
}

impl SecretStore for DpapiStore {
    fn load(&self, name: String) -> Result<Option<Vec<u8>>, CoreError> {
        match std::fs::read(self.path(&name)?) {
            Ok(blob) => Ok(Some(dpapi(&blob, false)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CoreError::Secret { msg: e.to_string() }),
        }
    }

    fn save(&self, name: String, value: Vec<u8>) -> Result<(), CoreError> {
        let blob = dpapi(&value, true)?;
        let path = self.path(&name)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, blob).map_err(|e| CoreError::Secret { msg: e.to_string() })?;
        std::fs::rename(&tmp, &path).map_err(|e| CoreError::Secret { msg: e.to_string() }) // atomic write
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_name_validation() {
        let dir = std::env::temp_dir().join(format!("pco-dpapi-{}", std::process::id()));
        let store = DpapiStore::new(dir.clone()).unwrap();
        store.save("db_key".into(), vec![7u8; 32]).unwrap();
        assert_eq!(store.load("db_key".into()).unwrap(), Some(vec![7u8; 32]));
        // The value is not left in plain text on disk
        assert_ne!(std::fs::read(dir.join("db_key.bin")).unwrap(), vec![7u8; 32]);
        assert!(store.load("..\\x".into()).is_err());
        assert_eq!(store.load("nothing".into()).unwrap(), None);
        std::fs::remove_dir_all(dir).ok();
    }
}
