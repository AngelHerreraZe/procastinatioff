// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The `Core` object: the only thing the platforms see (Kotlin, Swift, desktop agent).
//!
//! - A `Mutex` protects the state. It is never held during a network call.
//! - Every action returns the new `Status` so the UI updates without another call.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};

use rand::{RngCore, rngs::OsRng};

use crate::clock::{Clock, DAY_MS, MINUTE_MS};
use crate::crypto::{DeviceKeys, SecretStore, db_key};
use crate::error::CoreError;
use crate::events::{EventBody, SignedEvent, open, seal};
use crate::ledger::{Accepted, Ledger};
use crate::model::{
    Config, DayState, EventKind, Fact, Goal, Member, Method, PartnerAlert, RequestKind, Role, Status, TamperKind,
};
use crate::netpolicy::{self, CategorySet, DnsStep, FilterMode};
use crate::pairing::{self, JoinRequest, MAX_LABEL_CHARS, PairingOffer, PendingPairing};
use crate::rules::{self, ConfigHistory, Denied};
use crate::strictness::effective_from;
use crate::sync::Relay;
use crate::time::{TimeAnchor, corrected_now, verify_signed_time};

const META_RELAY: &str = "relay_url";
const META_MAILBOX: &str = "mailbox";
const META_CURSOR: &str = "cursor";
const META_GENESIS: &str = "genesis_author";
const META_TIME_KEY: &str = "time_key";
const META_KIND: &str = "kind";
const KIND_PARTNER: &str = "partner";
const RETENTION_DAYS: i64 = 90;
/// Days back that are checked when forwarding alerts to the partner.
const FORWARD_WINDOW_DAYS: i64 = 7;
/// Maximum difference between the device clock and the signed time before it is recorded.
const MAX_CLOCK_SKEW_MS: i64 = 5 * MINUTE_MS;
/// Unanswered requests a minor may have at once (each one notifies the guardian).
const MAX_OPEN_REQUESTS: usize = 5;
/// Days back covered by the mirror panel alerts.
const ALERT_WINDOW_DAYS: i64 = 7;
/// How often the DNS filter mode is recomputed (not on every query).
const DNS_MODE_TTL_MS: i64 = 5_000;

/// Public key (minisign, base64) that signs the category lists. It is generated on an
/// air-gapped machine; until then no list is accepted.
pub const PINNED_LISTS_PUBKEY: &str = "REEMPLAZAR_CON_CLAVE_PUBLICA_MINISIGN";
/// Where the list and its signature are published (`categories.txt` and `categories.txt.minisig`).
/// Set together with the key at release time (lists will ship inside the app; see the roadmap).
pub const LISTS_URL: &str = "https://releases.REEMPLAZAR_DOMINIO/lists/";
/// Maximum size of the downloaded list (≈ 200,000 domains).
const MAX_LIST_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncReport {
    pub uploaded: u32,
    pub received: u32,
    pub rejected: u32,
    pub gaps: u32,
}

/// Last moment each device was seen (cross heartbeats).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct DeviceSeen {
    pub device_id: String,
    /// SECURITY: Personal data: only for display in the UI.
    pub label: String,
    pub last_seen_ms: i64,
    pub is_me: bool,
}

/// A minor's request awaiting an answer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct PendingRequest {
    pub request_id: String,
    pub what: RequestKind,
    pub at_ms: i64,
}

/// What the guardian and the minor both see in the mirror panel (besides the day state).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
#[serde(tag = "alert", rename_all = "snake_case")]
pub enum FamilyAlert {
    Tamper {
        what: TamperKind,
    },
    Emergency,
    /// Distinct sites blocked per category in a day. SECURITY: Only the number.
    BlockedSites {
        count: u16,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct FamilyAlertView {
    pub at_ms: i64,
    pub alert: FamilyAlert,
    /// SECURITY: Personal data: only for display in the UI.
    pub device_label: String,
}

/// What to do with a DNS query (see netpolicy.rs).
#[derive(Debug, uniffi::Enum)]
pub enum DnsAction {
    /// Send this packet back to the client.
    Reply { packet: Vec<u8> },
    /// Send `packet` to the real DNS and pass its answer to `dns_finish`.
    Upstream { packet: Vec<u8>, rewritten: bool },
    /// Invalid packet: do not answer.
    Drop,
}

/// Alert as the partner sees it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PartnerAlertView {
    pub happened_at_ms: i64,
    pub alert: PartnerAlert,
    pub device_label: String,
}

struct Inner {
    db: crate::storage::Db,
    keys: DeviceKeys,
    ledger: Ledger,
    pairing: Option<PendingPairing>,
    /// Trusted time anchor (in memory; renewed on every sync).
    anchor: Option<TimeAnchor>,
    /// Whether the system clock was already off at the last anchoring (to record it only once).
    clock_skewed: bool,
    /// List of forbidden categories (signed; in memory only).
    categories: CategorySet,
    /// Recently computed DNS filter mode: (instant, mode, start of day).
    dns_mode: Option<(i64, FilterMode, i64)>,
    /// Sites blocked per category on the day starting at `.0`. SECURITY: Memory only; the number is recorded.
    /// ponytail: if the process dies the count for that day is lost; it is only an indicative number.
    blocked: (i64, HashSet<String>),
}

impl Inner {
    fn state(&self, clock: Clock) -> (ConfigHistory, Vec<Fact>) {
        // ponytail: recomputes from all events on every call (O(n), n ≈ thousands).
        // If it ever gets heavy, cache per day.
        let facts = self.ledger.facts();
        (ConfigHistory::from_facts_checked(&facts, self.ledger.is_family(), clock.utc_offset_min), facts)
    }

    fn my_role(&self) -> Option<Role> {
        let me = self.keys.device_id();
        self.ledger.members().iter().find(|m| m.device_id == me).map(|m| m.role)
    }

    /// Unanswered requests, oldest first.
    fn pending_requests(&self) -> Vec<PendingRequest> {
        let facts = self.ledger.facts();
        let decided: HashSet<&str> = facts
            .iter()
            .filter_map(|f| match &f.kind {
                EventKind::Decision { request_id, .. } => Some(request_id.as_str()),
                _ => None,
            })
            .collect();
        facts
            .iter()
            .filter_map(|f| match &f.kind {
                EventKind::Request { request_id, what } if !decided.contains(request_id.as_str()) => {
                    Some(PendingRequest { request_id: request_id.clone(), what: what.clone(), at_ms: f.at_ms })
                }
                _ => None,
            })
            .collect()
    }

