// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reglas del día. `estado = f(configuración, eventos, ahora)`: función pura, sin reloj,
//! disco ni red. Prioridad: EMERGENCIA > DORMIR > PERMISO > LIBRE > ENFOQUE > BLOQUEADO.

use crate::clock::{Clock, DAY_MS, MINUTE_MS, day_start_ms, in_sleep, weekday_index};
use crate::model::{ALL_DAYS, Config, DayState, EventKind, Fact, Goal, GoalKind, GoalProgress, Method, Status};
use crate::strictness::{effective_from, goal_kept_or_harder, is_stricter_or_equal};

/// Límites del plan del día (cada plan viaja en un evento de 64 KB como máximo).
pub const MAX_PLAN_GOALS: usize = 20;
pub const MAX_TITLE_CHARS: usize = 80;
pub const MAX_ID_CHARS: usize = 32;

/// Historial de configuraciones: cada una con el instante desde el que rige.
#[derive(Debug, Clone, Default)]
pub struct ConfigHistory {
    entries: Vec<(i64, Config)>, // ordenado por instante
}

impl ConfigHistory {
    pub fn push(&mut self, effective_from_ms: i64, config: Config) {
        self.entries.push((effective_from_ms, config));
        self.entries.sort_by_key(|(t, _)| *t);
    }

    /// Configuración vigente en `ms` (la última cuyo inicio sea <= ms).
    pub fn active_at(&self, ms: i64) -> Option<&Config> {
        self.entries.iter().rev().find(|(t, _)| *t <= ms).map(|(_, c)| c)
    }

    /// Configuraciones que rigen en algún momento desde `ms`: la vigente y las ya programadas.
    fn in_force_from(&self, ms: i64) -> impl Iterator<Item = &Config> {
        let first = self.entries.iter().rposition(|(t, _)| *t <= ms).unwrap_or(0);
        self.entries[first..].iter().map(|(_, c)| c)
    }

    /// 🔒 Reconstruye el historial SIN fiarse de cuándo dice el autor que rige cada cambio
    /// (un cliente modificado podría declarar "rige ya" al relajar). Función de los hechos en su
    /// orden canónico: todos los dispositivos llegan al mismo resultado.
    /// - Tutor (perfil familiar): rige cuando declare (relajar es inmediato), nunca antes de firmarlo.
    /// - Menor (perfil familiar): solo cuenta si endurece TODO lo vigente o programado; si no, se ignora.
    /// - Perfil personal: relajar rige, como pronto, desde el próximo despertar (anti-impulso).
    pub fn from_facts_checked(facts: &[Fact], family: bool, utc_offset_min: i32) -> Self {
        let mut h = Self::default();
        for f in facts {
            let EventKind::ConfigChanged { config, effective_from_ms } = &f.kind else { continue };
            let from = if f.by_guardian {
                (*effective_from_ms).max(f.at_ms)
            } else if family {
                if !h.in_force_from(f.at_ms).all(|c| is_stricter_or_equal(config, c)) {
                    continue;
                }
                f.at_ms
            } else {
                let at = Clock { now_ms: f.at_ms, utc_offset_min };
                (*effective_from_ms).max(effective_from(config, h.active_at(f.at_ms), at))
            };
            h.push(from, config.clone());
        }
        h
    }

