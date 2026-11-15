// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Authenticode signer (security-critical): the signature is verified **and** the signer name is extracted.
//! Results are cached by (path, size, modification date).

use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::HWND;
use windows::Win32::Security::Cryptography::{CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CHOICE_FILE,
    WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvSignerFromChain,
    WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows::core::{GUID, PCWSTR};

/// Returns the signer name if the embedded signature is valid.
pub fn verified_signer(path: &Path) -> Option<String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: PCWSTR(wide.as_ptr()),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            dwStateAction: WTD_STATEACTION_VERIFY,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
            ..Default::default()
        };
        let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let hwnd = HWND(-1isize as *mut c_void); // INVALID_HANDLE_VALUE: no interface
        let status = WinVerifyTrust(hwnd, &mut action, &mut data as *mut _ as *mut c_void);

        let mut result = None;
        if status == 0 {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if !prov.is_null() {
                let signer = WTHelperGetProvSignerFromChain(prov, 0, false, 0);
                if !signer.is_null() && !(*signer).pChainContext.is_null() {
                    let chain = &*(*signer).pChainContext;
                    if chain.cChain > 0 {
                        let simple = &**chain.rgpChain;
                        if simple.cElement > 0 {
                            let leaf_cert = (**simple.rgpElement).pCertContext;
                            let mut buf = [0u16; 256];
                            let n =
                                CertGetNameStringW(leaf_cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf));
                            if n > 1 {
                                result = Some(String::from_utf16_lossy(&buf[..(n - 1) as usize]));
                            }
                        }
                    }
                }
            }
        }
        data.dwStateAction = WTD_STATEACTION_CLOSE; // releases WinVerifyTrust's internal state
        WinVerifyTrust(hwnd, &mut action, &mut data as *mut _ as *mut c_void);
        result
    }
}

/// Cache: (path, size, modification date) → signer.
#[derive(Default)]
pub struct SignerCache(HashMap<(String, u64, i64), Option<String>>);

impl SignerCache {
    pub fn get(&mut self, path: &Path) -> Option<String> {
        let meta = std::fs::metadata(path).ok()?;
        let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
        let key = (path.to_string_lossy().to_lowercase(), meta.len(), mtime);
        if self.0.len() > 5_000 {
            self.0.clear();
        }
        self.0.entry(key).or_insert_with(|| verified_signer(path)).clone()
    }
}
