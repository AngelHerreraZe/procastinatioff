// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Day rules. `state = f(configuration, events, now)`: a pure function, without clock,
//! disk or network. Priority: EMERGENCY > SLEEP > PASS > FREE > FOCUS > LOCKED.

use crate::clock::{Clock, DAY_MS, MINUTE_MS, day_start_ms, in_sleep, weekday_index};
use crate::model::{ALL_DAYS, Config, DayState, EventKind, Fact, Goal, GoalKind, GoalProgress, Method, Status};
use crate::strictness::{effective_from, goal_kept_or_harder, is_stricter_or_equal};

/// Limits of the day plan (each plan travels in an event of at most 64 KB).
pub const MAX_PLAN_GOALS: usize = 20;
pub const MAX_TITLE_CHARS: usize = 80;
pub const MAX_ID_CHARS: usize = 32;

/// Configuration history: each one with the instant it takes effect.
#[derive(Debug, Clone, Default)]
pub struct ConfigHistory {
    entries: Vec<(i64, Config)>, // sorted by instant
}

impl ConfigHistory {
    pub fn push(&mut self, effective_from_ms: i64, config: Config) {
        self.entries.push((effective_from_ms, config));
        self.entries.sort_by_key(|(t, _)| *t);
    }

    /// Configuration in force at `ms` (the last one starting <= ms).
    pub fn active_at(&self, ms: i64) -> Option<&Config> {
        self.entries.iter().rev().find(|(t, _)| *t <= ms).map(|(_, c)| c)
    }

    /// Configurations in force at some point from `ms`: the current one and those already scheduled.
    fn in_force_from(&self, ms: i64) -> impl Iterator<Item = &Config> {
        let first = self.entries.iter().rposition(|(t, _)| *t <= ms).unwrap_or(0);
        self.entries[first..].iter().map(|(_, c)| c)
    }

    /// SECURITY: Rebuilds the history WITHOUT trusting when the author says each change takes effect
    /// (a modified client could declare "effective now" when relaxing). A function of the facts in
    /// their canonical order: every device reaches the same result.
    /// - Guardian (family profile): effective when they declare (relaxing is immediate), never before signing.
    /// - Minor (family profile): only counts if it tightens EVERYTHING in force or scheduled; otherwise ignored.
    /// - Personal profile: relaxing takes effect at the next wake-up at the earliest (anti-impulse).
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

    /// (Family profile) May a minor propose `config` now? Only if it tightens everything in force.
    pub fn minor_may_set(&self, config: &Config, now_ms: i64) -> bool {
        self.in_force_from(now_ms).all(|c| is_stricter_or_equal(config, c))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum Denied {
    #[error("no configuration")]
    NotConfigured,
    #[error("not available in the current state")]
    WrongState,
    #[error("no passes left today")]
    NoPermitsLeft,
    #[error("wait {minutes} min before the next pass")]
    Cooldown { minutes: u16 },
    #[error("unknown goal or not active today")]
    UnknownGoal,
    #[error("method not accepted for this goal")]
    MethodNotAllowed,
    #[error("goal already completed")]
    AlreadyCompleted,
    #[error("the plan has malformed goals")]
    InvalidPlan,
    #[error("today's plan can only be made stricter")]
    PlanLaxer,
    #[error("the plan needs at least one unlocking goal")]
    NoKeyGoal,
    #[error("only your guardian can relax the rules; you can ask them")]
    GuardianOnly,
    #[error("invalid domain")]
    InvalidDomain,
    #[error("too many requests awaiting an answer")]
    TooManyRequests,
}

impl Denied {
    /// Stable code that every UI translates (`denied.<code>` in the locale files).
    /// Never rename a code: translations depend on it.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::WrongState => "wrong_state",
            Self::NoPermitsLeft => "no_permits_left",
            Self::Cooldown { .. } => "cooldown",
            Self::UnknownGoal => "unknown_goal",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::AlreadyCompleted => "already_completed",
            Self::InvalidPlan => "invalid_plan",
            Self::PlanLaxer => "plan_laxer",
            Self::NoKeyGoal => "no_key_goal",
            Self::GuardianOnly => "guardian_only",
            Self::InvalidDomain => "invalid_domain",
            Self::TooManyRequests => "too_many_requests",
        }
    }
}

