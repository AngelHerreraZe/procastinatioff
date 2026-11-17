// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Desktop UI ↔ privileged service protocol (security-critical): one JSON line per request and one per
//! response. Used by the Windows agent (named pipe) and the macOS agent (XPC): both pass the
//! lines to `handle_json`, so the rules on what the UI may ask are the same in both.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::api::{AuthState, Core, DeviceSeen, FamilyAlertView, PendingRequest};
use crate::clock::Clock;
use crate::error::CoreError;
use crate::model::{Config, Goal, LocalFactor, Member, Method, RequestKind, Role, SecretHolder, Status};

pub const MAX_LINE: usize = 64 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    ActiveConfig,
    TodayGoals,
    Members,
    DevicesSeen,
    CreateProfile { label: String, config: Config },
    // Onboarding: the PIN, password or pattern chosen there is checked before creating the profile and
    // set right after. Setting one later needs the gate open, as on the phone (the core checks it).
    CheckSecret { factor: LocalFactor, secret: String },
    SetSecret { factor: LocalFactor, secret: String },
    SetConfig { config: Config },
    PlanDay { goals: Vec<Goal> },
    StartPass,
    StartFocus { goal_id: String },
    EndFocus,
    CompleteGoal { goal_id: String, method: Method, score: u8, minutes: u16 },
    Emergency,
    PairingOffer,
    PairingPoll,
    PairingConfirm,
    PairingCancel,
    PairingJoin { qr: String, label: String },
    // Family mode (the minor's PC; the guardian uses the phone)
    MyRole,
    FamilyRequest { what: RequestKind },
    PendingRequests,
    FamilyAlerts,
    // Settings: the local gate (PIN, password or pattern typed on this PC), the technical report and
    // "delete all my data". The core checks the gate and counts failed attempts.
    AuthState,
    AuthorizeLocal { factor: LocalFactor, secret: String },
    LockSettings,
    // `platform`: lines from the app (versions, states). SECURITY: no personal data, as in the core.
    DiagnosticReport { platform: Vec<String> },
    EraseAll,
}

/// Service status as the UI sees it: the core's plus what the platform agent knows.
#[derive(Debug, Clone, Default, Serialize, Deserialize, uniffi::Record)]
pub struct AgentStatus {
    pub core: Option<Status>,
    /// Name of the last closed app (to explain it on the shield).
    pub blocked_app: Option<String>,
    /// When it was closed (the UI dismisses the shield until the next close).
    pub blocked_at_ms: Option<i64>,
    /// Whether there is a countdown before closing apps ("save your work").
    pub grace_until_ms: Option<i64>,
    /// Instant of the last successful sync (status footer in the UI).
    pub synced_at_ms: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Status {
        agent: AgentStatus,
    },
    Config {
        config: Option<Config>,
    },
    Goals {
        goals: Vec<Goal>,
    },
    Members {
        members: Vec<Member>,
    },
    Devices {
        devices: Vec<DeviceSeen>,
    },
    Role {
        role: Option<Role>,
    },
    Requests {
        requests: Vec<PendingRequest>,
    },
    Alerts {
        alerts: Vec<FamilyAlertView>,
    },
    Auth {
        state: AuthState,
    },
    Effective {
        from_ms: i64,
    },
    Text {
        value: String,
    },
    MaybeText {
        value: Option<String>,
    },
    Ok,
    /// `code` = translation key (`denied.*`, `error.*`); `message` = English text for logs.
    Error {
        code: String,
        message: String,
        cooldown_minutes: u16,
    },
}

fn err(e: CoreError) -> Response {
    let cooldown_minutes = if let CoreError::Denied { cooldown_minutes, .. } = &e { *cooldown_minutes } else { 0 };
    Response::Error { code: e.code(), message: e.to_string(), cooldown_minutes }
}

fn ok_or<T>(r: Result<T, CoreError>, f: impl FnOnce(T) -> Response) -> Response {
    r.map(f).unwrap_or_else(err)
}

