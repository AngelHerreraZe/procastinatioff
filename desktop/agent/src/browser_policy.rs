// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Web allowlist in FOCUS (layer 1: enterprise browser policies).
//! Layers 2 (local DNS) and 3 (WFP) come with family mode.

use winreg::RegKey;

use crate::common::LOGIN_DOMAINS;
use winreg::enums::HKEY_LOCAL_MACHINE;

const CHROME: &str = r"SOFTWARE\Policies\Google\Chrome";
const EDGE: &str = r"SOFTWARE\Policies\Microsoft\Edge";
const FIREFOX: &str = r"SOFTWARE\Policies\Mozilla\Firefox";

fn write_list(hklm: &RegKey, base: &str, name: &str, values: &[String]) -> std::io::Result<()> {
    let _ = hklm.delete_subkey_all(format!(r"{base}\{name}"));
    let (key, _) = hklm.create_subkey(format!(r"{base}\{name}"))?;
    for (i, v) in values.iter().enumerate() {
        key.set_value((i + 1).to_string(), v)?; // Chrome/Edge lists are values "1", "2", …
    }
    Ok(())
}

/// Settings applied ALWAYS (they close easy ways around the filter).
pub fn apply_baseline() -> std::io::Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for base in [CHROME, EDGE] {
        let (k, _) = hklm.create_subkey(base)?;
        k.set_value("DnsOverHttpsMode", &"off")?; // respect the system DNS
        k.set_value("BrowserGuestModeEnabled", &0u32)?;
        k.set_value("BrowserAddPersonEnabled", &0u32)?; // no new profiles without policies
    }
    let (chrome, _) = hklm.create_subkey(CHROME)?;
    chrome.set_value("IncognitoModeAvailability", &1u32)?; // 1 = incognito disabled
    let (edge, _) = hklm.create_subkey(EDGE)?;
    edge.set_value("InPrivateModeAvailability", &1u32)?;
    let (ff, _) = hklm.create_subkey(FIREFOX)?;
    ff.set_value("DisablePrivateBrowsing", &1u32)?;
    let (doh, _) = hklm.create_subkey(format!(r"{FIREFOX}\DNSOverHTTPS"))?;
    doh.set_value("Enabled", &0u32)?;
    doh.set_value("Locked", &1u32)?;
    Ok(())
}

/// FOCUS: everything blocked except the allowlist. `None` = remove the restriction.
pub fn apply_allowlist(domains: Option<&[String]>) -> std::io::Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    match domains {
        Some(list) => {
            let mut allow: Vec<String> = list.to_vec();
            allow.extend(LOGIN_DOMAINS.iter().map(|d| d.to_string()));
            for base in [CHROME, EDGE] {
                write_list(&hklm, base, "URLBlocklist", &["*".to_string()])?;
                write_list(&hklm, base, "URLAllowlist", &allow)?;
            }
            write_list(&hklm, &format!(r"{FIREFOX}\WebsiteFilter"), "Block", &["<all_urls>".to_string()])?;
            let exceptions: Vec<String> = allow
                .iter()
                .map(|d| format!("https://*.{d}/*"))
                .chain(allow.iter().map(|d| format!("https://{d}/*")))
                .collect();
            write_list(&hklm, &format!(r"{FIREFOX}\WebsiteFilter"), "Exceptions", &exceptions)?;
        }
        None => {
            for base in [CHROME, EDGE] {
                let _ = hklm.delete_subkey_all(format!(r"{base}\URLBlocklist"));
                let _ = hklm.delete_subkey_all(format!(r"{base}\URLAllowlist"));
            }
            let _ = hklm.delete_subkey_all(format!(r"{FIREFOX}\WebsiteFilter"));
        }
    }
    Ok(())
}