    /// (Perfil familiar) ¿Puede un menor proponer `config` ahora? Solo si endurece todo lo vigente.
    pub fn minor_may_set(&self, config: &Config, now_ms: i64) -> bool {
        self.in_force_from(now_ms).all(|c| is_stricter_or_equal(config, c))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum Denied {
    #[error("no hay configuración")]
    NotConfigured,
    #[error("no disponible en el estado actual")]
    WrongState,
    #[error("no quedan permisos hoy")]
    NoPermitsLeft,
    #[error("espera {minutes} min antes del siguiente permiso")]
    Cooldown { minutes: u16 },
    #[error("meta desconocida o inactiva hoy")]
    UnknownGoal,
    #[error("método no aceptado para esta meta")]
    MethodNotAllowed,
    #[error("la meta ya está cumplida")]
    AlreadyCompleted,
    #[error("el plan tiene metas mal formadas")]
    InvalidPlan,
    #[error("el plan de hoy solo se puede endurecer")]
    PlanLaxer,
    #[error("el plan necesita al menos una meta que desbloquee")]
    NoKeyGoal,
    #[error("solo tu tutor puede relajar las reglas; puedes pedírselo")]
    GuardianOnly,
}

/// ¿Es día libre el día que empieza en `day_start`?
fn is_free_day(cfg: &Config, day_start: i64, offset: i32) -> bool {
    cfg.free_days & (1u8 << weekday_index(day_start, offset)) != 0
}

/// La noche que termina en el próximo despertar es libre si mañana es día libre.
fn night_is_free(cfg: &Config, day_start: i64, offset: i32) -> bool {
    is_free_day(cfg, day_start + DAY_MS, offset)
}

/// Eventos con instante en [from, to).
fn window(facts: &[Fact], from: i64, to: i64) -> impl Iterator<Item = &Fact> {
    facts.iter().filter(move |f| f.at_ms >= from && f.at_ms < to)
}

/// Plan del día: el primero registrado; los siguientes solo cuentan si lo endurecen
/// (así dos dispositivos sin conexión, o uno manipulado, no pueden aflojarlo).
fn day_plan(facts: &[Fact], day_start: i64, now: i64) -> Option<Vec<Goal>> {
    let mut plan: Option<Vec<Goal>> = None;
    for f in window(facts, day_start, now + 1) {
        if let EventKind::DayPlanned { goals } = &f.kind {
            match &plan {
                Some(cur) if !cur.iter().all(|g| goal_kept_or_harder(g, goals)) => {}
                _ => plan = Some(goals.clone()),
            }
        }
    }
    plan
}

/// Metas de hoy: las fijas de la configuración para este día de la semana (endurecidas por
/// el plan si este lo hace) más las que el plan añade. Día libre → ninguna.
fn day_goals(cfg: &Config, plan: Option<&[Goal]>, day_start: i64, offset: i32) -> Vec<Goal> {
    if is_free_day(cfg, day_start, offset) {
        return vec![];
    }
    let plan = plan.unwrap_or(&[]);
    let bit = 1u8 << weekday_index(day_start, offset);
    let mut out: Vec<Goal> = cfg
        .goals
        .iter()
        .filter(|b| b.weekdays & bit != 0)
        .map(|b| plan.iter().find(|p| p.id == b.id && goal_kept_or_harder(b, std::slice::from_ref(p))).unwrap_or(b))
        .cloned()
        .collect();
    let extra: Vec<Goal> = plan.iter().filter(|p| !out.iter().any(|g| g.id == p.id)).cloned().collect();
    out.extend(extra);
    out
}

/// Minutos de ENFOQUE acumulados en una meta dentro de la ventana (pares inicio/fin;
/// si hay un inicio abierto cuenta hasta `now`).
fn focus_minutes(facts: &[Fact], goal_id: &str, from: i64, now: i64) -> u16 {
    let mut total_ms = 0i64;
    let mut open: Option<i64> = None;
    for f in window(facts, from, now + 1) {
        match &f.kind {
            EventKind::FocusStarted { goal_id: g } if g == goal_id => open = Some(f.at_ms),
            EventKind::FocusEnded { goal_id: g } if g == goal_id => {
                if let Some(start) = open.take() {
                    total_ms += f.at_ms - start;
                }
            }
            _ => {}
        }
    }
    if let Some(start) = open {
        total_ms += now - start;
    }
    (total_ms / MINUTE_MS).clamp(0, u16::MAX as i64) as u16
}

fn progress(goals: &[Goal], facts: &[Fact], day_start: i64, now: i64) -> Vec<GoalProgress> {
    goals
        .iter()
        .map(|g| {
            let done = window(facts, day_start, now + 1)
                .filter(|f| matches!(&f.kind, EventKind::GoalCompleted { goal_id, .. } if goal_id == &g.id))
                .count()
                .min(u8::MAX as usize) as u8;
            let target = g.target_count.max(1);
            GoalProgress {
                goal_id: g.id.clone(),
                title: g.title.clone(),
                kind: g.kind,
                done_count: done,
                target_count: target,
                focus_minutes: focus_minutes(facts, &g.id, day_start, now),
                target_minutes: g.target_minutes,
                completed: done >= target,
            }
        })
        .collect()
}

/// Permisos que se pierden hoy por lo ocurrido ayer.
fn penalty(cfg: &Config, facts: &[Fact], day_start: i64, offset: i32) -> u8 {
    let prev_start = day_start - DAY_MS;
    let emergencies =
        window(facts, prev_start, day_start).filter(|f| matches!(f.kind, EventKind::EmergencyStarted)).count() as u32;
    let mut total = emergencies * cfg.emergency_penalty as u32;
    if cfg.habit_penalty && !rest_day(facts, prev_start, day_start - 1) {
        // ponytail: usa la configuración de hoy para juzgar los hábitos de ayer.
        let goals = day_goals(cfg, day_plan(facts, prev_start, day_start - 1).as_deref(), prev_start, offset);
        let failed = progress(&goals, facts, prev_start, day_start - 1)
            .iter()
            .filter(|p| p.kind == GoalKind::Habit && !p.completed)
            .count() as u32;
        total += failed;
    }
    total.min(u8::MAX as u32) as u8
}

/// El tutor concedió hoy un día de descanso.
fn rest_day(facts: &[Fact], day_start: i64, now: i64) -> bool {
    window(facts, day_start, now + 1).any(|f| matches!(f.kind, EventKind::RestDayGranted))
}

/// Permisos extra concedidos por un tutor en [from, to), con su duración.
fn grants(facts: &[Fact], from: i64, to: i64) -> impl Iterator<Item = u16> + '_ {
    window(facts, from, to).filter_map(|f| match f.kind {
        EventKind::GrantPermit { minutes } => Some(minutes),
        _ => None,
    })
}

/// Permisos del día que empieza en `start` (sin la bolsa semanal): diarios − penalización + concedidos.
fn allowance(cfg: &Config, facts: &[Fact], start: i64, end: i64, offset: i32) -> u32 {
    cfg.permits_per_day.saturating_sub(penalty(cfg, facts, start, offset)) as u32
        + grants(facts, start, end).count() as u32
}

/// Permisos que quedan en la bolsa semanal: se gasta con lo usado por encima de lo de cada día
/// en los últimos 7 días (ventana móvil, contando hoy).
fn bonus_left(cfg: &Config, facts: &[Fact], day_start: i64, now: i64, offset: i32) -> u32 {
    if cfg.weekly_bonus_permits == 0 {
        return 0;
    }
    let used: u32 = (0..7)
        .map(|d| {
            let start = day_start - d * DAY_MS;
            let end = if d == 0 { now + 1 } else { start + DAY_MS };
            let passes = passes_today(facts, start, end - 1).count() as u32;
            passes.saturating_sub(allowance(cfg, facts, start, end, offset))
        })
        .sum();
    (cfg.weekly_bonus_permits as u32).saturating_sub(used)
}

fn passes_today(facts: &[Fact], day_start: i64, now: i64) -> impl Iterator<Item = (i64, u16)> + '_ {
    window(facts, day_start, now + 1).filter_map(|f| match f.kind {
        EventKind::PassStarted { minutes } => Some((f.at_ms, minutes)),
        _ => None,
    })
}

