// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The `Core` object: the only thing the platforms see (Kotlin, Swift, desktop agent).
//!
//! - A `Mutex` protects the state. It is never held during a network call.
//! - Every action returns the new `Status` so the UI updates without another call.
//! - The network (peer.rs) lives in its own `Mutex`, never held together with the state.

use std::collections::{BTreeMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use base64::Engine;
use iroh::endpoint::Connection;
use procastinatioff_proto::{self as proto, Msg};
use rand::{RngCore, rngs::OsRng};

use crate::auth::{self, Secrets};
use crate::clock::{Clock, DAY_MS, MINUTE_MS};
use crate::crypto::{DeviceKeys, SECRET_DB, SECRET_DB_NEXT, SecretStore, db_key, random_32};
use crate::error::CoreError;
use crate::events::{EventBody, SignedEvent, open, seal};
use crate::ledger::{Accepted, Ledger};
use crate::model::{
    Config, ContactAlert, DayState, Effective, EventKind, Fact, Goal, GrantProof, LocalFactor, Member, Method, Notice,
    NoticeKind, OfflineAsk, RequestKind, Role, SecretHolder, Status, TamperKind,
};
use crate::netpolicy::{self, CategorySet, DnsStep, FilterMode};
use crate::offline_code;
use crate::pairing::{self, JoinRequest, MAX_LABEL_CHARS, PairingOffer, PendingPairing};
use crate::peer::{self, Peer, PeerNet};
use crate::rules::{self, CONTACT_WAIT_MS, ConfigHistory, Denied};
use crate::strictness::{effective_from, is_stricter_or_equal, only_leaves_contact};
use crate::time::{Sample, TimeAnchor, corrected_now, pick_anchor, sign_time, verify_signed_time};

const META_GENESIS: &str = "genesis_author";
const META_KIND: &str = "kind";
/// "1" if this device's clock cannot be changed by its user (see `set_time_source`).
const META_TIME_SOURCE: &str = "time_source";
/// Failed local-secret attempts in a row, and until when attempts are blocked (trusted time).
const META_FAILURES: &str = "auth_failures";
const META_LOCKED_UNTIL: &str = "auth_locked_until";
/// The offline request this device is waiting an answer for (`PendingCode` JSON).
const META_OFFLINE: &str = "offline_request";
/// Prefix of a configuration sent to the trusted contact, by hash (`change:<hash>` → JSON).
const META_CHANGE: &str = "change:";
/// Prefix of a contact decision already applied to the profile (`applied:<event id>`).
const META_APPLIED: &str = "applied:";
const KIND_CONTACT: &str = "contact";
const RETENTION_DAYS: i64 = 90;
/// Days back that are checked when forwarding alerts to the trusted contact.
const FORWARD_WINDOW_DAYS: i64 = 7;
/// A local unlock (PIN, password, pattern or biometrics) is valid this long.
const AUTH_WINDOW_MS: i64 = 5 * MINUTE_MS;
/// Maximum difference between the device clock and the signed time before it is recorded.
const MAX_CLOCK_SKEW_MS: i64 = 5 * MINUTE_MS;
/// Unanswered requests a minor may have at once (each one notifies the guardian).
const MAX_OPEN_REQUESTS: usize = 5;
/// Days back covered by the mirror panel alerts.
const ALERT_WINDOW_DAYS: i64 = 7;
/// How often the DNS filter mode is recomputed (not on every query).
const DNS_MODE_TTL_MS: i64 = 5_000;
/// A QR is only answered for this long.
const PAIRING_TTL: Duration = Duration::from_secs(10 * 60);
/// Another device's activity: a longer gap or a longer silence produces a `DeviceSilent` notice.
const SILENT_GAP_MS: i64 = 24 * 60 * MINUTE_MS;
const UNSEEN_MS: i64 = 72 * 60 * MINUTE_MS;
const ACTIVITY_WINDOW_DAYS: i64 = 7;
/// Events that are never pruned: without them the profile has no members, configuration,
/// discovery or secrets. SECURITY: approvals and resets stay too: pruning them would change how
/// older configurations and secrets are judged.
fn structural(ev: &SignedEvent) -> bool {
    matches!(
        ev.body.kind,
        EventKind::MembersChanged { .. }
            | EventKind::ConfigChanged { .. }
            | EventKind::DiscoveryKey { .. }
            | EventKind::SecretSet { .. }
            | EventKind::RecoveryCodeSet { .. }
            | EventKind::LocalResetStarted
            | EventKind::ContactApproval { .. }
    )
}

/// An offline request waiting for its answer (on the device that asked).
#[derive(serde::Serialize, serde::Deserialize)]
struct PendingCode {
    code: String,
    ask: OfflineAsk,
    created_ms: i64,
    attempts: u32,
}

/// How changes are authorized on this device, for the settings screen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct AuthState {
    /// Local methods in the policy.
    pub methods: Vec<LocalFactor>,
    /// Those usable now: with a secret set (biometrics: always; the OS checks it).
    pub usable: Vec<LocalFactor>,
    /// (Minor's device) Methods the guardian can type here ("my guardian is here").
    pub guardian_methods: Vec<LocalFactor>,
    /// Attempts blocked until this instant (0 = not blocked).
    pub locked_until_ms: i64,
    /// The recovery code was used: when the methods stop counting.
    pub reset_at_ms: Option<i64>,
    pub has_recovery_code: bool,
    pub contact_required: bool,
    pub contacts: u32,
    /// The local gate is open: no usable method, or one was used in the last 5 minutes.
    pub unlocked: bool,
}

/// What an approver's app shows for an offline request, and the code to read back.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OfflineAnswer {
    pub ask: OfflineAsk,
    pub response: String,
}

/// Public key (minisign, base64) that signs the category lists. It is generated on an
/// air-gapped machine; until then no list is accepted. The signed list ships inside the app.
pub const PINNED_LISTS_PUBKEY: &str = "REEMPLAZAR_CON_CLAVE_PUBLICA_MINISIGN";

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct SyncReport {
    /// Peers a session finished with.
    pub peers: u32,
    pub sent: u32,
    pub received: u32,
    pub rejected: u32,
    pub gaps: u32,
}

impl SyncReport {
    fn add(&mut self, o: &SyncReport) {
        self.peers += o.peers;
        self.sent += o.sent;
        self.received += o.received;
        self.rejected += o.rejected;
        self.gaps += o.gaps;
    }
}

/// Last moment each device was seen (cross heartbeats).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct DeviceSeen {
    pub device_id: String,
    /// SECURITY: Personal data: only for display in the UI.
    pub label: String,
    pub last_seen_ms: i64,
    /// Longest gap between two of its events in the last 7 days.
    pub longest_gap_ms: i64,
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
    /// A concession granted with an offline code or the guardian's secret in person.
    Granted {
        what: OfflineAsk,
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

/// Alert as the trusted contact sees it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ContactAlertView {
    pub happened_at_ms: i64,
    pub alert: ContactAlert,
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
    /// LAN addresses per network key (from the QR and mDNS). Memory only.
    addrs: BTreeMap<String, Vec<SocketAddr>>,
    /// Latest time sample per peer (network key). Memory only, like the anchor.
    samples: BTreeMap<String, Sample>,
    /// While joining: network key of the inviter (not in our ledger yet).
    join_peer: Option<String>,
    /// Last trusted time handed to the platform, and when. Stamps the `Hello` of incoming sessions.
    /// ponytail: `Instant` stops in Android deep sleep; the desktop service, which answers most
    /// sessions, asks for the time every second.
    last_now: Option<(i64, Instant)>,
    /// Last successful local unlock (trusted time). Memory only: a restart locks again.
    authorized_at: Option<i64>,
}

impl Inner {
    fn new(db: crate::storage::Db, keys: DeviceKeys, ledger: Ledger) -> Self {
        Self {
            db,
            keys,
            ledger,
            pairing: None,
            anchor: None,
            clock_skewed: false,
            categories: CategorySet::default(),
            dns_mode: None,
            blocked: (0, HashSet::new()),
            addrs: BTreeMap::new(),
            samples: BTreeMap::new(),
            join_peer: None,
            last_now: None,
            authorized_at: None,
        }
    }

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

    fn members_with(&self, role: Role) -> impl Iterator<Item = &Member> {
        self.ledger.members().iter().filter(move |m| m.role == role)
    }

    /// The local gate: open if no local method is usable, or one was used in the last 5 minutes.
    fn gate_open(&self, policy: &[LocalFactor], facts: &[Fact], now_ms: i64) -> bool {
        Secrets::from_facts(facts, now_ms).usable(policy).is_empty()
            || self.authorized_at.is_some_and(|t| (t..t + AUTH_WINDOW_MS).contains(&now_ms))
    }

