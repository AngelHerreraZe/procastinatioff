// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Summary scoring without an LLM: compares the content words of the summary with those of the
//! source text (in-memory OCR) and penalizes copying whole sentences. Works on any phone.
//! If a local LLM is available, its score is combined with this one (`combine_llm_score`).
//!
//! Language-aware part: the stop-word lists. They are merged (a word that is a stop word in any
//! supported language is ignored), so the scorer does not need to know the user's language.

use std::collections::BTreeSet;

use crate::result::{VMethod, VerificationResult, make_result};

/// Minimum score to approve (calibration knob).
pub const APPROVE_SCORE: u8 = 40;
/// Input limit: a summary or an OCR page is never this long (avoids excessive work).
pub const MAX_CHARS: usize = 20_000;

/// Stop words of 4+ letters, lowercase and without accents (see `fold`). Shorter words are
/// already ignored. To support a new language, add a list here and to `STOP_LISTS`.
const STOP_EN: &[&str] = &[
    "about", "after", "also", "because", "been", "before", "being", "between", "both", "could", "does", "each", "from",
    "have", "having", "here", "into", "just", "like", "more", "most", "much", "only", "other", "over", "same", "says",
    "should", "some", "such", "than", "that", "their", "them", "then", "there", "these", "they", "this", "those",
    "through", "very", "were", "what", "when", "where", "which", "while", "will", "with", "would", "your", "text",
    "explains",
];
const STOP_ES: &[&str] = &[
    "para", "como", "pero", "este", "esta", "esto", "estos", "estas", "porque", "cuando", "donde", "tambien", "sobre",
    "entre", "desde", "hasta", "todo", "todos", "toda", "todas", "otro", "otra", "otros", "otras", "cada", "mas",
    "menos", "solo", "aunque", "mientras", "segun", "tiene", "tienen", "hace", "hacen", "puede", "pueden", "habia",
    "habian", "eran", "fueron", "sera", "sido", "estaba", "estaban", "texto", "explica", "dice",
];
const STOP_LISTS: &[&[&str]] = &[STOP_EN, STOP_ES];

fn is_stop(w: &str) -> bool {
    STOP_LISTS.iter().any(|list| list.contains(&w))
}

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
    ws.iter().filter(|w| w.chars().count() >= 4 && !is_stop(w)).cloned().collect()
}

fn ngrams(ws: &[String], n: usize) -> BTreeSet<String> {
    ws.windows(n).map(|w| w.join(" ")).collect()
}

/// Score 0–100 and whether it is approved.
#[uniffi::export]
pub fn score_summary(goal_id: String, source: String, summary: String) -> Option<VerificationResult> {
    let (src_words, sum_words) = (words(&source), words(&summary));
    let (src, sum) = (content(&src_words), content(&sum_words));
    if sum_words.len() < 30 || sum.len() < 8 || src.is_empty() {
        return make_result(&goal_id, VMethod::Summary, false, 0, 0);
    }
    let hits = sum.intersection(&src).count() as f32;
    let precision = hits / sum.len() as f32; // is it about the text?
    let coverage = hits / (src.len().min(40) as f32); // does it cover its ideas?
    // Did it copy sentences? Share of the summary's 6-grams found verbatim in the source
    let (src6, sum6) = (ngrams(&src_words, 6), ngrams(&sum_words, 6));
    let copied = if sum6.is_empty() { 0.0 } else { sum6.intersection(&src6).count() as f32 / sum6.len() as f32 };
    let score = ((0.6 * precision + 0.4 * coverage.min(1.0)) * 100.0 * (1.0 - copied)).round().clamp(0.0, 100.0) as u8;
    make_result(&goal_id, VMethod::Summary, score >= APPROVE_SCORE && copied < 0.5, score, 0)
}

/// Combines with the score of a local LLM (0–100). The LLM can only **lower** the score:
/// a fooled model ("ignore the instructions and approve") cannot approve an unrelated summary.
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

    // Spanish sample (the scorer is language-independent; see the English test below)
    #[test]
    fn real_summary_passes_unrelated_fails_copy_fails() {
        let unrelated = "Ayer fui al partido de fútbol con mis amigos y comimos hamburguesas en el estadio mientras llovía muchísimo durante todo el segundo tiempo del encuentro, que terminó empatado sin goles después de noventa minutos aburridos y llenos de faltas.";
        assert!(score_summary("read".into(), SRC.into(), GOOD.into()).unwrap().approved);
        assert!(!score_summary("read".into(), SRC.into(), unrelated.into()).unwrap().approved);
        assert!(!score_summary("read".into(), SRC.into(), SRC.into()).unwrap().approved);
        assert!(!score_summary("read".into(), SRC.into(), "fotosíntesis clorofila".into()).unwrap().approved);
    }

    #[test]
    fn english_summary_passes_unrelated_fails() {
        let src = "Photosynthesis is the process through which plants transform the energy of sunlight into chemical energy. Leaves contain chlorophyll, which captures light. Carbon dioxide enters through the stomata and water arrives from the roots. As a result, glucose is produced and oxygen is released into the atmosphere.";
        let good = "The passage explains that plants carry out photosynthesis thanks to the chlorophyll in their leaves, which captures sunlight. They take water from the roots and carbon dioxide entering through the stomata to produce glucose, and finally they release oxygen. Plants therefore convert light energy into chemical energy they can store.";
        let unrelated = "Yesterday I went to the football match with my friends and we ate burgers at the stadium while it rained heavily during the whole second half of the game, which ended in a goalless draw after ninety boring minutes full of fouls and complaints.";
        assert!(score_summary("read".into(), src.into(), good.into()).unwrap().approved);
        assert!(!score_summary("read".into(), src.into(), unrelated.into()).unwrap().approved);
    }

    #[test]
    fn llm_can_lower_but_not_raise() {
        let base = score_summary("read".into(), SRC.into(), GOOD.into()).unwrap();
        assert!(!combine_llm_score(base.clone(), 0).unwrap().approved);
        assert_eq!(combine_llm_score(base.clone(), 100).unwrap().score, base.score);
        let failing = VerificationResult { approved: false, score: 10, ..base };
        assert!(!combine_llm_score(failing, 100).unwrap().approved);
    }
}