/// Próximo inicio de DORMIR que se aplicará.
fn sleep_at(cfg: &Config, day_start: i64, offset: i32) -> Option<i64> {
    let (b, w) = (cfg.schedule.bedtime.minutes as i64, cfg.schedule.wake.minutes as i64);
    if b == w || night_is_free(cfg, day_start, offset) {
        return None;
    }
    Some(day_start + (b - w).rem_euclid(24 * 60) * MINUTE_MS)
}

/// Metas de hoy con su definición completa (para prellenar el plan del día en la UI).
pub fn today_goals(history: &ConfigHistory, facts: &[Fact], clock: Clock) -> Vec<Goal> {
    let Some(cfg) = history.active_at(clock.now_ms) else { return vec![] };
    let day_start = day_start_ms(&cfg.schedule, clock);
    day_goals(cfg, day_plan(facts, day_start, clock.now_ms).as_deref(), day_start, clock.utc_offset_min)
}

/// Estado completo del día. Función pura: misma entrada → misma salida.
pub fn status(history: &ConfigHistory, facts: &[Fact], clock: Clock) -> Option<Status> {
    let now = clock.now_ms;
    let offset = clock.utc_offset_min;
    let cfg = history.active_at(now)?;
    let day_start = day_start_ms(&cfg.schedule, clock);
    // Día libre de la configuración o día de descanso concedido hoy por un tutor
    let free_day = is_free_day(cfg, day_start, offset) || rest_day(facts, day_start, now);
    let plan = day_plan(facts, day_start, now);
    let needs_plan = !free_day && plan.is_none();
    let goals = progress(&day_goals(cfg, plan.as_deref(), day_start, offset), facts, day_start, now);

    let used = passes_today(facts, day_start, now).count() as u32;
    let today = allowance(cfg, facts, day_start, now + 1, offset);
    let left = today.saturating_sub(used) + bonus_left(cfg, facts, day_start, now, offset);
    let permits_left = left.min(u8::MAX as u32) as u8;

    let state = compute_state(cfg, facts, clock, day_start, &goals, free_day, needs_plan);
    Some(Status {
        state,
        permits_left,
        day_start_ms: day_start,
        next_wake_ms: day_start + DAY_MS,
        sleep_at_ms: sleep_at(cfg, day_start, offset),
        free_day,
        needs_plan,
        goals,
    })
}

fn compute_state(
    cfg: &Config,
    facts: &[Fact],
    clock: Clock,
    day_start: i64,
    goals: &[GoalProgress],
    free_day: bool,
    needs_plan: bool,
) -> DayState {
    let now = clock.now_ms;

    // 1. EMERGENCIA: la última emergencia sigue vigente (puede venir de antes de dormir).
    let emergency_ms = cfg.emergency_minutes as i64 * MINUTE_MS;
    if let Some(f) = facts.iter().rev().find(|f| f.at_ms <= now && matches!(f.kind, EventKind::EmergencyStarted)) {
        let until = f.at_ms + emergency_ms;
        if until > now {
            return DayState::Emergency { until_ms: until };
        }
    }

    // 2. DORMIR, salvo la noche previa a un día libre
    if in_sleep(&cfg.schedule, clock.minute_of_day()) && !night_is_free(cfg, day_start, clock.utc_offset_min) {
        return DayState::Sleep;
    }

    // 3. PERMISO activo
    if let Some((at, minutes)) = passes_today(facts, day_start, now).last() {
        let until = at + minutes as i64 * MINUTE_MS;
        if until > now {
            return DayState::Pass { until_ms: until };
        }
    }

    // 4. LIBRE: día libre, o plan definido y todas sus metas llave cumplidas
    if free_day || (!needs_plan && goals.iter().filter(|g| g.kind == GoalKind::Key).all(|g| g.completed)) {
        return DayState::Free;
    }

    // 5. ENFOQUE: hay una sesión abierta (el último evento de foco de hoy es un inicio)
    let last_focus = window(facts, day_start, now + 1)
        .filter(|f| matches!(f.kind, EventKind::FocusStarted { .. } | EventKind::FocusEnded { .. }))
        .last();
    if let Some(Fact { kind: EventKind::FocusStarted { goal_id }, .. }) = last_focus {
        return DayState::Focus { goal_id: goal_id.clone() };
    }

    // 6. BLOQUEADO
    DayState::Locked
}