    fn meta_i64(&self, k: &str) -> i64 {
        self.db.meta(k).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// SECURITY: attempts are counted on disk with trusted time: restarting the app does not reset them.
    fn check_not_locked(&self, now_ms: i64) -> Result<(), CoreError> {
        let until = self.meta_i64(META_LOCKED_UNTIL);
        if now_ms < until {
            let minutes = ((until - now_ms + MINUTE_MS - 1) / MINUTE_MS).min(u16::MAX as i64) as u16;
            return Err(Denied::Locked { minutes }.into());
        }
        Ok(())
    }

    /// Records the result of a secret attempt: a failure counts and may block the next ones.
    fn note_attempt(&mut self, now_ms: i64, ok: bool) -> Result<(), CoreError> {
        if ok {
            self.db.set_meta(META_FAILURES, "0")?;
            return Ok(());
        }
        let failures = u32::try_from(self.meta_i64(META_FAILURES)).unwrap_or(0).saturating_add(1);
        self.db.set_meta(META_FAILURES, &failures.to_string())?;
        self.db.set_meta(META_LOCKED_UNTIL, &(now_ms + auth::lockout_ms(failures)).to_string())?;
        Err(Denied::WrongSecret.into())
    }

    /// Concessions without network recorded today (codes and the guardian in person).
    fn grants_today(&self, clock: Clock) -> usize {
        let Some(st) = self.status(clock) else { return 0 };
        self.ledger
            .facts()
            .iter()
            .filter(|f| f.at_ms >= st.day_start_ms && matches!(f.kind, EventKind::LocalGrant { .. }))
            .count()
    }

    /// Approvers of offline codes in THIS core: (device, key, profile). Guardians in a family
    /// profile; trusted contacts in a contact channel.
    fn approvers(&self) -> Vec<(String, [u8; 32], String)> {
        let (Some(key), Ok(Some(profile))) = (self.discovery_key(), self.db.meta(META_GENESIS)) else { return vec![] };
        let role = if self.is_contact_channel() { Role::TrustedContact } else { Role::Guardian };
        self.members_with(role)
            .map(|m| (m.device_id.clone(), offline_code::approver_key(&key, &m.device_id), profile.clone()))
            .collect()
    }

    /// The owner marks the channel when creating it; on the contact's device, being a contact
    /// says it (a contact is a full member only of a channel).
    fn is_contact_channel(&self) -> bool {
        self.db.meta(META_KIND).ok().flatten().as_deref() == Some(KIND_CONTACT)
            || self.my_role() == Some(Role::TrustedContact)
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
        // Peer sync uses LAN addresses, never DNS: nothing to add to the allowlist.
        let mode = match st.state {
            DayState::Focus { sites, .. } => FilterMode::Allowlist(sites),
            DayState::Sleep | DayState::Locked => FilterMode::Allowlist(vec![]),
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
        self.db.insert(&ev)
    }

    fn status(&self, clock: Clock) -> Option<Status> {
        let (h, f) = self.state(clock);
        rules::status(&h, &f, clock)
    }

    fn must_status(&self, clock: Clock) -> Result<Status, CoreError> {
        self.status(clock).ok_or_else(|| CoreError::protocol("no configuration"))
    }

    fn me_as_member(&self, label: String, role: Role) -> Member {
        Member {
            device_id: self.keys.device_id(),
            age_recipient: self.keys.age_recipient(),
            label,
            role,
            node_id: self.keys.node_id(),
            time_source: self.time_source(),
        }
    }

    fn time_source(&self) -> bool {
        self.db.meta(META_TIME_SOURCE).ok().flatten().as_deref() == Some("1")
    }

    /// First events of a profile or mailbox: its members and its discovery key.
    fn found(&mut self, at_ms: i64, member: Member) -> Result<(), CoreError> {
        let me = self.keys.device_id();
        self.db.set_meta(META_GENESIS, &me)?;
        self.ledger.set_genesis_author(me);
        self.record(at_ms, EventKind::MembersChanged { members: vec![member] })?;
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        self.record(at_ms, EventKind::DiscoveryKey { key: hex::encode(key) })
    }

    /// ponytail: the first key, never rotated. A removed member could still recognize the
    /// profile's mDNS names; rotate (a new event) if that matters.
    fn discovery_key(&self) -> Option<[u8; 32]> {
        self.ledger.facts().into_iter().find_map(|f| match f.kind {
            EventKind::DiscoveryKey { key } => hex::decode(key).ok()?.try_into().ok(),
            _ => None,
        })
    }

    /// Who this profile syncs with: every other member with a network key, plus the inviter while joining.
    /// SECURITY: a trusted contact listed in a profile (to check its signature) is never a peer
    /// there: it only syncs the contact channel, where it is a full member.
    fn peers(&self) -> Vec<Peer> {
        let me = self.keys.node_id();
        let channel = self.is_contact_channel();
        let mut out: Vec<Peer> = self
            .ledger
            .members()
            .iter()
            .filter(|m| !m.node_id.is_empty() && m.node_id != me && (channel || m.role != Role::TrustedContact))
            .map(|m| Peer {
                node_id: m.node_id.clone(),
                recipient: m.age_recipient.parse().ok(),
                device_id: Some(m.device_id.clone()),
                time_source: m.time_source,
            })
            .collect();
        if let Some(j) = &self.join_peer
            && !out.iter().any(|p| &p.node_id == j)
        {
            out.push(Peer { node_id: j.clone(), recipient: None, device_id: None, time_source: false });
        }
        out
    }

    /// `Hello` of this device and the profile it belongs to (its genesis author).
    fn hello(&self, now_ms: i64) -> Result<(Msg, String), CoreError> {
        let profile = self.db.meta(META_GENESIS)?.ok_or_else(|| CoreError::protocol("no profile"))?;
        let hello = Msg::Hello {
            v: proto::PROTOCOL_VERSION,
            profile: profile.clone(),
            have: self.ledger.heads(),
            time: sign_time(&self.keys.signing, now_ms),
        };
        Ok((hello, profile))
    }

    /// Opens, orders and applies the blobs a peer sent.
    fn ingest(&mut self, blobs: Vec<Vec<u8>>) -> Result<SyncReport, CoreError> {
        let mut report = SyncReport::default();
        let mut evs = Vec::with_capacity(blobs.len());
        for blob in blobs {
            match open(&blob, &self.keys.age) {
                Ok(ev) => evs.push(ev),
                Err(_) => report.rejected += 1,
            }
        }
        peer::sort_for_ledger(&mut evs);
        // The first copy of a profile is history, not news: no notices for it.
        let joining = self.ledger.members().is_empty();
        let gaps_before = self.ledger.gaps.len();
        let me = self.keys.device_id();
        // Events with the same time can come before the `MembersChanged` that makes their author a
        // member. An author whose event is rejected waits for the next pass with the rest of its
        // events (still in `seq` order); passes repeat while something new gets accepted.
        let mut pending = evs;
        loop {
            let (mut deferred, mut waiting, mut progress) = (Vec::new(), HashSet::new(), false);
            for ev in pending {
                if waiting.contains(&ev.body.author) {
                    deferred.push(ev);
                    continue;
                }
                match self.ledger.accept(ev.clone()) {
                    Ok(Accepted::New) => {
                        progress = true;
                        self.db.insert(&ev)?;
                        report.received += 1;
                        if !joining
                            && ev.body.author != me
                            && let Some(kind) = notice_for(self.my_role(), &ev.body.kind)
                        {
                            self.db.add_notice(&Notice { id: ev.id(), kind, at_ms: ev.body.at_ms })?;
                        }
                    }
                    Ok(Accepted::Duplicate) => {}
                    Err(_) => {
                        waiting.insert(ev.body.author.clone());
                        deferred.push(ev);
                    }
                }
            }
            if !progress || deferred.is_empty() {
                report.rejected += deferred.len() as u32;
                break;
            }
            pending = deferred;
        }
        report.gaps = (self.ledger.gaps.len() - gaps_before) as u32;
        Ok(report)
    }

    /// Re-anchors trusted time with the peers' samples and this device's own clock.
    fn reanchor(&mut self, device_now_ms: i64, mono_ms: i64) -> Result<(), CoreError> {
        let mut samples: Vec<Sample> = self.samples.values().copied().collect();
        samples.push(Sample { trusted_ms: device_now_ms, mono_ms, from_time_source: self.time_source() });
        self.anchor = pick_anchor(&samples, mono_ms);
        let trusted = corrected_now(self.anchor, device_now_ms, mono_ms);
        // SECURITY: the system clock is off from the peers' time: recorded once; the rules use the anchor.
        let skewed = (device_now_ms - trusted).abs() > MAX_CLOCK_SKEW_MS;
        if skewed && !self.clock_skewed && !self.ledger.members().is_empty() {
            self.record(trusted, EventKind::Tamper { what: TamperKind::ClockChanged })?;
        }
        self.clock_skewed = skewed;
        Ok(())
    }

    /// Per author: last activity and longest gap between its events in the last 7 days.
    fn activity(&self, now_ms: i64) -> BTreeMap<String, (i64, i64)> {
        let since = now_ms - ACTIVITY_WINDOW_DAYS * DAY_MS;
        let mut times: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        for (author, f) in self.ledger.authored() {
            times.entry(author).or_default().push(f.at_ms); // already in time order
        }
        times
            .into_iter()
            .map(|(author, ts)| {
                let last = ts.last().copied().unwrap_or(0);
                let recent: Vec<i64> = ts.into_iter().filter(|t| (since..=now_ms).contains(t)).collect();
                let gap = recent.windows(2).map(|w| w[1] - w[0]).max().unwrap_or(0);
                (author, (last, gap))
            })
            .collect()
    }

    /// `DeviceSilent` for another device with a gap > 24 h in its activity or unseen for 72 h.
    /// Informative only: without a server, not seeing a device for a day is normal.
    fn silent_notices(&self, now_ms: i64) -> Result<(), CoreError> {
        if !matches!(self.my_role(), Some(Role::Guardian | Role::Owner)) {
            return Ok(());
        }
        let me = self.keys.device_id();
        let activity = self.activity(now_ms);
        for m in self.ledger.members().iter().filter(|m| m.device_id != me) {
            let Some(&(last, gap)) = activity.get(&m.device_id) else { continue };
            // The id only has to be stable for one silence, so it is notified once.
            let mark = if now_ms - last > UNSEEN_MS {
                format!("unseen:{last}")
            } else if gap > SILENT_GAP_MS {
                format!("gap:{gap}")
            } else {
                continue;
            };
            let id = format!("silent:{}:{mark}", m.device_id);
            self.db.add_notice(&Notice { id, kind: NoticeKind::DeviceSilent, at_ms: now_ms })?;
        }
        Ok(())
    }

    /// (Inviter) A `Join` arrived. Accepted once, only while the QR is valid, and only from the
    /// network key the request names.
    fn accept_join(&mut self, remote_node: &str, token: &str, blob_b64: &str) -> bool {
        let Some(p) = self.pairing.as_mut() else { return false };
        if p.joiner.is_some() || p.created.elapsed() > PAIRING_TTL || p.offer.token != token {
            return false;
        }
        let Ok(blob) = base64::engine::general_purpose::STANDARD.decode(blob_b64) else { return false };
        let Ok(req) = pairing::open_join(&blob, &p.eph) else { return false };
        if req.node_id != remote_node {
            return false;
        }
        p.joiner = Some(req);
        true
    }
}

/// Which events from other devices this device's user hears about, by role.
fn notice_for(role: Option<Role>, kind: &EventKind) -> Option<NoticeKind> {
    match (role?, kind) {
        (Role::Guardian | Role::TrustedContact, EventKind::Request { .. }) => Some(NoticeKind::RequestReceived),
        (Role::Minor | Role::Owner, EventKind::Decision { approved, .. }) => {
            Some(NoticeKind::RequestAnswered { approved: *approved })
        }
        (Role::Guardian, EventKind::Tamper { .. }) => Some(NoticeKind::TamperAlert),
        (Role::Guardian, EventKind::EmergencyStarted) => Some(NoticeKind::EmergencyAlert),
        (Role::TrustedContact, EventKind::ForContact { alert, .. }) => match alert {
            ContactAlert::Tamper { .. } => Some(NoticeKind::TamperAlert),
            ContactAlert::Emergency => Some(NoticeKind::EmergencyAlert),
            ContactAlert::LocalReset => Some(NoticeKind::LocalResetStarted),
            ContactAlert::ExitScheduled => Some(NoticeKind::ContactExitScheduled),
            ContactAlert::DailySummary { .. } | ContactAlert::CodeGrant => None,
        },
        _ => None,
    }
}

#[derive(uniffi::Object)]
pub struct Core {
    inner: Mutex<Inner>,
    net: Mutex<Option<Arc<PeerNet>>>,
    secrets: Arc<dyn SecretStore>,
}

/// Secrets of the database at `path`. `core.db` (the device's own profile) keeps the original names;
/// any other database (contact channel, watching box, a minor's profile) gets its own copy. "Delete all
/// my data" re-keys each database: with shared names, erasing a second one left the first without its
/// key, unreadable after a restart.
fn scoped(path: &str, shared: Arc<dyn SecretStore>) -> Arc<dyn SecretStore> {
    let stem = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    if stem == "core" {
        return shared;
    }
    let suffix = stem.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    Arc::new(Scoped { shared, suffix })
}

struct Scoped {
    shared: Arc<dyn SecretStore>,
    suffix: String,
}

impl SecretStore for Scoped {
    fn load(&self, name: String) -> Result<Option<Vec<u8>>, CoreError> {
        let own = format!("{name}_{}", self.suffix);
        if let Some(v) = self.shared.load(own.clone())? {
            return Ok(Some(v));
        }
        // Databases from before per-database secrets used the shared ones: adopted on first use.
        // Platforms open every database before erasing any, so the shared key is still the old one.
        let Some(v) = self.shared.load(name)? else { return Ok(None) };
        self.shared.save(own, v.clone())?;
        Ok(Some(v))
    }

    fn save(&self, name: String, value: Vec<u8>) -> Result<(), CoreError> {
        self.shared.save(format!("{name}_{}", self.suffix), value)
    }
}

/// The database with its key. If "delete all my data" stopped after re-encrypting the file but
/// before saving the new key, the staged key opens it and the erase is finished here, in the same
/// order as `erase_all` (new identity, then the database key): the new keys are returned.
fn open_db(path: &str, secrets: &dyn SecretStore) -> Result<(crate::storage::Db, Option<DeviceKeys>), CoreError> {
    let key = db_key(secrets)?;
    let err = match crate::storage::Db::open(path, &key) {
        Ok(db) => return Ok((db, None)),
        Err(e) => e,
    };
    let Some(next) = secrets.load(SECRET_DB_NEXT.into())?.map(zeroize::Zeroizing::new) else { return Err(err) };
    if next.len() != 32 || *next == *key {
        return Err(err);
    }
    let db = crate::storage::Db::open(path, &next).map_err(|_| err)?;
    let keys = DeviceKeys::replace(secrets)?;
    secrets.save(SECRET_DB.into(), next.to_vec())?;
    Ok((db, Some(keys)))
}

impl Core {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // If another thread panicked, we continue with the state (it is consistent: every
        // write is atomic in SQLite) instead of propagating the panic to the app.
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn net_slot(&self) -> MutexGuard<'_, Option<Arc<PeerNet>>> {
        self.net.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The network endpoint, created on first use. Without `start_listening` it accepts nothing.
    fn net(&self) -> Result<Arc<PeerNet>, CoreError> {
        let mut slot = self.net_slot();
        if let Some(n) = slot.as_ref() {
            return Ok(n.clone());
        }
        let key = *self.lock().keys.net;
        let n = Arc::new(PeerNet::bind(&key, 0, false)?);
        *slot = Some(n.clone());
        Ok(n)
    }

    /// Time to stamp a `Hello` when answering (the platform passes no clock there); 0 = unknown.
    fn answer_time(&self) -> i64 {
        self.lock().last_now.map(|(t, at)| t + at.elapsed().as_millis() as i64).unwrap_or(0)
    }

    /// Events the peer lacks, each one sealed only for it.
    fn outgoing(&self, theirs: &BTreeMap<String, u64>, peer: &Peer) -> Result<Vec<String>, CoreError> {
        let Some(r) = &peer.recipient else { return Ok(vec![]) };
        let evs = {
            let g = self.lock();
            peer::missing_for(&g.db, &g.ledger.heads(), theirs)?
        };
        // Sealed without the lock: thousands of events cost a while.
        evs.iter().map(|ev| seal(ev, std::slice::from_ref(r)).map(|b| peer::encode_blob(&b))).collect()
    }

    /// An incoming connection (accept loop in peer.rs).
    pub(crate) async fn serve(self: Arc<Self>, conn: Connection) {
        let remote = peer::node_hex(&conn.remote_id());
        if conn.alpn() == proto::PAIR_ALPN {
            self.serve_pair(&conn, &remote).await;
        } else if conn.alpn() == proto::SYNC_ALPN {
            let peer = self.lock().peers().into_iter().find(|p| p.node_id == remote && p.device_id.is_some());
            // SECURITY: a key that is not a member is closed before anything is read from it.
            let Some(peer) = peer else { return conn.close(0u32.into(), b"") };
            let Ok((mut send, mut recv)) = conn.accept_bi().await else { return };
            let now = self.answer_time(); // before taking the lock: it takes it too
            let Ok((hello, profile)) = self.lock().hello(now) else { return };
            let ex = peer::session(&mut send, &mut recv, hello, &profile, |t| self.outgoing(t, &peer)).await;
            if let Ok(ex) = ex {
                let _ = self.lock().ingest(ex.blobs);
            }
            let _ = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;
        }
    }

    async fn serve_pair(&self, conn: &Connection, remote: &str) {
        // Nothing is read unless a QR is on screen.
        if self.lock().pairing.is_none() {
            return conn.close(0u32.into(), b"");
        }
        let Ok((mut send, mut recv)) = conn.accept_bi().await else { return };
        let Ok(Msg::Join { token, blob }) = proto::read_msg(&mut recv).await else { return };
        let accepted = self.lock().accept_join(remote, &token, &blob);
        if accepted && proto::write_msg(&mut send, &Msg::Done).await.is_ok() {
            let _ = send.finish();
            let _ = send.stopped().await;
        }
        conn.close(0u32.into(), b"");
    }

    /// One outgoing session with a peer. Returns what arrived and the peer's time sample.
    fn sync_with(
        &self,
        net: &PeerNet,
        peer: &Peer,
        addrs: &[SocketAddr],
        hello: &Msg,
        profile: &str,
    ) -> Result<peer::Exchange, CoreError> {
        net.block_on(async {
            tokio::time::timeout(peer::SESSION_TIMEOUT, async {
                let conn = net.connect(&peer.node_id, addrs, proto::SYNC_ALPN).await?;
                let (mut send, mut recv) = conn.open_bi().await.map_err(CoreError::network)?;
                let ex = peer::session(&mut send, &mut recv, hello.clone(), profile, |t| self.outgoing(t, peer)).await;
                conn.close(0u32.into(), b"");
                ex
            })
            .await
            .map_err(|_| CoreError::network("timeout"))?
        })
    }
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
        let member = g.me_as_member(label, role);
        g.found(clock.now_ms, member)?;
        g.record(clock.now_ms, EventKind::ConfigChanged { config, effective_from_ms: clock.now_ms })?;
        g.must_status(clock)
    }
}

#[uniffi::export]
impl Core {
    /// Opens the core. `db_path`: the app's private path. `secrets`: the platform keystore.
    #[uniffi::constructor]
    pub fn open(db_path: String, secrets: Arc<dyn SecretStore>) -> Result<Arc<Self>, CoreError> {
        let secrets = scoped(&db_path, secrets);
        let (db, erased_keys) = open_db(&db_path, secrets.as_ref())?;
        let keys = match erased_keys {
            Some(keys) => keys,
            None => DeviceKeys::load_or_create(secrets.as_ref())?,
        };
        let mut ledger = Ledger::new(db.meta(META_GENESIS)?);
        for ev in db.all()? {
            ledger.accept(ev)?; // re-validates signatures and chain on load
        }
        Ok(Arc::new(Self { inner: Mutex::new(Inner::new(db, keys, ledger)), net: Mutex::new(None), secrets }))
    }

