// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Servicio de Windows. **No acepta STOP**: `sc stop` devuelve error. Solo un administrador puede
//! matarlo (y eso se detecta como manipulación).

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use procastinacioff_core::api::Core;
use procastinacioff_ipc::AgentStatus;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

use crate::{enforce, family, ipc_server, secrets::DpapiStore};

pub const SERVICE_NAME: &str = "ProcastinaciOFF";
pub const DATA_DIR: &str = r"C:\ProgramData\ProcastinaciOFF";

define_windows_service!(ffi_service_main, service_main);

pub fn run_dispatcher() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = run() {
        tracing::error!(error = %e, "el servicio terminó con error");
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
        // Solo el apagado del equipo detiene el servicio
        ServiceControl::Shutdown => {
            stop_handler.store(true, Ordering::Relaxed);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    status_handle.set_service_status(status(ServiceState::Running, ServiceControlAccept::SHUTDOWN))?; // sin STOP

    let data_dir = PathBuf::from(DATA_DIR);
    let install_dir = std::env::current_exe()?.parent().ok_or("sin carpeta de instalación")?.to_path_buf();
    let secrets = Arc::new(DpapiStore::new(data_dir.join("secrets"))?);
    let core = Core::open(data_dir.join("core.db").to_string_lossy().into(), secrets)?;
    enforce::check_downtime(&core, &data_dir);

    let shared = Arc::new(Mutex::new(AgentStatus {
        core: None,
        blocked_app: None,
        blocked_at_ms: None,
        grace_until_ms: None,
        synced_at_ms: None,
    }));
    {
        let (core, shared, stop) = (core.clone(), shared.clone(), stop.clone());
        std::thread::spawn(move || enforce::sync_loop(core, shared, stop));
    }
    {
        // Modo familiar: filtro web si este PC es de un menor (comprueba el rol cada minuto)
        let (core, data_dir, stop) = (core.clone(), data_dir.clone(), stop.clone());
        std::thread::spawn(move || family::run(core, data_dir, stop));
    }
    {
        let (core, shared, install_dir) = (core.clone(), shared.clone(), install_dir.clone());
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => return tracing::error!(error = %e, "sin runtime para el IPC"),
            };
            if let Err(e) = rt.block_on(ipc_server::serve(core, shared, install_dir)) {
                tracing::error!(error = %e, "servidor IPC caído");
            }
        });
    }
    enforce::run(core, shared, stop, install_dir, data_dir); // bloquea hasta el apagado

    status_handle.set_service_status(status(ServiceState::Stopped, ServiceControlAccept::empty()))?;
    Ok(())
}
