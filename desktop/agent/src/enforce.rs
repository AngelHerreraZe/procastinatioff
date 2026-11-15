// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bucle de aplicación. Reglas de experiencia que no se pueden saltar:
//! - Al pasar de un estado permisivo a uno restrictivo hay 60 s de gracia ("guarda tu trabajo").
//! - Si el servicio estuvo caído con el equipo encendido → `Tamper::ServiceStopped`.
//! - Si el reloj del sistema salta más de 2 min respecto al monotónico → `Tamper::ClockChanged`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use procastinacioff_core::api::Core;
use procastinacioff_core::model::{DayState, TamperKind};

use crate::clock::now_clock;
use crate::ipc_server::{Shared, UI_EXE};
use crate::policy::{Verdict, decide};
use crate::signer::SignerCache;
use crate::{browser_policy, procs, session};

const TICK: Duration = Duration::from_millis(300);
const GRACE: Duration = Duration::from_secs(60);
const HEARTBEAT_EVERY: Duration = Duration::from_secs(15 * 60);

fn alive_file(data_dir: &Path) -> PathBuf {
    data_dir.join("alive")
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Al arrancar: ¿estuvo el servicio caído con el equipo encendido?
pub fn check_downtime(core: &Core, data_dir: &Path) {
    let last: i64 = std::fs::read_to_string(alive_file(data_dir)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    let uptime_ms = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } as i64;
    let gap = now_ms() - last;
    if last > 0 && gap > 5 * 60_000 && uptime_ms > gap {
        let _ = core.report_tamper(now_clock(core), TamperKind::ServiceStopped);
    }
}

pub fn run(core: Arc<Core>, shared: Shared, stop: Arc<AtomicBool>, install_dir: PathBuf, data_dir: PathBuf) {
    let windows_dir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let install = install_dir.to_string_lossy().to_string();
    let ui_exe = install_dir.join(UI_EXE);
    let mut cache = SignerCache::default();
    let mut restrict_since: Option<Instant> = None;
    let (mut last_wall, mut last_mono) = (now_ms(), Instant::now());
    let mut last_hb = Instant::now() - HEARTBEAT_EVERY;
    let mut last_ui = Instant::now() - Duration::from_secs(10);
    let mut last_policy: Option<Option<Vec<String>>> = None; // fuerza la primera escritura
    if let Err(e) = browser_policy::apply_baseline() {
        tracing::warn!(error = %e, "no se pudieron aplicar las políticas base del navegador");
    }

    while !stop.load(Ordering::Relaxed) {
        let clock = now_clock(&core);

        // Salto de reloj
        let (wall, mono) = (now_ms(), Instant::now());
        let drift = (wall - last_wall) - mono.duration_since(last_mono).as_millis() as i64;
        if drift.abs() > 120_000 {
            let _ = core.report_tamper(clock, TamperKind::ClockChanged);
        }
        (last_wall, last_mono) = (wall, mono);

        // Estado (sin configurar = no se bloquea nada: el usuario aún está en el onboarding)
        let state = core.status(clock).map(|s| s.state).unwrap_or(DayState::Free);
        let permissive = matches!(state, DayState::Free | DayState::Pass { .. } | DayState::Emergency { .. });
        restrict_since = if permissive { None } else { restrict_since.or(Some(Instant::now())) };
        let grace_left = restrict_since.map(|t| GRACE.saturating_sub(t.elapsed())).unwrap_or_default();
        let cfg = core.active_config(clock);

        if let (false, Some(cfg)) = (permissive, &cfg) {
            for p in procs::user_processes() {
                let id = procs::identity(&p, &mut cache);
                if decide(&state, &id, cfg, &install, &windows_dir) == Verdict::Block && grace_left.is_zero() {
                    procs::terminate(p.pid);
                    let mut s = shared.lock().unwrap_or_else(|p| p.into_inner());
                    s.blocked_app = Some(id.file_name.clone());
                    s.blocked_at_ms = Some(clock.now_ms);
                    drop(s);
                    tracing::info!("app cerrada por política"); // sin el nombre: dato personal
                }
            }
        }
        {
            let mut s = shared.lock().unwrap_or_else(|p| p.into_inner());
            s.grace_until_ms =
                (!permissive && !grace_left.is_zero()).then(|| clock.now_ms + grace_left.as_millis() as i64);
            if permissive {
                s.blocked_app = None;
                s.blocked_at_ms = None;
            }
        }

        // Lista blanca web: solo durante ENFOQUE, y solo se reescribe cuando cambia
        let want: Option<Vec<String>> = match (&state, &cfg) {
            (DayState::Focus { .. }, Some(cfg)) => Some(cfg.web_allowlist.clone()),
            _ => None,
        };
        if last_policy.as_ref() != Some(&want) {
            if let Err(e) = browser_policy::apply_allowlist(want.as_deref()) {
                tracing::warn!(error = %e, "no se pudo aplicar la política del navegador");
            }
            last_policy = Some(want);
        }

        // La UI debe estar siempre viva (el escudo y el plan del día viven en ella)
        if last_ui.elapsed() > Duration::from_secs(5) {
            last_ui = Instant::now();
            if !procs::is_running(UI_EXE)
                && let Err(e) = session::spawn_in_active_session(&ui_exe, "--background")
            {
                tracing::warn!(error = %e, "no se pudo lanzar la UI");
            }
        }

        if last_hb.elapsed() > HEARTBEAT_EVERY {
            last_hb = Instant::now();
            let _ = core.heartbeat(clock);
        }
        let _ = std::fs::write(alive_file(&data_dir), now_ms().to_string());
        std::thread::sleep(TICK);
    }
}

/// Sincronización en segundo plano cada 60 s (los errores de red se ignoran: modo sin conexión).
/// Antes de cada sync se re-ancla la hora confiable con la hora firmada del relay.
pub fn sync_loop(core: Arc<Core>, shared: Shared, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        if let Err(e) = core.anchor_time(crate::clock::device_now_ms(), crate::clock::mono_ms()) {
            tracing::debug!(error = %e, "sin hora firmada (se usa el reloj del sistema)");
        }
        match core.sync(now_clock(&core)) {
            Ok(_) => shared.lock().unwrap_or_else(|p| p.into_inner()).synced_at_ms = Some(now_ms()),
            Err(e) => tracing::debug!(error = %e, "sync fallida (se reintenta)"),
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}
