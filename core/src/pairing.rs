// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Vinculación por QR 🔒. La clave efímera solo viaja en el QR (el relay nunca la ve);
//! un código de 6 dígitos comparado en ambas pantallas evita el MITM.

use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::CoreError;

pub const MAX_LABEL_CHARS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingOffer {
    pub v: u8,
    pub relay: String,
    pub mailbox: String,
    pub token: String,
    /// Destinatario age efímero (solo existe en el QR).
    pub eph: String,
    /// ID del dispositivo que invita (único génesis aceptable para B).
    pub inviter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub device_id: String,
    pub age_recipient: String,
    pub label: String,
}

pub struct PendingPairing {
    pub offer: PairingOffer,
    pub eph: age::x25519::Identity,
    pub joiner: Option<JoinRequest>,
}

/// Código de 6 dígitos que ambos dispositivos calculan por separado.
pub fn sas(offer: &PairingOffer, joiner: &JoinRequest) -> String {
    let d = Sha256::new()
        .chain_update(b"procastinacioff/sas/v1\n")
        .chain_update(offer.token.as_bytes())
        .chain_update(b"\n")
        .chain_update(offer.inviter.as_bytes())
        .chain_update(b"\n")
        .chain_update(joiner.device_id.as_bytes())
        .chain_update(b"\n")
        .chain_update(joiner.age_recipient.as_bytes())
        .finalize();
    let n = u32::from_le_bytes([d[0], d[1], d[2], d[3]]) % 1_000_000;
    format!("{n:06}")
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn parse_offer(qr: &str) -> Result<PairingOffer, CoreError> {
    if qr.len() > 2048 {
        return Err(CoreError::protocol("QR demasiado grande"));
    }
    let o: PairingOffer = serde_json::from_str(qr).map_err(|_| CoreError::protocol("QR inválido"))?;
    let relay_ok = o.relay.starts_with("https://")
        || o.relay.starts_with("http://127.0.0.1")
        || o.relay.starts_with("http://localhost");
    if o.v != 1
        || !relay_ok
        || !is_hex(&o.token, 32)
        || !is_hex(&o.mailbox, 32)
        || procastinacioff_proto::parse_key(&o.inviter).is_none()
    {
        return Err(CoreError::protocol("QR inválido"));
    }
    o.eph.parse::<age::x25519::Recipient>().map_err(|_| CoreError::protocol("QR inválido"))?;
    Ok(o)
}

pub fn seal_join(req: &JoinRequest, eph: &str) -> Result<Vec<u8>, CoreError> {
    let r: age::x25519::Recipient = eph.parse().map_err(|_| CoreError::protocol("clave efímera inválida"))?;
    let enc = age::Encryptor::with_recipients(std::iter::once(&r as &dyn age::Recipient)).map_err(CoreError::crypto)?;
    let mut out = Vec::new();
    let mut w = enc.wrap_output(&mut out).map_err(CoreError::crypto)?;
    w.write_all(&serde_json::to_vec(req).map_err(CoreError::protocol)?).map_err(CoreError::crypto)?;
    w.finish().map_err(CoreError::crypto)?;
    Ok(out)
}

pub fn open_join(blob: &[u8], eph: &age::x25519::Identity) -> Result<JoinRequest, CoreError> {
    let dec = age::Decryptor::new(blob).map_err(CoreError::crypto)?;
    let mut r = dec.decrypt(std::iter::once(eph as &dyn age::Identity)).map_err(CoreError::crypto)?;
    let mut buf = Vec::new();
    r.by_ref().take(4096).read_to_end(&mut buf).map_err(CoreError::crypto)?;
    let req: JoinRequest = serde_json::from_slice(&buf).map_err(CoreError::protocol)?;
    if procastinacioff_proto::parse_key(&req.device_id).is_none() || req.label.chars().count() > MAX_LABEL_CHARS {
        return Err(CoreError::protocol("solicitud de vinculación inválida"));
    }
    req.age_recipient.parse::<age::x25519::Recipient>().map_err(|_| CoreError::protocol("destinatario inválido"))?;
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(eph: &age::x25519::Identity) -> PairingOffer {
        PairingOffer {
            v: 1,
            relay: "https://relay.example".into(),
            mailbox: "ab".repeat(16),
            token: "cd".repeat(16),
            eph: eph.to_public().to_string(),
            inviter: hex::encode(ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]).verifying_key().to_bytes()),
        }
    }

    #[test]
    fn join_roundtrip_and_both_sides_compute_same_code() {
        let eph = age::x25519::Identity::generate();
        let o = offer(&eph);
        let qr = serde_json::to_string(&o).unwrap();
        let parsed = parse_offer(&qr).unwrap();
        let joiner = age::x25519::Identity::generate();
        let req = JoinRequest {
            device_id: hex::encode(ed25519_dalek::SigningKey::from_bytes(&[4u8; 32]).verifying_key().to_bytes()),
            age_recipient: joiner.to_public().to_string(),
            label: "PC".into(),
        };
        let opened = open_join(&seal_join(&req, &parsed.eph).unwrap(), &eph).unwrap();
        assert_eq!(sas(&parsed, &req), sas(&o, &opened));
    }

    #[test]
    fn rejects_plain_http_relay_and_garbage() {
        let eph = age::x25519::Identity::generate();
        let mut o = offer(&eph);
        o.relay = "http://relay.example".into();
        assert!(parse_offer(&serde_json::to_string(&o).unwrap()).is_err());
        assert!(parse_offer("{}").is_err());
        assert!(parse_offer(&"x".repeat(5000)).is_err());
    }
}
