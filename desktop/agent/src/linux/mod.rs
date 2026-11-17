// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Linux service (systemd, root). Plan 09 · Part A.
//! - fanotify decides before each exec; apps opened before a restrictive state are closed after
//!   the 60 s grace. SLEEP also locks the sessions.
//! - Only root can stop it (`systemctl stop`); a stop while the computer is on shows up as
//!   `Tamper::ServiceStopped` at the next start, like on Windows.

mod apps;
mod browser_policy;
mod exec_guard;
mod family;
mod ipc;
mod policy;
mod secrets;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use procastinacioff_core::api::Core;
use procastinacioff_core::model::DayState;

use crate::clock::now_clock;
use crate::common::{self, Grace, Shared, TICK, Upkeep, note_blocked};
use crate::lan;
use apps::DesktopApps;
use exec_guard::{Guard, Snapshot, View};

pub const DATA_DIR: &str = "/var/lib/procastinacioff";
pub const INSTALL_DIR: &str = "/usr/lib/procastinacioff";
/// Where the UI package (Tauri `.deb`) installs the UI: root-owned, the user cannot replace it.
const UI_EXE: &str = "/usr/bin/procastinacioff-app";
/// New apps installed and drives mounted are picked up this often.
const RESCAN_EVERY: Duration = Duration::from_secs(60);
/// While sleeping, sessions are locked again this often if someone unlocks them.
const RELOCK_EVERY: Duration = Duration::from_secs(5 * 60);

pub fn main() {
    // journald adds the time; no personal data in the log.
    tracing_subscriber::fmt().with_ansi(false).without_time().with_writer(std::io::stderr).init();
    match std::env::args().nth(1).as_deref() {
        Some("service") => {
            if let Err(e) = service() {
                tracing::error!(error = %e, "the service ended with an error");
                std::process::exit(1);
            }
        }
        // The package calls it before removing the agent: a minor's computer must not keep a dead DNS.
        Some("restore-dns") => family::restore(Path::new(DATA_DIR)),
        _ => eprintln!("usage: procastinacioff-agent (service|restore-dns)"),
    }
}

/// Never blocked: the agent's folder and the UI.
fn ours(install_dir: &Path) -> Vec<PathBuf> {
    vec![install_dir.to_path_buf(), PathBuf::from(UI_EXE)]
}

/// systemd notifications (`Type=notify`, `WatchdogSec=`). Without systemd, nothing happens.
fn sd_notify(msg: &str) {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixDatagram};
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else { return };
    let path = path.to_string_lossy();
    let addr = match path.strip_prefix('@') {
        Some(name) => SocketAddr::from_abstract_name(name.as_bytes()),
        None => SocketAddr::from_pathname(path.as_ref()),
    };
    if let (Ok(sock), Ok(addr)) = (UnixDatagram::unbound(), addr) {
        let _ = sock.send_to_addr(msg.as_bytes(), &addr);
    }
}

