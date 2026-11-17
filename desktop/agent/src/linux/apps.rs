// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What an "app" is on Linux (security-critical): an executable referenced by a system `.desktop`
//! file that shows in the menu, plus Flatpak and Snap apps recognized by their path. Command-line
//! tools (`Terminal=true`), hidden entries and system services are not apps.
//!
//! SECURITY: only root-owned directories are read. A `.desktop` file in the user's home could claim
//! the id of an essential app and point it at any program.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

const APP_DIRS: [&str; 4] = [
    "/usr/share/applications",
    "/usr/local/share/applications",
    "/var/lib/flatpak/exports/share/applications",
    "/var/lib/snapd/desktop/applications",
];
const BIN_DIRS: [&str; 5] = ["/usr/local/bin", "/usr/bin", "/bin", "/usr/local/sbin", "/usr/sbin"];
/// Launchers shared by many apps: the app is recognized later, by the path it really runs.
const LAUNCHERS: [&str; 3] = ["flatpak", "snap", "env"];

#[derive(Debug, Default)]
pub struct DesktopApps {
    /// Canonical executable → `.desktop` id.
    by_exe: HashMap<PathBuf, String>,
    /// Snap name → `.desktop` id ("firefox" → "firefox_firefox").
    snaps: HashMap<String, String>,
}

/// The program an `Exec=` line runs: skips `env VAR=x` prefixes; field codes (%U) come after it.
/// ponytail: paths with spaces inside quotes are not split correctly; none in the usual packages.
fn exec_program(exec: &str) -> Option<&str> {
    let mut words = exec.split_whitespace().map(|w| w.trim_matches('"'));
    let first = words.next()?;
    if first == "env" || first.ends_with("/env") {
        return words.find(|w| !w.contains('=') && !w.starts_with('-'));
    }
    Some(first)
}

/// Fields of the `[Desktop Entry]` group that matter here.
fn entry(content: &str) -> Option<(String, bool)> {
    let mut in_entry = false;
    let (mut exec, mut shown) = (None, true);
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        match line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
            Some(("Exec", v)) => exec = exec_program(v).map(String::from),
            Some(("NoDisplay" | "Hidden" | "Terminal", "true")) => shown = false,
            Some(("Type", t)) if t != "Application" => shown = false,
            _ => {}
        }
    }
    Some((exec?, shown))
}

impl DesktopApps {
    /// Reads the system `.desktop` files. Cheap enough to redo every minute (new installs).
    pub fn load() -> Self {
        let files =
            APP_DIRS.iter().flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten()).filter_map(|e| {
                let path = e.path();
                let id = path.file_name()?.to_str()?.strip_suffix(".desktop")?.to_string();
                Some((id, std::fs::read_to_string(&path).ok()?))
            });
        Self::from_files(files, |program| {
            let path = if program.starts_with('/') {
                PathBuf::from(program)
            } else {
                BIN_DIRS.iter().map(|d| Path::new(d).join(program)).find(|p| p.exists())?
            };
            std::fs::canonicalize(path).ok()
        })
    }

    /// `files`: (id, content). `resolve`: program name → canonical path (the kernel reports real paths).
    fn from_files(files: impl Iterator<Item = (String, String)>, resolve: impl Fn(&str) -> Option<PathBuf>) -> Self {
        let mut apps = Self::default();
        for (id, content) in files {
            if let Some((name, _)) = id.split_once('_') {
                apps.snaps.entry(name.to_string()).or_insert_with(|| id.clone());
            }
            let Some((program, true)) = entry(&content) else { continue };
            let Some(exe) = resolve(&program) else { continue };
            let base = exe.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if LAUNCHERS.contains(&base) {
                continue;
            }
            apps.by_exe.entry(exe).or_insert(id);
        }
        apps
    }

    /// The `.desktop` id of what is being executed, or None if it is not an app.
    pub fn identify(&self, exe: &Path) -> Option<String> {
        if let Some(id) = self.by_exe.get(exe) {
            return Some(id.clone());
        }
        let s = exe.to_str()?;
        // Flatpak (system install): /var/lib/flatpak/app/<id>/<arch>/<branch>/<commit>/files/...
        if let Some(rest) = s.strip_prefix("/var/lib/flatpak/app/") {
            return rest.split('/').next().filter(|id| !id.is_empty()).map(String::from);
        }
        // Snap: /snap/<name>/<revision>/... VERIFY: the path the kernel reports for confined snaps (VM).
        if let Some(rest) = s.strip_prefix("/snap/") {
            let name = rest.split('/').next().filter(|n| !n.is_empty() && *n != "bin")?;
            return Some(self.snaps.get(name).cloned().unwrap_or_else(|| format!("{name}_{name}")));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps() -> DesktopApps {
        let files = [
            ("org.mozilla.firefox", "[Desktop Entry]\nType=Application\nExec=firefox %u\n"),
            ("code", "[Desktop Entry]\nExec=env GDK_BACKEND=x11 /usr/share/code/code --unity-launch %F\n"),
            ("htop", "[Desktop Entry]\nExec=htop\nTerminal=true\n"),
            ("polkit-agent", "[Desktop Entry]\nExec=/usr/libexec/polkit-agent\nNoDisplay=true\n"),
            ("org.gimp.GIMP", "[Desktop Entry]\nExec=/usr/bin/flatpak run org.gimp.GIMP\n"),
            ("firefox_firefox", "[Desktop Entry]\nExec=env BAMF=x /snap/bin/firefox %u\n"),
            ("other", "[Desktop Action new]\nExec=/usr/bin/rm\n[Desktop Entry]\nExec=/usr/bin/gedit\n"),
        ];
        DesktopApps::from_files(files.iter().map(|(a, b)| (a.to_string(), b.to_string())), |p| {
            Some(match p {
                "firefox" => PathBuf::from("/usr/lib/firefox/firefox.sh"), // a symlink, resolved
                "/snap/bin/firefox" => PathBuf::from("/usr/bin/snap"),
                p => PathBuf::from(p),
            })
        })
    }

    #[test]
    fn menu_apps_are_apps_and_tools_are_not() {
        let a = apps();
        let id = |p: &str| a.identify(Path::new(p));
        assert_eq!(id("/usr/lib/firefox/firefox.sh").as_deref(), Some("org.mozilla.firefox"));
        assert_eq!(id("/usr/share/code/code").as_deref(), Some("code"), "env prefix skipped");
        assert_eq!(id("/usr/bin/gedit").as_deref(), Some("other"), "only the [Desktop Entry] group");
        assert_eq!(id("/usr/bin/rm"), None);
        assert_eq!(id("htop"), None, "terminal tools are not apps");
        assert_eq!(id("/usr/libexec/polkit-agent"), None, "hidden entries are not apps");
        assert_eq!(id("/usr/bin/flatpak"), None, "shared launchers are never an app");
        assert_eq!(id("/usr/bin/snap"), None);
    }

    #[test]
    fn flatpak_and_snap_apps_are_recognized_by_path() {
        let a = apps();
        let id = |p: &str| a.identify(Path::new(p));
        assert_eq!(
            id("/var/lib/flatpak/app/org.gimp.GIMP/x86_64/stable/abc/files/bin/gimp").as_deref(),
            Some("org.gimp.GIMP")
        );
        assert_eq!(id("/snap/firefox/4793/usr/lib/firefox/firefox").as_deref(), Some("firefox_firefox"));
        assert_eq!(id("/snap/vlc/12/bin/vlc").as_deref(), Some("vlc_vlc"));
        assert_eq!(id("/snap/bin/firefox"), None);
    }
}
