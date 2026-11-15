// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Verificación con cámara en el PC 🔒: el verificador corre en un AppContainer con solo la
//! capacidad "webcam" (sin red). De su salida solo se usan los 5 campos de `VerificationResult`.

use std::sync::Mutex;

use procastinacioff_ipc::{Request, Response};
use serde_json::Value;

use crate::pipe;

/// Proceso aislado en curso (para el botón "Detener").
static RUNNING: Mutex<Option<u32>> = Mutex::new(None);
/// Repeticiones por sesión de ejercicio. ponytail: fijo; hacerlo configurable por meta si se pide.
pub const POSE_REPS: u16 = 20;

fn valid_goal_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Ejecuta la sesión y, si se aprueba, registra la meta cumplida en el servicio.
/// Devuelve si se aprobó.
#[tauri::command]
pub async fn verify_on_pc(goal_id: String, method: String, target_minutes: u16) -> Result<bool, String> {
    let (flag, target) = match method.as_str() {
        "reading_presence" => ("reading", target_minutes.clamp(1, 600)),
        "pose_reps" => ("pose", POSE_REPS),
        _ => return Err("Este método no está disponible en el PC".into()),
    };
    if !valid_goal_id(&goal_id) {
        return Err("Meta inválida".into()); // además impide inyectar argumentos en la línea de comandos
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("procastinacioff-verifier.exe");
    let cmd = format!("\"{}\" --method {flag} --goal {goal_id} --target {target}", exe.display());
    let timeout_ms = (target as u32 * 60 + 600) * 1000;
    let line =
        tauri::async_runtime::spawn_blocking(move || run(&cmd, timeout_ms)).await.map_err(|e| e.to_string())??;
    *RUNNING.lock().unwrap_or_else(|p| p.into_inner()) = None;

    let r: Value = serde_json::from_str(&line).map_err(|_| "La sesión se detuvo sin resultado".to_string())?;
    if let Some(e) = r["error"].as_str() {
        return Err(format!("No se pudo comprobar: {e}"));
    }
    let method = match r["method"].as_str() {
        Some("ReadingPresence") => parse_method("reading_presence"),
        Some("PoseReps") => parse_method("pose_reps"),
        _ => None,
    }
    .ok_or("Resultado inválido")?;
    if r["goal_id"].as_str() != Some(goal_id.as_str()) {
        return Err("Resultado de otra meta".into());
    }
    if r["approved"] != true {
        return Ok(false);
    }
    let req = Request::CompleteGoal {
        goal_id,
        method,
        score: r["score"].as_u64().unwrap_or(0).min(100) as u8,
        minutes: r["minutes"].as_u64().unwrap_or(0).min(1440) as u16,
    };
    match tauri::async_runtime::spawn_blocking(move || pipe::call(&req)).await.map_err(|e| e.to_string())?? {
        Response::Error { message, .. } => Err(message),
        _ => Ok(true),
    }
}

/// Convierte el nombre serde del método al tipo del núcleo (vía JSON: la UI no depende del núcleo).
fn parse_method(name: &str) -> Option<procastinacioff_ipc::Method> {
    serde_json::from_value(Value::String(name.into())).ok()
}

/// Botón "Detener": termina la sesión aislada (sin resultado).
#[tauri::command]
pub fn stop_verification() {
    if let Some(pid) = RUNNING.lock().unwrap_or_else(|p| p.into_inner()).take() {
        #[cfg(windows)]
        procastinacioff_sandbox::terminate(pid);
        #[cfg(not(windows))]
        let _ = pid;
    }
}

#[cfg(windows)]
fn run(cmd: &str, timeout_ms: u32) -> Result<String, String> {
    procastinacioff_sandbox::run_sandboxed(cmd, timeout_ms, |pid| {
        *RUNNING.lock().unwrap_or_else(|p| p.into_inner()) = Some(pid);
    })
    .map(|o| o.line)
    .map_err(|e| format!("No se pudo abrir el verificador aislado: {e}"))
}

#[cfg(not(windows))]
fn run(_: &str, _: u32) -> Result<String, String> {
    Err("El verificador de escritorio llega a macOS/Linux en las fases 4 y 5".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_ids_cannot_carry_extra_arguments() {
        assert!(valid_goal_id("leer-30"));
        assert!(!valid_goal_id("x --method pose"));
        assert!(!valid_goal_id("x\" & calc"));
        assert!(parse_method("pose_reps").is_some());
        assert!(parse_method("hack").is_none());
    }
}