fn service() -> Result<(), Box<dyn std::error::Error>> {
    // SECURITY: never dump core. A crash (or a panic: release builds abort) would make the kernel run
    // the core-dump helper, an exec that only this process can approve while it is dying: every
    // exec on the computer would then wait forever. Found in WSL with the watchdog's SIGABRT.
    nix::sys::prctl::set_dumpable(false)?;
    let stop = Arc::new(AtomicBool::new(false));
    let data_dir = PathBuf::from(DATA_DIR);
    let install_dir = PathBuf::from(INSTALL_DIR);
    let secrets = Arc::new(secrets::CredsStore::open(data_dir.join("secrets"))?);
    let core = Core::open(data_dir.join("core.db").to_string_lossy().into(), secrets)?;
    // A standard user cannot change the clock of a root service's computer: its time wins over the phones'.
    core.set_time_source(true)?;
    if let Err(e) = core.clone().start_listening(lan::PORT) {
        tracing::error!(error = %e, "cannot listen for the profile's devices (port in use?)");
    }
    common::check_downtime(&core, &data_dir);

    let shared = common::new_shared();
    let view = exec_guard::new_view();
    let apps = Arc::new(RwLock::new(DesktopApps::load()));
    let (wake, woken) = std::sync::mpsc::channel();
    {
        let (core, shared, stop) = (core.clone(), shared.clone(), stop.clone());
        std::thread::spawn(move || common::sync_loop(core, shared, stop, woken));
    }
    {
        let (core, stop) = (core.clone(), stop.clone());
        std::thread::spawn(move || lan::run(core, stop, wake));
    }
    {
        let (core, data_dir, install_dir, stop) = (core.clone(), data_dir.clone(), install_dir.clone(), stop.clone());
        std::thread::spawn(move || family::run(core, data_dir, install_dir, stop));
    }
    {
        let (core, shared, ui) = (core.clone(), shared.clone(), PathBuf::from(UI_EXE));
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => return tracing::error!(error = %e, "no runtime for the IPC"),
            };
            if let Err(e) = rt.block_on(ipc::serve(core, shared, ui)) {
                tracing::error!(error = %e, "IPC server down");
            }
        });
    }
    let guard = match Guard::new() {
        Ok(g) => {
            let g = Arc::new(g);
            tracing::info!(filesystems = g.watch(&exec_guard::mount_points()), "exec guard on");
            let (g2, view, apps, shared, ours) =
                (g.clone(), view.clone(), apps.clone(), shared.clone(), ours(&install_dir));
            std::thread::spawn(move || {
                let now = || {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis() as i64)
                };
                let end = g2.run(&view, &apps, &ours, |app| note_blocked(&shared, app, now()));
                // SECURITY: nobody would answer the kernel anymore and every exec on the computer would
                // wait. Exiting closes the descriptor (the kernel allows what is pending) and systemd
                // starts the agent again.
                tracing::error!(error = ?end.err(), "exec guard down: restarting the agent");
                std::process::exit(1);
            });
            Some(g)
        }
        // Without CAP_SYS_ADMIN (development) apps are still closed by `sweep`.
        Err(e) => {
            tracing::error!(error = %e, "no fanotify: only running apps are closed");
            None
        }
    };
    sd_notify("READY=1");
    enforce(&core, &shared, &view, &apps, guard.as_deref(), &stop, &data_dir, &install_dir);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn enforce(
    core: &Core,
    shared: &Shared,
    view: &View,
    apps: &RwLock<DesktopApps>,
    guard: Option<&Guard>,
    stop: &AtomicBool,
    data_dir: &Path,
    install_dir: &Path,
) {
    let (mut grace, mut upkeep) = (Grace::default(), Upkeep::new());
    let ours = ours(install_dir);
    let mut last_policy: Option<Option<Vec<String>>> = None; // forces the first write
    let mut last_scan = Instant::now();
    let mut locked_at: Option<Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        let clock = now_clock(core);
        // Not configured = nothing is blocked: the user is still onboarding.
        let state = core.status(clock).map(|s| s.state).unwrap_or(DayState::Free);
        let permissive = policy::permissive(&state);
        let left = grace.update(permissive, clock, shared);
        let cfg = core.active_config(clock);
        let snap = Snapshot {
            state: state.clone(),
            cfg: cfg.clone(),
            blocking: !permissive && left.is_zero() && cfg.is_some(),
        };
        // SECURITY: the write lock is never held while anything runs (the guard answers execs).
        *view.write().unwrap_or_else(|p| p.into_inner()) = snap.clone();

        if snap.blocking {
            let closed = exec_guard::sweep(&snap, &apps.read().unwrap_or_else(|p| p.into_inner()), &ours);
            if let Some(app) = closed.into_iter().last() {
                note_blocked(shared, app, clock.now_ms);
                tracing::info!("app closed by policy"); // no name: personal data
            }
        }
        // SLEEP: lock the sessions (once, then again every 5 min if someone unlocks them).
        locked_at = match (&state, snap.blocking) {
            (DayState::Sleep, true) if locked_at.is_none_or(|t| t.elapsed() > RELOCK_EVERY) => {
                let _ = std::process::Command::new("loginctl").arg("lock-sessions").status();
                Some(Instant::now())
            }
            (DayState::Sleep, true) => locked_at,
            _ => None,
        };
        // Web allowlist: only during FOCUS, and only rewritten when it changes.
        let want = match (&state, &cfg) {
            (DayState::Focus { .. }, Some(cfg)) => Some(cfg.web_allowlist.clone()),
            _ => None,
        };
        if last_policy.as_ref() != Some(&want) {
            if let Err(e) = browser_policy::apply(want.as_deref()) {
                tracing::warn!(error = %e, "could not apply the browser policy");
            }
            last_policy = Some(want);
        }
        if last_scan.elapsed() > RESCAN_EVERY {
            last_scan = Instant::now();
            let fresh = DesktopApps::load(); // read outside the lock
            *apps.write().unwrap_or_else(|p| p.into_inner()) = fresh;
            if let Some(g) = guard {
                g.watch(&exec_guard::mount_points());
            }
        }
        upkeep.tick(core, clock, data_dir);
        sd_notify("WATCHDOG=1");
        std::thread::sleep(TICK);
    }
}
