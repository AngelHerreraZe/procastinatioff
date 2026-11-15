// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Conteo de repeticiones por pose. Entrada: los 33 puntos de MediaPipe Pose como
//! `[x, y, visibilidad]` × 33 (99 números). Índices usados: 11/12 hombros, 13/14 codos,
//! 15/16 muñecas, 23/24 caderas, 25/26 rodillas, 27/28 tobillos.

use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Exercise {
    Squat,
    PushUp,
}

#[derive(Clone, Copy)]
struct P {
    x: f32,
    y: f32,
    v: f32,
}

fn point(lm: &[f32], i: usize) -> Option<P> {
    let p = P { x: *lm.get(i * 3)?, y: *lm.get(i * 3 + 1)?, v: *lm.get(i * 3 + 2)? };
    (p.v >= 0.5 && p.x.is_finite() && p.y.is_finite()).then_some(p) // ignorar puntos poco visibles o inválidos
}

/// Ángulo en `b` (grados) formado por a-b-c.
fn angle(a: P, b: P, c: P) -> f32 {
    let (v1, v2) = ((a.x - b.x, a.y - b.y), (c.x - b.x, c.y - b.y));
    let dot = v1.0 * v2.0 + v1.1 * v2.1;
    let norm = (v1.0.hypot(v1.1) * v2.0.hypot(v2.1)).max(1e-6);
    (dot / norm).clamp(-1.0, 1.0).acos().to_degrees()
}

struct State {
    down: bool,
    reps: u32,
    last_rep_ms: i64,
}

#[derive(uniffi::Object)]
pub struct RepCounter {
    exercise: Exercise,
    state: Mutex<State>,
}

#[uniffi::export]
impl RepCounter {
    #[uniffi::constructor]
    pub fn new(exercise: Exercise) -> Self {
        Self { exercise, state: Mutex::new(State { down: false, reps: 0, last_rep_ms: i64::MIN / 2 }) }
    }

    /// Procesa un cuadro. Devuelve el total de repeticiones.
    pub fn feed(&self, landmarks: Vec<f32>, at_ms: i64) -> u32 {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if landmarks.len() != 99 {
            return s.reps;
        }
        // Umbrales (perilla de calibración: ajustar con datos del spike E)
        let (joints, down_below, up_above) = match self.exercise {
            Exercise::Squat => ([(23, 25, 27), (24, 26, 28)], 100.0, 160.0), // cadera-rodilla-tobillo
            Exercise::PushUp => ([(11, 13, 15), (12, 14, 16)], 90.0, 155.0), // hombro-codo-muñeca
        };
        let angles = joints
            .iter()
            .filter_map(|&(a, b, c)| Some(angle(point(&landmarks, a)?, point(&landmarks, b)?, point(&landmarks, c)?)));
        let Some(min_angle) = angles.reduce(f32::min) else { return s.reps };
        if !s.down && min_angle < down_below {
            s.down = true;
        } else if s.down && min_angle > up_above {
            s.down = false;
            // Anti-trampa: una repetición real dura al menos 0,6 s
            if at_ms - s.last_rep_ms >= 600 {
                s.reps += 1;
                s.last_rep_ms = at_ms;
            }
        }
        s.reps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construye 33 puntos visibles con un ángulo dado en ambas rodillas.
    fn squat_frame(knee_angle_deg: f32) -> Vec<f32> {
        let mut lm = vec![0.0f32; 99];
        let set = |lm: &mut Vec<f32>, i: usize, x: f32, y: f32| {
            lm[i * 3] = x;
            lm[i * 3 + 1] = y;
            lm[i * 3 + 2] = 1.0;
        };
        let rad = knee_angle_deg.to_radians();
        for (hip, knee, ankle) in [(23, 25, 27), (24, 26, 28)] {
            set(&mut lm, knee, 0.5, 0.5);
            set(&mut lm, ankle, 0.5, 0.8); // tobillo justo debajo
            set(&mut lm, hip, 0.5 + 0.3 * rad.sin(), 0.5 + 0.3 * rad.cos()); // cadera según el ángulo
        }
        lm
    }

    #[test]
    fn counts_full_squats_only() {
        let c = RepCounter::new(Exercise::Squat);
        let mut t = 0;
        for _ in 0..3 {
            c.feed(squat_frame(170.0), t);
            t += 500;
            c.feed(squat_frame(80.0), t);
            t += 500;
            c.feed(squat_frame(170.0), t);
            t += 500;
        }
        assert_eq!(c.feed(squat_frame(170.0), t), 3);
        // Media sentadilla (no baja de 100°) no cuenta
        c.feed(squat_frame(120.0), t + 500);
        assert_eq!(c.feed(squat_frame(170.0), t + 1000), 3);
    }

    #[test]
    fn too_fast_reps_and_invisible_points_do_not_count() {
        let c = RepCounter::new(Exercise::Squat);
        c.feed(squat_frame(170.0), 0);
        c.feed(squat_frame(80.0), 100);
        assert_eq!(c.feed(squat_frame(170.0), 200), 1);
        c.feed(squat_frame(80.0), 300);
        assert_eq!(c.feed(squat_frame(170.0), 400), 1, "dos repeticiones en 0,2 s: la segunda no cuenta");
        let mut hidden = squat_frame(80.0);
        hidden.iter_mut().skip(2).step_by(3).for_each(|v| *v = 0.1);
        assert_eq!(c.feed(hidden, 2_000), 1);
        assert_eq!(c.feed(vec![0.0; 10], 3_000), 1, "entrada mal formada se ignora");
    }
}
