// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Trusted time (security-critical): time signed by the relay + the device's monotonic clock, which the
//! user cannot change. Moving the system clock forward no longer gets anyone out of SLEEP.
//!
//! The time key is pinned on first use (TOFU): the connection to the relay uses TLS with
//! Mozilla's roots (not the system's, which the user could tamper with).

use ed25519_dalek::Signature;
use procastinacioff_proto as proto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeAnchor {
    /// Relay time (UNIX ms) at the moment of anchoring.
    pub trusted_ms: i64,
    /// Device monotonic clock at that moment (Android: elapsedRealtime; Windows: GetTickCount64).
    pub mono_ms: i64,
}

pub fn verify_signed_time(t: &proto::SignedTime, key_hex: &str) -> bool {
    let mut msg = proto::TIME_DOMAIN.to_vec();
    msg.extend_from_slice(t.unix_ms.to_string().as_bytes());
    let Some(sig) = hex::decode(&t.sig).ok().and_then(|v| <[u8; 64]>::try_from(v).ok()) else { return false };
    proto::parse_key(key_hex).is_some_and(|k| k.verify_strict(&msg, &Signature::from_bytes(&sig)).is_ok())
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
    use ed25519_dalek::{Signer, SigningKey};

    fn signed(key: &SigningKey, unix_ms: i64) -> proto::SignedTime {
        let mut msg = proto::TIME_DOMAIN.to_vec();
        msg.extend_from_slice(unix_ms.to_string().as_bytes());
        proto::SignedTime {
            unix_ms,
            sig: hex::encode(key.sign(&msg).to_bytes()),
            key: hex::encode(key.verifying_key().to_bytes()),
        }
    }

    #[test]
    fn signature_binds_the_time_and_the_key() {
        let k = SigningKey::from_bytes(&[9u8; 32]);
        let other = hex::encode(SigningKey::from_bytes(&[8u8; 32]).verifying_key().to_bytes());
        let mut t = signed(&k, 1_000);
        assert!(verify_signed_time(&t, &t.key.clone()));
        assert!(!verify_signed_time(&t, &other));
        t.unix_ms = 2_000; // someone changes the time
        assert!(!verify_signed_time(&t, &t.key.clone()));
    }

    #[test]
    fn moving_the_device_clock_does_not_move_trusted_time() {
        let a = Some(TimeAnchor { trusted_ms: 1_000_000, mono_ms: 50_000 });
        // 10 s later, with the system clock 9 hours ahead
        assert_eq!(corrected_now(a, 1_010_000 + 9 * 3_600_000, 60_000), 1_010_000);
        // After the device reboots (lower monotonic) the system clock is used
        assert_eq!(corrected_now(a, 5_000, 1_000), 5_000);
        assert_eq!(corrected_now(None, 7, 7), 7);
    }
}
