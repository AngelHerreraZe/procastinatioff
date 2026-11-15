// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Lectura por páginas (`PageOcr`): número de página inicial y final por OCR + resumen aprobado,
//! en una sola sesión. El texto OCR solo existe en memoria y se usa como fuente del resumen.

use crate::result::{VMethod, VerificationResult, make_result};

/// Una línea reconocida por el OCR: texto y posición vertical (píxeles).
#[derive(Debug, Clone, uniffi::Record)]
pub struct OcrLine {
    pub text: String,
    pub top: i32,
    pub bottom: i32,
}

/// Número de página: el número aislado (1–4 cifras) más cercano al borde superior o inferior.
#[uniffi::export]
pub fn page_number(lines: Vec<OcrLine>, image_height: i32) -> Option<u16> {
    lines
        .iter()
        .filter(|l| {
            let t = l.text.trim();
            (1..=4).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_digit())
        })
        .min_by_key(|l| l.top.min(image_height - l.bottom))
        .and_then(|l| l.text.trim().parse().ok())
}

/// Aprueba si se leyeron al menos `min_pages` hacia delante y el resumen salió aprobado.
#[uniffi::export]
pub fn judge_pages(
    goal_id: String,
    first_page: u16,
    last_page: u16,
    min_pages: u16,
    summary: VerificationResult,
) -> Option<VerificationResult> {
    let pages = last_page.saturating_sub(first_page);
    // Más de 300 páginas en una sesión: casi seguro una lectura errónea del OCR, no se aprueba.
    let plausible = pages <= 300;
    let approved = plausible && pages >= min_pages.max(1) && summary.approved;
    make_result(&goal_id, VMethod::PageOcr, approved, summary.score, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, top: i32) -> OcrLine {
        OcrLine { text: text.into(), top, bottom: top + 20 }
    }

    #[test]
    fn finds_the_page_number_near_an_edge() {
        let lines = vec![line("Capítulo 3", 40), line("1998", 500), line("124", 1170), line("texto 7", 600)];
        assert_eq!(page_number(lines, 1200), Some(124));
        assert_eq!(page_number(vec![line("sin número", 10)], 1200), None);
    }

    #[test]
    fn needs_forward_progress_and_an_approved_summary() {
        let ok = make_result("leer", VMethod::Summary, true, 70, 0).unwrap();
        let bad = make_result("leer", VMethod::Summary, false, 20, 0).unwrap();
        assert!(judge_pages("leer".into(), 10, 25, 10, ok.clone()).unwrap().approved);
        assert!(!judge_pages("leer".into(), 25, 10, 10, ok.clone()).unwrap().approved, "hacia atrás");
        assert!(!judge_pages("leer".into(), 10, 15, 10, ok.clone()).unwrap().approved, "pocas páginas");
        assert!(!judge_pages("leer".into(), 1, 900, 10, ok).unwrap().approved, "inverosímil");
        assert!(!judge_pages("leer".into(), 10, 25, 10, bad).unwrap().approved);
    }
}
