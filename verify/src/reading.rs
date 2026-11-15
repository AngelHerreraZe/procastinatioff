// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reading presence: 1 sample every 20 s (is there a face? is there a book?) → minutes.

use std::sync::Mutex;

use crate::result::{VMethod, VerificationResult, make_result};

/// Minimum interval between samples (prevents "inflating" minutes by sending many).
pub const SAMPLE_EVERY_MS: i64 = 20_000;
/// Share of the target that is enough to approve (calibration knob).
pub const APPROVE_PERCENT: u32 = 85;

struct Inner {
    last_sample_ms: Option<i64>,
    good_samples: u32,
}

#[derive(uniffi::Object)]
pub struct ReadingTracker {
    inner: Mutex<Inner>,
}

impl Default for ReadingTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[uniffi::export]
impl ReadingTracker {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self { inner: Mutex::new(Inner { last_sample_ms: None, good_samples: 0 }) }
    }

    /// A sample: is there a face? is there a book/reader? Returns the accumulated minutes.
    pub fn add_sample(&self, at_ms: i64, face: bool, book: bool) -> u16 {
        let mut s = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(last) = s.last_sample_ms
            && at_ms - last < SAMPLE_EVERY_MS - 2_000
        {
            return Self::minutes_of(s.good_samples);
        }
        s.last_sample_ms = Some(at_ms);
        if face && book {
            s.good_samples += 1;
        }
        Self::minutes_of(s.good_samples)
    }

    /// Approved if it reaches 85 % of the target.
    pub fn finish(&self, goal_id: String, target_minutes: u16) -> Option<VerificationResult> {
        let minutes = Self::minutes_of(self.inner.lock().unwrap_or_else(|p| p.into_inner()).good_samples);
        let needed = (target_minutes as u32 * APPROVE_PERCENT).div_ceil(100) as u16;
        let score = ((minutes as u32 * 100) / target_minutes.max(1) as u32).min(100) as u8;
        make_result(&goal_id, VMethod::ReadingPresence, minutes >= needed, score, minutes)
    }
}

impl ReadingTracker {
    fn minutes_of(samples: u32) -> u16 {
        ((samples as i64 * SAMPLE_EVERY_MS) / 60_000).min(u16::MAX as i64) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spamming_samples_does_not_inflate_minutes() {
        let t = ReadingTracker::new();
        for i in 0..1000 {
            t.add_sample(i * 10, true, true); // 1000 samples in 10 s
        }
        assert_eq!(t.add_sample(10_001, true, true), 0);
    }

    #[test]
    fn thirty_minutes_of_good_samples_approve() {
        let t = ReadingTracker::new();
        for i in 0..90 {
            t.add_sample(i * SAMPLE_EVERY_MS, true, true);
        }
        let r = t.finish("read-30".into(), 30).unwrap();
        assert!(r.approved);
        assert_eq!(r.minutes, 30);
    }

    #[test]
    fn face_without_book_does_not_count() {
        let t = ReadingTracker::new();
        for i in 0..90 {
            t.add_sample(i * SAMPLE_EVERY_MS, true, i % 3 == 0); // book only in 1 out of 3 samples
        }
        let r = t.finish("read-30".into(), 30).unwrap();
        assert!(!r.approved);
        assert_eq!(r.minutes, 10);
    }
}
