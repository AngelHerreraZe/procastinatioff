// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Injected clock: the platform passes the time; the core never calls `now()`.

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Timelike};

use crate::model::{Schedule, TimeOfDay};

pub const MINUTE_MS: i64 = 60_000;
pub const DAY_MS: i64 = 24 * 60 * MINUTE_MS;

/// Current instant as the device sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Clock {
    pub now_ms: i64,
    /// Offset from UTC in minutes (UTC-6 → -360).
    pub utc_offset_min: i32,
}

impl Clock {
    pub fn local(&self) -> NaiveDateTime {
        local_at(self.now_ms, self.utc_offset_min)
    }

    pub fn minute_of_day(&self) -> TimeOfDay {
        let l = self.local();
        TimeOfDay { minutes: (l.hour() * 60 + l.minute()) as u16 }
    }
}

pub fn local_at(ms: i64, offset_min: i32) -> NaiveDateTime {
    // ponytail: outside chrono's range (±262,000 years) it saturates to the epoch; never happens with real clocks.
    let utc = DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc();
    utc + Duration::minutes(offset_min as i64)
}

/// Does minute `m` fall inside the sleep period? Supports periods that cross midnight.
pub fn in_sleep(schedule: &Schedule, m: TimeOfDay) -> bool {
    let (b, w, m) = (schedule.bedtime.minutes, schedule.wake.minutes, m.minutes);
    if b == w {
        false
    } else if b > w {
        // Normal case: 22:30 → 06:30 (crosses midnight)
        m >= b || m < w
    } else {
        // Case 01:00 → 09:00 (does not cross midnight)
        m >= b && m < w
    }
}

/// Start of the current "day" = the most recent wake-up time that is <= now (UTC ms).
/// The current sleep period (if any) ends at `day_start_ms + DAY_MS`.
///
/// ponytail: uses the current UTC offset; on the daylight-saving switch the day
/// lasts 23 or 25 h once a year. If it matters, pass the IANA zone and use chrono-tz.
pub fn day_start_ms(schedule: &Schedule, clock: Clock) -> i64 {
    let local = clock.local();
    let wake = local.date().and_hms_opt(0, 0, 0).unwrap_or_default() + Duration::minutes(schedule.wake.minutes as i64);
    let wake = if wake > local { wake - Duration::days(1) } else { wake };
    (wake - Duration::minutes(clock.utc_offset_min as i64)).and_utc().timestamp_millis()
}

/// Weekday (0 = Monday … 6 = Sunday) of the day that starts at `day_start`.
pub fn weekday_index(day_start: i64, offset_min: i32) -> u8 {
    local_at(day_start, offset_min).weekday().num_days_from_monday() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched() -> Schedule {
        Schedule { wake: TimeOfDay::hm(6, 30), bedtime: TimeOfDay::hm(22, 30) }
    }

    #[test]
    fn sleep_window_crossing_midnight() {
        assert!(in_sleep(&sched(), TimeOfDay::hm(23, 0)));
        assert!(in_sleep(&sched(), TimeOfDay::hm(2, 0)));
        assert!(!in_sleep(&sched(), TimeOfDay::hm(6, 30)));
        assert!(!in_sleep(&sched(), TimeOfDay::hm(12, 0)));
        assert!(in_sleep(&sched(), TimeOfDay::hm(22, 30)));
    }

    #[test]
    fn day_starts_at_last_wake() {
        // 2026-03-10 05:00 UTC with offset 0 → not awake yet → the day started yesterday at 06:30
        let clock = Clock { now_ms: 1_773_118_800_000, utc_offset_min: 0 };
        let start = day_start_ms(&sched(), clock);
        assert_eq!(local_at(start, 0).to_string(), "2026-03-09 06:30:00");
        assert_eq!(weekday_index(start, 0), 0, "lunes");
    }

    #[test]
    fn day_start_respects_utc_offset() {
        // 2026-03-09 12:00 UTC = 06:00 in UTC-6 → not awake yet in Mexico
        let clock = Clock { now_ms: 1_773_057_600_000, utc_offset_min: -360 };
        let start = day_start_ms(&sched(), clock);
        assert_eq!(local_at(start, -360).to_string(), "2026-03-08 06:30:00");
    }
}
