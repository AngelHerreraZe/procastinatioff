// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Event ledger (security-critical): decides which events are accepted.
//!
//! 1. The signature must be valid.
//! 2. The author must be a member. Exception: the genesis (first `MembersChanged`), only from the trusted author.
//! 3. Per-author chain: consecutive `seq` and correct `prev`. Gap → recorded. Same `seq` → fork → rejected.
//! 4. Family profile (has a guardian): some events can only be signed by a guardian (see `guardian_only`).
//!    That a minor can only make the configuration STRICTER is checked when rebuilding the history
//!    (`ConfigHistory::from_facts_checked`): there the result does not depend on arrival order.
//! 5. Any profile: nobody grants themselves anything, a trusted contact only answers, and an
//!    embedded contact approval must carry the contact's valid signature (see `signer_check`).
//!
//! Ordering assumption: each author's events arrive in the order they were created.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::CoreError;
use crate::events::SignedEvent;
use crate::model::{EventKind, Fact, GrantProof, LocalFactor, Member, Method, Role, SecretHolder};
use crate::offline_code::ask_in_range;

/// Trusted contacts per adult (one is enough to approve).
pub const MAX_CONTACTS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub author: String,
    pub expected_seq: u64,
    pub got_seq: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Accepted {
    New,
    Duplicate,
}

#[derive(Default)]
pub struct Ledger {
    events: BTreeMap<String, SignedEvent>,  // id → event
    heads: BTreeMap<String, (u64, String)>, // author → (seq, id) of the latest
    members: Vec<Member>,
    genesis_author: Option<String>,
    /// Events signed by whoever was a guardian when they were accepted.
    by_guardian: BTreeSet<String>,
    pub gaps: Vec<Gap>,
}

/// In a family profile, can only a guardian sign this event?
fn guardian_only(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::MembersChanged { .. }
            | EventKind::GrantPermit { .. }
            | EventKind::Decision { .. }
            | EventKind::RestDayGranted
            | EventKind::GoalCompleted { method: Method::Witness, .. }
            | EventKind::DiscoveryKey { .. }
    )
}

impl Ledger {
    pub fn new(genesis_author: Option<String>) -> Self {
        Self { genesis_author, ..Default::default() }
    }

    pub fn set_genesis_author(&mut self, author: String) {
        self.genesis_author = Some(author);
    }

    pub fn members(&self) -> &[Member] {
        &self.members
    }

    /// Family profile: has at least one guardian.
    pub fn is_family(&self) -> bool {
        self.members.iter().any(|m| m.role == Role::Guardian)
    }

    pub fn head(&self, author: &str) -> Option<&(u64, String)> {
        self.heads.get(author)
    }

    /// Author -> last `seq` held (the version vector sent in `Hello`).
    pub fn heads(&self) -> BTreeMap<String, u64> {
        self.heads.iter().map(|(a, (seq, _))| (a.clone(), *seq)).collect()
    }

    fn role_of(&self, device: &str) -> Option<Role> {
        self.members.iter().find(|m| m.device_id == device).map(|m| m.role)
    }

