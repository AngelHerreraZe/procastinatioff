// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Desktop UI ↔ privileged service protocol (security-critical): one JSON line per request and one per
//! response. Used by the Windows agent (named pipe) and the macOS agent (XPC): both pass the
//! lines to `handle_json`, so the rules on what the UI may ask are the same in both.

use serde::{Deserialize, Serialize};

use crate::api::{Core, DeviceSeen, FamilyAlertView, PendingRequest};
use crate::clock::Clock;
use crate::error::CoreError;
use crate::model::{Config, Goal, Member, Method, RequestKind, Role, Status};

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
    SetConfig { config: Config },
    PlanDay { goals: Vec<Goal> },
    StartPass,
    StartFocus { goal_id: String },
    EndFocus,
    CompleteGoal { goal_id: String, method: Method, score: u8, minutes: u16 },
    Emergency,
    ConnectRelay { url: String },
    Sync,
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
pub fn handle(core: &Core, clock: Clock, mut agent: AgentStatus, req: Request) -> Response {
    match req {
        Request::Status => {
            agent.core = core.status(clock);
            Response::Status { agent }
        }
        Request::ActiveConfig => Response::Config { config: core.active_config(clock) },
        Request::TodayGoals => Response::Goals { goals: core.today_goals(clock) },
        Request::Members => Response::Members { members: core.members() },
        Request::DevicesSeen => Response::Devices { devices: core.devices_seen() },
        Request::CreateProfile { label, config } => ok_or(core.create_profile(clock, label, config), |_| Response::Ok),
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
        Request::ConnectRelay { url } => ok_or(core.connect_relay(url), |_| Response::Ok),
        Request::Sync => ok_or(core.sync(clock), |_| Response::Ok),
        Request::PairingOffer => ok_or(core.pairing_offer(), |value| Response::Text { value }),
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
    }
}

/// One JSON line from the UI → one JSON response line. Never fails: errors go in the response.
pub fn handle_json(core: &Core, clock: Clock, agent: AgentStatus, line: &str) -> String {
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
    pub fn desktop_call(&self, clock: Clock, agent: AgentStatus, request_json: String) -> String {
        handle_json(self, clock, agent, &request_json)
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
        let out = core.desktop_call(clock, agent.clone(), r#"{"cmd":"status"}"#.into());
        assert!(out.starts_with(r#"{"type":"status","agent":{"core":null,"blocked_app":"juego.exe""#), "{out}");
        let out = core.desktop_call(clock, agent.clone(), r#"{"cmd":"start_pass"}"#.into());
        assert_eq!(
            out,
            r#"{"type":"error","code":"denied.not_configured","message":"denied: not_configured","cooldown_minutes":0}"#
        );
        assert!(core.desktop_call(clock, agent.clone(), "not json".into()).contains(r#""code":"error.bad_request""#));
        assert!(core.desktop_call(clock, agent, "x".repeat(MAX_LINE + 1)).contains(r#""code":"error.too_large""#));
    }
}
