// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! UI de escritorio: no decide nada. Muestra lo que dice el servicio y le pide acciones.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pipe;
mod verify;

use procastinacioff_ipc::{Request, Response};
use tauri::Manager;

/// Único puente UI → servicio. El JSON de la UI se valida al deserializar `Request`.
#[tauri::command]
async fn agent(request: Request) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || pipe::call(&request)).await.map_err(|e| e.to_string())?
}

/// Bloquea la sesión de Windows (al entrar en DORMIR).
#[tauri::command]
fn lock_screen() {
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::System::Shutdown::LockWorkStation();
    }
}

/// QR de vinculación como SVG, generado en local.
#[tauri::command]
fn qr_svg(text: String) -> Result<String, String> {
    if text.len() > 2048 {
        return Err("texto demasiado largo".into());
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
            // Cerrar la ventana solo la oculta: la protección sigue y el escudo debe poder volver.
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
        .unwrap_or_else(|e| eprintln!("error al iniciar la app: {e}"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn qr_is_svg_and_rejects_huge_input() {
        assert!(super::qr_svg("{\"v\":1}".into()).unwrap().contains("<svg"));
        assert!(super::qr_svg("x".repeat(3000)).is_err());
    }
}
