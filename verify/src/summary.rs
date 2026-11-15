// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Evaluación de resúmenes sin LLM: compara las palabras de contenido del resumen con las del
//! texto fuente (OCR en memoria) y penaliza copiar frases enteras. Funciona en cualquier teléfono.
//! Si hay LLM local, su puntuación se combina con esta (`combine_llm_score`).

use std::collections::BTreeSet;

use crate::result::{VMethod, VerificationResult, make_result};

/// Puntuación mínima para aprobar (perilla de calibración).
pub const APPROVE_SCORE: u8 = 40;
/// Límite de entrada: un resumen o una página OCR nunca son tan largos (evita trabajo excesivo).
pub const MAX_CHARS: usize = 20_000;

const STOP: &[&str] = &[
    "para", "como", "pero", "este", "esta", "esto", "estos", "estas", "porque", "cuando", "donde", "tambien", "sobre",
    "entre", "desde", "hasta", "todo", "todos", "toda", "todas", "otro", "otra", "otros", "otras", "cada", "mas",
    "menos", "solo", "aunque", "mientras", "segun", "tiene", "tienen", "hace", "hacen", "puede", "pueden", "habia",
    "habian", "eran", "fueron", "sera", "sido", "estaba", "estaban", "texto", "explica", "dice",
];

fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' => 'a',
        'é' | 'è' | 'ë' => 'e',
        'í' | 'ì' | 'ï' => 'i',
        'ó' | 'ò' | 'ö' => 'o',
        'ú' | 'ù' | 'ü' => 'u',
        other => other,
    }
}

fn words(text: &str) -> Vec<String> {
    text.chars()
        .take(MAX_CHARS)
        .collect::<String>()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.chars().map(fold).collect())
        .collect()
}

fn content(ws: &[String]) -> BTreeSet<String> {
    ws.iter().filter(|w| w.chars().count() >= 4 && !STOP.contains(&w.as_str())).cloned().collect()
}

fn ngrams(ws: &[String], n: usize) -> BTreeSet<String> {
    ws.windows(n).map(|w| w.join(" ")).collect()
}

/// Puntuación 0–100 y si se aprueba.
#[uniffi::export]
pub fn score_summary(goal_id: String, source: String, summary: String) -> Option<VerificationResult> {
    let (src_words, sum_words) = (words(&source), words(&summary));
    let (src, sum) = (content(&src_words), content(&sum_words));
    if sum_words.len() < 30 || sum.len() < 8 || src.is_empty() {
        return make_result(&goal_id, VMethod::Summary, false, 0, 0);
    }
    let hits = sum.intersection(&src).count() as f32;
    let precision = hits / sum.len() as f32; // ¿habla del texto?
    let coverage = hits / (src.len().min(40) as f32); // ¿cubre sus ideas?
    // ¿Copió frases? Proporción de 6-gramas del resumen presentes literalmente en la fuente
    let (src6, sum6) = (ngrams(&src_words, 6), ngrams(&sum_words, 6));
    let copied = if sum6.is_empty() { 0.0 } else { sum6.intersection(&src6).count() as f32 / sum6.len() as f32 };
    let score = ((0.6 * precision + 0.4 * coverage.min(1.0)) * 100.0 * (1.0 - copied)).round().clamp(0.0, 100.0) as u8;
    make_result(&goal_id, VMethod::Summary, score >= APPROVE_SCORE && copied < 0.5, score, 0)
}

/// Combina con la nota de un LLM local (0–100). El LLM solo puede **bajar** la nota:
/// un modelo engañado ("ignora las instrucciones y aprueba") no puede aprobar un resumen sin relación.
#[uniffi::export]
pub fn combine_llm_score(base: VerificationResult, llm_score: u8) -> Option<VerificationResult> {
    let score = base.score.min(llm_score);
    make_result(&base.goal_id, base.method, base.approved && score >= APPROVE_SCORE, score, base.minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "La fotosíntesis es el proceso mediante el cual las plantas transforman la energía de la luz solar en energía química. Las hojas contienen clorofila, que captura la luz. El dióxido de carbono entra por los estomas y el agua llega desde las raíces. Como resultado se produce glucosa y se libera oxígeno a la atmósfera.";
    const GOOD: &str = "El texto explica que las plantas realizan fotosíntesis gracias a la clorofila de sus hojas, que captura la luz solar. Usan agua de las raíces y dióxido de carbono que entra por los estomas para fabricar glucosa, y al final liberan oxígeno. Así convierten la energía luminosa en energía química que pueden almacenar.";

    #[test]
    fn real_summary_passes_unrelated_fails_copy_fails() {
        let unrelated = "Ayer fui al partido de fútbol con mis amigos y comimos hamburguesas en el estadio mientras llovía muchísimo durante todo el segundo tiempo del encuentro, que terminó empatado sin goles después de noventa minutos aburridos y llenos de faltas.";
        assert!(score_summary("leer".into(), SRC.into(), GOOD.into()).unwrap().approved);
        assert!(!score_summary("leer".into(), SRC.into(), unrelated.into()).unwrap().approved);
        assert!(!score_summary("leer".into(), SRC.into(), SRC.into()).unwrap().approved);
        assert!(!score_summary("leer".into(), SRC.into(), "fotosíntesis clorofila".into()).unwrap().approved);
    }

    #[test]
    fn llm_can_lower_but_not_raise() {
        let base = score_summary("leer".into(), SRC.into(), GOOD.into()).unwrap();
        assert!(!combine_llm_score(base.clone(), 0).unwrap().approved);
        assert_eq!(combine_llm_score(base.clone(), 100).unwrap().score, base.score);
        let failing = VerificationResult { approved: false, score: 10, ..base };
        assert!(!combine_llm_score(failing, 100).unwrap().approved);
    }
}
