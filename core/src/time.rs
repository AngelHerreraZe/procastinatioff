// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Hora confiable 🔒: hora firmada por el relay + reloj monotónico del dispositivo, que el
//! usuario no puede cambiar. Adelantar el reloj del sistema ya no saca a nadie de DORMIR.
//!
//! La clave de hora se fija la primera vez que se usa (TOFU, ver ADR 0007): la conexión al relay
//! va por TLS con las raíces de Mozilla (no las del sistema, que el usuario podría manipular).

use ed25519_dalek::Signature;
use procastinacioff_proto as proto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeAnchor {
    /// Hora del relay (ms UNIX) en el momento del anclaje.
    pub trusted_ms: i64,
    /// Reloj monotónico del dispositivo en ese momento (Android: elapsedRealtime; Windows: GetTickCount64).
    pub mono_ms: i64,
}

pub fn verify_signed_time(t: &proto::SignedTime, key_hex: &str) -> bool {
    let mut msg = proto::TIME_DOMAIN.to_vec();
    msg.extend_from_slice(t.unix_ms.to_string().as_bytes());
    let Some(sig) = hex::decode(&t.sig).ok().and_then(|v| <[u8; 64]>::try_from(v).ok()) else { return false };
    proto::parse_key(key_hex).is_some_and(|k| k.verify_strict(&msg, &Signature::from_bytes(&sig)).is_ok())
}

/// Hora corregida: ancla + tiempo monotónico transcurrido. Si el dispositivo reinició
/// (monotónico menor que el del ancla), se usa el reloj del sistema hasta el próximo anclaje.
/// ponytail: el ancla vive en memoria; tras reiniciar el proceso se re-ancla en la siguiente sync.
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
        t.unix_ms = 2_000; // alguien cambia la hora
        assert!(!verify_signed_time(&t, &t.key.clone()));
    }

    #[test]
    fn moving_the_device_clock_does_not_move_trusted_time() {
        let a = Some(TimeAnchor { trusted_ms: 1_000_000, mono_ms: 50_000 });
        // 10 s después, con el reloj del sistema adelantado 9 horas
        assert_eq!(corrected_now(a, 1_010_000 + 9 * 3_600_000, 60_000), 1_010_000);
        // Tras reiniciar el dispositivo (monotónico menor) se usa el reloj del sistema
        assert_eq!(corrected_now(a, 5_000, 1_000), 5_000);
        assert_eq!(corrected_now(None, 7, 7), 7);
    }
}
