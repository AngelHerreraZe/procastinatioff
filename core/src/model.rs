// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Domain types. SECURITY: Those that travel in events are signed and synced:
//! never `HashMap`/`HashSet` (non-deterministic order) nor `f32`/`f64`.
//! Text written by the user (`title`, `label`) is personal data: never to logs.

use serde::{Deserialize, Serialize};

/// Weekday mask: bit 0 = Monday … bit 6 = Sunday.
pub const ALL_DAYS: u8 = 0b0111_1111;
pub const SATURDAY: u8 = 1 << 5;
pub const SUNDAY: u8 = 1 << 6;

/// Minutes since midnight, 0..=1439. Example: 22:30 → 1350.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, uniffi::Record)]
pub struct TimeOfDay {
    pub minutes: u16,
}

impl TimeOfDay {
    pub const fn hm(hour: u16, minute: u16) -> Self {
        Self { minutes: hour * 60 + minute }
    }
}

/// Sleep schedule. If `bedtime == wake` there is no sleep period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Schedule {
    pub wake: TimeOfDay,
    pub bedtime: TimeOfDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    /// Must be completed to reach FREE.
    Key,
    /// Only recorded; missing it may take passes away tomorrow.
    Habit,
}

/// Verification methods (see the README's goal verification section).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    FocusSession,
    QrAnchor,
    Health,
    HonorDeclaration,
    Witness,
    FoodPhoto,
    ReadingPresence,
    PageOcr,
    Summary,
    PoseReps,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Goal {
    /// Stable identifier (e.g. "read-30"): lowercase letters, digits and hyphens. Never changes.
    pub id: String,
    /// User text. SECURITY: Personal data: never to logs.
    pub title: String,
    pub kind: GoalKind,
    /// Required minutes (0 if the goal is not measured by time).
    pub target_minutes: u16,
    /// Times it must be done in the day (minimum 1). E.g. "eat healthy 2 of 3" → 2.
    pub target_count: u8,
    /// Accepted methods. For now completing one is enough; full recipes come later.
    pub methods: Vec<Method>,
    /// Days on which the goal is fixed (base of the plan): `ALL_DAYS` mask.
    pub weekdays: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Config {
    pub schedule: Schedule,
    pub permits_per_day: u8,
    pub permit_minutes: u16,
    /// Minimum minutes between the end of a pass and the start of the next one.
    pub permit_cooldown_minutes: u16,
    pub emergency_minutes: u16,
    /// Passes taken from the next day for each emergency.
    pub emergency_penalty: u8,
    /// If true, each missed habit goal takes 1 pass from the next day.
    pub habit_penalty: bool,
    /// Fixed goals: the base of each morning's plan on the marked days.
    pub goals: Vec<Goal>,
    /// Free days (same mask as `Goal::weekdays`): that day there are no goals nor blocking
    /// on any device, and no SLEEP the night before.
    pub free_days: u8,
    /// Apps always allowed (except during SLEEP), per platform. Formats:
    /// "android:<package>@<sha256 cert>", "windows:pfn:<PackageFamilyName>", "windows:exe:<signer>|<file.exe>".
    pub essential_apps: Vec<String>,
    /// Apps also allowed during FOCUS (Word, managed browser, PDF reader…).
    pub focus_apps: Vec<String>,
    /// Domains allowed during FOCUS (e.g. "udemy.com").
    pub web_allowlist: Vec<String>,
    /// Extra passes pre-approved by the guardian, to spend on any day (rolling 7-day window).
    /// SECURITY: Omitted when serializing if 0: events signed before the field existed
    /// re-serialize identically and their signature stays valid.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weekly_bonus_permits: u8,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

impl Config {
    /// Example configuration used in tests (the UIs build their own, translated, during onboarding).
    pub fn example() -> Self {
        Self {
            schedule: Schedule { wake: TimeOfDay::hm(6, 30), bedtime: TimeOfDay::hm(22, 30) },
            permits_per_day: 3,
            permit_minutes: 5,
            permit_cooldown_minutes: 60,
            emergency_minutes: 15,
            emergency_penalty: 1,
            habit_penalty: true,
            goals: vec![
                Goal {
                    id: "read-30".into(),
                    title: "Read for 30 minutes".into(),
                    kind: GoalKind::Key,
                    target_minutes: 30,
                    target_count: 1,
                    methods: vec![Method::FocusSession, Method::HonorDeclaration],
                    weekdays: ALL_DAYS,
                },
                Goal {
                    id: "eat-healthy".into(),
                    title: "Eat healthy".into(),
                    kind: GoalKind::Habit,
                    target_minutes: 0,
                    target_count: 2,
                    methods: vec![Method::HonorDeclaration],
                    weekdays: ALL_DAYS,
                },
            ],
            free_days: SATURDAY | SUNDAY,
            essential_apps: vec![
                "android:com.whatsapp".into(),
                "windows:pfn:5319275A.WhatsAppDesktop_cv1g1gvanyjgm".into(),
            ],
            focus_apps: vec![
                "windows:exe:Microsoft Corporation|winword.exe".into(),
                "windows:exe:Google LLC|chrome.exe".into(),
            ],
            web_allowlist: vec!["udemy.com".into(), "coursera.org".into()],
            weekly_bonus_permits: 0,
        }
    }
}

/// What can happen in a day. Every variant is signed and synced (see events.rs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    ConfigChanged {
        config: Config,
        effective_from_ms: i64,
    },
    /// Goals of the day, set at the first unlock after bedtime.
    /// Only the first of the day counts; later ones only if they make it stricter.
    DayPlanned {
        goals: Vec<Goal>,
    },
    GoalCompleted {
        goal_id: String,
        method: Method,
        score: u8,
        minutes: u16,
    },
    PassStarted {
        minutes: u16,
    },
    FocusStarted {
        goal_id: String,
    },
    FocusEnded {
        goal_id: String,
    },
    EmergencyStarted,
    Tamper {
        what: TamperKind,
    },
    Heartbeat,
    /// Full list of devices of the mailbox. Events are encrypted only for this list.
    MembersChanged {
        members: Vec<Member>,
    },
    /// Only in the accountability partner's mailbox: what happened and when, nothing more.
    ForPartner {
        happened_at_ms: i64,
        alert: PartnerAlert,
    },
    // ── Family mode. Who may sign each one: ledger.rs ──
    /// The minor asks the guardian for something.
    Request {
        request_id: String,
        what: RequestKind,
    },
    /// The guardian's answer. If approved, the effect goes in a separate event (also from the guardian).
    Decision {
        request_id: String,
        approved: bool,
    },
    /// Extra pass only for the day it is granted; lasts `minutes`.
    GrantPermit {
        minutes: u16,
    },
    /// Rest day: FREE until bedtime, without habit penalties.
    RestDayGranted,
    /// Number of attempts blocked by the DNS filter that day. SECURITY: Only the number; never the domains.
    BlockedAttempts {
        count: u16,
    },
}

