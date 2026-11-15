// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Servidor IPC con verificación del cliente 🔒.
//! - La ACL del pipe admite SYSTEM, Administradores y usuarios interactivos; rechaza clientes remotos.
//! - Además, el proceso cliente debe ser nuestra UI firmada: otro programa del usuario no puede,
//!   por ejemplo, disparar una emergencia.

use std::os::windows::io::AsRawHandle;
use std::sync::{Arc, Mutex};

use procastinacioff_core::api::Core;
use procastinacioff_ipc::{AgentStatus, MAX_LINE, PIPE_NAME};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows::core::w;

use crate::clock::now_clock;
use crate::signer::SignerCache;

/// Firmante de nuestros binarios (CN del certificado de firma de código de la empresa).
pub const OUR_SIGNER: &str = "ProcastinaciOFF SAS"; // ← cambiar por el CN real del certificado (Fase 0, trámites)
pub const UI_EXE: &str = "procastinacioff-app.exe";

pub type Shared = Arc<Mutex<AgentStatus>>;

fn create_pipe(first: bool) -> std::io::Result<NamedPipeServer> {
    unsafe {
        // SY = SYSTEM, BA = Administradores, IU = usuarios interactivos (lectura/escritura)
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)"),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        let server = ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(PIPE_NAME, &mut sa as *mut _ as *mut _);
        let _ = LocalFree(HLOCAL(sd.0));
        server
    }
}

/// ¿El cliente es nuestra UI firmada, instalada junto al servicio?
fn client_is_trusted(pipe: &NamedPipeServer, cache: &Mutex<SignerCache>, install_dir: &std::path::Path) -> bool {
    let mut pid = 0u32;
    if unsafe { GetNamedPipeClientProcessId(HANDLE(pipe.as_raw_handle()), &mut pid) }.is_err() {
        return false;
    }
    let Some(path) = crate::procs::image_path(pid) else { return false };
    let expected = install_dir.join(UI_EXE);
    let path_ok = path.to_string_lossy().eq_ignore_ascii_case(&expected.to_string_lossy());
    let signer_ok = cache.lock().unwrap_or_else(|p| p.into_inner()).get(&path).as_deref() == Some(OUR_SIGNER);
    // En builds de desarrollo (sin firmar, desde target\) se permite con PCO_DEV=1. Nunca en release.
    let dev = cfg!(debug_assertions)
        && std::env::var("PCO_DEV").is_ok()
        && path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(UI_EXE));
    (path_ok && signer_ok) || dev
}

pub async fn serve(core: Arc<Core>, shared: Shared, install_dir: std::path::PathBuf) -> std::io::Result<()> {
    let cache = Arc::new(Mutex::new(SignerCache::default()));
    let mut server = create_pipe(true)?;
    loop {
        server.connect().await?;
        let connected = server;
        server = create_pipe(false)?; // siguiente instancia para el próximo cliente
        if !client_is_trusted(&connected, &cache, &install_dir) {
            tracing::warn!("cliente IPC no confiable rechazado");
            continue; // al soltar `connected` se cierra la conexión
        }
        let (core, shared) = (core.clone(), shared.clone());
        tokio::spawn(async move {
            let (reader, mut writer) = tokio::io::split(connected);
            // Límite de línea: un cliente no puede agotar la memoria del servicio.
            let mut lines = BufReader::new(reader.take(MAX_LINE as u64 * 64)).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let (core, shared) = (core.clone(), shared.clone());
                let reply = tokio::task::spawn_blocking(move || {
                    let agent = shared.lock().unwrap_or_else(|p| p.into_inner()).clone();
                    procastinacioff_core::desktop::handle_json(&core, now_clock(&core), agent, &line)
                })
                .await;
                let Ok(mut out) = reply else { break };
                out.push('\n');
                if writer.write_all(out.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
    }
}
