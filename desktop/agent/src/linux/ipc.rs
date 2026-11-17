// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! UI socket (security-critical): anyone may connect to /run/procastinacioff/agent.sock, but only
//! our UI, installed by the package in a root-owned path, is answered.
//! Honest limit (same as Windows): the check is the client's executable at connect time. The UI
//! cannot do anything the user cannot do by clicking; this only keeps scripts out.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use procastinacioff_core::api::Core;
use procastinacioff_ipc::SOCKET_PATH;
use tokio::net::{UnixListener, UnixStream};

use crate::common::{Shared, serve_lines};

fn client_is_trusted(stream: &UnixStream, ui_exe: &Path) -> bool {
    let Some(pid) = stream.peer_cred().ok().and_then(|c| c.pid()) else { return false };
    let Ok(exe) = std::fs::read_link(format!("/proc/{pid}/exe")) else { return false };
    // Development builds (unsigned, from target/) with PCO_DEV=1. Never in release.
    let dev = cfg!(debug_assertions) && std::env::var("PCO_DEV").is_ok() && exe.file_name() == ui_exe.file_name();
    exe == ui_exe || dev
}

pub async fn serve(core: Arc<Core>, shared: Shared, ui_exe: PathBuf) -> std::io::Result<()> {
    let path = Path::new(SOCKET_PATH);
    let _ = std::fs::remove_file(path); // left over from a previous run
    let listener = UnixListener::bind(path)?;
    // The user's UI connects without being root; who it is gets checked per connection.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
    loop {
        let (stream, _) = listener.accept().await?;
        if !client_is_trusted(&stream, &ui_exe) {
            tracing::warn!("untrusted IPC client rejected");
            continue;
        }
        tokio::spawn(serve_lines(stream, core.clone(), shared.clone()));
    }
}
