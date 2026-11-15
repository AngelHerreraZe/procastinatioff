// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reloj inyectado: la plataforma pasa la hora; el núcleo nunca llama a `now()`.

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Timelike};

use crate::model::{Schedule, TimeOfDay};

pub const MINUTE_MS: i64 = 60_000;
pub const DAY_MS: i64 = 24 * 60 * MINUTE_MS;

/// Instante actual tal como lo ve el dispositivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Clock {
    pub now_ms: i64,
    /// Desfase respecto a UTC en minutos (UTC-6 → -360).
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
    // ponytail: fuera del rango de chrono (±262 000 años) se satura a la época; nunca ocurre con relojes reales.
    let utc = DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc();
    utc + Duration::minutes(offset_min as i64)
}

/// ¿El minuto `m` cae dentro del periodo de sueño? Soporta periodos que cruzan medianoche.
pub fn in_sleep(schedule: &Schedule, m: TimeOfDay) -> bool {
    let (b, w, m) = (schedule.bedtime.minutes, schedule.wake.minutes, m.minutes);
    if b == w {
        false
    } else if b > w {
        // Caso normal: 22:30 → 06:30 (cruza medianoche)
        m >= b || m < w
    } else {
        // Caso 01:00 → 09:00 (no cruza medianoche)
        m >= b && m < w
    }
}

/// Inicio del "día" actual = la hora de despertar más reciente que sea <= ahora (en ms UTC).
/// El periodo de sueño en curso (si lo hay) termina en `day_start_ms + DAY_MS`.
///
/// ponytail: usa el desfase horario actual; en el cambio de horario de verano el día
/// dura 23 o 25 h una vez al año. Si molesta, pasar la zona IANA y usar chrono-tz.
pub fn day_start_ms(schedule: &Schedule, clock: Clock) -> i64 {
    let local = clock.local();
    let wake = local.date().and_hms_opt(0, 0, 0).unwrap_or_default() + Duration::minutes(schedule.wake.minutes as i64);
    let wake = if wake > local { wake - Duration::days(1) } else { wake };
    (wake - Duration::minutes(clock.utc_offset_min as i64)).and_utc().timestamp_millis()
}

/// Día de la semana (0 = lunes … 6 = domingo) del día que empieza en `day_start`.
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
        // 2026-03-10 05:00 UTC con desfase 0 → aún no despertó → el día empezó ayer 06:30
        let clock = Clock { now_ms: 1_773_118_800_000, utc_offset_min: 0 };
        let start = day_start_ms(&sched(), clock);
        assert_eq!(local_at(start, 0).to_string(), "2026-03-09 06:30:00");
        assert_eq!(weekday_index(start, 0), 0, "lunes");
    }

    #[test]
    fn day_start_respects_utc_offset() {
        // 2026-03-09 12:00 UTC = 06:00 en UTC-6 → todavía no despertó en México
        let clock = Clock { now_ms: 1_773_057_600_000, utc_offset_min: -360 };
        let start = day_start_ms(&sched(), clock);
        assert_eq!(local_at(start, -360).to_string(), "2026-03-08 06:30:00");
    }
}