/// Is the day starting at `day_start` a free day?
fn is_free_day(cfg: &Config, day_start: i64, offset: i32) -> bool {
    cfg.free_days & (1u8 << weekday_index(day_start, offset)) != 0
}

/// The night ending at the next wake-up is free if tomorrow is a free day.
fn night_is_free(cfg: &Config, day_start: i64, offset: i32) -> bool {
    is_free_day(cfg, day_start + DAY_MS, offset)
}

/// Events with an instant in [from, to).
fn window(facts: &[Fact], from: i64, to: i64) -> impl Iterator<Item = &Fact> {
    facts.iter().filter(move |f| f.at_ms >= from && f.at_ms < to)
}

/// Day plan: the first one recorded; later ones only count if they tighten it
/// (so two offline devices, or a tampered one, cannot loosen it).
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

/// Today's goals: the configuration's fixed ones for this weekday (tightened by
/// the plan if it does so) plus those the plan adds. Free day → none.
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

/// FOCUS minutes accumulated on a goal within the window (start/end pairs;
/// an open start counts until `now`).
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

/// Passes lost today because of what happened yesterday.
fn penalty(cfg: &Config, facts: &[Fact], day_start: i64, offset: i32) -> u8 {
    let prev_start = day_start - DAY_MS;
    let emergencies =
        window(facts, prev_start, day_start).filter(|f| matches!(f.kind, EventKind::EmergencyStarted)).count() as u32;
    let mut total = emergencies * cfg.emergency_penalty as u32;
    if cfg.habit_penalty && !rest_day(facts, prev_start, day_start - 1) {
        // ponytail: uses today's configuration to judge yesterday's habits.
        let goals = day_goals(cfg, day_plan(facts, prev_start, day_start - 1).as_deref(), prev_start, offset);
        let failed = progress(&goals, facts, prev_start, day_start - 1)
            .iter()
            .filter(|p| p.kind == GoalKind::Habit && !p.completed)
            .count() as u32;
        total += failed;
    }
    total.min(u8::MAX as u32) as u8
}

/// The guardian granted a rest day today.
fn rest_day(facts: &[Fact], day_start: i64, now: i64) -> bool {
    window(facts, day_start, now + 1).any(|f| matches!(f.kind, EventKind::RestDayGranted))
}

/// Extra passes granted by a guardian in [from, to), with their duration.
fn grants(facts: &[Fact], from: i64, to: i64) -> impl Iterator<Item = u16> + '_ {
    window(facts, from, to).filter_map(|f| match f.kind {
        EventKind::GrantPermit { minutes } => Some(minutes),
        _ => None,
    })
}

/// Passes of the day starting at `start` (without the weekly pool): daily − penalty + granted.
fn allowance(cfg: &Config, facts: &[Fact], start: i64, end: i64, offset: i32) -> u32 {
    cfg.permits_per_day.saturating_sub(penalty(cfg, facts, start, offset)) as u32
        + grants(facts, start, end).count() as u32
}

/// Passes left in the weekly pool: spent by whatever was used above each day's allowance
/// over the last 7 days (rolling window, including today).
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

/// Next start of SLEEP that will apply.
fn sleep_at(cfg: &Config, day_start: i64, offset: i32) -> Option<i64> {
    let (b, w) = (cfg.schedule.bedtime.minutes as i64, cfg.schedule.wake.minutes as i64);
    if b == w || night_is_free(cfg, day_start, offset) {
        return None;
    }
    Some(day_start + (b - w).rem_euclid(24 * 60) * MINUTE_MS)
}

