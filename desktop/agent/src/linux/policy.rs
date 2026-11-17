// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! May this exec go ahead? (security-critical) The same rules as Windows' `policy::decide`.
//! When in doubt about the SYSTEM, allow: a frozen computer is worse than a missed block.

use std::path::{Path, PathBuf};

use procastinatioff_core::model::{Config, DayState};

/// First uid of a person's account (Debian/Ubuntu/Fedora). Below: root and system services.
pub const FIRST_USER_UID: u32 = 1000;
/// Where a user can put their own executables (AppImage, copies of a system binary…).
const USER_DIRS: [&str; 5] = ["/home", "/tmp", "/var/tmp", "/dev/shm", "/run/user"];
/// Desktop settings, like Windows' Settings: never blocked.
const SYSTEM_APPS: [&str; 4] =
    ["org.gnome.Settings", "gnome-control-center", "systemsettings", "org.kde.systemsettings"];

pub fn permissive(state: &DayState) -> bool {
    matches!(state, DayState::Free | DayState::Pass { .. } | DayState::Emergency { .. })
}

/// `app`: `.desktop` id of the executable (apps.rs). `uid`: real uid of the process that runs it.
/// `ours`: our installation (the agent's folder and the UI binary), always allowed.
pub fn allow(
    state: &DayState,
    app: Option<&str>,
    path: &Path,
    uid: u32,
    cfg: Option<&Config>,
    ours: &[PathBuf],
) -> bool {
    if uid < FIRST_USER_UID || permissive(state) || ours.iter().any(|o| path.starts_with(o)) {
        return true;
    }
    let Some(id) = app else {
        // Not an app: system tools are allowed; the user's own executables are not.
        return !USER_DIRS.iter().any(|d| path.starts_with(d));
    };
    if SYSTEM_APPS.contains(&id) {
        return true;
    }
    if matches!(state, DayState::Sleep) {
        return false; // not even essential apps while sleeping
    }
    let Some(cfg) = cfg else { return false };
    let ident = format!("linux:desktop:{id}");
    let focus: &[String] = match state {
        DayState::Focus { apps, .. } => apps,
        _ => &[],
    };
    cfg.essential_apps.iter().chain(focus).any(|a| a.eq_ignore_ascii_case(&ident))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        let mut c = Config::example();
        c.essential_apps = vec!["linux:desktop:org.gnome.Calculator".into()];
        c.focus_apps = vec!["linux:desktop:org.libreoffice.LibreOffice.writer".into()];
        c
    }

    fn ok(state: DayState, app: Option<&str>, path: &str, uid: u32) -> bool {
        let ours = [PathBuf::from("/usr/lib/procastinatioff"), PathBuf::from("/usr/bin/procastinatioff-app")];
        allow(&state, app, Path::new(path), uid, Some(&cfg()), &ours)
    }

    #[test]
    fn locked_allows_essentials_and_system_tools_only() {
        assert!(ok(DayState::Locked, Some("org.gnome.Calculator"), "/usr/bin/gnome-calculator", 1000));
        assert!(!ok(DayState::Locked, Some("org.mozilla.firefox"), "/usr/lib/firefox/firefox", 1000));
        assert!(ok(DayState::Locked, None, "/usr/bin/ls", 1000), "command-line tools keep working");
        assert!(ok(DayState::Locked, Some("gnome-control-center"), "/usr/bin/gnome-control-center", 1000));
        assert!(ok(DayState::Locked, None, "/usr/lib/procastinatioff/procastinatioff-agent", 1000));
        // Our UI has a menu entry (an app), and must never be blocked
        assert!(ok(DayState::Sleep, Some("ProcastinatiOFF"), "/usr/bin/procastinatioff-app", 1000));
        assert!(!ok(DayState::Locked, Some("x"), "/usr/bin/procastinatioff-app-fake", 1000));
    }

    #[test]
    fn copies_in_user_folders_are_denied() {
        for p in ["/home/ana/ff", "/tmp/.mount_game/AppRun", "/dev/shm/x", "/var/tmp/x"] {
            assert!(!ok(DayState::Locked, None, p, 1000), "{p}");
        }
        assert!(ok(DayState::Free, None, "/home/ana/ff", 1000), "free: anything goes");
    }

    #[test]
    fn focus_adds_focus_apps_and_sleep_allows_no_app() {
        let writer = Some("org.libreoffice.LibreOffice.writer");
        assert!(!ok(DayState::Locked, writer, "/usr/lib/libreoffice/program/soffice.bin", 1000));
        let focus = DayState::Focus { goal_id: "g".into(), apps: cfg().focus_apps, sites: vec![] };
        assert!(ok(focus, writer, "/usr/lib/libreoffice/program/soffice.bin", 1000));
        assert!(!ok(DayState::Sleep, Some("org.gnome.Calculator"), "/usr/bin/gnome-calculator", 1000));
        assert!(ok(DayState::Sleep, None, "/usr/bin/gnome-screensaver", 1000));
    }

    #[test]
    fn system_accounts_are_never_blocked() {
        assert!(ok(DayState::Sleep, Some("org.mozilla.firefox"), "/usr/lib/firefox/firefox", 0));
        assert!(ok(DayState::Locked, None, "/tmp/apt-helper", 105));
    }
}
