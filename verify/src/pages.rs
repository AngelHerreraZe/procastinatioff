// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reading by pages (`PageOcr`): first and last page number by OCR + an approved summary,
//! in a single session. The OCR text only exists in memory and is used as the summary's source.

use crate::result::{VMethod, VerificationResult, make_result};

/// A line recognized by OCR: text and vertical position (pixels).
#[derive(Debug, Clone, uniffi::Record)]
pub struct OcrLine {
    pub text: String,
    pub top: i32,
    pub bottom: i32,
}

/// Page number: the isolated number (1–4 digits) closest to the top or bottom edge.
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

/// Approves if at least `min_pages` were read forward and the summary was approved.
#[uniffi::export]
pub fn judge_pages(
    goal_id: String,
    first_page: u16,
    last_page: u16,
    min_pages: u16,
    summary: VerificationResult,
) -> Option<VerificationResult> {
    let pages = last_page.saturating_sub(first_page);
    // More than 300 pages in one session: almost certainly an OCR misread; not approved.
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
        let lines = vec![line("Chapter 3", 40), line("1998", 500), line("124", 1170), line("text 7", 600)];
        assert_eq!(page_number(lines, 1200), Some(124));
        assert_eq!(page_number(vec![line("no number", 10)], 1200), None);
    }

    #[test]
    fn needs_forward_progress_and_an_approved_summary() {
        let ok = make_result("read", VMethod::Summary, true, 70, 0).unwrap();
        let bad = make_result("read", VMethod::Summary, false, 20, 0).unwrap();
        assert!(judge_pages("read".into(), 10, 25, 10, ok.clone()).unwrap().approved);
        assert!(!judge_pages("read".into(), 25, 10, 10, ok.clone()).unwrap().approved, "backwards");
        assert!(!judge_pages("read".into(), 10, 15, 10, ok.clone()).unwrap().approved, "too few pages");
        assert!(!judge_pages("read".into(), 1, 900, 10, ok).unwrap().approved, "implausible");
        assert!(!judge_pages("read".into(), 10, 25, 10, bad).unwrap().approved);
    }
}
