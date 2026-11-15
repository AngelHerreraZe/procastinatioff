// Global properties of the rules: thousands of random days; none breaks the guarantees.
use procastinacioff_core::clock::{Clock, DAY_MS, MINUTE_MS, day_start_ms, in_sleep, weekday_index};
use procastinacioff_core::model::{ALL_DAYS, Config, DayState, EventKind, Fact, Goal, GoalKind, Method};
use procastinacioff_core::rules::{ConfigHistory, status};
use proptest::prelude::*;

const MONDAY: i64 = 1_773_014_400_000; // 2026-03-09 00:00 UTC

fn arb_event() -> impl Strategy<Value = EventKind> {
    prop_oneof![
        Just(EventKind::EmergencyStarted),
        (1u16..30).prop_map(|minutes| EventKind::PassStarted { minutes }),
        Just(EventKind::FocusStarted { goal_id: "read-30".into() }),
        Just(EventKind::FocusEnded { goal_id: "read-30".into() }),
        Just(EventKind::Heartbeat),
        (0u16..90).prop_map(|m| EventKind::DayPlanned {
            goals: vec![Goal {
                id: "extra".into(),
                title: "Extra".into(),
                kind: GoalKind::Key,
                target_minutes: m,
                target_count: 1,
                methods: vec![Method::HonorDeclaration],
                weekdays: ALL_DAYS,
            }],
        }),
        Just(EventKind::GoalCompleted {
            goal_id: "read-30".into(),
            method: Method::HonorDeclaration,
            score: 100,
            minutes: 30,
        }),
    ]
}

fn is_free(cfg: &Config, day_start: i64) -> bool {
    cfg.free_days & (1 << weekday_index(day_start, 0)) != 0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// Any week, with any combination of free days and events.
    #[test]
    fn guarantees_hold(
        free_days in 0u8..128,
        events in prop::collection::vec((0i64..7 * 1440, arb_event()), 0..60),
        query in 0i64..7 * 1440,
    ) {
        let mut cfg = Config::example();
        cfg.free_days = free_days;
        let mut h = ConfigHistory::default();
        h.push(0, cfg.clone());
        let mut facts: Vec<Fact> = events
            .into_iter()
            .map(|(m, kind)| Fact::new(MONDAY + m * MINUTE_MS, kind))
            .collect();
        facts.sort_by_key(|f| f.at_ms);
        let clock = Clock { now_ms: MONDAY + query * MINUTE_MS, utc_offset_min: 0 };
        let st = status(&h, &facts, clock).unwrap();
        let day_start = day_start_ms(&cfg.schedule, clock);
        let sleeping = in_sleep(&cfg.schedule, clock.minute_of_day());

        // 1. Night before a normal day: only SLEEP or EMERGENCY. Never FREE nor PASS.
        if sleeping && !is_free(&cfg, day_start + DAY_MS) {
            let ok = matches!(st.state, DayState::Sleep | DayState::Emergency { .. });
            prop_assert!(ok, "en la noche: {:?}", st.state);
        }
        // 2. A free day never blocks (outside the night before a normal day).
        if is_free(&cfg, day_start) && !(sleeping && !is_free(&cfg, day_start + DAY_MS)) {
            let ok = matches!(st.state, DayState::Free | DayState::Pass { .. } | DayState::Emergency { .. });
            prop_assert!(ok, "on a free day: {:?}", st.state);
        }
        // 3. Without a day plan, a normal day never becomes FREE.
        if st.needs_plan {
            prop_assert!(st.state != DayState::Free);
        }
        // 4. Never more passes than configured.
        prop_assert!(st.permits_left <= cfg.permits_per_day);
    }
}