    /// Who may sign what, in any profile (security-critical).
    fn signer_check(&self, author: &str, kind: &EventKind) -> Result<(), &'static str> {
        let role = self.role_of(author);
        match kind {
            _ if role == Some(Role::TrustedContact)
                && !matches!(kind, EventKind::Decision { .. } | EventKind::Heartbeat) =>
            {
                Err("a trusted contact only answers requests")
            }
            // Nobody grants themselves passes or rest days, not even in a personal profile.
            EventKind::GrantPermit { .. } | EventKind::RestDayGranted if role != Some(Role::Guardian) => {
                Err("reserved for a guardian")
            }
            EventKind::Decision { .. } if !matches!(role, Some(Role::Guardian | Role::TrustedContact)) => {
                Err("only a guardian or a trusted contact answers")
            }
            EventKind::SecretSet { factor: LocalFactor::Biometric, .. } => Err("biometrics has no secret"),
            EventKind::SecretSet { holder: SecretHolder::Guardian, .. } if role != Some(Role::Guardian) => {
                Err("reserved for a guardian")
            }
            EventKind::RecoveryCodeSet { .. } | EventKind::LocalResetStarted if role != Some(Role::Owner) => {
                Err("only an adult's own device")
            }
            EventKind::ContactApproval { decision } if role != Some(Role::Owner) || !self.valid_approval(decision) => {
                Err("invalid contact approval")
            }
            EventKind::LocalGrant { what, request_code, proof } => {
                let approver_is = |id: &str, r: Role| self.role_of(id) == Some(r);
                let ok = ask_in_range(what)
                    && request_code.len() <= 16
                    && match (self.is_family(), role, proof) {
                        (true, Some(Role::Minor), GrantProof::GuardianSecret) => true,
                        (true, Some(Role::Minor), GrantProof::OfflineCode { approver }) => {
                            approver_is(approver, Role::Guardian)
                        }
                        (false, Some(Role::Owner), GrantProof::OfflineCode { approver }) => {
                            approver_is(approver, Role::TrustedContact)
                        }
                        _ => false,
                    };
                if ok { Ok(()) } else { Err("invalid concession") }
            }
            _ => Ok(()),
        }
    }

    /// A trusted contact's signed `Decision` approving a configuration (hash and when it applies).
    fn valid_approval(&self, json: &str) -> bool {
        let Ok(d) = serde_json::from_str::<SignedEvent>(json) else { return false };
        d.verify().is_ok()
            && self.role_of(&d.body.author) == Some(Role::TrustedContact)
            && matches!(
                d.body.kind,
                EventKind::Decision { approved: true, config_hash: Some(_), effective: Some(_), .. }
            )
    }

    pub fn accept(&mut self, ev: SignedEvent) -> Result<Accepted, CoreError> {
        let id = ev.id();
        if self.events.contains_key(&id) {
            return Ok(Accepted::Duplicate);
        }
        ev.verify()?;
        let author = ev.body.author.clone();

        // 1) Authorization
        match &ev.body.kind {
            EventKind::MembersChanged { members } if self.members.is_empty() => {
                if self.genesis_author.as_deref() != Some(author.as_str()) {
                    return Err(CoreError::protocol("genesis from an untrusted author"));
                }
                if !members.iter().any(|m| m.device_id == author) {
                    return Err(CoreError::protocol("the genesis must include its author"));
                }
            }
            _ if !self.members.iter().any(|m| m.device_id == author) => {
                return Err(CoreError::protocol("el autor no es miembro"));
            }
            _ => {}
        }

        // 1b) Family profile
        let genesis = self.members.is_empty();
        let family = self.is_family();
        let is_guardian = self.members.iter().any(|m| m.device_id == author && m.role == Role::Guardian);
        if family && guardian_only(&ev.body.kind) && !is_guardian {
            return Err(CoreError::protocol("action reserved for a guardian"));
        }
        self.signer_check(&author, &ev.body.kind).map_err(CoreError::protocol)?;
        if let EventKind::MembersChanged { members } = &ev.body.kind {
            let family_after = members.iter().any(|m| m.role == Role::Guardian);
            // A family profile never ends up without a guardian, only has guardians and minors, and a
            // personal one never becomes a family profile (the minor would lose protection or the owner control).
            let roles_ok = members.iter().all(|m| matches!(m.role, Role::Guardian | Role::Minor));
            if (family || (genesis && family_after)) && !(family_after && roles_ok) {
                return Err(CoreError::protocol("invalid members for a family profile"));
            }
            if !genesis && !family && family_after {
                return Err(CoreError::protocol("a personal profile cannot become a family profile"));
            }
            // Personal: its owner's devices plus up to 3 trusted contacts.
            let contacts = members.iter().filter(|m| m.role == Role::TrustedContact).count();
            let personal_ok = members.iter().all(|m| matches!(m.role, Role::Owner | Role::TrustedContact))
                && members.iter().any(|m| m.role == Role::Owner)
                && contacts <= MAX_CONTACTS;
            if !family_after && !personal_ok {
                return Err(CoreError::protocol("invalid members for a personal profile"));
            }
        }

        // 2) Per-author chain
        if let Some((seq, prev_id)) = self.heads.get(&author) {
            if ev.body.seq <= *seq {
                return Err(CoreError::protocol("fork: repeated seq with different content"));
            }
            if ev.body.seq != seq + 1 || ev.body.prev.as_deref() != Some(prev_id.as_str()) {
                self.gaps.push(Gap { author: author.clone(), expected_seq: seq + 1, got_seq: ev.body.seq });
            }
        } // None: first event seen from this author (may not be seq 1 because of retention)

        // 3) Apply
        if let EventKind::MembersChanged { members } = &ev.body.kind {
            self.members = members.clone();
        }
        if self.members.iter().any(|m| m.device_id == author && m.role == Role::Guardian) {
            self.by_guardian.insert(id.clone());
        }
        self.heads.insert(author, (ev.body.seq, id.clone()));
        self.events.insert(id, ev);
        Ok(Accepted::New)
    }

    /// Facts ordered by (time, author, seq): the same order on every device.
    pub fn facts(&self) -> Vec<Fact> {
        self.authored().into_iter().map(|(_, f)| f).collect()
    }

    /// Like `facts`, with the author of each fact.
    pub fn authored(&self) -> Vec<(String, Fact)> {
        let mut evs: Vec<(&String, &SignedEvent)> = self.events.iter().collect();
        evs.sort_by(|(_, a), (_, b)| {
            (a.body.at_ms, &a.body.author, a.body.seq).cmp(&(b.body.at_ms, &b.body.author, b.body.seq))
        });
        evs.into_iter()
            .map(|(id, e)| {
                let fact =
                    Fact { at_ms: e.body.at_ms, kind: e.body.kind.clone(), by_guardian: self.by_guardian.contains(id) };
                (e.body.author.clone(), fact)
            })
            .collect()
    }

    /// Last known activity instant per author (heartbeats and any other event).
    pub fn last_seen(&self) -> BTreeMap<String, i64> {
        let mut m = BTreeMap::new();
        for e in self.events.values() {
            let t = m.entry(e.body.author.clone()).or_insert(e.body.at_ms);
            *t = (*t).max(e.body.at_ms);
        }
        m
    }

    /// Instant of the last known event (from any author).
    pub fn last_at(&self) -> i64 {
        self.events.values().map(|e| e.body.at_ms).max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{DeviceKeys, testing::MemStore};
    use crate::events::EventBody;
    use crate::model::Role;

    fn keys() -> DeviceKeys {
        DeviceKeys::load_or_create(&MemStore::default()).unwrap()
    }

    fn member(k: &DeviceKeys) -> Member {
        Member {
            device_id: k.device_id(),
            age_recipient: k.age_recipient(),
            label: "x".into(),
            role: Role::Owner,
            node_id: k.node_id(),
            time_source: false,
        }
    }

    fn ev(k: &DeviceKeys, seq: u64, prev: Option<String>, kind: EventKind) -> SignedEvent {
        SignedEvent::sign(k, EventBody { author: k.device_id(), seq, prev, at_ms: seq as i64, kind })
    }

    #[test]
    fn genesis_only_from_trusted_author_and_outsiders_rejected() {
        let (a, mallory) = (keys(), keys());
        let mut l = Ledger::new(Some(a.device_id()));
        let fake = ev(&mallory, 1, None, EventKind::MembersChanged { members: vec![member(&mallory)] });
        assert!(l.accept(fake).is_err());
        let genesis = ev(&a, 1, None, EventKind::MembersChanged { members: vec![member(&a)] });
        let gid = genesis.id();
        assert_eq!(l.accept(genesis.clone()).unwrap(), Accepted::New);
        assert_eq!(l.accept(genesis).unwrap(), Accepted::Duplicate);
        assert!(l.accept(ev(&mallory, 1, None, EventKind::Heartbeat)).is_err());
        assert!(l.accept(ev(&a, 2, Some(gid), EventKind::Heartbeat)).is_ok());
        assert!(l.gaps.is_empty());
    }

    #[test]
    fn gap_is_detected_and_fork_rejected() {
        let a = keys();
        let mut l = Ledger::new(Some(a.device_id()));
        let g = ev(&a, 1, None, EventKind::MembersChanged { members: vec![member(&a)] });
        let gid = g.id();
        l.accept(g).unwrap();
        l.accept(ev(&a, 3, Some("x".into()), EventKind::Heartbeat)).unwrap(); // 2 is missing
        assert_eq!(l.gaps.len(), 1);
        let fork = ev(&a, 3, Some(gid), EventKind::EmergencyStarted);
        assert!(l.accept(fork).is_err());
    }

    // ── Family profile ──

    fn as_role(k: &DeviceKeys, role: Role) -> Member {
        Member { role, ..member(k) }
    }

    /// Guardian (genesis) + minor added by the guardian. Returns the id of the guardian's last event.
    fn family(g: &DeviceKeys, m: &DeviceKeys) -> (Ledger, String) {
        let mut l = Ledger::new(Some(g.device_id()));
        let genesis = ev(g, 1, None, EventKind::MembersChanged { members: vec![as_role(g, Role::Guardian)] });
        let gid = genesis.id();
        l.accept(genesis).unwrap();
        let add = ev(
            g,
            2,
            Some(gid),
            EventKind::MembersChanged { members: vec![as_role(g, Role::Guardian), as_role(m, Role::Minor)] },
        );
        let last = add.id();
        l.accept(add).unwrap();
        (l, last)
    }

    fn guardian_kinds(m: &DeviceKeys) -> Vec<EventKind> {
        vec![
            EventKind::GrantPermit { minutes: 15 },
            EventKind::RestDayGranted,
            EventKind::Decision { request_id: "ab".into(), approved: true, config_hash: None, effective: None },
            EventKind::GoalCompleted { goal_id: "read-30".into(), method: Method::Witness, score: 100, minutes: 0 },
            EventKind::MembersChanged { members: vec![as_role(m, Role::Guardian)] },
        ]
    }

    #[test]
    fn minor_cannot_sign_guardian_events() {
        let (g, m) = (keys(), keys());
        let (mut l, _) = family(&g, &m);
        for (i, kind) in guardian_kinds(&m).into_iter().enumerate() {
            assert!(l.accept(ev(&m, 1 + i as u64, None, kind.clone())).is_err(), "{kind:?}");
        }
        // Their own events are fine: asking, declaring a goal, heartbeats
        let req = ev(
            &m,
            1,
            None,
            EventKind::Request {
                request_id: "ab".into(),
                what: crate::model::RequestKind::ExtraPermit { minutes: 15 },
            },
        );
        assert!(l.accept(req).is_ok());
        let honor = EventKind::GoalCompleted {
            goal_id: "read-30".into(),
            method: Method::HonorDeclaration,
            score: 100,
            minutes: 0,
        };
        assert!(l.accept(ev(&m, 2, None, honor)).is_ok());
        assert!(l.facts().iter().all(|f| !f.by_guardian || matches!(f.kind, EventKind::MembersChanged { .. })));
    }

    #[test]
    fn guardian_can_sign_everything_and_is_marked() {
        let (g, m) = (keys(), keys());
        let (mut l, mut prev) = family(&g, &m);
        let mut kinds = guardian_kinds(&m);
        kinds.pop(); // the test MembersChanged would remove the guardian; tested separately
        kinds.push(EventKind::MembersChanged {
            members: vec![as_role(&g, Role::Guardian), as_role(&m, Role::Minor), as_role(&keys(), Role::Guardian)],
        });
        for (i, kind) in kinds.into_iter().enumerate() {
            let e = ev(&g, 3 + i as u64, Some(prev), kind);
            prev = e.id();
            assert!(l.accept(e).is_ok());
        }
        assert!(l.facts().iter().all(|f| f.by_guardian));
        assert!(l.gaps.is_empty());
    }

    #[test]
    fn family_never_loses_its_guardian_and_personal_never_turns_family() {
        let (g, m) = (keys(), keys());
        let (mut l, prev) = family(&g, &m);
        let orphan =
            ev(&g, 3, Some(prev.clone()), EventKind::MembersChanged { members: vec![as_role(&m, Role::Minor)] });
        assert!(l.accept(orphan).is_err());
        let owner = ev(
            &g,
            3,
            Some(prev),
            EventKind::MembersChanged { members: vec![as_role(&g, Role::Guardian), as_role(&m, Role::Owner)] },
        );
        assert!(l.accept(owner).is_err(), "a family profile has no owners");

        let a = keys();
        let mut p = Ledger::new(Some(a.device_id()));
        let genesis = ev(&a, 1, None, EventKind::MembersChanged { members: vec![member(&a)] });
        let gid = genesis.id();
        p.accept(genesis).unwrap();
        let takeover =
            ev(&a, 2, Some(gid), EventKind::MembersChanged { members: vec![member(&a), as_role(&m, Role::Guardian)] });
        assert!(p.accept(takeover).is_err());
    }
}