/// Translates a UI request into a core call. The core applies the rules.
/// `agent`: what the platform agent knows (closed app, countdown, last sync).
/// Sync is the agent's job (it has the monotonic clock): the UI cannot trigger it.
pub fn handle(core: &Arc<Core>, clock: Clock, mut agent: AgentStatus, req: Request) -> Response {
    match req {
        Request::Status => {
            agent.core = core.status(clock);
            Response::Status { agent }
        }
        Request::ActiveConfig => Response::Config { config: core.active_config(clock) },
        Request::TodayGoals => Response::Goals { goals: core.today_goals(clock) },
        Request::Members => Response::Members { members: core.members() },
        Request::DevicesSeen => Response::Devices { devices: core.devices_seen(clock.now_ms) },
        Request::CreateProfile { label, config } => ok_or(core.create_profile(clock, label, config), |_| Response::Ok),
        Request::CheckSecret { factor, secret } => ok_or(crate::api::check_secret(factor, secret), |_| Response::Ok),
        Request::SetSecret { factor, secret } => {
            ok_or(core.set_secret(clock, factor, secret, SecretHolder::Owner), |_| Response::Ok)
        }
        Request::SetConfig { config } => {
            ok_or(core.set_config(clock, config), |from_ms| Response::Effective { from_ms })
        }
        Request::PlanDay { goals } => ok_or(core.plan_day(clock, goals), |_| Response::Ok),
        Request::StartPass => ok_or(core.start_pass(clock), |_| Response::Ok),
        Request::StartFocus { goal_id } => ok_or(core.start_focus(clock, goal_id), |_| Response::Ok),
        Request::EndFocus => ok_or(core.end_focus(clock), |_| Response::Ok),
        Request::CompleteGoal { goal_id, method, score, minutes } => {
            ok_or(core.complete_goal(clock, goal_id, method, score, minutes), |_| Response::Ok)
        }
        Request::Emergency => ok_or(core.emergency(clock), |_| Response::Ok),
        Request::PairingOffer => ok_or(core.clone().pairing_offer(), |value| Response::Text { value }),
        Request::PairingPoll => ok_or(core.pairing_poll(), |value| Response::MaybeText { value }),
        // The desktop only invites other own devices (personal profile); the guardian pairs from the phone.
        Request::PairingConfirm => ok_or(core.pairing_confirm(clock, None), |_| Response::Ok),
        Request::PairingCancel => {
            core.pairing_cancel();
            Response::Ok
        }
        Request::PairingJoin { qr, label } => ok_or(core.pairing_join(qr, label), |value| Response::Text { value }),
        Request::MyRole => Response::Role { role: core.my_role() },
        Request::FamilyRequest { what } => ok_or(core.request(clock, what), |value| Response::Text { value }),
        Request::PendingRequests => Response::Requests { requests: core.pending_requests() },
        Request::FamilyAlerts => Response::Alerts { alerts: core.family_alerts(clock) },
        Request::AuthState => Response::Auth { state: core.auth_state(clock) },
        Request::AuthorizeLocal { factor, secret } => {
            ok_or(core.authorize_local(clock, factor, secret), |_| Response::Ok)
        }
        Request::LockSettings => {
            core.lock_settings();
            Response::Ok
        }
        Request::DiagnosticReport { platform } => {
            Response::Text { value: core.diagnostic_report(clock, platform.into_iter().take(8).collect()) }
        }
        Request::EraseAll => ok_or(core.clone().erase_all(clock), |_| Response::Ok),
    }
}

/// One JSON line from the UI → one JSON response line. Never fails: errors go in the response.
pub fn handle_json(core: &Arc<Core>, clock: Clock, agent: AgentStatus, line: &str) -> String {
    let resp = if line.len() > MAX_LINE {
        Response::Error { code: "error.too_large".into(), message: "message too large".into(), cooldown_minutes: 0 }
    } else {
        match serde_json::from_str::<Request>(line) {
            Ok(req) => handle(core, clock, agent, req),
            Err(_) => Response::Error {
                code: "error.bad_request".into(),
                message: "invalid request".into(),
                cooldown_minutes: 0,
            },
        }
    };
    serde_json::to_string(&resp).unwrap_or_else(|_| {
        r#"{"type":"error","code":"error.internal","message":"internal error","cooldown_minutes":0}"#.into()
    })
}

