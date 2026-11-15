// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Camera verification on the PC (security-critical): the verifier runs in an AppContainer with only the
//! "webcam" capability (no network). Only the 5 fields of `VerificationResult` are used from its output.

use std::sync::Mutex;

use procastinacioff_ipc::{Request, Response};
use serde_json::Value;

use crate::pipe;

/// Sandboxed process in progress (for the "Stop" button).
static RUNNING: Mutex<Option<u32>> = Mutex::new(None);
/// Reps per exercise session. ponytail: fixed; make it configurable per goal if asked for.
pub const POSE_REPS: u16 = 20;

fn valid_goal_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Runs the session and, if approved, records the completed goal in the service.
/// Returns whether it was approved. Errors are translation keys (`verify.*`) for the UI.
#[tauri::command]
pub async fn verify_on_pc(goal_id: String, method: String, target_minutes: u16) -> Result<bool, String> {
    let (flag, target) = match method.as_str() {
        "reading_presence" => ("reading", target_minutes.clamp(1, 600)),
        "pose_reps" => ("pose", POSE_REPS),
        _ => return Err("verify.unavailable".into()),
    };
    if !valid_goal_id(&goal_id) {
        return Err("verify.invalid_goal".into()); // also prevents injecting command-line arguments
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("procastinacioff-verifier.exe");
    let cmd = format!("\"{}\" --method {flag} --goal {goal_id} --target {target}", exe.display());
    let timeout_ms = (target as u32 * 60 + 600) * 1000;
    let line =
        tauri::async_runtime::spawn_blocking(move || run(&cmd, timeout_ms)).await.map_err(|e| e.to_string())??;
    *RUNNING.lock().unwrap_or_else(|p| p.into_inner()) = None;

    let r: Value = serde_json::from_str(&line).map_err(|_| "verify.no_result".to_string())?;
    if let Some(e) = r["error"].as_str() {
        return Err(format!("verify.failed:{e}"));
    }
    let method = match r["method"].as_str() {
        Some("ReadingPresence") => parse_method("reading_presence"),
        Some("PoseReps") => parse_method("pose_reps"),
        _ => None,
    }
    .ok_or("verify.invalid_result")?;
    if r["goal_id"].as_str() != Some(goal_id.as_str()) {
        return Err("verify.wrong_goal".into());
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

/// Converts the method's serde name to the core type (via JSON: the UI does not depend on the core).
fn parse_method(name: &str) -> Option<procastinacioff_ipc::Method> {
    serde_json::from_value(Value::String(name.into())).ok()
}

/// "Stop" button: ends the sandboxed session (no result).
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
    .map_err(|e| format!("verify.sandbox_failed:{e}"))
}

#[cfg(not(windows))]
fn run(_: &str, _: u32) -> Result<String, String> {
    Err("verify.unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_ids_cannot_carry_extra_arguments() {
        assert!(valid_goal_id("read-30"));
        assert!(!valid_goal_id("x --method pose"));
        assert!(!valid_goal_id("x\" & calc"));
        assert!(parse_method("pose_reps").is_some());
        assert!(parse_method("hack").is_none());
    }
}
