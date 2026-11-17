// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Web allowlist in FOCUS (layer 1) and the always-on settings that close easy ways around the
//! filter, as system-wide browser policies (root-owned JSON). The same policies as Windows.
//! ponytail: Firefox has a single system policies file; ours replaces any other one.

use serde_json::{Value, json};

use crate::common::LOGIN_DOMAINS;

/// Chrome, Chromium (Debian and Ubuntu) and Edge read every JSON file in these folders.
const CHROMIUM_DIRS: [&str; 4] = [
    "/etc/opt/chrome/policies/managed",
    "/etc/chromium/policies/managed",
    "/etc/chromium-browser/policies/managed",
    "/etc/opt/edge/policies/managed",
];
const CHROMIUM_FILE: &str = "procastinacioff.json";
/// Read by Firefox from the distribution and from the Snap.
const FIREFOX_DIR: &str = "/etc/firefox/policies";

fn allowed(list: &[String]) -> Vec<String> {
    list.iter().cloned().chain(LOGIN_DOMAINS.iter().map(|d| d.to_string())).collect()
}

pub fn chromium(focus: Option<&[String]>) -> Value {
    let mut p = json!({
        "DnsOverHttpsMode": "off",           // respect the system DNS
        "BrowserGuestModeEnabled": false,
        "BrowserAddPersonEnabled": false,    // no new profiles without policies
        "IncognitoModeAvailability": 1,      // Chrome/Chromium: incognito disabled
        "InPrivateModeAvailability": 1,      // Edge
    });
    if let Some(list) = focus {
        p["URLBlocklist"] = json!(["*"]);
        p["URLAllowlist"] = json!(allowed(list));
    }
    p
}

pub fn firefox(focus: Option<&[String]>) -> Value {
    let mut p = json!({
        "DisablePrivateBrowsing": true,
        "DNSOverHTTPS": { "Enabled": false, "Locked": true },
    });
    if let Some(list) = focus {
        let allow = allowed(list);
        let exceptions: Vec<String> = allow
            .iter()
            .map(|d| format!("https://*.{d}/*"))
            .chain(allow.iter().map(|d| format!("https://{d}/*")))
            .collect();
        p["WebsiteFilter"] = json!({ "Block": ["<all_urls>"], "Exceptions": exceptions });
    }
    json!({ "policies": p })
}

/// `focus`: the allowlist while in FOCUS; `None` = only the always-on settings.
pub fn apply(focus: Option<&[String]>) -> std::io::Result<()> {
    let chromium = serde_json::to_vec_pretty(&chromium(focus)).map_err(std::io::Error::other)?;
    for dir in CHROMIUM_DIRS {
        std::fs::create_dir_all(dir)?;
        std::fs::write(format!("{dir}/{CHROMIUM_FILE}"), &chromium)?;
    }
    std::fs::create_dir_all(FIREFOX_DIR)?;
    std::fs::write(
        format!("{FIREFOX_DIR}/policies.json"),
        serde_json::to_vec_pretty(&firefox(focus)).map_err(std::io::Error::other)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_blocks_everything_but_the_list_and_login() {
        let list = vec!["udemy.com".to_string()];
        let c = chromium(Some(&list));
        assert_eq!(c["URLBlocklist"], json!(["*"]));
        assert_eq!(c["URLAllowlist"][0], "udemy.com");
        assert!(c["URLAllowlist"].as_array().unwrap().contains(&json!("accounts.google.com")));
        let f = firefox(Some(&list));
        assert_eq!(f["policies"]["WebsiteFilter"]["Block"], json!(["<all_urls>"]));
        assert!(
            f["policies"]["WebsiteFilter"]["Exceptions"].as_array().unwrap().contains(&json!("https://*.udemy.com/*"))
        );
        // Outside FOCUS only the always-on settings remain
        assert!(chromium(None).get("URLBlocklist").is_none());
        assert_eq!(firefox(None)["policies"]["DNSOverHTTPS"]["Locked"], true);
        assert!(firefox(None)["policies"].get("WebsiteFilter").is_none());
    }
}