    /// DNS filter mode for the day state. Only filters on a minor's devices.
    fn filter_mode(&self, clock: Clock) -> (FilterMode, i64) {
        if self.my_role() != Some(Role::Minor) {
            return (FilterMode::Off, 0);
        }
        let (h, f) = self.state(clock);
        let Some(st) = rules::status(&h, &f, clock) else { return (FilterMode::Categories, 0) };
        // The profile's relay always resolves (without it the guardian's approvals would not arrive)
        let relay = self.db.meta(META_RELAY).ok().flatten().and_then(|u| relay_host(&u));
        let mode = match st.state {
            DayState::Focus { .. } => {
                let mut list = h.active_at(clock.now_ms).map(|c| c.web_allowlist.clone()).unwrap_or_default();
                list.extend(relay);
                FilterMode::Allowlist(list)
            }
            DayState::Sleep | DayState::Locked => FilterMode::Allowlist(relay.into_iter().collect()),
            DayState::Pass { .. } | DayState::Free | DayState::Emergency { .. } => FilterMode::Categories,
        };
        (mode, st.day_start_ms)
    }

    /// Signs, validates and stores an event of our own.
    fn record(&mut self, at_ms: i64, kind: EventKind) -> Result<(), CoreError> {
        let me = self.keys.device_id();
        let (seq, prev) = match self.ledger.head(&me) {
            Some((s, id)) => (s + 1, Some(id.clone())),
            None => (1, None),
        };
        // A new event's time is never earlier than the last known event (from any author).
        let last_at = self.ledger.last_at();
        // SECURITY: Clock moved back > 5 min: recorded as tampering and the old time is not accepted.
        if last_at - at_ms > 5 * MINUTE_MS && !matches!(kind, EventKind::Tamper { .. }) {
            self.record(last_at, EventKind::Tamper { what: TamperKind::ClockChanged })?;
            return self.record(last_at, kind);
        }
        let ev = SignedEvent::sign(&self.keys, EventBody { author: me, seq, prev, at_ms: at_ms.max(last_at), kind });
        self.ledger.accept(ev.clone())?;
        self.db.insert(&ev, false)
    }

    fn status(&self, clock: Clock) -> Option<Status> {
        let (h, f) = self.state(clock);
        rules::status(&h, &f, clock)
    }

    fn must_status(&self, clock: Clock) -> Result<Status, CoreError> {
        self.status(clock).ok_or_else(|| CoreError::protocol("no configuration"))
    }

