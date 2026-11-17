// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! QR pairing, directly between devices (security-critical). The QR says how to reach the inviter
//! (network key + LAN addresses) and carries an ephemeral key that exists only in the QR; a 6-digit
//! code compared on both screens prevents MITM.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::CoreError;

pub const MAX_LABEL_CHARS: usize = 64;
pub const MAX_ADDRS: usize = 4;
pub const OFFER_VERSION: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingOffer {
    pub v: u8,
    /// 16 random bytes (hex); only valid while the inviter keeps the QR on screen (10 min).
    pub token: String,
    /// Ephemeral age recipient (exists only in the QR).
    pub eph: String,
    /// ID of the profile's genesis author (the only genesis B accepts).
    pub inviter: String,
    /// Network key of the inviting device: the joiner only talks to this key.
    pub node_id: String,
    /// "192.168.1.20:47800": private addresses only.
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub device_id: String,
    pub age_recipient: String,
    pub label: String,
    pub node_id: String,
    pub time_source: bool,
}

pub struct PendingPairing {
    pub offer: PairingOffer,
    pub eph: age::x25519::Identity,
    pub joiner: Option<JoinRequest>,
    pub created: std::time::Instant,
}

/// 6-digit code that both devices compute separately. It covers both network keys too: whoever
/// swaps one of them gets a different code.
pub fn sas(offer: &PairingOffer, joiner: &JoinRequest) -> String {
    let mut h = Sha256::new().chain_update(b"procastinatioff/sas/v2\n");
    for part in
        [&offer.token, &offer.inviter, &offer.node_id, &joiner.device_id, &joiner.age_recipient, &joiner.node_id]
    {
        h.update(part.as_bytes());
        h.update(b"\n");
    }
    let d = h.finalize();
    let n = u32::from_le_bytes([d[0], d[1], d[2], d[3]]) % 1_000_000;
    format!("{n:06}")
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Address reachable only inside a home/office network (or this machine, for tests).
pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_unique_local() || v6.is_loopback(),
    }
}

pub fn parse_offer(qr: &str) -> Result<PairingOffer, CoreError> {
    let bad = || CoreError::protocol("invalid QR");
    if qr.len() > 2048 {
        return Err(CoreError::protocol("QR too large"));
    }
    let o: PairingOffer = serde_json::from_str(qr).map_err(|_| bad())?;
    let addrs_ok = (1..=MAX_ADDRS).contains(&o.addrs.len())
        && o.addrs.iter().all(|a| a.parse::<SocketAddr>().is_ok_and(|s| is_local(s.ip()) && s.port() != 0));
    if o.v != OFFER_VERSION
        || !addrs_ok
        || !is_hex(&o.token, 32)
        || procastinatioff_proto::parse_key(&o.inviter).is_none()
        || procastinatioff_proto::parse_key(&o.node_id).is_none()
    {
        return Err(bad());
    }
    o.eph.parse::<age::x25519::Recipient>().map_err(|_| bad())?;
    Ok(o)
}

pub fn seal_join(req: &JoinRequest, eph: &str) -> Result<Vec<u8>, CoreError> {
    let r: age::x25519::Recipient = eph.parse().map_err(|_| CoreError::protocol("invalid ephemeral key"))?;
    let enc = age::Encryptor::with_recipients(std::iter::once(&r as &dyn age::Recipient)).map_err(CoreError::crypto)?;
    let mut out = Vec::new();
    let mut w = enc.wrap_output(&mut out).map_err(CoreError::crypto)?;
    w.write_all(&serde_json::to_vec(req).map_err(CoreError::protocol)?).map_err(CoreError::crypto)?;
    w.finish().map_err(CoreError::crypto)?;
    Ok(out)
}

pub fn open_join(blob: &[u8], eph: &age::x25519::Identity) -> Result<JoinRequest, CoreError> {
    if blob.len() > 8192 {
        return Err(CoreError::protocol("invalid pairing request"));
    }
    let dec = age::Decryptor::new(blob).map_err(CoreError::crypto)?;
    let mut r = dec.decrypt(std::iter::once(eph as &dyn age::Identity)).map_err(CoreError::crypto)?;
    let mut buf = Vec::new();
    r.by_ref().take(4096).read_to_end(&mut buf).map_err(CoreError::crypto)?;
    let req: JoinRequest = serde_json::from_slice(&buf).map_err(CoreError::protocol)?;
    if procastinatioff_proto::parse_key(&req.device_id).is_none()
        || procastinatioff_proto::parse_key(&req.node_id).is_none()
        || req.label.chars().count() > MAX_LABEL_CHARS
    {
        return Err(CoreError::protocol("invalid pairing request"));
    }
    req.age_recipient.parse::<age::x25519::Recipient>().map_err(|_| CoreError::protocol("invalid recipient"))?;
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_hex(seed: u8) -> String {
        hex::encode(ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key().to_bytes())
    }

    fn offer(eph: &age::x25519::Identity) -> PairingOffer {
        PairingOffer {
            v: OFFER_VERSION,
            token: "cd".repeat(16),
            eph: eph.to_public().to_string(),
            inviter: key_hex(3),
            node_id: key_hex(5),
            addrs: vec!["192.168.1.20:47800".into(), "[fd00::1]:47800".into()],
        }
    }

    fn joiner() -> JoinRequest {
        JoinRequest {
            device_id: key_hex(4),
            age_recipient: age::x25519::Identity::generate().to_public().to_string(),
            label: "PC".into(),
            node_id: key_hex(6),
            time_source: true,
        }
    }

    #[test]
    fn join_roundtrip_and_both_sides_compute_same_code() {
        let eph = age::x25519::Identity::generate();
        let o = offer(&eph);
        let parsed = parse_offer(&serde_json::to_string(&o).unwrap()).unwrap();
        let req = joiner();
        let opened = open_join(&seal_join(&req, &parsed.eph).unwrap(), &eph).unwrap();
        assert_eq!(sas(&parsed, &req), sas(&o, &opened));
        // A swapped network key changes the code
        let swapped = JoinRequest { node_id: key_hex(7), ..req.clone() };
        assert_ne!(sas(&o, &req), sas(&o, &swapped));
    }

    #[test]
    fn rejects_public_addresses_old_versions_and_garbage() {
        let eph = age::x25519::Identity::generate();
        for change in [
            (|o: &mut PairingOffer| o.addrs = vec!["8.8.8.8:47800".into()]) as fn(&mut PairingOffer),
            |o| o.addrs = vec![],
            |o| o.addrs = vec!["192.168.1.2:1".into(); 5],
            |o| o.addrs = vec!["192.168.1.2:0".into()],
            |o| o.v = 1,
            |o| o.node_id = "zz".into(),
        ] {
            let mut o = offer(&eph);
            change(&mut o);
            assert!(parse_offer(&serde_json::to_string(&o).unwrap()).is_err(), "{o:?}");
        }
        assert!(parse_offer("{}").is_err());
        assert!(parse_offer(&"x".repeat(5000)).is_err());
    }
}
