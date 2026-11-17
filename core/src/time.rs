// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Trusted time from the peers (security-critical): each `Hello` carries the speaker's time, signed
//! with its device key. The anchor + the device's monotonic clock (which the user cannot change)
//! give the time the rules use: moving the system clock forward does not get anyone out of SLEEP.
//!
//! Anchor: the most recent sample from a time source (a device whose clock the user cannot change)
//! in the last 24 h; otherwise the median of the samples (one peer with a changed clock cannot win).

use ed25519_dalek::{Signature, Signer, SigningKey};
use procastinatioff_proto as proto;

const FRESH_MS: i64 = 24 * 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeAnchor {
    /// Trusted time (UNIX ms) at the moment of anchoring.
    pub trusted_ms: i64,
    /// Device monotonic clock at that moment (Android: elapsedRealtime; Windows: GetTickCount64).
    pub mono_ms: i64,
}

/// Time seen from a peer (or from this device) and the local monotonic clock when it was seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub trusted_ms: i64,
    pub mono_ms: i64,
    pub from_time_source: bool,
}

pub fn sign_time(key: &SigningKey, unix_ms: i64) -> proto::SignedTime {
    proto::SignedTime { unix_ms, sig: hex::encode(key.sign(&proto::time_message(unix_ms)).to_bytes()) }
}

/// `device_id_hex`: the device key of the member that sent it.
pub fn verify_signed_time(t: &proto::SignedTime, device_id_hex: &str) -> bool {
    let Some(sig) = hex::decode(&t.sig).ok().and_then(|v| <[u8; 64]>::try_from(v).ok()) else { return false };
    proto::parse_key(device_id_hex)
        .is_some_and(|k| k.verify_strict(&proto::time_message(t.unix_ms), &Signature::from_bytes(&sig)).is_ok())
}

/// Picks the anchor: the latest time-source sample; otherwise the median of the samples.
pub fn pick_anchor(samples: &[Sample], mono_now: i64) -> Option<TimeAnchor> {
    let fresh: Vec<&Sample> = samples.iter().filter(|s| (0..FRESH_MS).contains(&(mono_now - s.mono_ms))).collect();
    if let Some(s) = fresh.iter().filter(|s| s.from_time_source).max_by_key(|s| s.mono_ms) {
        return Some(TimeAnchor { trusted_ms: s.trusted_ms, mono_ms: s.mono_ms });
    }
    // Bring every sample to "now" and take the median: a single peer with a changed clock does not rule.
    let mut now_estimates: Vec<i64> = fresh.iter().map(|s| s.trusted_ms + (mono_now - s.mono_ms)).collect();
    if now_estimates.is_empty() {
        return None;
    }
    now_estimates.sort_unstable();
    Some(TimeAnchor { trusted_ms: now_estimates[now_estimates.len() / 2], mono_ms: mono_now })
}

/// Corrected time: anchor + elapsed monotonic time. If the device rebooted
/// (monotonic lower than the anchor's), the system clock is used until the next anchoring.
/// ponytail: the anchor lives in memory; after a process restart it re-anchors on the next sync.
pub fn corrected_now(anchor: Option<TimeAnchor>, device_now_ms: i64, mono_ms: i64) -> i64 {
    match anchor {
        Some(a) if mono_ms >= a.mono_ms => a.trusted_ms + (mono_ms - a.mono_ms),
        _ => device_now_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;

    #[test]
    fn signature_binds_the_time_and_the_key() {
        let k = SigningKey::from_bytes(&[9u8; 32]);
        let me = hex::encode(k.verifying_key().to_bytes());
        let other = hex::encode(SigningKey::from_bytes(&[8u8; 32]).verifying_key().to_bytes());
        let mut t = sign_time(&k, 1_000);
        assert!(verify_signed_time(&t, &me));
        assert!(!verify_signed_time(&t, &other));
        t.unix_ms = 2_000; // someone changes the time
        assert!(!verify_signed_time(&t, &me));
    }

    #[test]
    fn a_time_source_wins_even_if_older() {
        let pc = Sample { trusted_ms: 1_000 * H, mono_ms: 10 * H, from_time_source: true };
        let phone = Sample { trusted_ms: 2_000 * H, mono_ms: 11 * H, from_time_source: false };
        let a = pick_anchor(&[phone, pc], 12 * H).unwrap();
        assert_eq!(a, TimeAnchor { trusted_ms: 1_000 * H, mono_ms: 10 * H });
    }

    #[test]
    fn median_of_three_ignores_one_liar() {
        let s = |trusted_ms, mono_ms| Sample { trusted_ms, mono_ms, from_time_source: false };
        // Two honest peers (seen 1 h apart) and one 9 h ahead.
        let samples = [s(100 * H, 5 * H), s(101 * H, 6 * H), s(110 * H, 6 * H)];
        let a = pick_anchor(&samples, 6 * H).unwrap();
        assert_eq!(a.trusted_ms, 101 * H);
        assert_eq!(a.mono_ms, 6 * H);
    }

    #[test]
    fn samples_older_than_24_hours_are_ignored() {
        let old = Sample { trusted_ms: 1, mono_ms: 0, from_time_source: true };
        assert_eq!(pick_anchor(&[old], 25 * H), None);
        let fresh = Sample { trusted_ms: 7, mono_ms: 2 * H, from_time_source: false };
        assert_eq!(pick_anchor(&[old, fresh], 25 * H).unwrap().trusted_ms, 7 + 23 * H);
    }

    #[test]
    fn moving_the_device_clock_does_not_move_trusted_time() {
        let a = Some(TimeAnchor { trusted_ms: 1_000_000, mono_ms: 50_000 });
        // 10 s later, with the system clock 9 hours ahead
        assert_eq!(corrected_now(a, 1_010_000 + 9 * H, 60_000), 1_010_000);
        // After the device reboots (lower monotonic) the system clock is used
        assert_eq!(corrected_now(a, 5_000, 1_000), 5_000);
        assert_eq!(corrected_now(None, 7, 7), 7);
    }
}