/// Today's goals with their full definition (to prefill the day plan in the UI).
pub fn today_goals(history: &ConfigHistory, facts: &[Fact], clock: Clock) -> Vec<Goal> {
    let Some(cfg) = history.active_at(clock.now_ms) else { return vec![] };
    let day_start = day_start_ms(&cfg.schedule, clock);
    day_goals(cfg, day_plan(facts, day_start, clock.now_ms).as_deref(), day_start, clock.utc_offset_min)
}

/// Full state of the day. Pure function: same input → same output.
pub fn status(history: &ConfigHistory, facts: &[Fact], clock: Clock) -> Option<Status> {
    let now = clock.now_ms;
    let offset = clock.utc_offset_min;
    let cfg = history.active_at(now)?;
    let day_start = day_start_ms(&cfg.schedule, clock);
    // Free day from the configuration or rest day granted today by a guardian
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

    // 1. EMERGENCY: the last emergency is still running (it may come from before bedtime).
    let emergency_ms = cfg.emergency_minutes as i64 * MINUTE_MS;
    if let Some(f) = facts.iter().rev().find(|f| f.at_ms <= now && matches!(f.kind, EventKind::EmergencyStarted)) {
        let until = f.at_ms + emergency_ms;
        if until > now {
            return DayState::Emergency { until_ms: until };
        }
    }

    // 2. SLEEP, except the night before a free day
    if in_sleep(&cfg.schedule, clock.minute_of_day()) && !night_is_free(cfg, day_start, clock.utc_offset_min) {
        return DayState::Sleep;
    }

    // 3. Active PASS
    if let Some((at, minutes)) = passes_today(facts, day_start, now).last() {
        let until = at + minutes as i64 * MINUTE_MS;
        if until > now {
            return DayState::Pass { until_ms: until };
        }
    }

    // 4. FREE: free day, or plan set and all its key goals completed
    if free_day || (!needs_plan && goals.iter().filter(|g| g.kind == GoalKind::Key).all(|g| g.completed)) {
        return DayState::Free;
    }

    // 5. FOCUS: there is an open session (today's last focus event is a start)
    let last_focus = window(facts, day_start, now + 1)
        .filter(|f| matches!(f.kind, EventKind::FocusStarted { .. } | EventKind::FocusEnded { .. }))
        .last();
    if let Some(Fact { kind: EventKind::FocusStarted { goal_id }, .. }) = last_focus {
        return DayState::Focus { goal_id: goal_id.clone() };
    }

    // 6. LOCKED
    DayState::Locked
}

// ───────────────────────── Commands ─────────────────────────
// They validate a user action and return the event to record.
// They store nothing: the caller (api.rs) signs and persists.

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
    // Daily passes are spent first, then those granted by the guardian (with their duration) and finally the pool.
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

/// Closes the FOCUS session. If the minutes were reached, it also records the completed goal.
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
        // FocusSession is only completed via cmd_end_focus (the core measures the time).
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

