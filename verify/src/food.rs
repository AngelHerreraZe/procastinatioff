// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Comida: las etiquetas vienen del clasificador elegido en el spike E. La tabla de mapeo
//! depende del modelo y se ajusta con él.

use crate::result::{VMethod, VerificationResult, make_result};

#[derive(Debug, Clone, uniffi::Record)]
pub struct Label {
    pub name: String,
    pub score: u8, // 0–100
}

/// Etiquetas consideradas saludables / no saludables. El usuario puede ampliar las saludables.
const HEALTHY: &[&str] = &[
    "salad",
    "vegetable",
    "broccoli",
    "fruit",
    "apple",
    "banana",
    "orange",
    "fish",
    "beans",
    "lentils",
    "carrot",
    "tomato",
    "avocado",
    "egg",
    "yogurt",
    "oatmeal",
];
const UNHEALTHY: &[&str] = &[
    "pizza",
    "hamburger",
    "hot dog",
    "french fries",
    "donut",
    "cake",
    "ice cream",
    "candy",
    "soda",
    "chips",
    "cookie",
];
/// Umbrales (perilla de calibración).
pub const HEALTHY_MIN: u8 = 50;
pub const UNHEALTHY_MAX: u8 = 60;

#[uniffi::export]
pub fn judge_food(
    goal_id: String,
    labels: Vec<Label>,
    extra_healthy: Vec<String>,
    liveness_passed: bool,
    duplicate: bool,
) -> Option<VerificationResult> {
    let is = |l: &Label, list: &[&str]| list.iter().any(|h| l.name.to_lowercase().contains(h));
    let healthy = labels
        .iter()
        .filter(|l| is(l, HEALTHY) || extra_healthy.iter().any(|e| l.name.eq_ignore_ascii_case(e)))
        .map(|l| l.score.min(100))
        .max()
        .unwrap_or(0);
    let unhealthy = labels.iter().filter(|l| is(l, UNHEALTHY)).map(|l| l.score.min(100)).max().unwrap_or(0);
    let approved = liveness_passed && !duplicate && healthy >= HEALTHY_MIN && unhealthy < UNHEALTHY_MAX;
    make_result(&goal_id, VMethod::FoodPhoto, approved, healthy, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(name: &str, score: u8) -> Label {
        Label { name: name.into(), score }
    }

    #[test]
    fn healthy_live_and_new_passes_anything_else_fails() {
        let salad = vec![l("Greek salad", 80)];
        assert!(judge_food("comer".into(), salad.clone(), vec![], true, false).unwrap().approved);
        assert!(!judge_food("comer".into(), salad.clone(), vec![], false, false).unwrap().approved, "sin vivacidad");
        assert!(!judge_food("comer".into(), salad, vec![], true, true).unwrap().approved, "foto repetida");
        let mixed = vec![l("salad", 70), l("french fries", 75)];
        assert!(!judge_food("comer".into(), mixed, vec![], true, false).unwrap().approved);
        let custom = vec![l("ceviche", 90)];
        assert!(judge_food("comer".into(), custom, vec!["ceviche".into()], true, false).unwrap().approved);
    }
}
