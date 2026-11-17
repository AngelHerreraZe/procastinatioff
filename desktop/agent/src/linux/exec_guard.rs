// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! fanotify `FAN_OPEN_EXEC_PERM` (security-critical): the kernel asks before running any binary.
//!
//! - The answer comes from a snapshot the enforcement loop refreshes every tick: no core call, no
//!   disk access (beyond /proc) and no lock held while anything is executed.
//! - If deciding panics or a field cannot be read, the exec is ALLOWED: a stuck agent must never
//!   freeze the computer. If the whole agent stops, systemd's watchdog kills it and the kernel
//!   allows everything pending when the descriptor closes.
//! - Requires kernel ≥ 5.0 and CAP_SYS_ADMIN (the systemd unit grants it).

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use nix::fcntl::AT_FDCWD;
use nix::sys::fanotify::{EventFFlags, Fanotify, FanotifyResponse, InitFlags, MarkFlags, MaskFlags, Response};
use procastinatioff_core::model::{Config, DayState};

use super::apps::DesktopApps;
use super::policy;

/// What the guard needs to decide, refreshed by the enforcement loop.
#[derive(Clone)]
pub struct Snapshot {
    pub state: DayState,
    pub cfg: Option<Config>,
    /// Restrictive state, configured, and the 60 s grace is over.
    pub blocking: bool,
}

pub type View = Arc<RwLock<Snapshot>>;

pub fn new_view() -> View {
    Arc::new(RwLock::new(Snapshot { state: DayState::Free, cfg: None, blocking: false }))
}

/// Filesystem types that never hold programs to run.
const PSEUDO_FS: [&str; 14] = [
    "proc",
    "sysfs",
    "cgroup",
    "cgroup2",
    "devpts",
    "mqueue",
    "debugfs",
    "tracefs",
    "securityfs",
    "pstore",
    "bpf",
    "configfs",
    "fusectl",
    "autofs",
];

/// Mount points of real filesystems (/, /home, /tmp, squashfs of snaps, USB drives…).
pub fn mount_points() -> Vec<PathBuf> {
    std::fs::read_to_string("/proc/self/mounts")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (_, point, fstype) = (f.next()?, f.next()?, f.next()?);
            (!PSEUDO_FS.contains(&fstype)).then(|| PathBuf::from(point.replace("\\040", " ")))
        })
        .collect()
}

pub struct Guard {
    fan: Fanotify,
}

impl Guard {
    pub fn new() -> nix::Result<Self> {
        let fan = Fanotify::init(
            InitFlags::FAN_CLASS_CONTENT | InitFlags::FAN_CLOEXEC,
            EventFFlags::O_RDONLY | EventFFlags::O_LARGEFILE | EventFFlags::O_CLOEXEC,
        )?;
        Ok(Self { fan })
    }

    /// Watches execs on the filesystem of each path. Marking twice is harmless: call it again when
    /// something new is mounted. Returns how many filesystems are watched.
    pub fn watch(&self, points: &[PathBuf]) -> usize {
        let flags = MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_FILESYSTEM;
        points
            .iter()
            .filter(|p| self.fan.mark(flags, MaskFlags::FAN_OPEN_EXEC_PERM, AT_FDCWD, Some(p.as_path())).is_ok())
            .count()
    }

    /// Answers every exec until the descriptor fails. `on_block`: told what was denied.
    pub fn run(
        &self,
        view: &View,
        apps: &RwLock<DesktopApps>,
        ours: &[PathBuf],
        on_block: impl Fn(String),
    ) -> nix::Result<()> {
        loop {
            for event in self.fan.read_events()? {
                let Some(fd) = event.fd() else { continue };
                let decide = || {
                    let snap = view.read().unwrap_or_else(|p| p.into_inner());
                    if !snap.blocking {
                        return true; // fast path: permissive state or still in grace
                    }
                    let Ok(path) = std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd())) else { return true };
                    let Some(uid) = uid_of(event.pid()) else { return true };
                    let app = apps.read().unwrap_or_else(|p| p.into_inner()).identify(&path);
                    let ok = policy::allow(&snap.state, app.as_deref(), &path, uid, snap.cfg.as_ref(), ours);
                    if !ok {
                        on_block(shown_name(app, &path));
                    }
                    ok
                };
                let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(decide)).unwrap_or(true);
                let answer = if ok { Response::FAN_ALLOW } else { Response::FAN_DENY };
                self.fan.write_response(FanotifyResponse::new(fd, answer))?;
            }
        }
    }
}

