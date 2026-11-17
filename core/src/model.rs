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
    /// How changes are authorized (ADR 0013). Omitted when empty, like `weekly_bonus_permits`.
    #[serde(default, skip_serializing_if = "AuthPolicy::is_empty")]
    pub auth: AuthPolicy,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// Local methods (a gate the app applies on this device). Guardian and trusted contact are not
/// here: they are signatures every device checks (ADR 0013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum LocalFactor {
    Pin,
    Password,
    Pattern,
    Biometric,
}

/// Who knows a local secret: the profile's own user, or the guardian (typed on a minor's device).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum SecretHolder {
    Owner,
    Guardian,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, uniffi::Record)]
pub struct AuthPolicy {
    /// Local methods in use. Removing one relaxes; adding one tightens.
    pub local: Vec<LocalFactor>,
    /// Personal profile: relaxing needs a trusted contact's approval.
    pub contact_required: bool,
}

impl AuthPolicy {
    fn is_empty(&self) -> bool {
        self.local.is_empty() && !self.contact_required
    }
}

/// When a change approved by a trusted contact takes effect (the contact chooses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Effective {
    Now,
    NextWake,
}

/// Small concessions that can be authorized without network (offline_code.rs) or in person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum OfflineAsk {
    /// One more pass today (5–120 min).
    ExtraPermit {
        minutes: u16,
    },
    RestDay,
    /// Tonight only: bedtime `minutes` later (1–120).
    LaterBedtime {
        minutes: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "proof", rename_all = "snake_case")]
pub enum GrantProof {
    /// 8-digit answer computed by this approver's device (guardian or trusted contact).
    OfflineCode { approver: String },
    /// The guardian typed their own PIN/password/pattern on the minor's device.
    GuardianSecret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum AppList {
    Essential,
    Focus,
}

/// One line of a configuration change, as the trusted contact sees it. Typed, never free text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Change {
    Permits { from: u8, to: u8 },
    PermitMinutes { from: u16, to: u16 },
    Cooldown { from: u16, to: u16 },
    EmergencyMinutes { from: u16, to: u16 },
    EmergencyPenalty { from: u8, to: u8 },
    HabitPenalty { on: bool },
    Bedtime { from: TimeOfDay, to: TimeOfDay },
    Wake { from: TimeOfDay, to: TimeOfDay },
    GoalAdded { title: String },
    GoalRemoved { title: String },
    GoalChanged { title: String },
    FreeDays { from: u8, to: u8 },
    AppAdded { list: AppList, app: String },
    AppRemoved { list: AppList, app: String },
    SiteAdded { domain: String },
    SiteRemoved { domain: String },
    WeeklyBonus { from: u8, to: u8 },
    LocalMethods { from: Vec<LocalFactor>, to: Vec<LocalFactor> },
    ContactRequired { on: bool },
}

fn list_diff(
    out: &mut Vec<Change>,
    old: &[String],
    new: &[String],
    added: impl Fn(String) -> Change,
    removed: impl Fn(String) -> Change,
) {
    out.extend(new.iter().filter(|x| !old.contains(x)).cloned().map(added));
    out.extend(old.iter().filter(|x| !new.contains(x)).cloned().map(removed));
}

