// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! How a goal is checked, for every UI: the choices offered (`Check`), the methods each one accepts,
//! a suggestion from the goal's name (keyword catalog, no AI, works on every device) and the
//! codes printed on place QR anchors. The UIs translate `Check` with their own files.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::Method;

/// How a goal is checked, as the UIs offer it. Each one maps to the methods the core accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    /// Timed focus on the phone or PC (study, write, practice); its apps and sites open.
    Focus,
    Reading,
    Pages,
    Exercise,
    Food,
    /// Scan a QR stuck where the goal happens (gym, library, office).
    Place,
    /// Exercise minutes from Health Connect / HealthKit.
    Health,
    /// (Family profile) The guardian confirms it from their phone.
    Witness,
    Mark,
}

/// Methods a check accepts. Every check keeps the declaration with friction as a fallback, except
/// Witness: the guardian chose to confirm it themselves.
#[uniffi::export]
pub fn methods_for_check(check: Check, minutes: u16) -> Vec<Method> {
    use Method::*;
    match check {
        Check::Focus if minutes > 0 => vec![FocusSession, HonorDeclaration],
        Check::Focus | Check::Mark => vec![HonorDeclaration],
        Check::Reading => vec![ReadingPresence, FocusSession, HonorDeclaration],
        Check::Pages => vec![PageOcr, Summary, HonorDeclaration],
        Check::Exercise => vec![PoseReps, HonorDeclaration],
        Check::Food => vec![FoodPhoto, HonorDeclaration],
        Check::Place => vec![QrAnchor, HonorDeclaration],
        Check::Health => vec![Health, HonorDeclaration],
        Check::Witness => vec![Witness],
    }
}

/// The method that gives each check away, in priority order (Reading also accepts FocusSession).
const TELLS: [(Method, Check); 8] = [
    (Method::ReadingPresence, Check::Reading),
    (Method::PageOcr, Check::Pages),
    (Method::PoseReps, Check::Exercise),
    (Method::FoodPhoto, Check::Food),
    (Method::QrAnchor, Check::Place),
    (Method::Health, Check::Health),
    (Method::Witness, Check::Witness),
    (Method::FocusSession, Check::Focus),
];

/// The check a goal's methods came from (inverse of `methods_for_check`).
#[uniffi::export]
pub fn check_of(methods: Vec<Method>) -> Check {
    TELLS.iter().find(|(m, _)| methods.contains(m)).map_or(Check::Mark, |(_, c)| *c)
}

/// Checks measured in minutes (the rest count times).
#[uniffi::export]
pub fn check_uses_minutes(check: Check) -> bool {
    matches!(check, Check::Focus | Check::Reading | Check::Health)
}

/// Word stems per check, English and Spanish, lowercase without accents. A stem matches a word
/// that starts with it ("gimnas" → "gimnasio"). Ties go to the earlier row: the more specific
/// evidence first. Witness is never suggested (only a guardian picks it).
/// ponytail: prefix match over a fixed list; a small local embedding model if it misses too often.
const CATALOG: &[(Check, &[&str])] = &[
    (
        Check::Place,
        &[
            "gym", "gimnas", "library", "bibliotec", "office", "oficina", "school", "escuela", "colegio",
            "university", "universidad", "church", "iglesia", "templo", "temple", "misa", "dojo", "alberca", "pool",
        ],
    ),
    (Check::Pages, &["page", "pagina", "summar", "resum", "chapter", "capitulo"]),
    (Check::Reading, &["read", "lee", "lei", "lectura", "book", "libro", "novel", "kindle"]),
    (
        Check::Exercise,
        &[
            "squat", "sentadilla", "pushup", "push-up", "lagartija", "flexion", "situp", "abdominal", "burpee",
            "reps", "repeticion",
        ],
    ),
    (
        Check::Health,
        &[
            "run", "correr", "trot", "jog", "walk", "camin", "steps", "pasos", "bike", "bici", "cycl", "swim",
            "nadar", "natacion", "yoga", "cardio", "workout", "entren", "ejercicio", "exercise", "deporte", "sport",
            "pilates", "hike", "senderismo",
        ],
    ),
    (
        Check::Food,
        &[
            "eat", "comer", "comida", "food", "diet", "dieta", "vegetable", "verdura", "fruit", "fruta",
            "salad", "ensalada", "breakfast", "desayun", "cook", "cocin", "healthy", "sano", "saludable",
        ],
    ),
    (
        Check::Focus,
        &[
            "study", "estudi", "work", "trabaj", "program", "code", "coding", "write", "writing", "escrib",
            "homework", "tarea", "project", "proyecto", "language", "idioma", "english", "ingles", "duolingo",
            "course", "curso", "practic", "guitar", "piano", "instrument", "draw", "dibuj", "learn", "aprend",
            "medit", "repas", "review",
        ],
    ),
    (
        Check::Mark,
        &[
            "clean", "limpi", "tidy", "orden", "wash", "lavar", "laundry", "ropa", "bed", "cama", "call", "llamar",
            "water", "agua", "regar", "plant", "planta", "pay", "pagar", "vitamin", "pastilla", "pill",
        ],
    ),
];