// ───────────────────────── Comandos ─────────────────────────
// Validan una acción del usuario y devuelven el evento a registrar.
// No guardan nada: el que llama (api.rs) firma y persiste.

pub fn cmd_start_pass(history: &ConfigHistory, facts: &[Fact], clock: Clock) -> Result<EventKind, Denied> {
    let st = status(history, facts, clock).ok_or(Denied::NotConfigured)?;
    let cfg = history.active_at(clock.now_ms).ok_or(Denied::NotConfigured)?;
    if !matches!(st.state, DayState::Locked | DayState::Focus { .. }) {
        return Err(Denied::WrongState);
    }
    if st.permits_left == 0 {
        return Err(Denied::NoPermitsLeft);
    }
    if let Some((at, minutes)) = passes_today(facts, st.day_start_ms, clock.now_ms).last() {
        let ready_at = at + (minutes as i64 + cfg.permit_cooldown_minutes as i64) * MINUTE_MS;
        if ready_at > clock.now_ms {
            let wait = ((ready_at - clock.now_ms + MINUTE_MS - 1) / MINUTE_MS) as u16;
            return Err(Denied::Cooldown { minutes: wait });
        }
    }
    // Se gastan primero los diarios, luego los concedidos por el tutor (con su duración) y al final la bolsa.
    let used = passes_today(facts, st.day_start_ms, clock.now_ms).count();
    let daily = cfg.permits_per_day.saturating_sub(penalty(cfg, facts, st.day_start_ms, clock.utc_offset_min)) as usize;
    let minutes = used
        .checked_sub(daily)
        .and_then(|k| grants(facts, st.day_start_ms, clock.now_ms + 1).nth(k))
        .unwrap_or(cfg.permit_minutes);
    Ok(EventKind::PassStarted { minutes })
}

fn find_goal(history: &ConfigHistory, facts: &[Fact], clock: Clock, goal_id: &str) -> Result<Goal, Denied> {
    today_goals(history, facts, clock).into_iter().find(|g| g.id == goal_id).ok_or(Denied::UnknownGoal)
}

pub fn cmd_start_focus(
    history: &ConfigHistory,
    facts: &[Fact],
    clock: Clock,
    goal_id: &str,
) -> Result<EventKind, Denied> {
    let goal = find_goal(history, facts, clock, goal_id)?;
    if !goal.methods.contains(&Method::FocusSession) {
        return Err(Denied::MethodNotAllowed);
    }
    let st = status(history, facts, clock).ok_or(Denied::NotConfigured)?;
    if !matches!(st.state, DayState::Locked | DayState::Free) {
        return Err(Denied::WrongState);
    }
    Ok(EventKind::FocusStarted { goal_id: goal_id.to_owned() })
}

/// Cierra la sesión de ENFOQUE. Si se alcanzaron los minutos, también registra la meta cumplida.
pub fn cmd_end_focus(history: &ConfigHistory, facts: &[Fact], clock: Clock) -> Result<Vec<EventKind>, Denied> {
    let st = status(history, facts, clock).ok_or(Denied::NotConfigured)?;
    let DayState::Focus { goal_id } = st.state else { return Err(Denied::WrongState) };
    let goal = find_goal(history, facts, clock, &goal_id)?;
    let minutes = focus_minutes(facts, &goal_id, st.day_start_ms, clock.now_ms);
    let mut out = vec![EventKind::FocusEnded { goal_id: goal_id.clone() }];
    let already = st.goals.iter().any(|g| g.goal_id == goal_id && g.completed);
    if !already && goal.target_minutes > 0 && minutes >= goal.target_minutes {
        out.push(EventKind::GoalCompleted { goal_id, method: Method::FocusSession, score: 100, minutes });
    }
    Ok(out)
}

pub fn cmd_complete_goal(
    history: &ConfigHistory,
    facts: &[Fact],
    clock: Clock,
    goal_id: &str,
    method: Method,
    score: u8,
    minutes: u16,
) -> Result<EventKind, Denied> {
    let goal = find_goal(history, facts, clock, goal_id)?;
    if !goal.methods.contains(&method) || method == Method::FocusSession {
        // FocusSession solo se completa vía cmd_end_focus (el núcleo mide el tiempo).
        return Err(Denied::MethodNotAllowed);
    }
    let st = status(history, facts, clock).ok_or(Denied::NotConfigured)?;
    if matches!(st.state, DayState::Sleep) {
        return Err(Denied::WrongState);
    }
    if st.goals.iter().any(|g| g.goal_id == goal_id && g.completed) {
        return Err(Denied::AlreadyCompleted);
    }
    Ok(EventKind::GoalCompleted { goal_id: goal_id.to_owned(), method, score: score.min(100), minutes })
}

pub fn cmd_emergency(history: &ConfigHistory, facts: &[Fact], clock: Clock) -> Result<EventKind, Denied> {
    let st = status(history, facts, clock).ok_or(Denied::NotConfigured)?;
    if matches!(st.state, DayState::Emergency { .. }) {
        return Err(Denied::WrongState);
    }
    Ok(EventKind::EmergencyStarted)
}

