// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Desktop UI: it decides nothing. It shows what the service says and asks it for actions.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pipe;
mod verify;

use procastinacioff_ipc::{Request, Response};
use tauri::Manager;

/// The only UI → service bridge. The UI's JSON is validated when deserializing `Request`.
#[tauri::command]
async fn agent(request: Request) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || pipe::call(&request)).await.map_err(|e| e.to_string())?
}

/// Locks the Windows session (when entering SLEEP).
#[tauri::command]
fn lock_screen() {
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::System::Shutdown::LockWorkStation();
    }
}

/// Pairing QR as SVG, generated locally.
#[tauri::command]
fn qr_svg(text: String) -> Result<String, String> {
    if text.len() > 2048 {
        return Err("error.too_large".into());
    }
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code.render::<qrcode::render::svg::Color>().min_dimensions(256, 256).quiet_zone(true).build())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            agent,
            lock_screen,
            qr_svg,
            verify::verify_on_pc,
            verify::stop_verification
        ])
        .on_window_event(|window, event| {
            // Closing the window only hides it: protection continues and the shield must be able to come back.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            if std::env::args().any(|a| a == "--background")
                && let Some(w) = app.get_webview_window("main")
            {
                let _ = w.hide();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| eprintln!("error starting the app: {e}"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn qr_is_svg_and_rejects_huge_input() {
        assert!(super::qr_svg("{\"v\":1}".into()).unwrap().contains("<svg"));
        assert!(super::qr_svg("x".repeat(3000)).is_err());
    }
}
