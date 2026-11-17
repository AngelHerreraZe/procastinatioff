// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Blocking policy, pure and tested: decides whether a process is allowed. Does not touch Windows.

use procastinacioff_core::model::{Config, DayState};

/// How we identify an app on Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    /// Full path in lowercase.
    pub path: String,
    /// Executable name in lowercase (e.g. "chrome.exe").
    pub file_name: String,
    /// Verified Authenticode signer (e.g. "Google LLC"), if any.
    pub signer: Option<String>,
    /// Package Family Name if it is a Microsoft Store app.
    pub store_pfn: Option<String>,
}

impl AppIdentity {
    /// Identifiers in the format of `Config::essential_apps` / `focus_apps`.
    pub fn ids(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(pfn) = &self.store_pfn {
            v.push(format!("windows:pfn:{pfn}"));
        }
        if let Some(s) = &self.signer {
            v.push(format!("windows:exe:{s}|{}", self.file_name));
        }
        v
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Block,
}

/// WhatsApp Desktop's Package Family Name (Microsoft Store).
const WHATSAPP_PFN: &str = "5319275A.WhatsAppDesktop_cv1g1gvanyjgm";

/// Windows subfolders a standard user can write to (known AppLocker bypass).
const USER_WRITABLE: [&str; 8] = [
    "\\temp\\",
    "\\tasks\\",
    "\\tracing\\",
    "\\registration\\",
    "\\system32\\tasks\\",
    "\\syswow64\\tasks\\",
    "\\system32\\spool\\",
    "\\system32\\microsoft\\crypto\\",
];

/// Operating system binary: under `windows_dir` and outside the user-writable folders.
/// ponytail: fixed list; if more paths show up, verify Microsoft's catalog signature (WinVerifyTrust + catalog).
pub fn is_system_path(path: &str, windows_dir: &str) -> bool {
    let root = windows_dir.to_lowercase();
    match path.strip_prefix(root.as_str()) {
        Some(rest) if rest.starts_with('\\') => !USER_WRITABLE.iter().any(|d| rest.starts_with(d)),
        _ => false,
    }
}

/// Package Family Name from the WindowsApps folder.
/// "c:\program files\windowsapps\5319275a.whatsappdesktop_2.24.1.0_x64__cv1g1gvanyjgm\whatsapp.exe"
///   → "5319275a.whatsappdesktop_cv1g1gvanyjgm"
/// Only TrustedInstaller can write to WindowsApps: the path is a reliable identity.
pub fn pfn_from_path(path: &str) -> Option<String> {
    let rest = path.split("\\windowsapps\\").nth(1)?;
    let folder = rest.split('\\').next()?;
    let parts: Vec<&str> = folder.split('_').collect();
    if parts.len() == 5 { Some(format!("{}_{}", parts[0], parts[4])) } else { None }
}

pub fn decide(state: &DayState, app: &AppIdentity, cfg: &Config, install_dir: &str, windows_dir: &str) -> Verdict {
    // 1. Permissive states (includes free days: the core returns them as FREE)
    if matches!(state, DayState::Free | DayState::Pass { .. } | DayState::Emergency { .. }) {
        return Verdict::Allow;
    }
    // 2. Our binaries and the operating system's
    if app.path.starts_with(&format!("{}\\", install_dir.to_lowercase())) || is_system_path(&app.path, windows_dir) {
        return Verdict::Allow;
    }
    // 3. SLEEP: nothing else
    if matches!(state, DayState::Sleep) {
        return Verdict::Block;
    }
    // WhatsApp is always open outside SLEEP, like the phone's calls and SMS: it is never configured.
    // Its Store identity comes from the WindowsApps path, which only TrustedInstaller can write.
    if app.store_pfn.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(WHATSAPP_PFN)) {
        return Verdict::Allow;
    }
    // 4. LOCKED / FOCUS: configuration lists (case-insensitive comparison)
    let lists: Vec<&String> = match state {
        DayState::Focus { apps, .. } => cfg.essential_apps.iter().chain(apps).collect(),
        _ => cfg.essential_apps.iter().collect(),
    };
    let ids = app.ids();
    if ids.iter().any(|id| lists.iter().any(|allowed| allowed.eq_ignore_ascii_case(id))) {
        Verdict::Allow
    } else {
        Verdict::Block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: &str = r"C:\Windows";
    const INSTALL: &str = r"C:\Program Files\ProcastinaciOFF";

    fn app(path: &str, signer: Option<&str>) -> AppIdentity {
        let path = path.to_lowercase();
        AppIdentity {
            file_name: path.rsplit('\\').next().unwrap().to_owned(),
            store_pfn: pfn_from_path(&path),
            signer: signer.map(String::from),
            path,
        }
    }

    const WHATSAPP: &str =
        r"C:\Program Files\WindowsApps\5319275A.WhatsAppDesktop_2.24.1.0_x64__cv1g1gvanyjgm\WhatsApp.exe";

    #[test]
    fn locked_allows_whatsapp_store_and_blocks_game() {
        let mut cfg = Config::example();
        cfg.essential_apps.clear(); // WhatsApp is open without being listed
        let wa = app(WHATSAPP, None);
        let game = app(r"C:\Games\game.exe", Some("Game Studio"));
        assert_eq!(decide(&DayState::Locked, &wa, &cfg, INSTALL, WIN), Verdict::Allow);
        assert_eq!(decide(&DayState::Locked, &game, &cfg, INSTALL, WIN), Verdict::Block);
    }

    #[test]
    fn renamed_exe_without_right_signer_is_blocked() {
        let cfg = Config::example();
        // Someone renames a game to chrome.exe: the signer is not Google
        let fake = app(r"C:\Users\x\chrome.exe", Some("Game Studio"));
        let focus = DayState::Focus { goal_id: "read-30".into(), apps: cfg.focus_apps.clone(), sites: vec![] };
        assert_eq!(decide(&focus, &fake, &cfg, INSTALL, WIN), Verdict::Block);
        let real = app(r"C:\Program Files\Google\Chrome\Application\chrome.exe", Some("Google LLC"));
        assert_eq!(decide(&focus, &real, &cfg, INSTALL, WIN), Verdict::Allow);
    }

    #[test]
    fn sleep_blocks_even_essential_but_not_system() {
        let cfg = Config::example();
        let wa = app(WHATSAPP, None);
        let explorer = app(r"C:\Windows\explorer.exe", None);
        assert_eq!(decide(&DayState::Sleep, &wa, &cfg, INSTALL, WIN), Verdict::Block);
        assert_eq!(decide(&DayState::Sleep, &explorer, &cfg, INSTALL, WIN), Verdict::Allow);
    }

    #[test]
    fn user_writable_windows_folders_are_not_system() {
        let cfg = Config::example();
        for p in [
            r"C:\Windows\Temp\juego.exe",
            r"C:\Windows\Tasks\juego.exe",
            r"C:\Windows\System32\spool\drivers\color\j.exe",
        ] {
            assert_eq!(decide(&DayState::Locked, &app(p, None), &cfg, INSTALL, WIN), Verdict::Block, "{p}");
        }
        let notepad = app(r"C:\Windows\System32\notepad.exe", None);
        assert_eq!(decide(&DayState::Locked, &notepad, &cfg, INSTALL, WIN), Verdict::Allow);
    }

    #[test]
    fn a_folder_named_like_ours_is_not_ours() {
        let cfg = Config::example();
        // "C:\Program Files\ProcastinaciOFF-falso\juego.exe" starts like the install folder
        let fake = app(r"C:\Program Files\ProcastinaciOFF-falso\juego.exe", None);
        assert_eq!(decide(&DayState::Locked, &fake, &cfg, INSTALL, WIN), Verdict::Block);
    }
}
