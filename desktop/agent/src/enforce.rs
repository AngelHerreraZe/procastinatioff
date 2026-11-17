// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Windows enforcement loop: closes the apps the day state does not allow. The grace,
//! downtime and clock-jump rules live in `common.rs` (shared with Linux).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use procastinatioff_core::api::Core;
use procastinatioff_core::model::DayState;

use crate::clock::now_clock;
use crate::common::{Grace, Shared, TICK, Upkeep, note_blocked};
use crate::ipc_server::UI_EXE;
use crate::policy::{Verdict, decide};
use crate::signer::SignerCache;
use crate::{browser_policy, procs, session};

pub fn run(core: Arc<Core>, shared: Shared, stop: Arc<AtomicBool>, install_dir: PathBuf, data_dir: PathBuf) {
    let windows_dir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let install = install_dir.to_string_lossy().to_string();
    let ui_exe = install_dir.join(UI_EXE);
    let mut cache = SignerCache::default();
    let (mut grace, mut upkeep) = (Grace::default(), Upkeep::new());
    let mut last_ui = Instant::now() - Duration::from_secs(10);
    let mut last_policy: Option<Option<Vec<String>>> = None; // forces the first write
    if let Err(e) = browser_policy::apply_baseline() {
        tracing::warn!(error = %e, "could not apply the base browser policies");
    }

    while !stop.load(Ordering::Relaxed) {
        let clock = now_clock(&core);

        // State (not configured = nothing is blocked: the user is still onboarding)
        let state = core.status(clock).map(|s| s.state).unwrap_or(DayState::Free);
        let permissive = matches!(state, DayState::Free | DayState::Pass { .. } | DayState::Emergency { .. });
        let grace_left = grace.update(permissive, clock, &shared);
        let cfg = core.active_config(clock);

        if let (false, Some(cfg)) = (permissive, &cfg) {
            for p in procs::user_processes() {
                let id = procs::identity(&p, &mut cache);
                if decide(&state, &id, cfg, &install, &windows_dir) == Verdict::Block && grace_left.is_zero() {
                    procs::terminate(p.pid);
                    note_blocked(&shared, id.file_name.clone(), clock.now_ms);
                    tracing::info!("app closed by policy"); // no name: personal data
                }
            }
        }
        // Web allowlist: only during FOCUS, and only rewritten when it changes
        let want: Option<Vec<String>> = match &state {
            DayState::Focus { sites, .. } => Some(sites.clone()),
            _ => None,
        };
        if last_policy.as_ref() != Some(&want) {
            if let Err(e) = browser_policy::apply_allowlist(want.as_deref()) {
                tracing::warn!(error = %e, "could not apply the browser policy");
            }
            last_policy = Some(want);
        }

        // The UI must always be alive (the shield and the day plan live in it)
        if last_ui.elapsed() > Duration::from_secs(5) {
            last_ui = Instant::now();
            if !procs::is_running(UI_EXE)
                && let Err(e) = session::spawn_in_active_session(&ui_exe, "--background")
            {
                tracing::warn!(error = %e, "could not launch the UI");
            }
        }

        upkeep.tick(&core, clock, &data_dir);
        std::thread::sleep(TICK);
    }
}
