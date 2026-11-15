// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Lista blanca web en ENFOQUE (capa 1: políticas empresariales del navegador).
//! Capas 2 (DNS local) y 3 (WFP) llegan con el modo familiar (Fase 3).

use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

const CHROME: &str = r"SOFTWARE\Policies\Google\Chrome";
const EDGE: &str = r"SOFTWARE\Policies\Microsoft\Edge";
const FIREFOX: &str = r"SOFTWARE\Policies\Mozilla\Firefox";

/// Dominios que siempre hacen falta para iniciar sesión en plataformas de estudio.
const LOGIN_DOMAINS: [&str; 2] = ["accounts.google.com", "login.microsoftonline.com"];

fn write_list(hklm: &RegKey, base: &str, name: &str, values: &[String]) -> std::io::Result<()> {
    let _ = hklm.delete_subkey_all(format!(r"{base}\{name}"));
    let (key, _) = hklm.create_subkey(format!(r"{base}\{name}"))?;
    for (i, v) in values.iter().enumerate() {
        key.set_value((i + 1).to_string(), v)?; // las listas de Chrome/Edge son valores "1", "2", …
    }
    Ok(())
}

/// Ajustes que se aplican SIEMPRE (cierran salidas fáciles del filtro).
pub fn apply_baseline() -> std::io::Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for base in [CHROME, EDGE] {
        let (k, _) = hklm.create_subkey(base)?;
        k.set_value("DnsOverHttpsMode", &"off")?; // que respete el DNS del sistema
        k.set_value("BrowserGuestModeEnabled", &0u32)?;
        k.set_value("BrowserAddPersonEnabled", &0u32)?; // sin perfiles nuevos sin políticas
    }
    let (chrome, _) = hklm.create_subkey(CHROME)?;
    chrome.set_value("IncognitoModeAvailability", &1u32)?; // 1 = incógnito deshabilitado
    let (edge, _) = hklm.create_subkey(EDGE)?;
    edge.set_value("InPrivateModeAvailability", &1u32)?;
    let (ff, _) = hklm.create_subkey(FIREFOX)?;
    ff.set_value("DisablePrivateBrowsing", &1u32)?;
    let (doh, _) = hklm.create_subkey(format!(r"{FIREFOX}\DNSOverHTTPS"))?;
    doh.set_value("Enabled", &0u32)?;
    doh.set_value("Locked", &1u32)?;
    Ok(())
}

/// ENFOQUE: todo bloqueado salvo la lista blanca. `None` = quitar la restricción.
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
