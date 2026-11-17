// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Privileged desktop service: Windows (service) and Linux (systemd). macOS arrives in phase 4.

#[cfg(windows)]
mod browser_policy;
#[cfg(any(windows, target_os = "linux"))]
mod clock;
#[cfg(any(windows, target_os = "linux"))]
mod common;
#[cfg(windows)]
mod enforce;
#[cfg(windows)]
mod family;
#[cfg(windows)]
mod install;
#[cfg(windows)]
mod ipc_server;
#[cfg(any(windows, target_os = "linux"))]
mod lan;
#[cfg(target_os = "linux")]
mod linux;
#[cfg_attr(not(windows), allow(dead_code))] // Windows identities; elsewhere only its tests use it
mod policy;
#[cfg(windows)]
mod procs;
#[cfg(windows)]
mod secrets;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod session;
#[cfg(windows)]
mod signer;

/// ponytail: the log grows without rotation; rotate at startup above 5 MB when it shows in testing.
#[cfg(windows)]
fn main() {
    // File log inside the protected folder; no personal data.
    let log_dir = std::path::Path::new(service::DATA_DIR).join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(log_dir.join("agent.log")) {
        tracing_subscriber::fmt().with_writer(std::sync::Mutex::new(file)).with_ansi(false).init();
    }

    match std::env::args().nth(1).as_deref() {
        Some("service") => {
            if let Err(e) = service::run_dispatcher() {
                eprintln!("dispatcher: {e}");
            }
        }
        Some("install") => {
            if let Err(e) = install::install() {
                eprintln!("service installation: {e}");
                std::process::exit(1);
            }
        }
        // The uninstaller calls it before deleting anything: without it a minor's PC would be left without DNS.
        Some("restore-dns") => family::restore(std::path::Path::new(service::DATA_DIR)),
        _ => eprintln!("uso: procastinatioff-agent (service|install|restore-dns)"),
    }
}

#[cfg(target_os = "linux")]
fn main() {
    linux::main();
}

#[cfg(not(any(windows, target_os = "linux")))]
fn main() {
    eprintln!("The macOS agent arrives in phase 4.");
}