    /// "Delete all my data" on THIS device (privacy contract, point 9): empties the database,
    /// re-encrypts it and replaces every key (database, signing, network, age), so nothing written
    /// before can be read again. The same `Core` goes on as a new device without a profile; the
    /// other devices keep their copy and will see this one go silent.
    /// It relaxes, so (plan 11b) it needs what relaxing needs: the local gate; with "changes need
    /// my contact", first stop needing them (their approval, or 7 days); a minor cannot (only the
    /// guardian, by removing the device management). Call it first for the device's own profile
    /// and, only if that worked, for the other databases (contact channels, minors' profiles).
    /// A device that was listening (the desktop service) listens again on the same port, new key.
    pub fn erase_all(self: Arc<Self>, clock: Clock) -> Result<(), CoreError> {
        let port = self.listening_port();
        self.stop_listening();
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        let now = clock.now_ms;
        match g.my_role() {
            Some(Role::Minor) => return Err(Denied::GuardianOnly.into()),
            Some(Role::Owner) => {
                if let Some(cfg) = h.active_at(now) {
                    if !g.gate_open(&cfg.auth.local, &f, now) {
                        return Err(Denied::AuthRequired.into());
                    }
                    if cfg.auth.contact_required {
                        return Err(Denied::NeedsContact.into());
                    }
                }
            }
            // No profile, a guardian's copy of a minor's profile, or my role as someone's contact.
            _ => {}
        }
        let path = g.db.path().ok_or_else(|| CoreError::storage("no database path"))?;
        // SECURITY: crash-safe order. A crash before the rekey leaves everything as it was; after
        // it, `open` finds the staged key and finishes (new identity, then the database key).
        let key = random_32();
        self.secrets.save(SECRET_DB_NEXT.into(), key.to_vec())?;
        g.db.wipe_and_rekey(&key[..])?;
        let keys = DeviceKeys::replace(self.secrets.as_ref())?;
        self.secrets.save(SECRET_DB.into(), key.to_vec())?;
        *g = Inner::new(crate::storage::Db::open(&path, &key[..])?, keys, Ledger::new(None));
        drop(g);
        match port {
            Some(port) => self.start_listening(port),
            None => Ok(()),
        }
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

    /// Proposes a new configuration. Returns when it takes effect (now, the next wake-up, or in
    /// 7 days when leaving the trusted contact without their approval).
    /// In a family profile: the guardian changes it immediately; the minor can only make it stricter.
    /// Adult: relaxing needs the local gate (`authorize_local`/`authorize_biometric` in the last
    /// 5 minutes) and, with "changes need my contact", their approval (`request_change`).
    pub fn set_config(&self, clock: Clock, config: Config) -> Result<i64, CoreError> {
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        let now = clock.now_ms;
        let from = match g.my_role() {
            Some(Role::Guardian) => now,
            Some(Role::Minor) if !h.minor_may_set(&config, now) => return Err(Denied::GuardianOnly.into()),
            Some(Role::Minor) => now,
            _ => match h.active_at(now) {
                Some(old) if !is_stricter_or_equal(&config, old) => {
                    if !g.gate_open(&old.auth.local, &f, now) {
                        return Err(Denied::AuthRequired.into());
                    }
                    if !old.auth.contact_required {
                        effective_from(&config, Some(old), clock)
                    } else if only_leaves_contact(&config, old) {
                        now + CONTACT_WAIT_MS
                    } else {
                        return Err(Denied::NeedsContact.into());
                    }
                }
                old => {
                    if config.auth.contact_required && g.members_with(Role::TrustedContact).next().is_none() {
                        return Err(Denied::NoContact.into());
                    }
                    effective_from(&config, old, clock)
                }
            },
        };
        g.record(now, EventKind::ConfigChanged { config, effective_from_ms: from })?;
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

    /// A place goal: `code` is the scanned QR. Errors: `denied.wrong_anchor` if it is another QR.
    pub fn complete_at_anchor(&self, clock: Clock, goal_id: String, code: String) -> Result<Status, CoreError> {
        let mut g = self.lock();
        let (h, f) = g.state(clock);
        let ev = rules::cmd_complete_at_anchor(&h, &f, clock, &goal_id, &code)?;
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

    // ───────────────────────── Peer sync (LAN) ─────────────────────────

    /// Declares that the user cannot change this device's clock (desktop service, minor's phone
    /// with Device Owner). Call at startup, before creating or joining a profile: it goes into the
    /// signed member list and makes this device's time win over the others'.
    pub fn set_time_source(&self, source: bool) -> Result<(), CoreError> {
        self.lock().db.set_meta(META_TIME_SOURCE, if source { "1" } else { "0" })
    }

    /// Accepts connections from the profile's devices on `port` (0 = any free port).
    /// The desktop service listens always; a phone only while showing a QR or in "available" mode.
    pub fn start_listening(self: Arc<Self>, port: u16) -> Result<(), CoreError> {
        let mut slot = self.net_slot();
        if slot.as_ref().is_some_and(|n| n.listening) {
            return Ok(());
        }
        // The old endpoint (if any) goes first: the new one may need the same port.
        *slot = None;
        let key = *self.lock().keys.net;
        let net = Arc::new(PeerNet::bind(&key, port, true)?);
        net.spawn_accept_loop(Arc::downgrade(&self));
        *slot = Some(net);
        Ok(())
    }

    pub fn stop_listening(&self) {
        *self.net_slot() = None;
    }

    /// UDP port this device listens on (to announce it over mDNS), if it is listening.
    pub fn listening_port(&self) -> Option<u16> {
        let slot = self.net_slot();
        let net = slot.as_ref().filter(|n| n.listening)?;
        net.ep.bound_sockets().into_iter().find(|a| a.is_ipv4()).map(|a| a.port())
    }

    /// Syncs with every device of the profile whose address is known (QR, mDNS) and re-anchors
    /// trusted time with their clocks. Blocking: call from a background thread. Unreachable
    /// devices are skipped (outside home is normal); the report says how many answered.
    /// `mono_ms`: SystemClock.elapsedRealtime() on Android, GetTickCount64() on Windows.
    pub fn sync_now(&self, device_now_ms: i64, mono_ms: i64) -> Result<SyncReport, CoreError> {
        let started = Instant::now();
        let now = self.trusted_now_ms(device_now_ms, mono_ms);
        let (targets, hello, profile) = {
            let g = self.lock();
            let (hello, profile) = g.hello(now)?;
            let targets: Vec<(Peer, Vec<SocketAddr>)> =
                g.peers().into_iter().filter_map(|p| g.addrs.get(&p.node_id).map(|a| (p, a.clone()))).collect();
            (targets, hello, profile)
        };
        let mut report = SyncReport::default();
        if !targets.is_empty() {
            let net = self.net()?;
            for (peer, addrs) in targets {
                let session_start = started.elapsed().as_millis() as i64;
                let Ok(ex) = self.sync_with(&net, &peer, &addrs, &hello, &profile) else { continue };
                let mut g = self.lock();
                let mut got = g.ingest(ex.blobs)?;
                got.peers = 1;
                got.sent = ex.sent;
                report.add(&got);
                // Its signed time, with our monotonic clock at the moment its `Hello` arrived.
                if let Some(dev) = &peer.device_id
                    && ex.time.unix_ms > 0
                    && verify_signed_time(&ex.time, dev)
                {
                    let mono = mono_ms + session_start + ex.hello_after_ms;
                    let sample =
                        Sample { trusted_ms: ex.time.unix_ms, mono_ms: mono, from_time_source: peer.time_source };
                    g.samples.insert(peer.node_id.clone(), sample);
                }
            }
        }
        let elapsed = started.elapsed().as_millis() as i64;
        let mut g = self.lock();
        g.reanchor(device_now_ms + elapsed, mono_ms + elapsed)?;
        let now = corrected_now(g.anchor, device_now_ms + elapsed, mono_ms + elapsed);
        g.silent_notices(now)?;
        g.db.prune_before(now - RETENTION_DAYS * DAY_MS, structural)?;
        Ok(report)
    }

    /// This device's mDNS instance name today (None until it belongs to a profile).
    /// Service type: `_pco._udp`, no TXT records.
    pub fn discovery_tag(&self, now_ms: i64) -> Option<String> {
        let g = self.lock();
        let key = g.discovery_key()?;
        Some(peer::discovery_tag(&key, &g.keys.node_id(), now_ms.div_euclid(DAY_MS)))
    }

    /// An mDNS service `_pco._udp` was found at `host:port`. If its name is the tag of a device of
    /// this profile (yesterday, today or tomorrow: clocks drift), its address is remembered for
    /// `sync_now`. `host` must be an IP address on the local network.
    pub fn lan_peer_seen(&self, now_ms: i64, tag: String, host: String, port: u16) -> bool {
        let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() else { return false };
        if !pairing::is_local(ip) || port == 0 {
            return false;
        }
        let mut g = self.lock();
        let Some(key) = g.discovery_key() else { return false };
        let me = g.keys.node_id();
        let day = now_ms.div_euclid(DAY_MS);
        let found = g.ledger.members().iter().find_map(|m| {
            let hit = !m.node_id.is_empty()
                && m.node_id != me
                && (day - 1..=day + 1).any(|d| peer::discovery_tag(&key, &m.node_id, d) == tag);
            hit.then(|| m.node_id.clone())
        });
        let Some(node) = found else { return false };
        g.addrs.insert(node, vec![SocketAddr::new(ip, port)]);
        true
    }

    /// Notices not yet delivered (requests, answers, alerts, silent devices); marks them delivered.
    /// The platform shows them as local notifications with generic text.
    pub fn take_notices(&self) -> Result<Vec<Notice>, CoreError> {
        self.lock().db.take_notices()
    }

    // ───────────────────────── Encrypted backup ─────────────────────────

    /// This device's keys and events, encrypted with `passphrase` (use `new_backup_passphrase`
    /// and show it once). The platform saves the file where the user chooses.
    pub fn export_backup(&self, passphrase: String) -> Result<Vec<u8>, CoreError> {
        let c = {
            let g = self.lock();
            crate::backup::contents(
                &g.keys,
                g.db.meta(META_GENESIS)?,
                g.db.meta(META_KIND)?,
                g.time_source(),
                g.db.all()?,
            )
        };
        crate::backup::seal(&c, &passphrase)
    }

    /// Restores a backup on a device without a profile: it becomes the same device (same keys).
    /// Use the returned `Core` instead of any opened before; remove the old device from the profile.
    /// Nothing is written unless the whole backup is valid.
    #[uniffi::constructor]
    pub fn restore_backup(
        db_path: String,
        secrets: Arc<dyn SecretStore>,
        data: Vec<u8>,
        passphrase: String,
    ) -> Result<Arc<Self>, CoreError> {
        let c = crate::backup::open(&data, &passphrase)?;
        let db = crate::storage::Db::open(&db_path, &db_key(secrets.as_ref())?)?;
        if !db.all()?.is_empty() {
            return Err(CoreError::protocol("this device already has a profile"));
        }
        // Validate everything before touching the keystore.
        let signing: zeroize::Zeroizing<Vec<u8>> =
            zeroize::Zeroizing::new(hex::decode(&c.signing).map_err(CoreError::crypto)?);
        let net: zeroize::Zeroizing<Vec<u8>> = zeroize::Zeroizing::new(hex::decode(&c.net).map_err(CoreError::crypto)?);
        let age_id: age::x25519::Identity = c.age.parse().map_err(|_| CoreError::crypto("invalid age identity"))?;
        if signing.len() != 32 || net.len() != 32 {
            return Err(CoreError::crypto("invalid key"));
        }
        let mut ledger = Ledger::new(c.genesis.clone());
        for ev in &c.events {
            ledger.accept(ev.clone())?;
        }
        let me = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&signing[..].try_into().map_err(CoreError::crypto)?)
                .verifying_key()
                .to_bytes(),
        );
        if !ledger.members().iter().any(|m| m.device_id == me) {
            return Err(CoreError::protocol("the backup's device is not a member of its profile"));
        }
        secrets.save(crate::crypto::SECRET_SIGNING.into(), signing.to_vec())?;
        secrets.save(crate::crypto::SECRET_NET.into(), net.to_vec())?;
        {
            use secrecy::ExposeSecret;
            secrets.save(crate::crypto::SECRET_AGE.into(), age_id.to_string().expose_secret().as_bytes().to_vec())?;
        }
        if let Some(g) = &c.genesis {
            db.set_meta(META_GENESIS, g)?;
        }
        if let Some(k) = &c.kind {
            db.set_meta(META_KIND, k)?;
        }
        db.set_meta(META_TIME_SOURCE, if c.time_source { "1" } else { "0" })?;
        for ev in &c.events {
            db.insert(ev)?;
        }
        drop(db);
        Self::open(db_path, secrets)
    }

    // ───────────────────────── Pairing ─────────────────────────

    /// (A) Generates the QR content. Starts listening if it was not; valid for 10 minutes.
    pub fn pairing_offer(self: Arc<Self>) -> Result<String, CoreError> {
        self.clone().start_listening(0)?;
        let addrs = self.net()?.local_addrs();
        if addrs.is_empty() {
            return Err(CoreError::network("no local network"));
        }
        let mut g = self.lock();
        if g.ledger.members().is_empty() {
            return Err(CoreError::protocol("no profile"));
        }
        let mut t = [0u8; 16];
        OsRng.fill_bytes(&mut t);
        let eph = age::x25519::Identity::generate();
        // The joiner must trust the genesis author, even if another device of the profile invites it.
        let inviter = g.db.meta(META_GENESIS)?.unwrap_or_else(|| g.keys.device_id());
        let offer = PairingOffer {
            v: pairing::OFFER_VERSION,
            token: hex::encode(t),
            eph: eph.to_public().to_string(),
            inviter,
            node_id: g.keys.node_id(),
            addrs,
        };
        let qr = serde_json::to_string(&offer).map_err(CoreError::protocol)?;
        g.pairing = Some(PendingPairing { offer, eph, joiner: None, created: Instant::now() });
        Ok(qr)
    }

    /// (B) Processes the scanned QR and sends the request to A. Returns the 6-digit code to
    /// compare. Then call `sync_now` until `members()` is not empty (A confirmed).
    pub fn pairing_join(&self, qr: String, label: String) -> Result<String, CoreError> {
        check_label(&label)?;
        let offer = pairing::parse_offer(&qr)?;
        let addrs: Vec<SocketAddr> = offer.addrs.iter().filter_map(|a| a.parse().ok()).collect();
        let req = {
            let mut g = self.lock();
            if !g.ledger.members().is_empty() {
                return Err(CoreError::protocol("this device already belongs to a profile"));
            }
            g.db.set_meta(META_GENESIS, &offer.inviter)?;
            g.ledger.set_genesis_author(offer.inviter.clone());
            g.join_peer = Some(offer.node_id.clone());
            g.addrs.insert(offer.node_id.clone(), addrs.clone());
            JoinRequest {
                device_id: g.keys.device_id(),
                age_recipient: g.keys.age_recipient(),
                label,
                node_id: g.keys.node_id(),
                time_source: g.time_source(),
            }
        };
        let join =
            Msg::Join { token: offer.token.clone(), blob: peer::encode_blob(&pairing::seal_join(&req, &offer.eph)?) };
        let net = self.net()?;
        net.block_on(async {
            let conn = net.connect(&offer.node_id, &addrs, proto::PAIR_ALPN).await?;
            let (mut send, mut recv) = conn.open_bi().await.map_err(CoreError::network)?;
            proto::write_msg(&mut send, &join).await.map_err(CoreError::network)?;
            send.finish().map_err(CoreError::network)?;
            let ack = tokio::time::timeout(Duration::from_secs(10), proto::read_msg(&mut recv)).await;
            conn.close(0u32.into(), b"");
            match ack {
                Ok(Ok(Msg::Done)) => Ok(()),
                _ => Err(CoreError::protocol("the other device did not accept the request")),
            }
        })?;
        Ok(pairing::sas(&offer, &req))
    }

    /// (A) Whether B has answered. Returns the 6-digit code once its request arrived.
    pub fn pairing_poll(&self) -> Result<Option<String>, CoreError> {
        let g = self.lock();
        let p = g.pairing.as_ref().ok_or_else(|| CoreError::protocol("no pairing in progress"))?;
        Ok(p.joiner.as_ref().map(|req| pairing::sas(&p.offer, req)))
    }

    /// (A) The user confirmed that the 6 digits match. `role`: in a family profile, `Minor`
    /// (the minor's device) or `Guardian` (another guardian); otherwise `None`.
    /// B then fetches the history with its next `sync_now`.
    pub fn pairing_confirm(&self, clock: Clock, role: Option<Role>) -> Result<(), CoreError> {
        let mut g = self.lock();
        // In the contact channel the joiner is the trusted contact; in a personal profile, another own device.
        let channel = g.is_contact_channel();
        let role = match (channel, g.ledger.is_family(), role) {
            (true, _, None | Some(Role::TrustedContact)) => Role::TrustedContact,
            (false, true, Some(r @ (Role::Guardian | Role::Minor))) => r,
            (false, false, None | Some(Role::Owner)) => Role::Owner,
            _ => return Err(CoreError::protocol("invalid role for this profile")),
        };
        let p = g.pairing.take().ok_or_else(|| CoreError::protocol("no pairing in progress"))?;
        let joiner = p.joiner.ok_or_else(|| CoreError::protocol("the other device has not answered yet"))?;
        let mut members = g.ledger.members().to_vec();
        members.retain(|m| m.device_id != joiner.device_id);
        members.push(Member {
            device_id: joiner.device_id,
            age_recipient: joiner.age_recipient,
            label: joiner.label,
            role,
            node_id: joiner.node_id,
            time_source: joiner.time_source,
        });
        g.record(clock.now_ms, EventKind::MembersChanged { members })
    }

    /// (A) Cancels the pairing in progress (the digits do not match).
    pub fn pairing_cancel(&self) {
        self.lock().pairing = None;
    }

    // ───────────────────────── Trusted time ─────────────────────────

    /// Time the platforms must use to build `Clock`: the anchor from the last `sync_now` + the
    /// monotonic clock. `mono_ms`: SystemClock.elapsedRealtime() on Android, GetTickCount64() on Windows.
    pub fn trusted_now_ms(&self, device_now_ms: i64, mono_ms: i64) -> i64 {
        let mut g = self.lock();
        let now = corrected_now(g.anchor, device_now_ms, mono_ms);
        g.last_now = Some((now, Instant::now()));
        now
    }

    // ───────────────────────── Heartbeats ─────────────────────────

    /// When each device of the profile was last seen, and its longest silence this week.
    pub fn devices_seen(&self, now_ms: i64) -> Vec<DeviceSeen> {
        let g = self.lock();
        let activity = g.activity(now_ms);
        let me = g.keys.device_id();
        g.ledger
            .members()
            .iter()
            .filter(|m| m.role != Role::TrustedContact) // a person, not a device of the profile
            .map(|m| {
                let (last_seen_ms, longest_gap_ms) = activity.get(&m.device_id).copied().unwrap_or((0, 0));
                DeviceSeen {
                    device_id: m.device_id.clone(),
                    label: m.label.clone(),
                    last_seen_ms,
                    longest_gap_ms,
                    is_me: m.device_id == me,
                }
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
        g.record(clock.now_ms, EventKind::Decision { request_id, approved, config_hash: None, effective: None })?;
        if !approved {
            return Ok(());
        }
        let effect = match what {
            RequestKind::ConfigChange { .. } => return Ok(()), // only asked of a trusted contact
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
                    EventKind::LocalGrant { what, .. } => FamilyAlert::Granted { what },
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

    // ───────────────────────── Support ─────────────────────────

    /// Technical report for support (plan 10 · 4): the user reads it and decides whether to send
    /// it. `platform`: lines from the app ("accessibility=on", last error codes…).
    /// SECURITY: only versions, states and counts. Never goals, names, labels, domains, keys or
    /// device ids; the platform lines must follow the same rule.
    pub fn diagnostic_report(&self, clock: Clock, platform: Vec<String>) -> String {
        let g = self.lock();
        let role = g.my_role().map_or("none".to_string(), |r| format!("{r:?}"));
        let kind = if g.is_contact_channel() {
            "contact-channel"
        } else if g.ledger.is_family() {
            "family"
        } else {
            "personal"
        };
        let mut out = vec![
            format!("core={} protocol={}", env!("CARGO_PKG_VERSION"), proto::PROTOCOL_VERSION),
            format!("profile={kind} role={role} members={}", g.ledger.members().len()),
        ];
        if let Some(st) = g.status(clock) {
            let state = match st.state {
                DayState::Sleep => "sleep",
                DayState::Locked => "locked",
                DayState::Pass { .. } => "pass",
                DayState::Focus { .. } => "focus",
                DayState::Free => "free",
                DayState::Emergency { .. } => "emergency",
            };
            out.push(format!(
                "state={state} needs_plan={} free_day={} permits_left={} goals={}",
                st.needs_plan,
                st.free_day,
                st.permits_left,
                st.goals.len()
            ));
        }
        let (h, f) = g.state(clock);
        if let Some(cfg) = h.active_at(clock.now_ms) {
            let methods: Vec<String> = cfg.auth.local.iter().map(|m| format!("{m:?}")).collect();
            out.push(format!("auth={} contact_required={}", methods.join("+"), cfg.auth.contact_required));
        }
        let secrets = Secrets::from_facts(&f, clock.now_ms);
        out.push(format!(
            "locked_attempts={} reset_pending={} time_anchored={} clock_skewed={} gaps={}",
            g.meta_i64(META_LOCKED_UNTIL) > clock.now_ms,
            secrets.reset_at_ms.is_some(),
            g.anchor.is_some(),
            g.clock_skewed,
            g.ledger.gaps.len()
        ));
        let me = g.keys.device_id();
        let mut seen: Vec<String> = g
            .activity(clock.now_ms)
            .into_iter()
            .filter(|(author, _)| *author != me)
            .map(|(_, (last, _))| format!("{}h", (clock.now_ms - last).max(0) / (60 * MINUTE_MS)))
            .collect();
        seen.sort();
        out.push(format!("other_devices_last_seen=[{}]", seen.join(",")));
        out.extend(platform);
        out.join("\n")
    }

    // ───────────────────────── Change authorization (ADR 0013) ─────────────────────────

    /// Methods, attempts and contact for the settings screen.
    pub fn auth_state(&self, clock: Clock) -> AuthState {
        let g = self.lock();
        let (h, f) = g.state(clock);
        let policy = h.active_at(clock.now_ms).map(|c| c.auth.clone()).unwrap_or_default();
        let s = Secrets::from_facts(&f, clock.now_ms);
        let guardian_methods = s.phc.keys().filter(|(_, h)| *h == SecretHolder::Guardian).map(|(f, _)| *f).collect();
        let locked = g.meta_i64(META_LOCKED_UNTIL);
        AuthState {
            usable: s.usable(&policy.local),
            unlocked: g.gate_open(&policy.local, &f, clock.now_ms),
            methods: policy.local,
            guardian_methods,
            locked_until_ms: if locked > clock.now_ms { locked } else { 0 },
            reset_at_ms: s.reset_at_ms,
            has_recovery_code: s.recovery.is_some(),
            contact_required: policy.contact_required,
            contacts: g.members_with(Role::TrustedContact).count() as u32,
        }
    }

    /// Sets a PIN, password or pattern ("0-4-8-5": cells of a 3×3 grid). `Owner`: this profile's
    /// user (also adds the method to the policy: tightening); replacing one in use needs the gate.
    /// `Guardian`: from a guardian's device, the secret it can type on the minor's device.
    pub fn set_secret(
        &self,
        clock: Clock,
        factor: LocalFactor,
        secret: String,
        holder: SecretHolder,
    ) -> Result<(), CoreError> {
        let secret = zeroize::Zeroizing::new(secret);
        auth::check_new_secret(factor, &secret)?;
        {
            let g = self.lock();
            match (g.my_role(), holder) {
                (Some(Role::Guardian), SecretHolder::Guardian) => {}
                (Some(Role::Owner | Role::Minor), SecretHolder::Owner) => {
                    let (h, f) = g.state(clock);
                    let policy = h.active_at(clock.now_ms).map(|c| c.auth.local.clone()).unwrap_or_default();
                    if !g.gate_open(&policy, &f, clock.now_ms) {
                        return Err(Denied::AuthRequired.into());
                    }
                }
                _ => return Err(CoreError::protocol("invalid secret holder for this device")),
            }
        }
        let phc = auth::hash_secret(factor, &secret)?; // slow on purpose: outside the lock
        let mut g = self.lock();
        g.record(clock.now_ms, EventKind::SecretSet { factor, holder, phc })?;
        if holder == SecretHolder::Owner {
            let (h, _) = g.state(clock);
            if let Some(mut cfg) = h.active_at(clock.now_ms).cloned()
                && !cfg.auth.local.contains(&factor)
            {
                cfg.auth.local.push(factor);
                g.record(clock.now_ms, EventKind::ConfigChanged { config: cfg, effective_from_ms: clock.now_ms })?;
            }
            g.authorized_at = Some(clock.now_ms); // whoever just set it knows it
        }
        Ok(())
    }

    /// Opens the local gate for 5 minutes with a PIN, password or pattern.
    /// Errors: `denied.wrong_secret`, `denied.locked` (with the minutes), `denied.method_not_set`.
    pub fn authorize_local(&self, clock: Clock, factor: LocalFactor, secret: String) -> Result<(), CoreError> {
        let secret = zeroize::Zeroizing::new(secret);
        let phc = {
            let g = self.lock();
            g.check_not_locked(clock.now_ms)?;
            let (h, f) = g.state(clock);
            let policy = h.active_at(clock.now_ms).map(|c| c.auth.local.clone()).unwrap_or_default();
            let s = Secrets::from_facts(&f, clock.now_ms);
            match s.phc.get(&(factor, SecretHolder::Owner)) {
                Some(phc) if s.usable(&policy).contains(&factor) => phc.clone(),
                _ => return Err(Denied::MethodNotSet.into()),
            }
        };
        let ok = auth::verify_secret(factor, &secret, &phc);
        let mut g = self.lock();
        g.note_attempt(clock.now_ms, ok)?;
        g.authorized_at = Some(clock.now_ms);
        Ok(())
    }

    /// Opens the local gate after the OS confirmed biometrics. Android: only after a
    /// `BiometricPrompt` with a `CryptoObject` succeeded (see LocalAuth.kt).
    pub fn authorize_biometric(&self, clock: Clock) -> Result<(), CoreError> {
        let mut g = self.lock();
        let (h, _) = g.state(clock);
        if !h.active_at(clock.now_ms).is_some_and(|c| c.auth.local.contains(&LocalFactor::Biometric)) {
            return Err(Denied::MethodNotSet.into());
        }
        g.authorized_at = Some(clock.now_ms);
        Ok(())
    }

    /// Closes the local gate (leaving settings).
    pub fn lock_settings(&self) {
        self.lock().authorized_at = None;
    }

    /// (Adult) A new recovery code for the local methods: show it once to print or write down.
    /// Replacing an existing one needs the gate.
    pub fn create_recovery_code(&self, clock: Clock) -> Result<String, CoreError> {
        {
            let g = self.lock();
            if g.my_role() != Some(Role::Owner) {
                return Err(CoreError::protocol("only an adult's own device has a recovery code"));
            }
            let (h, f) = g.state(clock);
            let policy = h.active_at(clock.now_ms).map(|c| c.auth.local.clone()).unwrap_or_default();
            if Secrets::from_facts(&f, clock.now_ms).recovery.is_some() && !g.gate_open(&policy, &f, clock.now_ms) {
                return Err(Denied::AuthRequired.into());
            }
        }
        let code = auth::new_recovery_code();
        let phc = auth::hash_recovery(&code)?;
        self.lock().record(clock.now_ms, EventKind::RecoveryCodeSet { phc })?;
        Ok(code)
    }

    /// (Adult) Forgot the PIN/password/pattern: with the recovery code, the local methods reset
    /// in 72 h (the trusted contact is told). It never skips the contact's approval.
    /// Returns when the reset applies.
    pub fn use_recovery_code(&self, clock: Clock, code: String) -> Result<i64, CoreError> {
        let phc = {
            let g = self.lock();
            g.check_not_locked(clock.now_ms)?;
            if g.my_role() != Some(Role::Owner) {
                return Err(CoreError::protocol("only an adult's own device has a recovery code"));
            }
            let (_, f) = g.state(clock);
            Secrets::from_facts(&f, clock.now_ms).recovery.ok_or(Denied::MethodNotSet)?
        };
        let ok = auth::verify_recovery(&code, &phc);
        let mut g = self.lock();
        g.note_attempt(clock.now_ms, ok)?;
        g.record(clock.now_ms, EventKind::LocalResetStarted)?;
        Ok(clock.now_ms + auth::RESET_DELAY_MS)
    }

    // ───────────────────────── Offline codes and the guardian in person ─────────────────────────

    /// (Minor, or adult with a trusted contact) A request code to dictate to the approver, for a
    /// small concession. Only one at a time: a new one replaces the previous.
    pub fn offline_request(&self, clock: Clock, ask: OfflineAsk) -> Result<String, CoreError> {
        if !offline_code::ask_in_range(&ask) {
            return Err(CoreError::protocol("request out of range"));
        }
        let g = self.lock();
        match (g.my_role(), g.ledger.is_family()) {
            (Some(Role::Minor), true) => {}
            (Some(Role::Owner), false) if g.members_with(Role::TrustedContact).next().is_some() => {}
            (Some(Role::Owner), false) => return Err(Denied::NoContact.into()),
            _ => return Err(CoreError::protocol("this device does not ask for concessions")),
        }
        if g.grants_today(clock) >= offline_code::MAX_PER_DAY {
            return Err(Denied::TooManyGrants.into());
        }
        let code = offline_code::request_code(&ask, OsRng.next_u32());
        let pending = PendingCode { code: code.clone(), ask, created_ms: clock.now_ms, attempts: 0 };
        g.db.set_meta(META_OFFLINE, &serde_json::to_string(&pending).map_err(CoreError::storage)?)?;
        Ok(code)
    }

    /// (Guardian in the minor's profile, or trusted contact in the channel) What a request code
    /// asks for and the 8 digits to read back. Show `ask` and confirm with biometrics/PIN first.
    pub fn offline_answer(&self, request_code: String) -> Result<OfflineAnswer, CoreError> {
        let (ask, _, canonical) = offline_code::parse_request(&request_code).ok_or(Denied::InvalidCode)?;
        let g = self.lock();
        let me = g.keys.device_id();
        let (_, key, profile) = g
            .approvers()
            .into_iter()
            .find(|(id, _, _)| *id == me)
            .ok_or_else(|| CoreError::protocol("not an approver"))?;
        Ok(OfflineAnswer { ask, response: offline_code::response_code(&key, &profile, &canonical) })
    }

    /// (The device that asked) Checks the 8 digits and grants the concession. `channel`: the
    /// trusted contact channel (adults); `None` in a minor's profile. 3 wrong answers or 24 h
    /// cancel the request.
    pub fn offline_redeem(
        &self,
        clock: Clock,
        response: String,
        channel: Option<Arc<Core>>,
    ) -> Result<Status, CoreError> {
        let digits: String = response.chars().filter(char::is_ascii_digit).collect();
        let mut candidates = match &channel {
            Some(ch) if !std::ptr::eq(self, ch.as_ref()) => ch.lock().approvers(),
            _ => vec![],
        };
        let mut g = self.lock();
        candidates.extend(g.approvers());
        let pending: PendingCode =
            g.db.meta(META_OFFLINE)?.and_then(|j| serde_json::from_str(&j).ok()).ok_or(Denied::InvalidCode)?;
        if clock.now_ms - pending.created_ms > offline_code::REQUEST_TTL_MS {
            g.db.set_meta(META_OFFLINE, "")?;
            return Err(Denied::InvalidCode.into());
        }
        let hit = candidates
            .into_iter()
            .find(|(_, key, profile)| offline_code::response_code(key, profile, &pending.code) == digits);
        let Some((approver, _, _)) = hit else {
            let attempts = pending.attempts + 1;
            let left = PendingCode { attempts, ..pending };
            let keep = if attempts >= offline_code::MAX_ATTEMPTS {
                String::new()
            } else {
                serde_json::to_string(&left).map_err(CoreError::storage)?
            };
            g.db.set_meta(META_OFFLINE, &keep)?;
            return Err(Denied::InvalidCode.into());
        };
        if g.grants_today(clock) >= offline_code::MAX_PER_DAY {
            return Err(Denied::TooManyGrants.into());
        }
        g.db.set_meta(META_OFFLINE, "")?; // each nonce once
        let grant = EventKind::LocalGrant {
            what: pending.ask,
            request_code: auth::normalize_code(&pending.code),
            proof: GrantProof::OfflineCode { approver },
        };
        g.record(clock.now_ms, grant)?;
        g.must_status(clock)
    }

    /// (Minor's device) "My guardian is here": the guardian types THEIR PIN/password/pattern.
    /// Same limits as the codes. The tablet's biometrics never counts: the OS cannot tell whose
    /// finger it is.
    pub fn grant_in_person(
        &self,
        clock: Clock,
        factor: LocalFactor,
        secret: String,
        ask: OfflineAsk,
    ) -> Result<Status, CoreError> {
        let secret = zeroize::Zeroizing::new(secret);
        if !offline_code::ask_in_range(&ask) {
            return Err(CoreError::protocol("request out of range"));
        }
        let phc = {
            let g = self.lock();
            g.check_not_locked(clock.now_ms)?;
            if g.my_role() != Some(Role::Minor) {
                return Err(CoreError::protocol("only on a minor's device"));
            }
            if g.grants_today(clock) >= offline_code::MAX_PER_DAY {
                return Err(Denied::TooManyGrants.into());
            }
            let (_, f) = g.state(clock);
            Secrets::from_facts(&f, clock.now_ms)
                .phc
                .remove(&(factor, SecretHolder::Guardian))
                .ok_or(Denied::MethodNotSet)?
        };
        let ok = auth::verify_secret(factor, &secret, &phc);
        let mut g = self.lock();
        g.note_attempt(clock.now_ms, ok)?;
        let code = offline_code::request_code(&ask, OsRng.next_u32());
        let grant = EventKind::LocalGrant {
            what: ask,
            request_code: auth::normalize_code(&code),
            proof: GrantProof::GuardianSecret,
        };
        g.record(clock.now_ms, grant)?;
        g.must_status(clock)
    }

    // ───────────────────────── Trusted contact ─────────────────────────

    /// Creates, in THIS `Core` (another database), the channel shared with your trusted contacts.
    /// It has no configuration: requests, their answers and alerts (tampering, emergency, summary).
    pub fn create_contact_channel(&self, clock: Clock, label: String) -> Result<(), CoreError> {
        check_label(&label)?;
        let mut g = self.lock();
        if !g.ledger.members().is_empty() {
            return Err(CoreError::protocol("the channel already exists"));
        }
        g.db.set_meta(META_KIND, KIND_CONTACT)?;
        let member = g.me_as_member(label, Role::Owner);
        g.found(clock.now_ms, member)
    }

    /// (Adult with "changes need my contact") Sends a relaxing configuration to the contacts for
    /// approval: they only see the typed lines (`Change`). Returns the request id. Once a contact
    /// approves, `forward_to_contact` applies it.
    pub fn request_change(&self, clock: Clock, config: Config, channel: Arc<Core>) -> Result<String, CoreError> {
        if std::ptr::eq(self, channel.as_ref()) {
            return Err(CoreError::protocol("the channel must be a different core"));
        }
        let what = {
            let g = self.lock();
            if g.my_role() != Some(Role::Owner) {
                return Err(CoreError::protocol("only an adult asks their trusted contact"));
            }
            let (h, _) = g.state(clock);
            let cur = h.active_at(clock.now_ms).ok_or_else(|| CoreError::protocol("no configuration"))?;
            let config_hash = config.hash();
            g.db.set_meta(
                &format!("{META_CHANGE}{config_hash}"),
                &serde_json::to_string(&config).map_err(CoreError::storage)?,
            )?;
            RequestKind::ConfigChange { changes: cur.diff(&config), config_hash }
        };
        let mut c = channel.lock();
        if !c.is_contact_channel() || c.my_role() != Some(Role::Owner) {
            return Err(CoreError::protocol("not your contact channel"));
        }
        if c.pending_requests().len() >= MAX_OPEN_REQUESTS {
            return Err(Denied::TooManyRequests.into());
        }
        let mut id = [0u8; 8];
        OsRng.fill_bytes(&mut id);
        let request_id = hex::encode(id);
        c.record(clock.now_ms, EventKind::Request { request_id: request_id.clone(), what })?;
        Ok(request_id)
    }

    /// (Trusted contact, in the channel) Approves or rejects a requested change and, if approved,
    /// when it applies. Confirm with biometrics/PIN in the app first.
    pub fn contact_decide(
        &self,
        clock: Clock,
        request_id: String,
        approved: bool,
        effective: Effective,
    ) -> Result<(), CoreError> {
        let mut g = self.lock();
        if g.my_role() != Some(Role::TrustedContact) {
            return Err(CoreError::protocol("only a trusted contact answers here"));
        }
        let hash = match g.pending_requests().into_iter().find(|r| r.request_id == request_id).map(|r| r.what) {
            Some(RequestKind::ConfigChange { config_hash, .. }) => config_hash,
            _ => return Err(CoreError::protocol("request not found or already answered")),
        };
        let decision = EventKind::Decision {
            request_id,
            approved,
            config_hash: Some(hash),
            effective: approved.then_some(effective),
        };
        g.record(clock.now_ms, decision)
    }

    /// Keeps the profile and the contact channel in step. Idempotent: call after each sync.
    /// 1. Lists the channel's contacts in the profile (only to check their signatures).
    /// 2. Applies the changes a contact approved (`ContactApproval` + `ConfigChanged`).
    /// 3. Copies to the channel ONLY tampering, emergencies, codes used, resets, the exit and each
    ///    finished day's summary (numbers). Returns how many alerts it added.
    pub fn forward_to_contact(&self, channel: Arc<Core>, clock: Clock) -> Result<u32, CoreError> {
        if std::ptr::eq(self, channel.as_ref()) {
            return Err(CoreError::protocol("the contact channel must be a different one"));
        }
        let (since, contacts, decisions) = {
            let c = channel.lock();
            if !c.is_contact_channel() {
                return Err(CoreError::protocol("not a contact channel"));
            }
            let contacts: Vec<Member> = c.members_with(Role::TrustedContact).cloned().collect();
            let decisions: Vec<SignedEvent> =
                c.db.all()?
                    .into_iter()
                    .filter(|e| {
                        contacts.iter().any(|m| m.device_id == e.body.author)
                            && matches!(
                                e.body.kind,
                                EventKind::Decision { approved: true, config_hash: Some(_), effective: Some(_), .. }
                            )
                    })
                    .collect();
            (c.ledger.facts().first().map(|f| f.at_ms).unwrap_or(i64::MAX), contacts, decisions)
        };
        let alerts = {
            let mut g = self.lock();
            if g.my_role() != Some(Role::Owner) || g.ledger.is_family() {
                return Err(CoreError::protocol("only an adult's profile has trusted contacts"));
            }
            // 1. Contacts (adding one tightens; it can approve at once only if added before
            //    "changes need my contact" was on: rules.rs).
            let mut members = g.ledger.members().to_vec();
            let before = members.len();
            for m in &contacts {
                if !members.iter().any(|x| x.device_id == m.device_id) {
                    members.push(Member { node_id: String::new(), time_source: false, ..m.clone() });
                }
            }
            if members.len() != before {
                g.record(clock.now_ms, EventKind::MembersChanged { members })?;
            }
            // 2. Approved changes, each decision once.
            for d in decisions {
                let EventKind::Decision { config_hash: Some(hash), .. } = &d.body.kind else { continue };
                let applied = format!("{META_APPLIED}{}", d.id());
                if g.db.meta(&applied)?.is_some() {
                    continue;
                }
                let key = format!("{META_CHANGE}{hash}");
                let Some(config) = g.db.meta(&key)?.and_then(|j| serde_json::from_str::<Config>(&j).ok()) else {
                    continue;
                };
                let decision = serde_json::to_string(&d).map_err(CoreError::storage)?;
                g.record(clock.now_ms, EventKind::ContactApproval { decision })?;
                g.record(clock.now_ms, EventKind::ConfigChanged { config, effective_from_ms: clock.now_ms })?;
                g.db.set_meta(&applied, "1")?;
                g.db.set_meta(&key, "")?;
            }
            // 3. Alerts
            let (h, f) = g.state(clock);
            let window_start = since.max(clock.now_ms - FORWARD_WINDOW_DAYS * DAY_MS);
            let mut v: Vec<(i64, ContactAlert)> = Vec::new();
            let (mut required, mut approved) = (false, false);
            for x in f.iter().filter(|x| x.at_ms <= clock.now_ms) {
                let alert = match &x.kind {
                    EventKind::Tamper { what } => Some(ContactAlert::Tamper { what: *what }),
                    EventKind::EmergencyStarted => Some(ContactAlert::Emergency),
                    EventKind::LocalGrant { .. } => Some(ContactAlert::CodeGrant),
                    EventKind::LocalResetStarted => Some(ContactAlert::LocalReset),
                    EventKind::ConfigChanged { config, .. } => {
                        let exit = required && !config.auth.contact_required && !approved;
                        required = config.auth.contact_required;
                        exit.then_some(ContactAlert::ExitScheduled)
                    }
                    _ => None,
                };
                approved = matches!(x.kind, EventKind::ContactApproval { .. });
                if let Some(a) = alert
                    && x.at_ms >= window_start
                {
                    v.push((x.at_ms, a));
                }
            }
            if let Some(today) = rules::status(&h, &f, clock) {
                for k in 0..FORWARD_WINDOW_DAYS {
                    let past = Clock { now_ms: today.day_start_ms - k * DAY_MS - 1, ..clock };
                    let Some(day) = rules::status(&h, &f, past) else { continue };
                    if day.goals.is_empty() || day.day_start_ms + DAY_MS <= since {
                        continue; // free day, no goals, or before the channel existed
                    }
                    let completed = day.goals.iter().filter(|g| g.completed).count().min(255) as u8;
                    let total = day.goals.len().min(255) as u8;
                    v.push((day.day_start_ms, ContactAlert::DailySummary { completed, total }));
                }
            }
            v
        };
        let mut c = channel.lock();
        let have: Vec<(i64, ContactAlert)> = c
            .ledger
            .facts()
            .into_iter()
            .filter_map(|f| match f.kind {
                EventKind::ForContact { happened_at_ms, alert } => Some((happened_at_ms, alert)),
                _ => None,
            })
            .collect();
        let mut added = 0;
        for (happened_at_ms, alert) in alerts {
            if !have.contains(&(happened_at_ms, alert.clone())) {
                c.record(clock.now_ms, EventKind::ForContact { happened_at_ms, alert })?;
                added += 1;
            }
        }
        Ok(added)
    }

    /// (Trusted contact) Alerts received from the person you support, newest first.
    pub fn contact_alerts(&self) -> Vec<ContactAlertView> {
        let g = self.lock();
        let members = g.ledger.members();
        let mut out: Vec<ContactAlertView> = g
            .ledger
            .authored()
            .into_iter()
            .filter_map(|(author, f)| {
                let owner = members.iter().find(|m| m.device_id == author && m.role == Role::Owner)?;
                match f.kind {
                    EventKind::ForContact { happened_at_ms, alert } => {
                        Some(ContactAlertView { happened_at_ms, alert, device_label: owner.label.clone() })
                    }
                    _ => None,
                }
            })
            .collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.happened_at_ms));
        out
    }
}

/// A passphrase for `export_backup`: 5 random groups of 5 characters. Show it once.
#[uniffi::export]
pub fn new_backup_passphrase() -> String {
    crate::backup::new_passphrase()
}

/// Checks a new PIN, password or pattern without storing it. Onboarding calls it before creating the
/// profile, so a weak secret never leaves a profile half set up. Same errors as `set_secret`.
#[uniffi::export]
pub fn check_secret(factor: LocalFactor, secret: String) -> Result<(), CoreError> {
    let secret = zeroize::Zeroizing::new(secret);
    Ok(auth::check_new_secret(factor, &secret)?)
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
        let seen = c.devices_seen(later.now_ms);
        assert_eq!(seen.len(), 1);
        assert!(seen[0].is_me);
        assert_eq!(seen[0].last_seen_ms, later.now_ms);
    }

    #[test]
    fn contact_gets_only_alerts_and_numbers_once() {
        let (me, channel) = (core(), core());
        me.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        channel.create_contact_channel(MONDAY_9, "Phone".into()).unwrap();
        me.plan_day(MONDAY_9, vec![]).unwrap();
        let at = |h: i64| Clock { now_ms: MONDAY_9.now_ms + h * 60 * MINUTE_MS, ..MONDAY_9 };
        me.complete_goal(at(1), "read-30".into(), Method::HonorDeclaration, 100, 30).unwrap();
        me.report_tamper(at(2), TamperKind::PermissionRevoked).unwrap();
        me.emergency(at(3)).unwrap();
        let tuesday = Clock { now_ms: MONDAY_9.now_ms + DAY_MS, ..MONDAY_9 };
        assert_eq!(me.forward_to_contact(channel.clone(), tuesday).unwrap(), 3);
        assert_eq!(me.forward_to_contact(channel.clone(), tuesday).unwrap(), 0, "idempotente");
        let alerts: Vec<ContactAlert> = channel.contact_alerts().into_iter().map(|a| a.alert).collect();
        assert_eq!(alerts.len(), 3);
        assert!(alerts.contains(&ContactAlert::Emergency));
        assert!(alerts.contains(&ContactAlert::Tamper { what: TamperKind::PermissionRevoked }));
        // Monday: "read-30" done, "eat-healthy" not → 1 of 2. No titles, nor which goals they were.
        assert!(alerts.contains(&ContactAlert::DailySummary { completed: 1, total: 2 }));
        // The contact channel has no configuration nor day state
        assert!(channel.status(tuesday).is_none());
        assert!(me.forward_to_contact(me.clone(), tuesday).is_err());
    }

    // ── Family mode ──

    /// What a sync session does, without sockets: `from` sends what `to` lacks, sealed for `to`.
    fn deliver(from: &Core, to: &Core) -> SyncReport {
        let theirs = to.lock().ledger.heads();
        let to_recipient = to.lock().keys.age.to_public();
        let blobs: Vec<Vec<u8>> = {
            let g = from.lock();
            let evs = peer::missing_for(&g.db, &g.ledger.heads(), &theirs).unwrap();
            evs.iter().map(|e| seal(e, std::slice::from_ref(&to_recipient)).unwrap()).collect()
        };
        to.lock().ingest(blobs).unwrap()
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
        minor.lock().db.set_meta(META_GENESIS, &guardian.device_id()).unwrap(); // as pairing_join does
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
    fn missing_for_sends_only_what_the_peer_lacks_in_order() {
        let (guardian, minor) = family();
        minor.request(later(1), RequestKind::RestDay).unwrap();
        minor.heartbeat(later(2)).unwrap();
        let g = minor.lock();
        let mine = g.ledger.heads();
        let all = peer::missing_for(&g.db, &mine, &BTreeMap::new()).unwrap();
        assert_eq!(all.len(), g.db.all().unwrap().len());
        assert!(peer::missing_for(&g.db, &mine, &mine).unwrap().is_empty());
        // The guardian has its own events but not the minor's two: exactly those, in seq order.
        let theirs = guardian.lock().ledger.heads();
        let seqs: Vec<u64> = peer::missing_for(&g.db, &mine, &theirs).unwrap().iter().map(|e| e.body.seq).collect();
        assert_eq!(seqs, vec![1, 2]);
    }

    #[test]
    fn retention_keeps_members_config_and_discovery() {
        let dir = std::env::temp_dir().join(format!("pco-ret-{}-{}", std::process::id(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let path: String = dir.join("db").to_string_lossy().into();
        let store = Arc::new(MemStore::default());
        let c = Core::open(path.clone(), store.clone()).unwrap();
        c.create_profile(MONDAY_9, "PC".into(), Config::example()).unwrap();
        c.heartbeat(later(1)).unwrap();
        let in_100_days = Clock { now_ms: MONDAY_9.now_ms + 100 * DAY_MS, ..MONDAY_9 };
        c.lock().db.prune_before(in_100_days.now_ms - RETENTION_DAYS * DAY_MS, structural).unwrap();
        assert_eq!(c.lock().db.all().unwrap().len(), 3, "the heartbeat went, the profile stayed");
        drop(c);
        let again = Core::open(path, store).unwrap();
        assert!(again.status(in_100_days).is_some());
        assert!(again.discovery_tag(in_100_days.now_ms).is_some());
    }

    #[test]
    fn silent_devices_are_noticed_once() {
        let (guardian, minor) = family();
        let h = |hours: i64| later(hours * 60);
        // Continuous activity for 30 h, seen 30 h ago: nothing to say.
        for hour in 0..=30 {
            minor.heartbeat(h(hour)).unwrap();
        }
        deliver(&minor, &guardian);
        guardian.lock().silent_notices(h(60).now_ms).unwrap();
        assert!(guardian.take_notices().unwrap().is_empty());
        // A 30 h hole in its activity: one notice, also after asking again.
        minor.heartbeat(h(61)).unwrap();
        deliver(&minor, &guardian);
        for _ in 0..2 {
            guardian.lock().silent_notices(h(62).now_ms).unwrap();
        }
        let kinds: Vec<NoticeKind> = guardian.take_notices().unwrap().into_iter().map(|n| n.kind).collect();
        assert_eq!(kinds, vec![NoticeKind::DeviceSilent]);
        assert_eq!(
            guardian.devices_seen(h(62).now_ms).iter().map(|d| d.longest_gap_ms).max(),
            Some(31 * 60 * MINUTE_MS)
        );
        // The minor does not watch the guardian's devices
        minor.lock().silent_notices(h(200).now_ms).unwrap();
        assert!(minor.take_notices().unwrap().is_empty());
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

    #[test]
    fn diagnostic_report_has_no_personal_data() {
        let (guardian, minor) = family();
        minor.request(later(1), RequestKind::AddSite { domain: "khanacademy.org".into() }).unwrap();
        deliver(&minor, &guardian);
        let report = guardian.diagnostic_report(later(2), vec!["accessibility=on".into()]);
        assert!(report.contains("profile=family role=Guardian members=2"), "{report}");
        assert!(report.ends_with("accessibility=on"));
        for secret in [
            "Mom".to_string(),
            "Tablet".to_string(),
            "Read for 30 minutes".to_string(),
            "khanacademy.org".to_string(),
            guardian.device_id(),
            minor.device_id(),
        ] {
            assert!(!report.contains(&secret), "{secret} leaked:\n{report}");
        }
    }

    // ── Change authorization (ADR 0013) ──

    fn days(n: i64) -> Clock {
        Clock { now_ms: MONDAY_9.now_ms + n * DAY_MS, ..MONDAY_9 }
    }

    fn laxer(c: &Core, at: Clock) -> Config {
        let mut cfg = c.active_config(at).unwrap();
        cfg.permits_per_day += 2;
        cfg
    }

    /// Lists `contact` in `me`'s profile as a trusted contact (what `forward_to_contact` does).
    fn list_contact(me: &Core, contact: &Core, at: Clock) {
        let mut m = contact.lock().me_as_member("Laura".into(), Role::TrustedContact);
        m.node_id.clear();
        let mut g = me.lock();
        let mut members = g.ledger.members().to_vec();
        members.push(m);
        g.record(at.now_ms, EventKind::MembersChanged { members }).unwrap();
    }

    /// A contact's signed decision approving `config`, as JSON (what `ContactApproval` carries).
    fn approval(contact: &Core, at: Clock, config: &Config, effective: Effective) -> String {
        let g = contact.lock();
        let kind = EventKind::Decision {
            request_id: "r1".into(),
            approved: true,
            config_hash: Some(config.hash()),
            effective: Some(effective),
        };
        let body = EventBody { author: g.keys.device_id(), seq: 1, prev: None, at_ms: at.now_ms, kind };
        serde_json::to_string(&SignedEvent::sign(&g.keys, body)).unwrap()
    }

    /// Adult with Laura listed (before) and "changes need my contact" turned on at later(1).
    fn adult_with_contact() -> (Arc<Core>, Arc<Core>) {
        let (me, laura) = (core(), core());
        me.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        list_contact(&me, &laura, MONDAY_9);
        let mut cfg = me.active_config(MONDAY_9).unwrap();
        cfg.auth.contact_required = true;
        me.set_config(later(1), cfg).unwrap();
        (me, laura)
    }

    /// What a modified client could sign: a configuration change, bypassing every API check.
    fn force_config(c: &Core, at: Clock, config: Config) {
        c.lock().record(at.now_ms, EventKind::ConfigChanged { config, effective_from_ms: at.now_ms }).unwrap();
    }

    #[test]
    fn relaxing_needs_the_contacts_approval_of_that_exact_config() {
        let (me, laura) = adult_with_contact();
        let pc = core();
        let wanted = laxer(&me, later(2));
        // A modified client relaxes without approval: ignored, today and tomorrow.
        force_config(&me, later(2), wanted.clone());
        // An approval of ANOTHER configuration does not cover this one.
        let mut other = wanted.clone();
        other.permits_per_day += 1;
        me.lock()
            .record(
                later(3).now_ms,
                EventKind::ContactApproval { decision: approval(&laura, later(3), &other, Effective::Now) },
            )
            .unwrap();
        force_config(&me, later(3), wanted.clone());
        for at in [later(4), days(1)] {
            assert_eq!(me.active_config(at).unwrap().permits_per_day, 3, "not approved");
        }
        // Approved, "today": applies at once, on this device and on the others.
        me.lock()
            .record(
                later(5).now_ms,
                EventKind::ContactApproval { decision: approval(&laura, later(5), &wanted, Effective::Now) },
            )
            .unwrap();
        force_config(&me, later(5), wanted.clone());
        pc.lock().ledger.set_genesis_author(me.device_id());
        deliver(&me, &pc);
        for c in [&me, &pc] {
            assert_eq!(c.active_config(later(6)).unwrap().permits_per_day, 5);
        }
        // Each approval once: back to 3, then the same 5 again without a new approval.
        let mut back = wanted.clone();
        back.permits_per_day = 3;
        force_config(&me, later(7), back);
        force_config(&me, later(8), wanted);
        assert_eq!(me.active_config(days(2)).unwrap().permits_per_day, 3, "the approval was used");
        // A forged approval (not signed by a contact) is rejected outright.
        let mallory = core();
        let forged = approval(&mallory, later(9), &laxer(&me, later(9)), Effective::Now);
        assert!(me.lock().record(later(9).now_ms, EventKind::ContactApproval { decision: forged }).is_err());
    }

    #[test]
    fn a_contact_added_after_the_requirement_waits_7_days() {
        let (me, _) = adult_with_contact();
        let puppet = core(); // e.g. the user's second phone posing as a contact
        list_contact(&me, &puppet, later(2));
        let wanted = laxer(&me, later(3));
        let try_at = |at: Clock| {
            me.lock()
                .record(
                    at.now_ms,
                    EventKind::ContactApproval { decision: approval(&puppet, at, &wanted, Effective::Now) },
                )
                .unwrap();
            force_config(&me, at, wanted.clone());
            me.active_config(Clock { now_ms: at.now_ms + 1, ..at }).unwrap().permits_per_day
        };
        assert_eq!(try_at(days(6)), 3, "6 days: does not count yet");
        assert_eq!(try_at(days(8)), 5, "after 7 days it is a contact like any other");
    }

    #[test]
    fn leaving_the_contact_takes_7_days_and_nothing_else_relaxes() {
        let (me, _) = adult_with_contact();
        let mut exit = me.active_config(later(2)).unwrap();
        exit.auth.contact_required = false;
        let mut sneaky = exit.clone();
        sneaky.permits_per_day += 1;
        assert_eq!(me.set_config(later(2), sneaky).unwrap_err().code(), "denied.needs_contact");
        assert_eq!(me.set_config(later(2), exit).unwrap(), later(2).now_ms + 7 * DAY_MS);
        assert!(
            me.active_config(Clock { now_ms: later(2).now_ms + 7 * DAY_MS - 1, ..MONDAY_9 })
                .unwrap()
                .auth
                .contact_required
        );
        assert!(
            !me.active_config(Clock { now_ms: later(2).now_ms + 7 * DAY_MS, ..MONDAY_9 })
                .unwrap()
                .auth
                .contact_required
        );
        // Changing your mind: turning it back on cancels the pending exit.
        let mut again = me.active_config(later(3)).unwrap();
        again.auth.contact_required = true;
        me.set_config(later(3), again).unwrap();
        assert!(me.active_config(days(8)).unwrap().auth.contact_required);
    }

    #[test]
    fn local_gate_counts_attempts_across_restarts() {
        let dir = std::env::temp_dir().join(format!("pco-auth-{}-{}", std::process::id(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let path: String = dir.join("db").to_string_lossy().into();
        let store = Arc::new(MemStore::default());
        let c = Core::open(path.clone(), store.clone()).unwrap();
        c.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        assert_eq!(
            c.set_secret(MONDAY_9, LocalFactor::Pin, "123456".into(), SecretHolder::Owner).unwrap_err().code(),
            "denied.easy_pin"
        );
        c.set_secret(MONDAY_9, LocalFactor::Pin, "482916".into(), SecretHolder::Owner).unwrap();
        assert_eq!(
            c.active_config(MONDAY_9).unwrap().auth.local,
            vec![LocalFactor::Pin],
            "the method joins the policy"
        );
        c.lock_settings();
        assert_eq!(c.set_config(later(1), laxer(&c, later(1))).unwrap_err().code(), "denied.auth_required");
        for _ in 0..5 {
            assert_eq!(
                c.authorize_local(later(1), LocalFactor::Pin, "000000".into()).unwrap_err().code(),
                "denied.wrong_secret"
            );
        }
        drop(c);
        let c = Core::open(path, store).unwrap(); // restarting does not reset the count
        match c.authorize_local(later(1), LocalFactor::Pin, "482916".into()) {
            Err(CoreError::Denied { reason, cooldown_minutes }) => {
                assert_eq!((reason.as_str(), cooldown_minutes), ("locked", 1))
            }
            other => panic!("{other:?}"),
        }
        let after = Clock { now_ms: later(1).now_ms + 30_000, ..MONDAY_9 };
        assert!(c.authorize_local(after, LocalFactor::Pattern, "0-1-2-3".into()).is_err(), "pattern not set");
        c.authorize_local(after, LocalFactor::Pin, "482916".into()).unwrap();
        assert!(c.auth_state(after).unlocked);
        assert!(c.set_config(after, laxer(&c, after)).unwrap() > after.now_ms, "allowed, still from tomorrow");
        // Biometrics only if it is in the policy
        assert_eq!(c.authorize_biometric(after).unwrap_err().code(), "denied.method_not_set");
    }

    // ── Delete all my data ──

    fn store_and_path() -> (String, Arc<MemStore>) {
        let dir = std::env::temp_dir().join(format!("pco-erase-{}-{}", std::process::id(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        (dir.join("core.db").to_string_lossy().into(), Arc::new(MemStore::default()))
    }

    #[test]
    fn erasing_every_database_leaves_each_one_readable() {
        let (path, store) = store_and_path();
        let contact_path = path.replace("core.db", "contact.db");
        let main = Core::open(path.clone(), store.clone()).unwrap();
        if !main.lock().db.encrypted() {
            return; // needs SQLCipher: without it any key opens the file
        }
        // A contact channel from before per-database secrets: encrypted with the shared key.
        let shared = crate::crypto::db_key(store.as_ref()).unwrap();
        drop(crate::storage::Db::open(&contact_path, &shared).unwrap());
        let contact = Core::open(contact_path.clone(), store.clone()).unwrap();
        assert_eq!(contact.device_id(), main.device_id(), "adopts the shared identity");
        // "Delete all my data": the profile first, then the other databases (CoreHolder.eraseAll)
        main.clone().erase_all(MONDAY_9).unwrap();
        contact.clone().erase_all(MONDAY_9).unwrap();
        drop((main, contact));
        Core::open(path, store.clone()).unwrap();
        Core::open(contact_path, store).unwrap();
    }

    #[test]
    fn erase_all_needs_what_relaxing_needs() {
        let c = core();
        c.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        c.set_secret(MONDAY_9, LocalFactor::Pin, "482916".into(), SecretHolder::Owner).unwrap();
        c.lock_settings();
        assert_eq!(c.clone().erase_all(later(1)).unwrap_err().code(), "denied.auth_required");
        assert!(c.status(later(1)).is_some(), "nothing was erased");
        c.authorize_local(later(1), LocalFactor::Pin, "482916".into()).unwrap();
        c.clone().erase_all(later(1)).unwrap();
        // With "changes need my contact": first stop needing them (approval or 7 days)
        let (me, _laura) = adult_with_contact();
        assert_eq!(me.clone().erase_all(later(2)).unwrap_err().code(), "denied.needs_contact");
        // A minor cannot; erasing the guardian's copy does not touch the minor's rules
        let (guardian, minor) = family();
        assert_eq!(minor.clone().erase_all(later(1)).unwrap_err().code(), "denied.guardian_only");
        guardian.clone().erase_all(later(1)).unwrap();
        assert!(guardian.status(later(1)).is_none());
        assert!(minor.status(later(1)).is_some());
    }

    #[test]
    fn erase_all_replaces_every_key_and_leaves_a_new_empty_device() {
        let (path, store) = store_and_path();
        let c = Core::open(path.clone(), store.clone()).unwrap();
        c.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        let (before, old_id) = (store.0.lock().unwrap().clone(), c.device_id());
        c.clone().erase_all(later(1)).unwrap();
        let after = store.0.lock().unwrap().clone();
        for name in [SECRET_DB, crate::crypto::SECRET_SIGNING, crate::crypto::SECRET_NET, crate::crypto::SECRET_AGE] {
            assert!(after.contains_key(name) && before.get(name) != after.get(name), "{name} replaced");
        }
        assert_ne!(c.device_id(), old_id);
        assert!(c.status(later(1)).is_none() && c.members().is_empty() && c.my_role().is_none());
        // The same Core goes on as a new device, and the database opens again after a restart
        c.create_profile(later(2), "Phone".into(), Config::example()).unwrap();
        drop(c);
        let c = Core::open(path, store).unwrap();
        assert_eq!(c.members().len(), 1);
    }

    #[test]
    fn an_interrupted_erase_is_finished_on_open() {
        let (path, store) = store_and_path();
        let c = Core::open(path.clone(), store.clone()).unwrap();
        c.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        if !c.lock().db.encrypted() {
            return; // needs SQLCipher: without it any key opens the file
        }
        let old_id = c.device_id();
        // The process dies right after re-encrypting: the keystore still has the old database key.
        let key = random_32();
        store.save(SECRET_DB_NEXT.into(), key.to_vec()).unwrap();
        c.lock().db.wipe_and_rekey(&key[..]).unwrap();
        drop(c);
        let c = Core::open(path, store.clone()).unwrap();
        assert!(c.members().is_empty());
        assert_ne!(c.device_id(), old_id, "new identity");
        assert_eq!(store.load(SECRET_DB.into()).unwrap().unwrap(), key.to_vec());
    }

    #[test]
    fn recovery_code_resets_local_methods_after_72_hours() {
        let c = core();
        c.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        c.set_secret(MONDAY_9, LocalFactor::Password, "correct horse".into(), SecretHolder::Owner).unwrap();
        let code = c.create_recovery_code(MONDAY_9).unwrap();
        c.lock_settings();
        assert!(c.use_recovery_code(later(1), "AAAAA-AAAAA-AAAAA-AAAAA".into()).is_err());
        let at = c.use_recovery_code(later(1), code.to_lowercase()).unwrap();
        let st = |ms: i64| c.auth_state(Clock { now_ms: ms, ..MONDAY_9 });
        assert_eq!(st(at - 1).usable, vec![LocalFactor::Password], "71 h: the old password still counts");
        assert_eq!(st(at - 1).reset_at_ms, Some(at));
        assert!(!st(at - 1).unlocked, "the code does not open the gate");
        assert!(st(at).usable.is_empty() && st(at).unlocked, "72 h: set new methods");
    }

    #[test]
    fn nobody_grants_themselves_anything_in_a_personal_profile() {
        let c = core();
        c.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
        for kind in [EventKind::GrantPermit { minutes: 60 }, EventKind::RestDayGranted] {
            assert!(c.lock().record(later(1).now_ms, kind).is_err());
        }
        let grant = EventKind::LocalGrant {
            what: OfflineAsk::RestDay,
            request_code: "X".into(),
            proof: GrantProof::OfflineCode { approver: c.device_id() },
        };
        assert!(c.lock().record(later(1).now_ms, grant).is_err(), "the approver must be a contact");
    }

    #[test]
    fn minor_gets_small_grants_with_the_guardian_in_person_or_by_code() {
        let (guardian, minor) = family();
        guardian.set_secret(MONDAY_9, LocalFactor::Pin, "482916".into(), SecretHolder::Guardian).unwrap();
        deliver(&guardian, &minor);
        assert_eq!(minor.auth_state(later(1)).guardian_methods, vec![LocalFactor::Pin]);
        let wrong =
            minor.grant_in_person(later(1), LocalFactor::Pin, "111111".into(), OfflineAsk::ExtraPermit { minutes: 20 });
        assert_eq!(wrong.unwrap_err().code(), "denied.wrong_secret");
        let st = minor
            .grant_in_person(later(1), LocalFactor::Pin, "482916".into(), OfflineAsk::ExtraPermit { minutes: 20 })
            .unwrap();
        assert_eq!(st.permits_left, 4);

        // By code: a wrong answer, then the right one
        let code = minor.offline_request(later(2), OfflineAsk::LaterBedtime { minutes: 60 }).unwrap();
        let answer = guardian.offline_answer(code.to_lowercase()).unwrap();
        assert_eq!(answer.ask, OfflineAsk::LaterBedtime { minutes: 60 });
        assert_eq!(minor.offline_redeem(later(2), "00000000".into(), None).unwrap_err().code(), "denied.invalid_code");
        let st = minor.offline_redeem(later(2), answer.response.clone(), None).unwrap();
        assert_eq!(st.sleep_at_ms, Some(MONDAY_9.now_ms + (23 * 60 + 30 - 9 * 60) * MINUTE_MS), "tonight at 23:30");
        assert!(minor.offline_redeem(later(2), answer.response, None).is_err(), "each code once");

        // Three wrong answers cancel the request
        let code = minor.offline_request(later(3), OfflineAsk::RestDay).unwrap();
        for _ in 0..3 {
            assert!(minor.offline_redeem(later(3), "12345678".into(), None).is_err());
        }
        let right = guardian.offline_answer(code).unwrap().response;
        assert!(minor.offline_redeem(later(3), right, None).is_err(), "cancelled: ask again");
        let code = minor.offline_request(later(4), OfflineAsk::RestDay).unwrap();
        let right = guardian.offline_answer(code).unwrap().response;
        assert_eq!(minor.offline_redeem(later(4), right, None).unwrap().state, DayState::Free);

        // 3 a day; and the guardian sees each one
        assert_eq!(minor.offline_request(later(5), OfflineAsk::RestDay).unwrap_err().code(), "denied.too_many_grants");
        deliver(&minor, &guardian);
        let granted = guardian
            .family_alerts(later(6))
            .into_iter()
            .filter(|a| matches!(a.alert, FamilyAlert::Granted { .. }))
            .count();
        assert_eq!(granted, 3);
        assert!(guardian.offline_answer("K7Q2-M9XA-3U".into()).is_err());
        assert!(
            minor.offline_answer(minor.offline_request(days(1), OfflineAsk::RestDay).unwrap()).is_err(),
            "the minor is not an approver"
        );
    }

    /// minisign vector with a throwaway key (BLAKE2b prehash, like `minisign -S`).
    const TEST_PK: &str = "RWSR1VaqRkzVxD9R0i0mjqvxxVDb0CSIcbePF1vq29RvuFMv6ZhUbi0l";
    const TEST_LIST: &[u8] = b"procastinatioff-categories v1\nexample-adult.com\n";
    const TEST_SIG: &str = "untrusted comment: prueba
RUSR1VaqRkzVxPmMcdavqT6j6T8fC6lOi12olW5+/bklsEgN4WtwiY3eDzf/lapp3Db5piAJ31F/whPh8sWZ+VLz2oHOfLCE6g0=
trusted comment: timestamp:0\tfile:categories.txt\tprehashed
jejp/L9GYq8GlNdlzjzuwTYvooWG1WLF58tS9hY2CxHKOIqH3Azwf+m0qLKtVsve5/88jRWSaGnfyS8136H4CQ==
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