/// Real uid of a process (the one that owns it, not a setuid effective one).
pub fn uid_of(pid: i32) -> Option<u32> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status.lines().find_map(|l| l.strip_prefix("Uid:"))?.split_whitespace().next()?.parse().ok()
}

/// What the shield shows: the app id, or the file name of a non-app executable.
pub fn shown_name(app: Option<String>, path: &Path) -> String {
    app.unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

/// Running user processes the snapshot does not allow (opened before the state changed): killed.
/// Returns what was closed, for the shield.
pub fn sweep(snap: &Snapshot, apps: &DesktopApps, ours: &[PathBuf]) -> Vec<String> {
    let mut closed = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else { return closed };
    for pid in entries.flatten().filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok()) {
        let Some(uid) = uid_of(pid).filter(|u| *u >= policy::FIRST_USER_UID) else { continue };
        let Ok(exe) = std::fs::read_link(format!("/proc/{pid}/exe")) else { continue };
        let app = apps.identify(&exe);
        if !policy::allow(&snap.state, app.as_deref(), &exe, uid, snap.cfg.as_ref(), ours) {
            let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGKILL);
            closed.push(shown_name(app, &exe));
        }
    }
    closed
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    use super::*;

    /// A real fanotify guard on a scratch tmpfs only (the rest of the system is not watched).
    #[test]
    #[ignore = "needs root and a kernel with fanotify permission events"]
    fn denies_user_executables_but_not_system_accounts_and_closes_open_ones() {
        let dir = PathBuf::from(format!("/tmp/pco-fan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(Command::new("mount").args(["-t", "tmpfs", "tmpfs"]).arg(&dir).status().unwrap().success());
        for (from, to) in [("/usr/bin/true", "app"), ("/usr/bin/sleep", "slow")] {
            std::fs::copy(from, dir.join(to)).unwrap();
        }
        let user = |p: &Path| Command::new(p).uid(policy::FIRST_USER_UID).status();

        let view = new_view();
        let guard = Arc::new(Guard::new().unwrap());
        assert_eq!(guard.watch(std::slice::from_ref(&dir)), 1);
        let (g, v) = (guard.clone(), view.clone());
        std::thread::spawn(move || g.run(&v, &RwLock::new(DesktopApps::default()), &[], |_| {}));

        // Before anything restrictive: allowed. A program opened now keeps running.
        assert!(user(&dir.join("app")).unwrap().success());
        let mut open = Command::new(dir.join("slow")).arg("30").uid(policy::FIRST_USER_UID).spawn().unwrap();

        let locked = Snapshot { state: DayState::Locked, cfg: Some(Config::example()), blocking: true };
        *view.write().unwrap() = locked.clone();
        let denied = user(&dir.join("app")).unwrap_err();
        assert_eq!(denied.kind(), std::io::ErrorKind::PermissionDenied, "a user executable in /tmp is denied");
        assert!(Command::new(dir.join("app")).status().unwrap().success(), "root is never blocked");
        assert!(user(Path::new("/usr/bin/true")).unwrap().success(), "system tools keep working");

        // The one opened before is closed by the sweep
        let closed = sweep(&locked, &DesktopApps::default(), &[]);
        assert!(closed.contains(&"slow".to_string()), "{closed:?}");
        assert!(!open.wait().unwrap().success());

        *view.write().unwrap() = Snapshot { blocking: false, ..locked };
        assert!(user(&dir.join("app")).unwrap().success(), "free again");
        let _ = Command::new("umount").arg("-l").arg(&dir).status();
    }
}
