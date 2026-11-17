// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Windows service. **Does not accept STOP**: `sc stop` returns an error. Only an administrator can
//! kill it (and that is detected as tampering).

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use procastinacioff_core::api::Core;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

use crate::{common, enforce, family, ipc_server, lan, secrets::DpapiStore};

pub const SERVICE_NAME: &str = "ProcastinaciOFF";
pub const DATA_DIR: &str = r"C:\ProgramData\ProcastinaciOFF";

define_windows_service!(ffi_service_main, service_main);

pub fn run_dispatcher() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = run() {
        tracing::error!(error = %e, "the service ended with an error");
    }
}

fn status(state: ServiceState, accept: ServiceControlAccept) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accept,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_handler = stop.clone();
    let status_handle = service_control_handler::register(SERVICE_NAME, move |control| match control {
        // Only shutting down the computer stops the service
        ServiceControl::Shutdown => {
            stop_handler.store(true, Ordering::Relaxed);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    status_handle.set_service_status(status(ServiceState::Running, ServiceControlAccept::SHUTDOWN))?; // no STOP

    let data_dir = PathBuf::from(DATA_DIR);
    let install_dir = std::env::current_exe()?.parent().ok_or("no install folder")?.to_path_buf();
    let secrets = Arc::new(DpapiStore::new(data_dir.join("secrets"))?);
    let core = Core::open(data_dir.join("core.db").to_string_lossy().into(), secrets)?;
    // A standard user cannot change the clock of a SYSTEM service's machine: its time wins over the phones'.
    core.set_time_source(true)?;
    if let Err(e) = core.clone().start_listening(lan::PORT) {
        tracing::error!(error = %e, "cannot listen for the profile's devices (port in use?)");
    }
    common::check_downtime(&core, &data_dir);

    let shared = common::new_shared();
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
        // Family mode: web filter if this PC belongs to a minor (checks the role every minute)
        let (core, data_dir, install_dir, stop) = (core.clone(), data_dir.clone(), install_dir.clone(), stop.clone());
        std::thread::spawn(move || family::run(core, data_dir, install_dir, stop));
    }
    {
        let (core, shared, install_dir) = (core.clone(), shared.clone(), install_dir.clone());
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => return tracing::error!(error = %e, "no runtime for the IPC"),
            };
            if let Err(e) = rt.block_on(ipc_server::serve(core, shared, install_dir)) {
                tracing::error!(error = %e, "IPC server down");
            }
        });
    }
    enforce::run(core, shared, stop, install_dir, data_dir); // blocks until shutdown

    status_handle.set_service_status(status(ServiceState::Stopped, ServiceControlAccept::empty()))?;
    Ok(())
}