impl Config {
    /// What changes from `self` to `new`, line by line. The destructuring has no `..`: adding a
    /// field to `Config` does not compile until it has its `Change`.
    pub fn diff(&self, new: &Config) -> Vec<Change> {
        let Config {
            schedule,
            permits_per_day,
            permit_minutes,
            permit_cooldown_minutes,
            emergency_minutes,
            emergency_penalty,
            habit_penalty,
            goals,
            free_days,
            essential_apps,
            focus_apps,
            web_allowlist,
            weekly_bonus_permits,
            auth,
        } = new;
        let old = self;
        let mut out = Vec::new();
        macro_rules! num {
            ($field:ident, $variant:ident) => {
                if old.$field != *$field {
                    out.push(Change::$variant { from: old.$field, to: *$field });
                }
            };
        }
        num!(permits_per_day, Permits);
        num!(permit_minutes, PermitMinutes);
        num!(permit_cooldown_minutes, Cooldown);
        num!(emergency_minutes, EmergencyMinutes);
        num!(emergency_penalty, EmergencyPenalty);
        num!(free_days, FreeDays);
        num!(weekly_bonus_permits, WeeklyBonus);
        if old.habit_penalty != *habit_penalty {
            out.push(Change::HabitPenalty { on: *habit_penalty });
        }
        if old.schedule.bedtime != schedule.bedtime {
            out.push(Change::Bedtime { from: old.schedule.bedtime, to: schedule.bedtime });
        }
        if old.schedule.wake != schedule.wake {
            out.push(Change::Wake { from: old.schedule.wake, to: schedule.wake });
        }
        for g in goals {
            match old.goals.iter().find(|o| o.id == g.id) {
                None => out.push(Change::GoalAdded { title: g.title.clone() }),
                Some(o) if o != g => out.push(Change::GoalChanged { title: g.title.clone() }),
                Some(_) => {}
            }
        }
        for o in old.goals.iter().filter(|o| !goals.iter().any(|g| g.id == o.id)) {
            out.push(Change::GoalRemoved { title: o.title.clone() });
        }
        list_diff(
            &mut out,
            &old.essential_apps,
            essential_apps,
            |app| Change::AppAdded { list: AppList::Essential, app },
            |app| Change::AppRemoved { list: AppList::Essential, app },
        );
        list_diff(
            &mut out,
            &old.focus_apps,
            focus_apps,
            |app| Change::AppAdded { list: AppList::Focus, app },
            |app| Change::AppRemoved { list: AppList::Focus, app },
        );
        list_diff(
            &mut out,
            &old.web_allowlist,
            web_allowlist,
            |domain| Change::SiteAdded { domain },
            |domain| Change::SiteRemoved { domain },
        );
        if old.auth.local != auth.local {
            out.push(Change::LocalMethods { from: old.auth.local.clone(), to: auth.local.clone() });
        }
        if old.auth.contact_required != auth.contact_required {
            out.push(Change::ContactRequired { on: auth.contact_required });
        }
        out
    }