/// Lowercase without accents ("Página" → "pagina"), so stems match in either language.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// Checks that fit a goal's name, best first (at most 3; empty if nothing matches). Only a
/// suggestion: the person always picks, and the core only accepts the chosen check's methods.
#[uniffi::export]
pub fn suggest_checks(title: String) -> Vec<Check> {
    let folded = fold(&title);
    let words: Vec<&str> = folded.split(|c: char| !c.is_alphanumeric() && c != '-').filter(|w| !w.is_empty()).collect();
    let mut scored: Vec<(usize, usize, Check)> = CATALOG
        .iter()
        .enumerate()
        .map(|(row, (check, stems))| {
            let hits = words.iter().filter(|w| stems.iter().any(|s| w.starts_with(s))).count();
            (hits, row, *check)
        })
        .filter(|(hits, ..)| *hits > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(3).map(|(.., c)| c).collect()
}

/// A place goal's QR: `code` is printed (shown once), `hash` goes in `Goal::anchor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, uniffi::Record)]
pub struct PlaceAnchor {
    pub code: String,
    pub hash: String,
}

const ANCHOR_PREFIX: &str = "procastinatioff:place:";

/// SHA-256 (hex) of a scanned or generated code: what `Goal::anchor` stores.
pub fn anchor_hash(code: &str) -> String {
    hex::encode(Sha256::digest(code.trim().as_bytes()))
}

/// A new random QR code for a place goal. The configuration keeps only its hash, so no device can
/// show it again: to get a new QR the goal changes, and changing an anchor relaxes (next wake-up).
#[uniffi::export]
pub fn new_place_anchor() -> PlaceAnchor {
    let code = format!("{ANCHOR_PREFIX}{}", hex::encode(&crate::crypto::random_32()[..16]));
    PlaceAnchor { hash: anchor_hash(&code), code }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Check; 9] = [
        Check::Focus,
        Check::Reading,
        Check::Pages,
        Check::Exercise,
        Check::Food,
        Check::Place,
        Check::Health,
        Check::Witness,
        Check::Mark,
    ];

    #[test]
    fn editing_a_goal_shows_the_check_it_was_created_with() {
        for c in ALL {
            assert_eq!(check_of(methods_for_check(c, 30)), c);
        }
        // An untimed focus is just a declaration
        assert_eq!(check_of(methods_for_check(Check::Focus, 0)), Check::Mark);
    }

    #[test]
    fn only_witness_has_no_declaration_fallback() {
        for c in ALL {
            assert_eq!(methods_for_check(c, 30).contains(&Method::HonorDeclaration), c != Check::Witness, "{c:?}");
        }
    }

    #[test]
    fn suggests_from_the_name_in_english_and_spanish() {
        let first = |t: &str| suggest_checks(t.into()).first().copied();
        assert_eq!(first("Ir al gimnasio"), Some(Check::Place));
        assert_eq!(first("Go to the library"), Some(Check::Place));
        assert_eq!(first("Leer un libro"), Some(Check::Reading));
        assert_eq!(first("Leer 20 páginas"), Some(Check::Pages));
        assert_eq!(first("Read 30 minutes"), Some(Check::Reading));
        assert_eq!(first("Salir a correr"), Some(Check::Health));
        assert_eq!(first("Walk 10,000 steps"), Some(Check::Health));
        assert_eq!(first("50 sentadillas"), Some(Check::Exercise));
        assert_eq!(first("Comer sano"), Some(Check::Food));
        assert_eq!(first("Estudiar inglés"), Some(Check::Focus));
        assert_eq!(first("Practicar guitarra"), Some(Check::Focus));
        assert_eq!(first("Limpiar mi cuarto"), Some(Check::Mark));
        assert_eq!(first("Tender la cama"), Some(Check::Mark));
        assert!(suggest_checks("zzz".into()).is_empty());
        assert!(suggest_checks("Leer en la biblioteca y luego correr".into()).len() <= 3);
    }

    #[test]
    fn anchors_are_random_and_hashed() {
        let (a, b) = (new_place_anchor(), new_place_anchor());
        assert_ne!(a.code, b.code);
        assert!(a.code.starts_with(ANCHOR_PREFIX));
        assert_eq!(anchor_hash(&a.code), a.hash);
        assert_eq!(anchor_hash(&format!(" {}\n", a.code)), a.hash, "scanners may add whitespace");
        assert_ne!(anchor_hash(&b.code), a.hash);
    }
}
