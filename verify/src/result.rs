// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! La frontera de datos del verificador: lo ÚNICO que sale de él.

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VMethod {
    FoodPhoto,
    ReadingPresence,
    PageOcr,
    Summary,
    PoseReps,
}

/// Tamaño fijo: sin bytes, sin texto libre (el id de meta se valida).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VerificationResult {
    pub goal_id: String,
    pub method: VMethod,
    pub approved: bool,
    pub score: u8,
    pub minutes: u16,
}

/// Valida el id de meta (corto y sin texto libre) antes de emitir un resultado.
pub fn make_result(
    goal_id: &str,
    method: VMethod,
    approved: bool,
    score: u8,
    minutes: u16,
) -> Option<VerificationResult> {
    let ok = !goal_id.is_empty()
        && goal_id.len() <= 32
        && goal_id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    ok.then(|| VerificationResult { goal_id: goal_id.to_owned(), method, approved, score: score.min(100), minutes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_text_cannot_leave_through_the_goal_id() {
        assert!(make_result("leer-30", VMethod::Summary, true, 250, 0).is_some_and(|r| r.score == 100));
        assert!(make_result("Hoy leí sobre mi vida privada", VMethod::Summary, true, 90, 0).is_none());
        assert!(make_result(&"a".repeat(33), VMethod::Summary, true, 90, 0).is_none());
        assert!(make_result("", VMethod::Summary, true, 90, 0).is_none());
    }
}
