// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Anti-impulse rule (security-critical): tightening takes effect now; relaxing, at the next wake-up.
//! When in doubt, a change counts as "relaxing".

use crate::clock::{Clock, DAY_MS, day_start_ms, in_sleep};
use crate::model::{Config, Goal, TimeOfDay};

/// true if `new` is equal to or stricter than `old` in EVERY aspect.
pub fn is_stricter_or_equal(new: &Config, old: &Config) -> bool {
    new.permits_per_day <= old.permits_per_day
        && new.permit_minutes <= old.permit_minutes
        && new.permit_cooldown_minutes >= old.permit_cooldown_minutes
        && new.emergency_minutes <= old.emergency_minutes
        && new.emergency_penalty >= old.emergency_penalty
        && (new.habit_penalty || !old.habit_penalty)
        && new.free_days & !old.free_days == 0 // a new free day relaxes
        && sleep_contains(new, old)
        && old.goals.iter().all(|g| goal_kept_or_harder(g, &new.goals))
        && new.essential_apps.iter().all(|a| old.essential_apps.contains(a))
        && new.focus_apps.iter().all(|a| old.focus_apps.contains(a))
        && new.web_allowlist.iter().all(|d| old.web_allowlist.contains(d))
        && new.weekly_bonus_permits <= old.weekly_bonus_permits
        // Removing a local method or the contact's approval relaxes (ADR 0013)
        && old.auth.local.iter().all(|f| new.auth.local.contains(f))
        && (new.auth.contact_required || !old.auth.contact_required)
}

/// The only thing that relaxes is turning off "changes need my contact" (the 7-day exit).
pub fn only_leaves_contact(new: &Config, old: &Config) -> bool {
    let mut probe = new.clone();
    probe.auth.contact_required = true;
    !new.auth.contact_required && old.auth.contact_required && is_stricter_or_equal(&probe, old)
}

/// The new sleep period covers every minute of the old one.
/// ponytail: walks the 1440 minutes; it is O(1) in practice and has no edge cases.
fn sleep_contains(new: &Config, old: &Config) -> bool {
    (0..1440u16).all(|m| {
        let t = TimeOfDay { minutes: m };
        !in_sleep(&old.schedule, t) || in_sleep(&new.schedule, t)
    })
}

/// `old` stays in `new_goals` with the same identity and equal or larger targets.
pub fn goal_kept_or_harder(old: &Goal, new_goals: &[Goal]) -> bool {
    new_goals.iter().any(|n| {
        n.id == old.id
            && n.kind == old.kind
            && n.target_minutes >= old.target_minutes
            && n.target_count >= old.target_count
            && n.weekdays & old.weekdays == old.weekdays
            && n.methods.iter().all(|m| old.methods.contains(m))
    })
}

/// Instant from which a change proposed now takes effect.
pub fn effective_from(new: &Config, current: Option<&Config>, clock: Clock) -> i64 {
    match current {
        None => clock.now_ms, // first configuration: immediate
        Some(old) if is_stricter_or_equal(new, old) => clock.now_ms,
        Some(old) => day_start_ms(&old.schedule, clock) + DAY_MS, // next wake-up
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Config, SATURDAY};
    use proptest::prelude::*;

    #[test]
    fn more_permits_is_laxer() {
        let old = Config::example();
        let mut new = old.clone();
        new.permits_per_day += 1;
        assert!(!is_stricter_or_equal(&new, &old));
    }

    #[test]
    fn removing_a_goal_is_laxer_adding_one_is_stricter() {
        let old = Config::example();
        let mut removed = old.clone();
        removed.goals.pop();
        assert!(!is_stricter_or_equal(&removed, &old));
        let mut added = old.clone();
        let mut g = added.goals[0].clone();
        g.id = "ejercicio".into();
        added.goals.push(g);
        assert!(is_stricter_or_equal(&added, &old));
    }

    #[test]
    fn later_bedtime_is_laxer() {
        let old = Config::example();
        let mut new = old.clone();
        new.schedule.bedtime = TimeOfDay::hm(23, 30);
        assert!(!is_stricter_or_equal(&new, &old));
        new.schedule.bedtime = TimeOfDay::hm(21, 30);
        assert!(is_stricter_or_equal(&new, &old));
    }

    #[test]
    fn adding_a_free_day_is_laxer_removing_one_is_stricter() {
        let old = Config::example(); // Saturday and Sunday
        let mut more = old.clone();
        more.free_days |= 1; // + Monday
        assert!(!is_stricter_or_equal(&more, &old));
        let mut fewer = old.clone();
        fewer.free_days &= !SATURDAY;
        assert!(is_stricter_or_equal(&fewer, &old));
    }

    #[test]
    fn dropping_a_method_or_the_contact_is_laxer() {
        use crate::model::LocalFactor;
        let mut old = Config::example();
        old.auth.local = vec![LocalFactor::Pin, LocalFactor::Biometric];
        old.auth.contact_required = true;
        let mut fewer = old.clone();
        fewer.auth.local.pop();
        assert!(!is_stricter_or_equal(&fewer, &old));
        let mut more = old.clone();
        more.auth.local.push(LocalFactor::Pattern);
        assert!(is_stricter_or_equal(&more, &old));
        let mut exit = old.clone();
        exit.auth.contact_required = false;
        assert!(!is_stricter_or_equal(&exit, &old));
        assert!(only_leaves_contact(&exit, &old));
        exit.permits_per_day += 1;
        assert!(!only_leaves_contact(&exit, &old), "the exit cannot carry anything else");
    }

    proptest! {
        /// Property: a configuration is always "equal or stricter" than itself.
        #[test]
        fn reflexive(p in 0u8..10, m in 1u16..60, c in 0u16..240, free in 0u8..128) {
            let mut cfg = Config::example();
            cfg.permits_per_day = p; cfg.permit_minutes = m; cfg.permit_cooldown_minutes = c; cfg.free_days = free;
            prop_assert!(is_stricter_or_equal(&cfg, &cfg));
        }

        /// Property: if the change gives more passes, it can NEVER take effect immediately.
        #[test]
        fn more_permits_never_immediate(extra in 1u8..10, now in 0i64..4_000_000_000_000) {
            let old = Config::example();
            let mut new = old.clone();
            new.permits_per_day = old.permits_per_day.saturating_add(extra);
            let clock = Clock { now_ms: now, utc_offset_min: 0 };
            prop_assert!(effective_from(&new, Some(&old), clock) > now);
        }

        /// Property: adding any free day NEVER takes effect immediately.
        #[test]
        fn new_free_day_never_immediate(day in 0u8..7, now in 0i64..4_000_000_000_000) {
            let mut old = Config::example();
            old.free_days = 0;
            let mut new = old.clone();
            new.free_days = 1 << day;
            let clock = Clock { now_ms: now, utc_offset_min: 0 };
            prop_assert!(effective_from(&new, Some(&old), clock) > now);
        }
    }
}