/// Sets (or tightens) today's goals. Asked at the first unlock after bedtime.
/// `goals` is the full list for the day; the configuration's fixed ones are kept even if missing.
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
        g.weekdays = ALL_DAYS; // not applicable to a daily plan; this way "tightening" never fails because of the mask
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

    /// 2026-03-09 (Monday) at the given local time, with offset 0.
    fn at(h: i64, m: i64) -> Clock {
        let monday_midnight = 1_773_014_400_000; // 2026-03-09 00:00 UTC
        Clock { now_ms: monday_midnight + (h * 60 + m) * MINUTE_MS, utc_offset_min: 0 }
    }

    /// Monday + `days` days at the given time.
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

    /// Accepts the day's fixed goals (what the UI does on "Start the day").
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
        let ev = cmd_start_focus(&h, &f, at(8, 0), "read-30").unwrap();
        record(&mut f, at(8, 0), ev);
        assert_eq!(status(&h, &f, at(8, 10)).unwrap().state, DayState::Focus { goal_id: "read-30".into() });
        for ev in cmd_end_focus(&h, &f, at(8, 31)).unwrap() {
            record(&mut f, at(8, 31), ev);
        }
        // "eat-healthy" is a habit: it does not prevent reaching FREE
        assert_eq!(status(&h, &f, at(8, 32)).unwrap().state, DayState::Free);
        // …but when night comes, sleep happens anyway
        assert_eq!(status(&h, &f, at(22, 31)).unwrap().state, DayState::Sleep);
    }

    #[test]
    fn goals_done_without_a_plan_do_not_free_the_day() {
        let (h, mut f) = setup();
        record(
            &mut f,
            at(8, 0),
            EventKind::GoalCompleted {
                goal_id: "read-30".into(),
                method: Method::HonorDeclaration,
                score: 100,
                minutes: 30,
            },
        );
        assert_eq!(status(&h, &f, at(8, 1)).unwrap().state, DayState::Locked, "the plan is still missing");
        plan(&h, &mut f, at(8, 2));
        assert_eq!(status(&h, &f, at(8, 3)).unwrap().state, DayState::Free);
    }

    #[test]
    fn short_focus_does_not_complete_goal() {
        let (h, mut f) = setup();
        record(&mut f, at(8, 0), EventKind::FocusStarted { goal_id: "read-30".into() });
        let evs = cmd_end_focus(&h, &f, at(8, 10)).unwrap();
        assert_eq!(evs, vec![EventKind::FocusEnded { goal_id: "read-30".into() }]);
    }

    #[test]
    fn emergency_overrides_sleep_and_costs_a_permit_tomorrow() {
        let (h, mut f) = setup();
        record(&mut f, at(23, 0), EventKind::EmergencyStarted);
        assert!(matches!(status(&h, &f, at(23, 10)).unwrap().state, DayState::Emergency { .. }));
        assert_eq!(status(&h, &f, at(23, 16)).unwrap().state, DayState::Sleep);
        // Tuesday: 3 passes − 1 (emergency) − 1 (habit "eat-healthy" missed on Monday) = 1
        assert_eq!(status(&h, &f, on(1, 9, 0)).unwrap().permits_left, 1);
    }

    #[test]
    fn honor_declaration_completes_habit_but_not_twice_beyond_target() {
        let (h, mut f) = setup();
        for minute in [0, 1] {
            let ev =
                cmd_complete_goal(&h, &f, at(13, minute), "eat-healthy", Method::HonorDeclaration, 100, 0).unwrap();
            record(&mut f, at(13, minute), ev);
        }
        assert_eq!(
            cmd_complete_goal(&h, &f, at(20, 0), "eat-healthy", Method::HonorDeclaration, 100, 0),
            Err(Denied::AlreadyCompleted)
        );
    }

    #[test]
    fn no_passes_during_sleep() {
        let (h, f) = setup();
        assert_eq!(cmd_start_pass(&h, &f, at(23, 30)), Err(Denied::WrongState));
    }

    // ── Free days (example: Saturday and Sunday) ──

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
        // Friday 23:00: tomorrow is Saturday (free) → no SLEEP. Friday still asks for its goals.
        let friday_night = status(&h, &f, on(4, 23, 0)).unwrap();
        assert_eq!(friday_night.state, DayState::Locked);
        assert_eq!(friday_night.sleep_at_ms, None);
        // Saturday 23:30 and Sunday 03:00: free
        assert_eq!(status(&h, &f, on(5, 23, 30)).unwrap().state, DayState::Free);
        assert_eq!(status(&h, &f, on(6, 3, 0)).unwrap().state, DayState::Free);
        // Sunday 23:00: tomorrow is Monday → SLEEP
        assert_eq!(status(&h, &f, on(6, 23, 0)).unwrap().state, DayState::Sleep);
        assert_eq!(status(&h, &f, on(6, 12, 0)).unwrap().sleep_at_ms, Some(on(6, 22, 30).now_ms));
    }

    #[test]
    fn free_day_does_not_judge_habits_for_monday_permits() {
        let (h, f) = setup();
        // Monday of the following week: Sunday was free → no habit penalty
        assert_eq!(status(&h, &f, on(7, 9, 0)).unwrap().permits_left, 3);
    }

    // ── Day plan ──

    #[test]
    fn plan_keeps_fixed_goals_and_can_add_more() {
        let (h, mut f) = setup();
        let extra = goal("estudiar", GoalKind::Key, 60);
        let ev = cmd_plan_day(&h, &f, at(7, 0), vec![extra]).unwrap();
        record(&mut f, at(7, 0), ev);
        let ids: Vec<String> = status(&h, &f, at(7, 1)).unwrap().goals.into_iter().map(|g| g.goal_id).collect();
        assert_eq!(ids, ["read-30", "eat-healthy", "estudiar"]);
    }

    #[test]
    fn plan_cannot_weaken_a_fixed_goal() {
        let (h, mut f) = setup();
        let mut lazy = goal("read-30", GoalKind::Key, 5); // the fixed one asks for 30 min
        lazy.methods = vec![Method::FocusSession];
        let ev = cmd_plan_day(&h, &f, at(7, 0), vec![lazy]).unwrap();
        record(&mut f, at(7, 0), ev);
        let read_goal = status(&h, &f, at(7, 1)).unwrap().goals.into_iter().find(|g| g.goal_id == "read-30").unwrap();
        assert_eq!(read_goal.target_minutes, 30, "the fixed version is kept");
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
        record(&mut f, at(7, 5), EventKind::DayPlanned { goals: vec![] }); // another device, offline
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
        cfg.goals.clear(); // no fixed goals: everything is set in the morning
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
        // The next day the plan does not carry over
        assert!(status(&h, &f, on(1, 7, 0)).unwrap().needs_plan);
    }

    // ── Family mode ──

    #[test]
    fn guardian_grant_and_weekly_bonus_add_permits() {
        let mut h = ConfigHistory::default();
        let mut cfg = Config::example();
        cfg.weekly_bonus_permits = 2;
        h.push(0, cfg);
        let mut f = vec![];
        record(&mut f, at(7, 0), EventKind::GrantPermit { minutes: 15 });
        // 3 daily + 1 granted + 2 from the pool = 6
        assert_eq!(status(&h, &f, at(7, 1)).unwrap().permits_left, 6);
    }

    #[test]
    fn granted_permit_lasts_its_minutes_after_the_daily_ones() {
        let (h, mut f) = setup();
        record(&mut f, at(7, 0), EventKind::GrantPermit { minutes: 15 });
        for hour in [8, 10, 12] {
            let ev = cmd_start_pass(&h, &f, at(hour, 0)).unwrap();
            assert_eq!(ev, EventKind::PassStarted { minutes: 5 }, "daily passes first");
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
        // Monday: 4 passes (3 daily + 1 from the pool)
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
        assert_eq!(status(&h, &f, at(23, 0)).unwrap().state, DayState::Sleep, "until bedtime");
        // Tuesday: no penalty for the habit missed on Monday
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
        assert_eq!(
            h.active_at(on(3, 9, 0).now_ms).unwrap().permits_per_day,
            3,
            "the minor cannot relax, not even tomorrow"
        );

        let h = ConfigHistory::from_facts_checked(&[base.clone(), change(at(9, 0), stricter.clone(), false)], true, 0);
        assert_eq!(h.active_at(at(9, 0).now_ms).unwrap().permits_per_day, 1, "endurecer rige ya");

        let h = ConfigHistory::from_facts_checked(&[base.clone(), change(at(9, 0), laxer.clone(), true)], true, 0);
        assert_eq!(h.active_at(at(9, 0).now_ms).unwrap().permits_per_day, 9, "the guardian relaxes immediately");

        // The guardian tightens and the minor tries to go back with a later event
        let facts = [base, change(at(9, 0), stricter, true), change(at(10, 0), Config::example(), false)];
        let h = ConfigHistory::from_facts_checked(&facts, true, 0);
        assert_eq!(h.active_at(on(1, 9, 0).now_ms).unwrap().permits_per_day, 1);
    }
}