    fn me_as_member(&self, label: String, role: Role) -> Member {
        Member { device_id: self.keys.device_id(), age_recipient: self.keys.age_recipient(), label, role }
    }
}

#[derive(uniffi::Object)]
pub struct Core {
    inner: Mutex<Inner>,
}

impl Core {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // If another thread panicked, we continue with the state (it is consistent: every
        // write is atomic in SQLite) instead of propagating the panic to the app.
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn is_urgent(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Tamper { .. }
            | EventKind::EmergencyStarted
            | EventKind::Request { .. }
            | EventKind::Decision { .. }
            | EventKind::ForPartner { alert: PartnerAlert::Tamper { .. } | PartnerAlert::Emergency, .. }
    )
}

/// Host of the relay URL ("https://relay.x.com:8443/a" → "relay.x.com").
fn relay_host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', ':']).next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Domain acceptable for the allowlist: letters, digits and hyphens, with at least one dot.
fn valid_domain(d: &str) -> bool {
    let labels: Vec<&str> = d.split('.').collect();
    d.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| {
            (1..=63).contains(&l.len())
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        && labels.last().is_some_and(|tld| tld.bytes().any(|b| b.is_ascii_lowercase()))
}

fn check_label(label: &str) -> Result<(), CoreError> {
    if label.chars().count() > MAX_LABEL_CHARS {
        return Err(CoreError::protocol("device name too long"));
    }
    Ok(())
}

impl Core {
    fn create(&self, clock: Clock, label: String, config: Config, role: Role) -> Result<Status, CoreError> {
        check_label(&label)?;
        let mut g = self.lock();
        if !g.ledger.members().is_empty() {
            return Err(CoreError::protocol("the profile already exists"));
        }
        let me = g.keys.device_id();
        g.db.set_meta(META_GENESIS, &me)?;
        g.ledger.set_genesis_author(me);
        let member = g.me_as_member(label, role);
        g.record(clock.now_ms, EventKind::MembersChanged { members: vec![member] })?;
        g.record(clock.now_ms, EventKind::ConfigChanged { config, effective_from_ms: clock.now_ms })?;
        g.must_status(clock)
    }
}

#[uniffi::export]
impl Core {
    /// Opens the core. `db_path`: the app's private path. `secrets`: the platform keystore.
    #[uniffi::constructor]
    pub fn open(db_path: String, secrets: Arc<dyn SecretStore>) -> Result<Arc<Self>, CoreError> {
        let keys = DeviceKeys::load_or_create(secrets.as_ref())?;
        let db = crate::storage::Db::open(&db_path, &db_key(secrets.as_ref())?)?;
        let mut ledger = Ledger::new(db.meta(META_GENESIS)?);
        for ev in db.all()? {
            ledger.accept(ev)?; // re-validates signatures and chain on load
        }
        Ok(Arc::new(Self {
            inner: Mutex::new(Inner {
                db,
                keys,
                ledger,
                pairing: None,
                anchor: None,
                clock_skewed: false,
                categories: CategorySet::default(),
                dns_mode: None,
                blocked: (0, HashSet::new()),
            }),
        }))
    }

    pub fn device_id(&self) -> String {
        self.lock().keys.device_id()
    }

    pub fn members(&self) -> Vec<Member> {
        self.lock().ledger.members().to_vec()
    }

    pub fn status(&self, clock: Clock) -> Option<Status> {
        self.lock().status(clock)
    }

    pub fn active_config(&self, clock: Clock) -> Option<Config> {
        let (h, _) = self.lock().state(clock);
        h.active_at(clock.now_ms).cloned()
    }

    /// Today's goals with their full definition (to prefill the day plan).
    pub fn today_goals(&self, clock: Clock) -> Vec<Goal> {
        let (h, f) = self.lock().state(clock);
        rules::today_goals(&h, &f, clock)
    }

    /// Creates the profile on this device (no network). First time only.
    pub fn create_profile(&self, clock: Clock, label: String, config: Config) -> Result<Status, CoreError> {
        self.create(clock, label, config, Role::Owner)
    }

    /// (Guardian) Creates a minor's profile in THIS `Core` (one database per minor).
    /// From here on, everything that relaxes rules carries a guardian's signature.
    pub fn create_family_profile(
        &self,
        clock: Clock,
        guardian_label: String,
        config: Config,
    ) -> Result<Status, CoreError> {
        self.create(clock, guardian_label, config, Role::Guardian)
    }

    /// Proposes a new configuration. Returns when it takes effect (now or at the next wake-up).
    /// In a family profile: the guardian changes it immediately; the minor can only make it stricter.
    pub fn set_config(&self, clock: Clock, config: Config) -> Result<i64, CoreError> {
        let mut g = self.lock();
        let (h, _) = g.state(clock);
        let from = match g.my_role() {
            Some(Role::Guardian) => clock.now_ms,
            Some(Role::Minor) if !h.minor_may_set(&config, clock.now_ms) => return Err(Denied::GuardianOnly.into()),
            Some(Role::Minor) => clock.now_ms,
            _ => effective_from(&config, h.active_at(clock.now_ms), clock),
        };
        g.record(clock.now_ms, EventKind::ConfigChanged { config, effective_from_ms: from })?;
        Ok(from)
    }

    /// Sets (or tightens) today's goals. Asked at the first unlock after bedtime.
    pub fn plan_day(&self, clock: Clock, goals: Vec<Goal>) -> Result<Status, CoreError> {
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        let ev = rules::cmd_plan_day(&h, &f, clock, goals)?;
        g.record(clock.now_ms, ev)?;
        g.must_status(clock)
    }

    pub fn start_pass(&self, clock: Clock) -> Result<Status, CoreError> {
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        let ev = rules::cmd_start_pass(&h, &f, clock)?;
        g.record(clock.now_ms, ev)?;
        g.must_status(clock)
    }

    pub fn start_focus(&self, clock: Clock, goal_id: String) -> Result<Status, CoreError> {
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        let ev = rules::cmd_start_focus(&h, &f, clock, &goal_id)?;
        g.record(clock.now_ms, ev)?;
        g.must_status(clock)
    }

    pub fn end_focus(&self, clock: Clock) -> Result<Status, CoreError> {
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        for ev in rules::cmd_end_focus(&h, &f, clock)? {
            g.record(clock.now_ms, ev)?;
        }
        g.must_status(clock)
    }

    pub fn complete_goal(
        &self,
        clock: Clock,
        goal_id: String,
        method: Method,
        score: u8,
        minutes: u16,
    ) -> Result<Status, CoreError> {
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        let ev = rules::cmd_complete_goal(&h, &f, clock, &goal_id, method, score, minutes)?;
        g.record(clock.now_ms, ev)?;
        g.must_status(clock)
    }

    pub fn emergency(&self, clock: Clock) -> Result<Status, CoreError> {
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        let ev = rules::cmd_emergency(&h, &f, clock)?;
        g.record(clock.now_ms, ev)?;
        g.must_status(clock)
    }

    pub fn report_tamper(&self, clock: Clock, what: TamperKind) -> Result<(), CoreError> {
        self.lock().record(clock.now_ms, EventKind::Tamper { what })
    }

    pub fn heartbeat(&self, clock: Clock) -> Result<(), CoreError> {
        self.lock().record(clock.now_ms, EventKind::Heartbeat)
    }

    // ───────────────────────── Relay and sync ─────────────────────────

    /// Creates the mailbox on the relay (once, from the device that created the profile).
    pub fn connect_relay(&self, relay_url: String) -> Result<(), CoreError> {
        let key = self.lock().keys.signing.clone();
        let relay = Relay::new(&relay_url)?;
        let mailbox = relay.create_mailbox(&key)?; // network, without the lock
        let g = self.lock();
        g.db.set_meta(META_RELAY, &relay_url)?;
        g.db.set_meta(META_MAILBOX, &mailbox)?;
        g.db.set_meta(META_CURSOR, "0")
    }

    pub fn sync(&self, clock: Clock) -> Result<SyncReport, CoreError> {
        // 1) Copy what is needed while holding the lock
        let (relay_url, mailbox, key, identity, pending, recipients, cursor) = {
            let g = self.lock();
            let relay_url = g.db.meta(META_RELAY)?.ok_or_else(|| CoreError::protocol("no relay configured"))?;
            let mailbox = g.db.meta(META_MAILBOX)?.ok_or_else(|| CoreError::protocol("no mailbox"))?;
            let cursor: u64 = g.db.meta(META_CURSOR)?.and_then(|c| c.parse().ok()).unwrap_or(0);
            let recipients = g
                .ledger
                .members()
                .iter()
                .map(|m| m.age_recipient.parse::<age::x25519::Recipient>().map_err(CoreError::protocol))
                .collect::<Result<Vec<_>, _>>()?;
            (relay_url, mailbox, g.keys.signing.clone(), g.keys.age.clone(), g.db.pending_all()?, recipients, cursor)
        };

        // 2) Network, without the lock
        let relay = Relay::new(&relay_url)?;
        let mut uploaded = Vec::new();
        for ev in &pending {
            let blob = seal(ev, &recipients)?;
            relay.post_event(&key, &mailbox, &blob, is_urgent(&ev.body.kind))?;
            uploaded.push(ev.id());
        }
        let mut blobs = Vec::new();
        let mut after = cursor;
        loop {
            let page = relay.events(&key, &mailbox, after)?;
            // A relay that does not advance the cursor cannot trap us in an infinite loop.
            if page.events.is_empty() || page.next_after <= after {
                break;
            }
            blobs.extend(page.events.into_iter().map(|e| e.blob));
            after = page.next_after;
        }

        // 3) Apply while holding the lock
        let mut g = self.lock();
        for id in &uploaded {
            g.db.mark_uploaded(id)?;
        }
        let mut report = SyncReport { uploaded: uploaded.len() as u32, received: 0, rejected: 0, gaps: 0 };
        let gaps_before = g.ledger.gaps.len();
        for b64 in blobs {
            let Ok(blob) = Relay::decode_blob(&b64) else {
                report.rejected += 1;
                continue;
            };
            // Events encrypted before this device was a member cannot be opened; that is normal.
            let Ok(ev) = open(&blob, &identity) else { continue };
            match g.ledger.accept(ev.clone()) {
                Ok(Accepted::New) => {
                    g.db.insert(&ev, true)?;
                    report.received += 1;
                }
                Ok(Accepted::Duplicate) => {}
                Err(_) => report.rejected += 1,
            }
        }
        report.gaps = (g.ledger.gaps.len() - gaps_before) as u32;
        g.db.set_meta(META_CURSOR, &after.to_string())?;
        g.db.prune_before(clock.now_ms - RETENTION_DAYS * DAY_MS)?;
        Ok(report)
    }

    /// Relay URL of this profile (the partner mailbox connects to the same one).
    pub fn relay_url(&self) -> Option<String> {
        self.lock().db.meta(META_RELAY).ok().flatten()
    }

    pub fn register_push(&self, platform: String, token: String) -> Result<(), CoreError> {
        let (url, key) = {
            let g = self.lock();
            (g.db.meta(META_RELAY)?.ok_or_else(|| CoreError::protocol("no relay configured"))?, g.keys.signing.clone())
        };
        Relay::new(&url)?.register_push(&key, &platform, &token)
    }

    // ───────────────────────── Pairing ─────────────────────────

    /// (A) Generates the QR content. Valid for 10 minutes (the relay expires it).
    pub fn pairing_offer(&self) -> Result<String, CoreError> {
        let mut g = self.lock();
        let relay = g.db.meta(META_RELAY)?.ok_or_else(|| CoreError::protocol("no relay configured"))?;
        let mailbox = g.db.meta(META_MAILBOX)?.ok_or_else(|| CoreError::protocol("no mailbox"))?;
        let mut t = [0u8; 16];
        OsRng.fill_bytes(&mut t);
        let eph = age::x25519::Identity::generate();
        // The joiner must trust the genesis author, even if another device of the profile invites it.
        let inviter = g.db.meta(META_GENESIS)?.unwrap_or_else(|| g.keys.device_id());
        let offer =
            PairingOffer { v: 1, relay, mailbox, token: hex::encode(t), eph: eph.to_public().to_string(), inviter };
        let qr = serde_json::to_string(&offer).map_err(CoreError::protocol)?;
        g.pairing = Some(PendingPairing { offer, eph, joiner: None });
        Ok(qr)
    }

    /// (B) Processes the scanned QR. Returns the 6-digit code to compare.
    pub fn pairing_join(&self, qr: String, label: String) -> Result<String, CoreError> {
        check_label(&label)?;
        let offer = pairing::parse_offer(&qr)?;
        let req = {
            let mut g = self.lock();
            if !g.ledger.members().is_empty() {
                return Err(CoreError::protocol("this device already belongs to a profile"));
            }
            g.db.set_meta(META_RELAY, &offer.relay)?;
            g.db.set_meta(META_MAILBOX, &offer.mailbox)?;
            g.db.set_meta(META_CURSOR, "0")?;
            g.db.set_meta(META_GENESIS, &offer.inviter)?;
            g.ledger.set_genesis_author(offer.inviter.clone());
            JoinRequest { device_id: g.keys.device_id(), age_recipient: g.keys.age_recipient(), label }
        };
        Relay::new(&offer.relay)?.put_pairing(&offer.token, &pairing::seal_join(&req, &offer.eph)?)?;
        Ok(pairing::sas(&offer, &req))
    }

    /// (A) Checks whether B has answered. Returns the 6-digit code when it arrives.
    pub fn pairing_poll(&self) -> Result<Option<String>, CoreError> {
        let (relay, token) = {
            let g = self.lock();
            let p = g.pairing.as_ref().ok_or_else(|| CoreError::protocol("no pairing in progress"))?;
            (p.offer.relay.clone(), p.offer.token.clone())
        };
        let Some(blob) = Relay::new(&relay)?.get_pairing(&token)? else { return Ok(None) };
        let mut g = self.lock();
        let p = g.pairing.as_mut().ok_or_else(|| CoreError::protocol("no pairing in progress"))?;
        let req = pairing::open_join(&blob, &p.eph)?;
        let code = pairing::sas(&p.offer, &req);
        p.joiner = Some(req);
        Ok(Some(code))
    }

    /// (A) The user confirmed that the 6 digits match. `role`: in a family profile, `Minor`
    /// (the minor's device) or `Guardian` (another guardian); otherwise `None`.
    pub fn pairing_confirm(&self, clock: Clock, role: Option<Role>) -> Result<(), CoreError> {
        let (relay, mailbox, key, joiner) = {
            let mut g = self.lock();
            // In the partner mailbox the joiner is the partner; in a personal profile, another own device.
            let partner_box = g.db.meta(META_KIND)?.as_deref() == Some(KIND_PARTNER);
            let role = match (partner_box, g.ledger.is_family(), role) {
                (true, _, None | Some(Role::Partner)) => Role::Partner,
                (false, true, Some(r @ (Role::Guardian | Role::Minor))) => r,
                (false, false, None | Some(Role::Owner)) => Role::Owner,
                _ => return Err(CoreError::protocol("invalid role for this profile")),
            };
            let p = g.pairing.take().ok_or_else(|| CoreError::protocol("no pairing in progress"))?;
            let joiner = p.joiner.ok_or_else(|| CoreError::protocol("the other device has not answered yet"))?;
            let mut members = g.ledger.members().to_vec();
            members.retain(|m| m.device_id != joiner.device_id);
            members.push(Member {
                device_id: joiner.device_id.clone(),
                age_recipient: joiner.age_recipient.clone(),
                label: joiner.label.clone(),
                role,
            });
            g.record(clock.now_ms, EventKind::MembersChanged { members })?;
            g.db.mark_all_for_reupload()?; // the history is re-encrypted including the new device
            (p.offer.relay, p.offer.mailbox, g.keys.signing.clone(), joiner)
        };
        Relay::new(&relay)?.add_member(&key, &mailbox, &joiner.device_id)?;
        self.sync(clock).map(|_| ())
    }

    /// (A) Cancels the pairing in progress (the digits do not match).
    pub fn pairing_cancel(&self) {
        self.lock().pairing = None;
    }

    // ───────────────────────── Trusted time ─────────────────────────

    /// Asks the relay for the signed time and anchors the clock. Call at startup and on every sync.
    /// `mono_ms`: SystemClock.elapsedRealtime() on Android, GetTickCount64() on Windows.
    pub fn anchor_time(&self, device_now_ms: i64, mono_ms: i64) -> Result<(), CoreError> {
        let (url, pinned) = {
            let g = self.lock();
            (
                g.db.meta(META_RELAY)?.ok_or_else(|| CoreError::protocol("no relay configured"))?,
                g.db.meta(META_TIME_KEY)?,
            )
        };
        let t = Relay::new(&url)?.time()?; // network, without the lock
        let key = pinned.clone().unwrap_or_else(|| t.key.clone()); // TOFU: the first key gets pinned
        if t.key != key || !verify_signed_time(&t, &key) {
            return Err(CoreError::protocol("invalid signed time or a different key"));
        }
        let mut g = self.lock();
        if pinned.is_none() {
            g.db.set_meta(META_TIME_KEY, &key)?;
        }
        g.anchor = Some(TimeAnchor { trusted_ms: t.unix_ms, mono_ms });
        let skewed = (device_now_ms - t.unix_ms).abs() > MAX_CLOCK_SKEW_MS;
        if skewed && !g.clock_skewed && !g.ledger.members().is_empty() {
            g.record(t.unix_ms, EventKind::Tamper { what: TamperKind::ClockChanged })?;
        }
        g.clock_skewed = skewed;
        Ok(())
    }

    /// Time the platforms must use to build `Clock`.
    pub fn trusted_now_ms(&self, device_now_ms: i64, mono_ms: i64) -> i64 {
        corrected_now(self.lock().anchor, device_now_ms, mono_ms)
    }

    // ───────────────────────── Heartbeats ─────────────────────────

    /// When each device of the profile was last seen.
    pub fn devices_seen(&self) -> Vec<DeviceSeen> {
        let g = self.lock();
        let seen = g.ledger.last_seen();
        let me = g.keys.device_id();
        g.ledger
            .members()
            .iter()
            .map(|m| DeviceSeen {
                device_id: m.device_id.clone(),
                label: m.label.clone(),
                last_seen_ms: seen.get(&m.device_id).copied().unwrap_or(0),
                is_me: m.device_id == me,
            })
            .collect()
    }

    // ───────────────────────── Family mode ─────────────────────────

    /// Role of THIS device in the profile (None if it does not belong to one yet).
    pub fn my_role(&self) -> Option<Role> {
        self.lock().my_role()
    }

    /// (Minor) Asks the guardian for something. Returns the request id.
    pub fn request(&self, clock: Clock, what: RequestKind) -> Result<String, CoreError> {
        let what = match what {
            RequestKind::ExtraPermit { minutes } if (1..=120).contains(&minutes) => what,
            RequestKind::LaterBedtime { minutes } if (5..=180).contains(&minutes) => what,
            RequestKind::AddSite { domain } => {
                let domain = netpolicy::normalize(&domain);
                if !valid_domain(&domain) {
                    return Err(Denied::InvalidDomain.into());
                }
                RequestKind::AddSite { domain }
            }
            RequestKind::RestDay => what,
            _ => return Err(CoreError::protocol("request out of range")),
        };
        let mut g = self.lock();
        if g.my_role() != Some(Role::Minor) {
            return Err(CoreError::protocol("only the minor makes requests"));
        }
        if g.pending_requests().len() >= MAX_OPEN_REQUESTS {
            return Err(Denied::TooManyRequests.into());
        }
        let mut id = [0u8; 8];
        OsRng.fill_bytes(&mut id);
        let request_id = hex::encode(id);
        g.record(clock.now_ms, EventKind::Request { request_id: request_id.clone(), what })?;
        Ok(request_id)
    }

    /// Unanswered requests (seen by the guardian and the minor).
    pub fn pending_requests(&self) -> Vec<PendingRequest> {
        self.lock().pending_requests()
    }

    /// (Guardian) Approves or rejects. If approved, records the effect, also signed by the guardian.
    pub fn decide(&self, clock: Clock, request_id: String, approved: bool) -> Result<(), CoreError> {
        let mut g = self.lock();
        if g.my_role() != Some(Role::Guardian) {
            return Err(CoreError::protocol("only a guardian answers requests"));
        }
        let what = g
            .pending_requests()
            .into_iter()
            .find(|r| r.request_id == request_id)
            .ok_or_else(|| CoreError::protocol("request not found or already answered"))?
            .what;
        let (history, _) = g.state(clock);
        let mut cfg =
            history.active_at(clock.now_ms).cloned().ok_or_else(|| CoreError::protocol("no configuration"))?;
        g.record(clock.now_ms, EventKind::Decision { request_id, approved })?;
        if !approved {
            return Ok(());
        }
        let effect = match what {
            RequestKind::ExtraPermit { minutes } => EventKind::GrantPermit { minutes },
            RequestKind::RestDay => EventKind::RestDayGranted,
            RequestKind::AddSite { domain } => {
                if !cfg.web_allowlist.contains(&domain) {
                    cfg.web_allowlist.push(domain);
                }
                EventKind::ConfigChanged { config: cfg, effective_from_ms: clock.now_ms }
            }
            RequestKind::LaterBedtime { minutes } => {
                cfg.schedule.bedtime.minutes = (cfg.schedule.bedtime.minutes + minutes) % 1440;
                EventKind::ConfigChanged { config: cfg, effective_from_ms: clock.now_ms }
            }
        };
        g.record(clock.now_ms, effect)
    }

    /// Mirror panel: tampering, emergencies and blocked sites from the last 7 days, newest
    /// first. The guardian and the minor see exactly this (and the day state).
    pub fn family_alerts(&self, clock: Clock) -> Vec<FamilyAlertView> {
        let g = self.lock();
        let members = g.ledger.members();
        let since = clock.now_ms - ALERT_WINDOW_DAYS * DAY_MS;
        let mut out: Vec<FamilyAlertView> = g
            .ledger
            .authored()
            .into_iter()
            .filter(|(_, f)| f.at_ms >= since)
            .filter_map(|(author, f)| {
                let alert = match f.kind {
                    EventKind::Tamper { what } => FamilyAlert::Tamper { what },
                    EventKind::EmergencyStarted => FamilyAlert::Emergency,
                    EventKind::BlockedAttempts { count } => FamilyAlert::BlockedSites { count },
                    _ => return None,
                };
                let device_label = members.iter().find(|m| m.device_id == author).map(|m| m.label.clone())?;
                Some(FamilyAlertView { at_ms: f.at_ms, alert, device_label })
            })
            .collect();
        out.reverse();
        out
    }

    // ───────────────────────── DNS filter (a minor's devices) ─────────────────────────

    /// Loads the list of forbidden categories if its signature (minisign, pinned key) is valid.
    /// On failure, the previous one is kept.
    pub fn load_category_list(&self, data: Vec<u8>, minisig: String) -> Result<u32, CoreError> {
        let set = verified_list(PINNED_LISTS_PUBKEY, &data, &minisig)?;
        let n = set.len() as u32;
        self.lock().categories = set;
        Ok(n)
    }

    /// (A minor's devices) Downloads the category list and loads it if the signature is valid.
    /// Call at startup and once a day. Returns how many domains it has (0 if not applicable).
    pub fn refresh_category_list(&self) -> Result<u32, CoreError> {
        if self.my_role() != Some(Role::Minor) {
            return Ok(0);
        }
        let data = crate::sync::fetch_public(&format!("{LISTS_URL}categories.txt"), MAX_LIST_BYTES)?;
        let sig = crate::sync::fetch_public(&format!("{LISTS_URL}categories.txt.minisig"), 4096)?;
        self.load_category_list(data, String::from_utf8(sig).map_err(CoreError::protocol)?)
    }

    /// First step of a DNS query. The platform forwards or answers it depending on the result.
    /// When the day changes, records how many sites were blocked per category the day before (only the number).
    pub fn dns_step(&self, clock: Clock, query: Vec<u8>) -> DnsAction {
        let mut g = self.lock();
        let (mode, day) = match &g.dns_mode {
            Some((at, mode, day)) if clock.now_ms.abs_diff(*at) < DNS_MODE_TTL_MS as u64 => (mode.clone(), *day),
            _ => {
                let (mode, day) = g.filter_mode(clock);
                g.dns_mode = Some((clock.now_ms, mode.clone(), day));
                (mode, day)
            }
        };
        if day != 0 && g.blocked.0 != day {
            let count = std::mem::take(&mut g.blocked.1).len().min(u16::MAX as usize) as u16;
            g.blocked.0 = day;
            if count > 0 {
                let _ = g.record(clock.now_ms, EventKind::BlockedAttempts { count });
            }
        }
        let Inner { categories, blocked, .. } = &mut *g;
        match netpolicy::step(&query, &mode, categories, &mut blocked.1) {
            Some(DnsStep::Reply(packet)) => DnsAction::Reply { packet },
            Some(DnsStep::Upstream { packet, rewritten }) => DnsAction::Upstream { packet, rewritten },
            None => DnsAction::Drop,
        }
    }

    /// Second step: final answer built from the real DNS answer (None = drop).
    pub fn dns_finish(&self, query: Vec<u8>, reply: Vec<u8>, rewritten: bool) -> Option<Vec<u8>> {
        netpolicy::finish(&query, &reply, rewritten)
    }

    // ───────────────────────── Accountability partner ─────────────────────────

    /// Creates, in THIS `Core` (another database), the mailbox you will share with your partner.
    /// It has no configuration: it only receives alerts (tampering, emergency, daily summary).
    pub fn create_partner_mailbox(&self, clock: Clock, label: String) -> Result<(), CoreError> {
        check_label(&label)?;
        let mut g = self.lock();
        if !g.ledger.members().is_empty() {
            return Err(CoreError::protocol("the mailbox already exists"));
        }
        let me = g.keys.device_id();
        g.db.set_meta(META_GENESIS, &me)?;
        g.db.set_meta(META_KIND, KIND_PARTNER)?;
        g.ledger.set_genesis_author(me);
        let member = g.me_as_member(label, Role::Owner);
        g.record(clock.now_ms, EventKind::MembersChanged { members: vec![member] })
    }

    /// Copies to the partner mailbox ONLY tampering, emergencies and the summary of each finished
    /// day (numbers), since the mailbox exists. Idempotent. Returns how many alerts it added.
    pub fn forward_to_partner(&self, partner: Arc<Core>, clock: Clock) -> Result<u32, CoreError> {
        if std::ptr::eq(self, partner.as_ref()) {
            return Err(CoreError::protocol("the partner mailbox must be a different one"));
        }
        let since = {
            let p = partner.lock();
            if p.db.meta(META_KIND)?.as_deref() != Some(KIND_PARTNER) {
                return Err(CoreError::protocol("not a partner mailbox"));
            }
            p.ledger.facts().first().map(|f| f.at_ms).unwrap_or(i64::MAX)
        };
        let alerts: Vec<(i64, PartnerAlert)> = {
            let g = self.lock();
            let (h, f) = g.state(clock);
            let window_start = since.max(clock.now_ms - FORWARD_WINDOW_DAYS * DAY_MS);
            let mut v: Vec<(i64, PartnerAlert)> = f
                .iter()
                .filter(|x| x.at_ms >= window_start && x.at_ms <= clock.now_ms)
                .filter_map(|x| match &x.kind {
                    EventKind::Tamper { what } => Some((x.at_ms, PartnerAlert::Tamper { what: *what })),
                    EventKind::EmergencyStarted => Some((x.at_ms, PartnerAlert::Emergency)),
                    _ => None,
                })
                .collect();
            if let Some(today) = rules::status(&h, &f, clock) {
                for k in 0..FORWARD_WINDOW_DAYS {
                    let past = Clock { now_ms: today.day_start_ms - k * DAY_MS - 1, ..clock };
                    let Some(day) = rules::status(&h, &f, past) else { continue };
                    if day.goals.is_empty() || day.day_start_ms + DAY_MS <= since {
                        continue; // free day, no goals, or before the mailbox existed
                    }
                    let completed = day.goals.iter().filter(|g| g.completed).count().min(255) as u8;
                    let total = day.goals.len().min(255) as u8;
                    v.push((day.day_start_ms, PartnerAlert::DailySummary { completed, total }));
                }
            }
            v
        };
        let mut p = partner.lock();
        let have: Vec<(i64, PartnerAlert)> = p
            .ledger
            .facts()
            .into_iter()
            .filter_map(|f| match f.kind {
                EventKind::ForPartner { happened_at_ms, alert } => Some((happened_at_ms, alert)),
                _ => None,
            })
            .collect();
        let mut added = 0;
        for (happened_at_ms, alert) in alerts {
            if !have.contains(&(happened_at_ms, alert.clone())) {
                p.record(clock.now_ms, EventKind::ForPartner { happened_at_ms, alert })?;
                added += 1;
            }
        }
        Ok(added)
    }

    /// (Partner) Alerts received from the person you support, newest first.
    pub fn partner_alerts(&self) -> Vec<PartnerAlertView> {
        let g = self.lock();
        let members = g.ledger.members();
        let mut out: Vec<PartnerAlertView> = g
            .ledger
            .authored()
            .into_iter()
            .filter_map(|(author, f)| {
                let owner = members.iter().find(|m| m.device_id == author && m.role == Role::Owner)?;
                match f.kind {
                    EventKind::ForPartner { happened_at_ms, alert } => {
                        Some(PartnerAlertView { happened_at_ms, alert, device_label: owner.label.clone() })
                    }
                    _ => None,
                }
            })
            .collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.happened_at_ms));
        out
    }
}