fn valid_goal(g: &Goal) -> bool {
    let id_ok = !g.id.is_empty()
        && g.id.len() <= MAX_ID_CHARS
        && g.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    let title_chars = g.title.trim().chars().count();
    id_ok && (1..=MAX_TITLE_CHARS).contains(&title_chars) && g.target_count >= 1 && !g.methods.is_empty()
}

/// Define (o endurece) las metas de hoy. Se pide al primer desbloqueo tras la hora de dormir.
/// `goals` es la lista completa del día; las fijas de la configuración se mantienen aunque falten.
pub fn cmd_plan_day(
    history: &ConfigHistory,
    facts: &[Fact],
    clock: Clock,
    mut goals: Vec<Goal>,
) -> Result<EventKind, Denied> {
    let cfg = history.active_at(clock.now_ms).ok_or(Denied::NotConfigured)?;
    let st = status(history, facts, clock).ok_or(Denied::NotConfigured)?;
    if st.free_day || matches!(st.state, DayState::Sleep) {
        return Err(Denied::WrongState);
    }
    let unique = goals.iter().enumerate().all(|(i, g)| goals[..i].iter().all(|o| o.id != g.id));
    if goals.len() > MAX_PLAN_GOALS || !unique || !goals.iter().all(valid_goal) {
        return Err(Denied::InvalidPlan);
    }
    for g in &mut goals {
        g.title = g.title.trim().to_owned();
        g.weekdays = ALL_DAYS; // en un plan diario no aplica; así "endurecer" nunca falla por la máscara
    }
    if let Some(current) = day_plan(facts, st.day_start_ms, clock.now_ms)
        && !current.iter().all(|g| goal_kept_or_harder(g, &goals))
    {
        return Err(Denied::PlanLaxer);
    }
    let result = day_goals(cfg, Some(&goals), st.day_start_ms, clock.utc_offset_min);
    if !result.iter().any(|g| g.kind == GoalKind::Key) {
        return Err(Denied::NoKeyGoal);
    }
    Ok(EventKind::DayPlanned { goals })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::local_at;
    use crate::model::{Config, Method};

    /// 2026-03-09 (lunes) a la hora local indicada, con desfase 0.
    fn at(h: i64, m: i64) -> Clock {
        let monday_midnight = 1_773_014_400_000; // 2026-03-09 00:00 UTC
        Clock { now_ms: monday_midnight + (h * 60 + m) * MINUTE_MS, utc_offset_min: 0 }
    }

    /// Lunes + `days` días a la hora indicada.
    fn on(days: i64, h: i64, m: i64) -> Clock {
        Clock { now_ms: at(h, m).now_ms + days * DAY_MS, utc_offset_min: 0 }
    }

    fn setup() -> (ConfigHistory, Vec<Fact>) {
        let mut h = ConfigHistory::default();
        h.push(0, Config::example());
        (h, vec![])
    }

    fn record(facts: &mut Vec<Fact>, clock: Clock, kind: EventKind) {
        facts.push(Fact::new(clock.now_ms, kind));
    }

    /// Acepta las metas fijas del día (lo que hace la UI al pulsar "Empezar el día").
    fn plan(h: &ConfigHistory, facts: &mut Vec<Fact>, clock: Clock) {
        let ev = cmd_plan_day(h, facts, clock, vec![]).unwrap();
        record(facts, clock, ev);
    }

    fn goal(id: &str, kind: GoalKind, minutes: u16) -> Goal {
        Goal {
            id: id.into(),
            title: "Meta".into(),
            kind,
            target_minutes: minutes,
            target_count: 1,
            methods: vec![Method::FocusSession, Method::HonorDeclaration],
            weekdays: ALL_DAYS,
        }
    }

    #[test]
    fn helper_date_is_monday() {
        assert_eq!(local_at(at(0, 0).now_ms, 0).to_string(), "2026-03-09 00:00:00");
    }

    #[test]
    fn night_is_sleep_and_morning_is_locked_until_planned() {
        let (h, mut f) = setup();
        assert_eq!(status(&h, &f, at(23, 0)).unwrap().state, DayState::Sleep);
        let st = status(&h, &f, at(7, 0)).unwrap();
        assert_eq!(st.state, DayState::Locked);
        assert!(st.needs_plan);
        plan(&h, &mut f, at(7, 0));
        assert!(!status(&h, &f, at(7, 1)).unwrap().needs_plan);
    }

    #[test]
    fn pass_lasts_its_minutes_and_consumes_a_permit() {
        let (h, mut f) = setup();
        let ev = cmd_start_pass(&h, &f, at(9, 0)).unwrap();
        record(&mut f, at(9, 0), ev);
        let st = status(&h, &f, at(9, 4)).unwrap();
        assert!(matches!(st.state, DayState::Pass { .. }));
        assert_eq!(st.permits_left, 2);
        assert_eq!(status(&h, &f, at(9, 5)).unwrap().state, DayState::Locked);
    }

    #[test]
    fn cooldown_blocks_back_to_back_passes() {
        let (h, mut f) = setup();
        record(&mut f, at(9, 0), EventKind::PassStarted { minutes: 5 });
        assert_eq!(cmd_start_pass(&h, &f, at(9, 10)), Err(Denied::Cooldown { minutes: 55 }));
        assert!(cmd_start_pass(&h, &f, at(10, 5)).is_ok());
    }

    #[test]
    fn focus_session_completes_goal_and_frees_the_day() {
        let (h, mut f) = setup();
        plan(&h, &mut f, at(7, 59));
        let ev = cmd_start_focus(&h, &f, at(8, 0), "leer-30").unwrap();
        record(&mut f, at(8, 0), ev);
        assert_eq!(status(&h, &f, at(8, 10)).unwrap().state, DayState::Focus { goal_id: "leer-30".into() });
        for ev in cmd_end_focus(&h, &f, at(8, 31)).unwrap() {
            record(&mut f, at(8, 31), ev);
        }
        // "comer-sano" es hábito: no impide pasar a LIBRE
        assert_eq!(status(&h, &f, at(8, 32)).unwrap().state, DayState::Free);
        // …pero al llegar la noche, se duerme igual
        assert_eq!(status(&h, &f, at(22, 31)).unwrap().state, DayState::Sleep);
    }

    #[test]
    fn goals_done_without_a_plan_do_not_free_the_day() {
        let (h, mut f) = setup();
        record(
            &mut f,
            at(8, 0),
            EventKind::GoalCompleted {
                goal_id: "leer-30".into(),
                method: Method::HonorDeclaration,
                score: 100,
                minutes: 30,
            },
        );
        assert_eq!(status(&h, &f, at(8, 1)).unwrap().state, DayState::Locked, "falta definir el plan");
        plan(&h, &mut f, at(8, 2));
        assert_eq!(status(&h, &f, at(8, 3)).unwrap().state, DayState::Free);
    }

    #[test]
    fn short_focus_does_not_complete_goal() {
        let (h, mut f) = setup();
        record(&mut f, at(8, 0), EventKind::FocusStarted { goal_id: "leer-30".into() });
        let evs = cmd_end_focus(&h, &f, at(8, 10)).unwrap();
        assert_eq!(evs, vec![EventKind::FocusEnded { goal_id: "leer-30".into() }]);
    }

    #[test]
    fn emergency_overrides_sleep_and_costs_a_permit_tomorrow() {
        let (h, mut f) = setup();
        record(&mut f, at(23, 0), EventKind::EmergencyStarted);
        assert!(matches!(status(&h, &f, at(23, 10)).unwrap().state, DayState::Emergency { .. }));
        assert_eq!(status(&h, &f, at(23, 16)).unwrap().state, DayState::Sleep);
        // Martes: 3 permisos − 1 (emergencia) − 1 (hábito "comer-sano" incumplido el lunes) = 1
        assert_eq!(status(&h, &f, on(1, 9, 0)).unwrap().permits_left, 1);
    }

    #[test]
    fn honor_declaration_completes_habit_but_not_twice_beyond_target() {
        let (h, mut f) = setup();
        for minute in [0, 1] {
            let ev = cmd_complete_goal(&h, &f, at(13, minute), "comer-sano", Method::HonorDeclaration, 100, 0).unwrap();
            record(&mut f, at(13, minute), ev);
        }
        assert_eq!(
            cmd_complete_goal(&h, &f, at(20, 0), "comer-sano", Method::HonorDeclaration, 100, 0),
            Err(Denied::AlreadyCompleted)
        );
    }

    #[test]
    fn no_passes_during_sleep() {
        let (h, f) = setup();
        assert_eq!(cmd_start_pass(&h, &f, at(23, 30)), Err(Denied::WrongState));
    }

    // ── Días libres (ejemplo: sábado y domingo) ──

    #[test]
    fn free_days_have_no_goals_nor_lock() {
        let (h, f) = setup();
        let saturday = on(5, 10, 0);
        let st = status(&h, &f, saturday).unwrap();
        assert_eq!(st.state, DayState::Free);
        assert!(st.free_day && !st.needs_plan && st.goals.is_empty());
        assert_eq!(cmd_plan_day(&h, &f, saturday, vec![]), Err(Denied::WrongState));
        assert_eq!(cmd_start_pass(&h, &f, saturday), Err(Denied::WrongState));
    }

    #[test]
    fn night_before_a_free_day_has_no_sleep_but_sunday_night_does() {
        let (h, f) = setup();
        // Viernes 23:00: mañana es sábado (libre) → sin DORMIR. El viernes sigue pidiendo sus metas.
        let friday_night = status(&h, &f, on(4, 23, 0)).unwrap();
        assert_eq!(friday_night.state, DayState::Locked);
        assert_eq!(friday_night.sleep_at_ms, None);
        // Sábado 23:30 y domingo 03:00: libres
        assert_eq!(status(&h, &f, on(5, 23, 30)).unwrap().state, DayState::Free);
        assert_eq!(status(&h, &f, on(6, 3, 0)).unwrap().state, DayState::Free);
        // Domingo 23:00: mañana es lunes → DORMIR
        assert_eq!(status(&h, &f, on(6, 23, 0)).unwrap().state, DayState::Sleep);
        assert_eq!(status(&h, &f, on(6, 12, 0)).unwrap().sleep_at_ms, Some(on(6, 22, 30).now_ms));
    }

    #[test]
    fn free_day_does_not_judge_habits_for_monday_permits() {
        let (h, f) = setup();
        // Lunes de la semana siguiente: el domingo fue libre → sin penalización por hábitos
        assert_eq!(status(&h, &f, on(7, 9, 0)).unwrap().permits_left, 3);
    }

    // ── Plan del día ──

    #[test]
    fn plan_keeps_fixed_goals_and_can_add_more() {
        let (h, mut f) = setup();
        let extra = goal("estudiar", GoalKind::Key, 60);
        let ev = cmd_plan_day(&h, &f, at(7, 0), vec![extra]).unwrap();
        record(&mut f, at(7, 0), ev);
        let ids: Vec<String> = status(&h, &f, at(7, 1)).unwrap().goals.into_iter().map(|g| g.goal_id).collect();
        assert_eq!(ids, ["leer-30", "comer-sano", "estudiar"]);
    }

    #[test]
    fn plan_cannot_weaken_a_fixed_goal() {
        let (h, mut f) = setup();
        let mut lazy = goal("leer-30", GoalKind::Key, 5); // la fija pide 30 min
        lazy.methods = vec![Method::FocusSession];
        let ev = cmd_plan_day(&h, &f, at(7, 0), vec![lazy]).unwrap();
        record(&mut f, at(7, 0), ev);
        let leer = status(&h, &f, at(7, 1)).unwrap().goals.into_iter().find(|g| g.goal_id == "leer-30").unwrap();
        assert_eq!(leer.target_minutes, 30, "se mantiene la versión fija");
    }

    #[test]
    fn plan_can_only_get_harder_during_the_day() {
        let (h, mut f) = setup();
        let ev = cmd_plan_day(&h, &f, at(7, 0), vec![goal("estudiar", GoalKind::Key, 60)]).unwrap();
        record(&mut f, at(7, 0), ev);
        assert_eq!(cmd_plan_day(&h, &f, at(9, 0), vec![]), Err(Denied::PlanLaxer));
        assert_eq!(cmd_plan_day(&h, &f, at(9, 0), vec![goal("estudiar", GoalKind::Key, 30)]), Err(Denied::PlanLaxer));
        assert!(cmd_plan_day(&h, &f, at(9, 0), vec![goal("estudiar", GoalKind::Key, 90)]).is_ok());
    }

    #[test]
    fn a_laxer_plan_from_another_device_is_ignored() {
        let (h, mut f) = setup();
        record(&mut f, at(7, 0), EventKind::DayPlanned { goals: vec![goal("estudiar", GoalKind::Key, 60)] });
        record(&mut f, at(7, 5), EventKind::DayPlanned { goals: vec![] }); // otro dispositivo, sin conexión
        let ids: Vec<String> = status(&h, &f, at(7, 6)).unwrap().goals.into_iter().map(|g| g.goal_id).collect();
        assert!(ids.contains(&"estudiar".to_string()));
    }

    #[test]
    fn plan_needs_a_key_goal_and_valid_goals() {
        let mut cfg = Config::example();
        cfg.goals.retain(|g| g.kind == GoalKind::Habit);
        let mut h = ConfigHistory::default();
        h.push(0, cfg);
        assert_eq!(cmd_plan_day(&h, &[], at(7, 0), vec![]), Err(Denied::NoKeyGoal));
        let mut bad = goal("Con Espacios", GoalKind::Key, 10);
        assert_eq!(cmd_plan_day(&h, &[], at(7, 0), vec![bad.clone()]), Err(Denied::InvalidPlan));
        bad.id = "ok".into();
        bad.title = "   ".into();
        assert_eq!(cmd_plan_day(&h, &[], at(7, 0), vec![bad]), Err(Denied::InvalidPlan));
        let dup = vec![goal("a", GoalKind::Key, 10), goal("a", GoalKind::Key, 20)];
        assert_eq!(cmd_plan_day(&h, &[], at(7, 0), dup), Err(Denied::InvalidPlan));
    }

    #[test]
    fn planned_goal_can_be_focused_and_frees_the_day() {
        let mut cfg = Config::example();
        cfg.goals.clear(); // sin metas fijas: todo se define por la mañana
        let mut h = ConfigHistory::default();
        h.push(0, cfg);
        let mut f = vec![];
        let ev = cmd_plan_day(&h, &f, at(7, 0), vec![goal("estudiar", GoalKind::Key, 20)]).unwrap();
        record(&mut f, at(7, 0), ev);
        let ev = cmd_start_focus(&h, &f, at(8, 0), "estudiar").unwrap();
        record(&mut f, at(8, 0), ev);
        for ev in cmd_end_focus(&h, &f, at(8, 20)).unwrap() {
            record(&mut f, at(8, 20), ev);
        }
        assert_eq!(status(&h, &f, at(8, 21)).unwrap().state, DayState::Free);
        // Al día siguiente el plan no se arrastra
        assert!(status(&h, &f, on(1, 7, 0)).unwrap().needs_plan);
    }

    // ── Modo familiar (Fase 3) ──

    #[test]
    fn guardian_grant_and_weekly_bonus_add_permits() {
        let mut h = ConfigHistory::default();
        let mut cfg = Config::example();
        cfg.weekly_bonus_permits = 2;
        h.push(0, cfg);
        let mut f = vec![];
        record(&mut f, at(7, 0), EventKind::GrantPermit { minutes: 15 });
        // 3 diarios + 1 concedido + 2 de la bolsa = 6
        assert_eq!(status(&h, &f, at(7, 1)).unwrap().permits_left, 6);
    }

    #[test]
    fn granted_permit_lasts_its_minutes_after_the_daily_ones() {
        let (h, mut f) = setup();
        record(&mut f, at(7, 0), EventKind::GrantPermit { minutes: 15 });
        for hour in [8, 10, 12] {
            let ev = cmd_start_pass(&h, &f, at(hour, 0)).unwrap();
            assert_eq!(ev, EventKind::PassStarted { minutes: 5 }, "primero los diarios");
            record(&mut f, at(hour, 0), ev);
        }
        assert_eq!(cmd_start_pass(&h, &f, at(14, 0)).unwrap(), EventKind::PassStarted { minutes: 15 });
    }

    #[test]
    fn weekly_bonus_is_spent_across_days_and_renews_after_a_week() {
        let mut cfg = Config::example();
        cfg.weekly_bonus_permits = 2;
        cfg.habit_penalty = false;
        cfg.free_days = 0;
        let mut h = ConfigHistory::default();
        h.push(0, cfg);
        let mut f = vec![];
        // Lunes: 4 permisos (3 diarios + 1 de la bolsa)
        for hour in [8, 10, 12, 14] {
            let ev = cmd_start_pass(&h, &f, at(hour, 0)).unwrap();
            record(&mut f, at(hour, 0), ev);
        }
        assert_eq!(status(&h, &f, at(16, 0)).unwrap().permits_left, 1, "queda 1 en la bolsa");
        assert_eq!(status(&h, &f, on(1, 9, 0)).unwrap().permits_left, 4, "martes: 3 + 1 de la bolsa");
        assert_eq!(status(&h, &f, on(7, 9, 0)).unwrap().permits_left, 5, "lunes siguiente: bolsa completa");
    }

    #[test]
    fn rest_day_frees_the_day_without_habit_penalty() {
        let (h, mut f) = setup();
        record(&mut f, at(9, 0), EventKind::RestDayGranted);
        let st = status(&h, &f, at(9, 1)).unwrap();
        assert_eq!(st.state, DayState::Free);
        assert!(st.free_day && !st.needs_plan);
        assert_eq!(status(&h, &f, at(23, 0)).unwrap().state, DayState::Sleep, "hasta dormir");
        // Martes: sin penalización por el hábito no cumplido el lunes
        assert_eq!(status(&h, &f, on(1, 9, 0)).unwrap().permits_left, 3);
    }

    fn change(at: Clock, config: Config, by_guardian: bool) -> Fact {
        Fact { at_ms: at.now_ms, kind: EventKind::ConfigChanged { config, effective_from_ms: at.now_ms }, by_guardian }
    }

    #[test]
    fn personal_relaxing_waits_for_next_wake_even_if_declared_now() {
        let mut laxer = Config::example();
        laxer.permits_per_day = 9;
        let facts = vec![change(at(7, 0), Config::example(), false), change(at(9, 0), laxer, false)];
        let h = ConfigHistory::from_facts_checked(&facts, false, 0);
        assert_eq!(h.active_at(at(12, 0).now_ms).unwrap().permits_per_day, 3, "un cliente modificado no lo adelanta");
        assert_eq!(h.active_at(on(1, 7, 0).now_ms).unwrap().permits_per_day, 9);
    }

    #[test]
    fn minor_can_only_harden_and_guardian_relaxes_at_once() {
        let base = change(at(7, 0), Config::example(), true);
        let mut laxer = Config::example();
        laxer.permits_per_day = 9;
        let mut stricter = Config::example();
        stricter.permits_per_day = 1;

        let h = ConfigHistory::from_facts_checked(&[base.clone(), change(at(9, 0), laxer.clone(), false)], true, 0);
        assert_eq!(h.active_at(on(3, 9, 0).now_ms).unwrap().permits_per_day, 3, "el menor no puede relajar, ni mañana");

        let h = ConfigHistory::from_facts_checked(&[base.clone(), change(at(9, 0), stricter.clone(), false)], true, 0);
        assert_eq!(h.active_at(at(9, 0).now_ms).unwrap().permits_per_day, 1, "endurecer rige ya");

        let h = ConfigHistory::from_facts_checked(&[base.clone(), change(at(9, 0), laxer.clone(), true)], true, 0);
        assert_eq!(h.active_at(at(9, 0).now_ms).unwrap().permits_per_day, 9, "el tutor relaja al momento");

        // El tutor endurece y el menor intenta volver a lo de antes con un evento posterior
        let facts = [base, change(at(9, 0), stricter, true), change(at(10, 0), Config::example(), false)];
        let h = ConfigHistory::from_facts_checked(&facts, true, 0);
        assert_eq!(h.active_at(on(1, 9, 0).now_ms).unwrap().permits_per_day, 1);
    }
}