    /// Fingerprint of this exact configuration (what a trusted contact approves).
    pub fn hash(&self) -> String {
        use sha2::{Digest, Sha256};
        // Config has no maps nor floats: its JSON is the same on every device.
        hex::encode(Sha256::digest(serde_json::to_vec(self).unwrap_or_default()))
    }
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
            auth: AuthPolicy::default(),
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
    /// Only in the trusted contact's channel: what happened and when, nothing more.
    ForContact {
        happened_at_ms: i64,
        alert: ContactAlert,
    },
    // ── Family mode and trusted contact. Who may sign each one: ledger.rs ──
    /// The minor asks the guardian, or the adult asks their trusted contact (in the channel).
    Request {
        request_id: String,
        what: RequestKind,
    },
    /// The guardian's or contact's answer. If approved, the effect goes in a separate event.
    /// A contact's answer to a `ConfigChange` also carries the hash it approves and when it applies.
    /// SECURITY: Both omitted when empty: guardian decisions keep their old serialization.
    Decision {
        request_id: String,
        approved: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        config_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effective: Option<Effective>,
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
    /// Profile key (32 random bytes, hex) for the rotating LAN discovery names. Recorded once by
    /// whoever creates the profile; only members receive it (events travel sealed for them).
    DiscoveryKey {
        key: String,
    },
    // ── Change authorization (ADR 0013) ──
    /// Argon2id hash (PHC string, with salt and parameters) of a PIN, password or pattern.
    SecretSet {
        factor: LocalFactor,
        holder: SecretHolder,
        phc: String,
    },
    /// Hash of the adult's recovery code for their local methods.
    RecoveryCodeSet {
        phc: String,
    },
    /// The recovery code was used: 72 h later the owner's secrets stop counting.
    LocalResetStarted,
    /// The trusted contact approved a change: their signed `Decision` (JSON of the `SignedEvent`),
    /// which every device verifies. The `ConfigChanged` it approves comes right after.
    ContactApproval {
        decision: String,
    },
    /// Small concession authorized with an offline code or the guardian's secret in person.
    LocalGrant {
        what: OfflineAsk,
        request_code: String,
        proof: GrantProof,
    },
}

/// Something the platform shows as a local notification. SECURITY: The lock-screen text is generic;
/// the details are only shown inside the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "notice", rename_all = "snake_case")]
pub enum NoticeKind {
    /// (Guardian) A minor asked for something.
    RequestReceived,
    /// (Minor) The guardian answered.
    RequestAnswered {
        approved: bool,
    },
    TamperAlert,
    EmergencyAlert,
    /// Another device of the profile has a long gap in its activity or has not been seen in days.
    DeviceSilent,
    /// (Trusted contact) The person used their recovery code: their local methods reset in 72 h.
    LocalResetStarted,
    /// (Trusted contact) The person turned off "changes need my contact": it applies in 7 days.
    ContactExitScheduled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Notice {
    pub id: String,
    pub kind: NoticeKind,
    pub at_ms: i64,
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
    /// (Adult → trusted contact) A configuration change: the typed lines and the exact hash.
    ConfigChange { changes: Vec<Change>, config_hash: String },
}

/// The ONLY thing the trusted contact sees besides requested changes: types and numbers, never
/// goals or texts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "alert", rename_all = "snake_case")]
pub enum ContactAlert {
    Tamper {
        what: TamperKind,
    },
    Emergency,
    /// Goals completed on the day that started at `happened_at_ms` (all of them, key and habits).
    DailySummary {
        completed: u8,
        total: u8,
    },
    /// A small concession was granted with an offline code.
    CodeGrant,
    /// The recovery code was used.
    LocalReset,
    /// "Changes need my contact" was turned off without their approval (applies in 7 days).
    ExitScheduled,
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
    /// An adult's trusted contact. A full member only of the contact channel; in the profile it is
    /// listed just so every device can check its signature (never a recipient nor a sync peer).
    TrustedContact,
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
    /// Network key (hex) accepted from this device when it connects. SECURITY: Omitted when empty:
    /// members signed before peer sync re-serialize identically and their signature stays valid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[uniffi(default = "")]
    pub node_id: String,
    /// Its clock cannot be changed by the user (desktop service, minor's phone with Device Owner):
    /// its time wins when anchoring (time.rs).
    #[serde(default, skip_serializing_if = "is_false")]
    #[uniffi(default = false)]
    pub time_source: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_lists_each_change_as_a_typed_line() {
        let old = Config::example();
        assert!(old.diff(&old).is_empty());
        let mut new = old.clone();
        new.permits_per_day = 5;
        new.schedule.bedtime = TimeOfDay::hm(0, 30);
        new.goals.retain(|g| g.id != "read-30");
        new.web_allowlist.push("khanacademy.org".into());
        new.auth.local.push(LocalFactor::Pin);
        assert_eq!(
            old.diff(&new),
            vec![
                Change::Permits { from: 3, to: 5 },
                Change::Bedtime { from: TimeOfDay::hm(22, 30), to: TimeOfDay::hm(0, 30) },
                Change::GoalRemoved { title: "Read for 30 minutes".into() },
                Change::SiteAdded { domain: "khanacademy.org".into() },
                Change::LocalMethods { from: vec![], to: vec![LocalFactor::Pin] },
            ]
        );
    }

    #[test]
    fn hash_changes_with_any_field_and_old_events_keep_their_bytes() {
        let a = Config::example();
        let mut b = a.clone();
        b.auth.contact_required = true;
        assert_ne!(a.hash(), b.hash());
        // An empty policy is not serialized: configurations signed before it existed verify.
        assert!(!serde_json::to_string(&a).unwrap().contains("auth"));
    }
}