/// What a minor can ask their guardian for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "what", rename_all = "snake_case")]
pub enum RequestKind {
    /// One more pass today, of `minutes` minutes.
    ExtraPermit { minutes: u16 },
    /// Add a domain to the focus allowlist (permanent configuration change).
    AddSite { domain: String },
    /// Move bedtime later (permanent schedule change).
    LaterBedtime { minutes: u16 },
    /// Today, a rest day.
    RestDay,
}

/// The ONLY thing the accountability partner sees: types and numbers, never goals or texts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "alert", rename_all = "snake_case")]
pub enum PartnerAlert {
    Tamper {
        what: TamperKind,
    },
    Emergency,
    /// Goals completed on the day that started at `happened_at_ms` (all of them, key and habits).
    DailySummary {
        completed: u8,
        total: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum TamperKind {
    PermissionRevoked,
    ServiceStopped,
    ClockChanged,
    UninstallAttempt,
    SafeMode,
    DeviceSilent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Profile owner (personal mode).
    Owner,
    /// Guardian (family mode).
    Guardian,
    /// Minor (family mode).
    Minor,
    /// Accountability partner (separate mailbox).
    Partner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Member {
    /// Ed25519 public key in hex = device ID.
    pub device_id: String,
    /// age recipient ("age1…") to encrypt events for it.
    pub age_recipient: String,
    /// Display name ("Study PC"). SECURITY: Personal data: never to logs.
    pub label: String,
    pub role: Role,
}

/// An already verified event (signature and author checked in ledger.rs), ready for the rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub at_ms: i64,
    pub kind: EventKind,
    /// Signed by a guardian of the profile (their configuration changes take effect immediately).
    pub by_guardian: bool,
}

impl Fact {
    pub fn new(at_ms: i64, kind: EventKind) -> Self {
        Self { at_ms, kind, by_guardian: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DayState {
    Sleep,
    Locked,
    Pass { until_ms: i64 },
    Focus { goal_id: String },
    Free,
    Emergency { until_ms: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct GoalProgress {
    pub goal_id: String,
    /// SECURITY: Personal data: shown in the UI, never to logs.
    pub title: String,
    pub kind: GoalKind,
    pub done_count: u8,
    pub target_count: u8,
    pub focus_minutes: u16,
    pub target_minutes: u16,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Status {
    pub state: DayState,
    pub permits_left: u8,
    pub day_start_ms: i64,
    pub next_wake_ms: i64,
    /// Next start of SLEEP (None if there is no sleep or tonight is free).
    pub sleep_at_ms: Option<i64>,
    /// Today is a free day: no goals nor blocking.
    pub free_day: bool,
    /// Today's goals are not set yet (the UI shows the day plan).
    pub needs_plan: bool,
    pub goals: Vec<GoalProgress>,
}