/// Verifies a minisign-signed category list and parses it.
fn verified_list(pubkey_b64: &str, data: &[u8], minisig: &str) -> Result<CategorySet, CoreError> {
    let pk = minisign_verify::PublicKey::from_base64(pubkey_b64).map_err(CoreError::crypto)?;
    let sig = minisign_verify::Signature::decode(minisig).map_err(CoreError::crypto)?;
    pk.verify(data, &sig, false).map_err(|_| CoreError::crypto("invalid list signature"))?;
    let text = std::str::from_utf8(data).map_err(CoreError::protocol)?;
    CategorySet::parse(text).ok_or_else(|| CoreError::protocol("invalid list format"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::testing::MemStore;
    use crate::model::DayState;

    fn core() -> Arc<Core> {
        let dir = std::env::temp_dir().join(format!("pco-api-{}-{}", std::process::id(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        Core::open(dir.join("db").to_string_lossy().into(), Arc::new(MemStore::default())).unwrap()
    }

    const MONDAY_9: Clock = Clock { now_ms: 1_773_046_800_000, utc_offset_min: 0 };

    #[test]
    fn profile_plan_and_goal_through_the_api() {
        let c = core();
        let st = c.create_profile(MONDAY_9, "PC".into(), Config::example()).unwrap();
        assert!(st.needs_plan);
        assert_eq!(c.today_goals(MONDAY_9).len(), 2);
        c.plan_day(MONDAY_9, vec![]).unwrap();
        let st = c.complete_goal(MONDAY_9, "read-30".into(), Method::HonorDeclaration, 100, 30).unwrap();
        assert_eq!(st.state, DayState::Free);
        // A refusal reaches the UI as a stable code, without personal data
        let err = c.start_pass(MONDAY_9).unwrap_err();
        assert_eq!(err.code(), "denied.wrong_state");
    }

    #[test]
    fn clock_going_back_is_recorded_as_tamper() {
        let c = core();
        c.create_profile(MONDAY_9, "PC".into(), Config::example()).unwrap();
        let earlier = Clock { now_ms: MONDAY_9.now_ms - 60 * MINUTE_MS, ..MONDAY_9 };
        c.heartbeat(earlier).unwrap();
        let facts = c.lock().ledger.facts();
        assert!(facts.iter().any(|f| f.kind == EventKind::Tamper { what: TamperKind::ClockChanged }));
        assert!(facts.iter().all(|f| f.at_ms >= MONDAY_9.now_ms));
    }

    #[test]
    fn devices_seen_reports_last_activity() {
        let c = core();
        c.create_profile(MONDAY_9, "PC".into(), Config::example()).unwrap();
        let later = Clock { now_ms: MONDAY_9.now_ms + 15 * MINUTE_MS, ..MONDAY_9 };
        c.heartbeat(later).unwrap();
        let seen = c.devices_seen();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].is_me);
        assert_eq!(seen[0].last_seen_ms, later.now_ms);
    }

    #[test]
    fn partner_gets_only_alerts_and_numbers_once() {
        let (me, partner) = (core(), core());
        me.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        partner.create_partner_mailbox(MONDAY_9, "Phone".into()).unwrap();
        me.plan_day(MONDAY_9, vec![]).unwrap();
        let at = |h: i64| Clock { now_ms: MONDAY_9.now_ms + h * 60 * MINUTE_MS, ..MONDAY_9 };
        me.complete_goal(at(1), "read-30".into(), Method::HonorDeclaration, 100, 30).unwrap();
        me.report_tamper(at(2), TamperKind::PermissionRevoked).unwrap();
        me.emergency(at(3)).unwrap();
        let tuesday = Clock { now_ms: MONDAY_9.now_ms + DAY_MS, ..MONDAY_9 };
        assert_eq!(me.forward_to_partner(partner.clone(), tuesday).unwrap(), 3);
        assert_eq!(me.forward_to_partner(partner.clone(), tuesday).unwrap(), 0, "idempotente");
        let alerts: Vec<PartnerAlert> = partner.partner_alerts().into_iter().map(|a| a.alert).collect();
        assert_eq!(alerts.len(), 3);
        assert!(alerts.contains(&PartnerAlert::Emergency));
        assert!(alerts.contains(&PartnerAlert::Tamper { what: TamperKind::PermissionRevoked }));
        // Monday: "read-30" done, "eat-healthy" not → 1 of 2. No titles, nor which goals they were.
        assert!(alerts.contains(&PartnerAlert::DailySummary { completed: 1, total: 2 }));
        // The partner mailbox has no configuration nor day state
        assert!(partner.status(tuesday).is_none());
        assert!(me.forward_to_partner(me.clone(), tuesday).is_err());
    }

    // ── Family mode ──

    /// Copies the events from `from` to `to` (what the relay would do).
    fn deliver(from: &Core, to: &Core) {
        let evs = from.lock().db.all().unwrap();
        let mut g = to.lock();
        for ev in evs {
            if g.ledger.accept(ev.clone()).ok() == Some(Accepted::New) {
                g.db.insert(&ev, true).unwrap();
            }
        }
    }

    /// Guardian with a family profile and the minor's device already paired (no relay).
    fn family() -> (Arc<Core>, Arc<Core>) {
        let (guardian, minor) = (core(), core());
        guardian.create_family_profile(MONDAY_9, "Mom".into(), Config::example()).unwrap();
        let kid = minor.lock().me_as_member("Tablet".into(), Role::Minor);
        {
            let mut g = guardian.lock();
            let mut members = g.ledger.members().to_vec();
            members.push(kid);
            g.record(MONDAY_9.now_ms, EventKind::MembersChanged { members }).unwrap();
        }
        minor.lock().ledger.set_genesis_author(guardian.device_id());
        deliver(&guardian, &minor);
        (guardian, minor)
    }

    fn later(minutes: i64) -> Clock {
        Clock { now_ms: MONDAY_9.now_ms + minutes * MINUTE_MS, ..MONDAY_9 }
    }

    #[test]
    fn minor_asks_guardian_approves_and_the_permit_arrives() {
        let (guardian, minor) = family();
        assert_eq!((guardian.my_role(), minor.my_role()), (Some(Role::Guardian), Some(Role::Minor)));
        let id = minor.request(later(1), RequestKind::ExtraPermit { minutes: 15 }).unwrap();
        assert!(guardian.request(later(1), RequestKind::RestDay).is_err(), "the guardian does not make requests");
        deliver(&minor, &guardian);
        assert_eq!(guardian.pending_requests().len(), 1);
        assert!(minor.decide(later(2), id.clone(), true).is_err(), "the minor cannot approve themselves");
        guardian.decide(later(2), id.clone(), true).unwrap();
        assert!(guardian.decide(later(3), id, true).is_err(), "already answered");
        deliver(&guardian, &minor);
        assert!(minor.pending_requests().is_empty());
        // 3 daily + 1 granted
        assert_eq!(minor.status(later(4)).unwrap().permits_left, 4);
    }

    #[test]
    fn a_tampered_minor_device_cannot_grant_itself_anything() {
        let (guardian, minor) = family();
        for kind in [EventKind::GrantPermit { minutes: 60 }, EventKind::RestDayGranted] {
            assert!(minor.lock().record(later(1).now_ms, kind).is_err());
        }
        // Nor relax the configuration (the core rejects it and, if it arrived signed, it would be ignored)
        let mut laxer = Config::example();
        laxer.permits_per_day = 9;
        assert!(matches!(minor.set_config(later(1), laxer.clone()), Err(CoreError::Denied { .. })));
        minor
            .lock()
            .record(later(2).now_ms, EventKind::ConfigChanged { config: laxer.clone(), effective_from_ms: 0 })
            .unwrap();
        deliver(&minor, &guardian);
        let tomorrow = Clock { now_ms: MONDAY_9.now_ms + DAY_MS, ..MONDAY_9 };
        for c in [&guardian, &minor] {
            assert_eq!(c.active_config(tomorrow).unwrap().permits_per_day, 3);
        }
        // Making it stricter is allowed, and immediately
        let mut stricter = Config::example();
        stricter.permits_per_day = 1;
        assert_eq!(minor.set_config(later(3), stricter).unwrap(), later(3).now_ms);
        // The guardian relaxes immediately
        assert_eq!(guardian.set_config(later(4), laxer).unwrap(), later(4).now_ms);
    }

    #[test]
    fn rest_day_and_new_site_requests() {
        let (guardian, minor) = family();
        assert!(minor.request(later(1), RequestKind::AddSite { domain: "com".into() }).is_err());
        assert!(minor.request(later(1), RequestKind::ExtraPermit { minutes: 600 }).is_err());
        let site = minor.request(later(1), RequestKind::AddSite { domain: "Khanacademy.org.".into() }).unwrap();
        let rest = minor.request(later(1), RequestKind::RestDay).unwrap();
        deliver(&minor, &guardian);
        guardian.decide(later(2), site, true).unwrap();
        guardian.decide(later(2), rest, false).unwrap();
        deliver(&guardian, &minor);
        assert!(minor.active_config(later(3)).unwrap().web_allowlist.contains(&"khanacademy.org".to_string()));
        assert_ne!(minor.status(later(3)).unwrap().state, DayState::Free, "descanso rechazado");
        let rest = minor.request(later(4), RequestKind::RestDay).unwrap();
        deliver(&minor, &guardian);
        guardian.decide(later(5), rest, true).unwrap();
        deliver(&guardian, &minor);
        assert_eq!(minor.status(later(6)).unwrap().state, DayState::Free);
    }

    #[test]
    fn pairing_role_must_fit_the_profile() {
        let c = core();
        c.create_profile(MONDAY_9, "PC".into(), Config::example()).unwrap();
        let err = c.pairing_confirm(MONDAY_9, Some(Role::Guardian)).unwrap_err().to_string();
        assert!(err.contains("invalid role"), "{err}");
        let (guardian, _) = family();
        assert!(guardian.pairing_confirm(MONDAY_9, None).unwrap_err().to_string().contains("invalid role"));
    }

    /// minisign vector with a throwaway key (BLAKE2b prehash, like `minisign -S`).
    const TEST_PK: &str = "RWQlDdRnNvC7WVLh30EvLQwEavQfCouzpAMSx6NTBaRAfOKIjJDumRaD";
    const TEST_LIST: &[u8] = b"procastinacioff-categories v1\nexample-adult.com\n";
    const TEST_SIG: &str = "untrusted comment: prueba
RUQlDdRnNvC7Wcl60rnIlQPVfyM7Nc9QRq23JVyEqLSz8YcUVrRIL3LLe3HWaZ47i6dxagLR8vVpLVOLOFPSp/xxiswxcchvbAQ=
trusted comment: timestamp:0\tfile:categories.txt\tprehashed
cYXq6Cd/cf1X9l7OI+KZ0w8776VB/XSZ/5WeXKfE0oMuGPfhhJrh0KVkSwSaYR44zj/LaHT0UgDHyT2NGf5xBA==
";

    #[test]
    fn category_lists_need_a_valid_signature() {
        assert_eq!(verified_list(TEST_PK, TEST_LIST, TEST_SIG).unwrap().len(), 1);
        let mut tampered = TEST_LIST.to_vec();
        tampered.extend(b"wikipedia.org\n");
        assert!(verified_list(TEST_PK, &tampered, TEST_SIG).is_err());
        // Without the real pinned key, no list is accepted
        assert!(core().load_category_list(TEST_LIST.to_vec(), TEST_SIG.into()).is_err());
    }

    fn dns_query(name: &str) -> Vec<u8> {
        use hickory_proto::op::{Message, MessageType, OpCode, Query};
        use hickory_proto::rr::{Name, RecordType};
        let mut m = Message::new(7, MessageType::Query, OpCode::Query);
        m.add_query(Query::query(Name::from_ascii(name).unwrap(), RecordType::A));
        m.to_vec().unwrap()
    }

    #[test]
    fn dns_filters_only_the_minor_and_counts_blocked_sites_per_day() {
        let (guardian, minor) = family();
        let blocked =
            |c: &Core, at: Clock, name: &str| matches!(c.dns_step(at, dns_query(name)), DnsAction::Reply { .. });
        // LOCKED: only essentials on the minor's device; the guardian's is not filtered
        assert!(blocked(&minor, later(1), "example.org"));
        assert!(!blocked(&minor, later(1), "web.whatsapp.com"));
        assert!(!blocked(&guardian, later(1), "example.org"));
        // Rest day (FREE): everything except categories
        minor.lock().categories = verified_list(TEST_PK, TEST_LIST, TEST_SIG).unwrap();
        let rest = minor.request(later(2), RequestKind::RestDay).unwrap();
        deliver(&minor, &guardian);
        guardian.decide(later(3), rest, true).unwrap();
        deliver(&guardian, &minor);
        let free = later(10); // after the mode TTL
        assert!(!blocked(&minor, free, "example.org"));
        assert!(blocked(&minor, free, "a.example-adult.com"));
        assert!(blocked(&minor, free, "b.example-adult.com"));
        // The next day the number is recorded (1 site), never which one
        let tomorrow = Clock { now_ms: MONDAY_9.now_ms + DAY_MS, ..MONDAY_9 };
        minor.dns_step(tomorrow, dns_query("example.org"));
        let alerts: Vec<FamilyAlert> = minor.family_alerts(tomorrow).into_iter().map(|a| a.alert).collect();
        assert_eq!(alerts, vec![FamilyAlert::BlockedSites { count: 1 }]);
    }
}
