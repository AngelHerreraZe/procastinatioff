// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Servicio privilegiado de escritorio. Windows en la Fase 1; macOS/Linux en las fases 4 y 5.

#[cfg(windows)]
mod browser_policy;
#[cfg(windows)]
mod clock;
#[cfg(windows)]
mod enforce;
#[cfg(windows)]
mod family;
#[cfg(windows)]
mod install;
#[cfg(windows)]
mod ipc_server;
#[allow(dead_code)] // en macOS/Linux solo la usan las pruebas hasta las fases 4 y 5
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

/// ponytail: el log crece sin rotación; rotar al arrancar si supera 5 MB cuando se note en pruebas.
#[cfg(windows)]
fn main() {
    // Log en archivo dentro de la carpeta protegida; sin datos personales.
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
                eprintln!("instalación del servicio: {e}");
                std::process::exit(1);
            }
        }
        // El desinstalador lo llama antes de borrar nada: sin esto el PC de un menor se quedaría sin DNS.
        Some("restore-dns") => family::restore(std::path::Path::new(service::DATA_DIR)),
        _ => eprintln!("uso: procastinacioff-agent (service|install|restore-dns)"),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("El agente de macOS/Linux se implementa en las fases 4 y 5.");
}