#[uniffi::export]
impl Core {
    /// (macOS agent, over XPC) Handles a JSON line from the desktop UI.
    pub fn desktop_call(self: Arc<Self>, clock: Clock, agent: AgentStatus, request_json: String) -> String {
        handle_json(&self, clock, agent, &request_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::testing::MemStore;
    use std::sync::Arc;

    #[test]
    fn wire_format_is_stable() {
        let r: Request = serde_json::from_str(r#"{"cmd":"start_focus","goal_id":"read-30"}"#).unwrap();
        assert!(matches!(r, Request::StartFocus { goal_id } if goal_id == "read-30"));
        let r: Request = serde_json::from_str(r#"{"cmd":"plan_day","goals":[]}"#).unwrap();
        assert!(matches!(r, Request::PlanDay { goals } if goals.is_empty()));
        assert_eq!(serde_json::to_string(&Response::Ok).unwrap(), r#"{"type":"ok"}"#);
        let r: Request =
            serde_json::from_str(r#"{"cmd":"family_request","what":{"what":"extra_permit","minutes":15}}"#).unwrap();
        assert!(matches!(r, Request::FamilyRequest { what: RequestKind::ExtraPermit { minutes: 15 } }));
        let role = serde_json::to_string(&Response::Role { role: Some(Role::Minor) }).unwrap();
        assert_eq!(role, r#"{"type":"role","role":"minor"}"#);
        let r: Request = serde_json::from_str(r#"{"cmd":"authorize_local","factor":"pin","secret":"482916"}"#).unwrap();
        assert!(matches!(r, Request::AuthorizeLocal { factor: LocalFactor::Pin, .. }));
        assert!(matches!(serde_json::from_str(r#"{"cmd":"erase_all"}"#).unwrap(), Request::EraseAll));
        // Any unknown command is rejected at deserialization (the UI cannot invent orders)
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"run","exe":"cmd.exe"}"#).is_err());
    }

    #[test]
    fn json_round_trip_through_the_core() {
        let dir = std::env::temp_dir().join(format!("pco-desktop-{}-{}", std::process::id(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let core = Core::open(dir.join("db").to_string_lossy().into(), Arc::new(MemStore::default())).unwrap();
        let clock = Clock { now_ms: 1_773_046_800_000, utc_offset_min: 0 };
        let agent = AgentStatus { blocked_app: Some("juego.exe".into()), ..Default::default() };
        let out = core.clone().desktop_call(clock, agent.clone(), r#"{"cmd":"status"}"#.into());
        assert!(out.starts_with(r#"{"type":"status","agent":{"core":null,"blocked_app":"juego.exe""#), "{out}");
        let out = core.clone().desktop_call(clock, agent.clone(), r#"{"cmd":"start_pass"}"#.into());
        assert_eq!(
            out,
            r#"{"type":"error","code":"denied.not_configured","message":"denied: not_configured","cooldown_minutes":0}"#
        );
        assert!(
            core.clone()
                .desktop_call(clock, agent.clone(), "not json".into())
                .contains(r#""code":"error.bad_request""#)
        );
        assert!(
            core.clone()
                .desktop_call(clock, agent.clone(), "x".repeat(MAX_LINE + 1))
                .contains(r#""code":"error.too_large""#)
        );
        // Settings: the gate, then "delete all my data" leaves the PC without a profile
        // Onboarding: a weak PIN is caught before the profile exists; the good one is set right after.
        let call = |line: &str| core.clone().desktop_call(clock, agent.clone(), line.into());
        assert!(call(r#"{"cmd":"check_secret","factor":"pin","secret":"123456"}"#).contains("denied.easy_pin"));
        assert_eq!(call(r#"{"cmd":"check_secret","factor":"pin","secret":"482916"}"#), r#"{"type":"ok"}"#);
        core.create_profile(clock, "PC".into(), Config::example()).unwrap();
        assert_eq!(call(r#"{"cmd":"set_secret","factor":"pin","secret":"482916"}"#), r#"{"type":"ok"}"#);
        core.lock_settings();
        // With a method set, setting another one needs the gate open
        assert!(call(r#"{"cmd":"set_secret","factor":"pin","secret":"917364"}"#).contains("denied.auth_required"));
        assert!(call(r#"{"cmd":"auth_state"}"#).contains(r#""unlocked":false"#));
        assert!(call(r#"{"cmd":"erase_all"}"#).contains(r#""code":"denied.auth_required""#));
        assert!(call(r#"{"cmd":"authorize_local","factor":"pin","secret":"000000"}"#).contains("denied.wrong_secret"));
        assert_eq!(call(r#"{"cmd":"authorize_local","factor":"pin","secret":"482916"}"#), r#"{"type":"ok"}"#);
        assert_eq!(call(r#"{"cmd":"erase_all"}"#), r#"{"type":"ok"}"#);
        assert!(call(r#"{"cmd":"status"}"#).contains(r#""core":null"#));
    }
}
