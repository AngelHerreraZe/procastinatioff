// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Libro de eventos 🔒: decide qué eventos se aceptan.
//!
//! 1. La firma debe ser válida.
//! 2. El autor debe ser miembro. Excepción: el génesis (primer `MembersChanged`), solo del autor de confianza.
//! 3. Cadena por autor: `seq` consecutivo y `prev` correcto. Hueco → se registra. Mismo `seq` → bifurcación → se rechaza.
//! 4. Perfil familiar (hay un tutor): ciertos eventos solo los firma un tutor (ver `guardian_only`).
//!    Que un menor solo pueda ENDURECER la configuración se comprueba al reconstruir el historial
//!    (`ConfigHistory::from_facts_checked`): ahí el resultado no depende del orden de llegada.
//!
//! Supuesto de orden: el relay entrega los eventos de un buzón en el orden en que se subieron.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::CoreError;
use crate::events::SignedEvent;
use crate::model::{EventKind, Fact, Member, Method, Role};

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
    events: BTreeMap<String, SignedEvent>,  // id → evento
    heads: BTreeMap<String, (u64, String)>, // autor → (seq, id) del último
    members: Vec<Member>,
    genesis_author: Option<String>,
    /// Eventos firmados por quien era tutor al aceptarlos.
    by_guardian: BTreeSet<String>,
    pub gaps: Vec<Gap>,
}

/// En un perfil familiar, ¿solo un tutor puede firmar este evento?
fn guardian_only(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::MembersChanged { .. }
            | EventKind::GrantPermit { .. }
            | EventKind::Decision { .. }
            | EventKind::RestDayGranted
            | EventKind::GoalCompleted { method: Method::Witness, .. }
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

    /// Perfil familiar: tiene al menos un tutor.
    pub fn is_family(&self) -> bool {
        self.members.iter().any(|m| m.role == Role::Guardian)
    }

    pub fn head(&self, author: &str) -> Option<&(u64, String)> {
        self.heads.get(author)
    }

    pub fn accept(&mut self, ev: SignedEvent) -> Result<Accepted, CoreError> {
        let id = ev.id();
        if self.events.contains_key(&id) {
            return Ok(Accepted::Duplicate);
        }
        ev.verify()?;
        let author = ev.body.author.clone();

        // 1) Autorización
        match &ev.body.kind {
            EventKind::MembersChanged { members } if self.members.is_empty() => {
                if self.genesis_author.as_deref() != Some(author.as_str()) {
                    return Err(CoreError::protocol("génesis de un autor no confiable"));
                }
                if !members.iter().any(|m| m.device_id == author) {
                    return Err(CoreError::protocol("el génesis debe incluir a su autor"));
                }
            }
            _ if !self.members.iter().any(|m| m.device_id == author) => {
                return Err(CoreError::protocol("el autor no es miembro"));
            }
            _ => {}
        }

        // 1b) Perfil familiar
        let genesis = self.members.is_empty();
        let family = self.is_family();
        let is_guardian = self.members.iter().any(|m| m.device_id == author && m.role == Role::Guardian);
        if family && guardian_only(&ev.body.kind) && !is_guardian {
            return Err(CoreError::protocol("acción reservada al tutor"));
        }
        if let EventKind::MembersChanged { members } = &ev.body.kind {
            let family_after = members.iter().any(|m| m.role == Role::Guardian);
            // Un perfil familiar nunca se queda sin tutor, solo tiene tutores y menores, y uno
            // personal no se convierte en familiar (el menor perdería su protección o el dueño su control).
            let roles_ok = members.iter().all(|m| matches!(m.role, Role::Guardian | Role::Minor));
            if (family || (genesis && family_after)) && !(family_after && roles_ok) {
                return Err(CoreError::protocol("miembros no válidos para un perfil familiar"));
            }
            if !genesis && !family && family_after {
                return Err(CoreError::protocol("un perfil personal no puede pasar a familiar"));
            }
        }

        // 2) Cadena por autor
        if let Some((seq, prev_id)) = self.heads.get(&author) {
            if ev.body.seq <= *seq {
                return Err(CoreError::protocol("bifurcación: seq repetido con contenido distinto"));
            }
            if ev.body.seq != seq + 1 || ev.body.prev.as_deref() != Some(prev_id.as_str()) {
                self.gaps.push(Gap { author: author.clone(), expected_seq: seq + 1, got_seq: ev.body.seq });
            }
        } // None: primer evento visto de este autor (puede no ser seq 1 por retención)

        // 3) Aplicar
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

    /// Hechos ordenados por (hora, autor, seq): el mismo orden en todos los dispositivos.
    pub fn facts(&self) -> Vec<Fact> {
        self.authored().into_iter().map(|(_, f)| f).collect()
    }

    /// Como `facts`, con el autor de cada hecho.
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

    /// Último instante conocido de actividad por autor (latidos y cualquier otro evento).
    pub fn last_seen(&self) -> BTreeMap<String, i64> {
        let mut m = BTreeMap::new();
        for e in self.events.values() {
            let t = m.entry(e.body.author.clone()).or_insert(e.body.at_ms);
            *t = (*t).max(e.body.at_ms);
        }
        m
    }

    /// Instante del último evento conocido (de cualquier autor).
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
        Member { device_id: k.device_id(), age_recipient: k.age_recipient(), label: "x".into(), role: Role::Owner }
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
        l.accept(ev(&a, 3, Some("x".into()), EventKind::Heartbeat)).unwrap(); // falta el 2
        assert_eq!(l.gaps.len(), 1);
        let fork = ev(&a, 3, Some(gid), EventKind::EmergencyStarted);
        assert!(l.accept(fork).is_err());
    }

    // ── Perfil familiar ──

    fn as_role(k: &DeviceKeys, role: Role) -> Member {
        Member { role, ..member(k) }
    }

    /// Tutor (génesis) + menor añadido por el tutor. Devuelve el id del último evento del tutor.
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
            EventKind::Decision { request_id: "ab".into(), approved: true },
            EventKind::GoalCompleted { goal_id: "leer-30".into(), method: Method::Witness, score: 100, minutes: 0 },
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
        // Lo suyo sí: pedir, declarar una meta, latidos
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
            goal_id: "leer-30".into(),
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
        kinds.pop(); // el MembersChanged de prueba quitaría al tutor; se prueba aparte
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
        assert!(l.accept(owner).is_err(), "en un perfil familiar no hay dueños");

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
