// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Messages between paired devices (security-critical). No state; the network is in core/src/peer.rs.
//!
//! Wire format: `u32` big-endian length + JSON. A reader rejects any length above `MAX_FRAME`
//! before allocating.

use std::collections::BTreeMap;
use std::io;

use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Sync between members of a profile.
pub const SYNC_ALPN: &[u8] = b"procastinatioff/sync/1";
/// Pairing: only the device showing the QR accepts it, and only while the QR is valid.
pub const PAIR_ALPN: &[u8] = b"procastinatioff/pair/1";
/// 2: change authorization (ADR 0013): the trusted contact role and its events.
pub const PROTOCOL_VERSION: u8 = 2;
/// Oldest version still accepted. A family's devices update at different paces: after raising
/// `PROTOCOL_VERSION`, keep the previous one here for 6 months (plan 10 · 1.1), then raise it.
/// Version 1 never shipped (its role names changed in 2), so nothing older is accepted yet.
pub const MIN_PROTOCOL_VERSION: u8 = 2;

/// Can this device sync with a peer that speaks version `v`?
pub fn version_supported(v: u8) -> bool {
    (MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&v)
}
/// Maximum size of one message (blobs are already padded to <= 64 KB).
pub const MAX_FRAME: usize = 1 << 20;
pub const MAX_EVENTS_PER_FRAME: usize = 256;
/// A session never moves more than this; whatever is left goes in the next sync.
pub const MAX_EVENTS_PER_SESSION: usize = 10_000;
/// 64 KB of padding + age header.
pub const MAX_BLOB: usize = 70 * 1024;
/// Domain separation: a time signature is never valid as an event signature.
pub const TIME_DOMAIN: &[u8] = b"procastinatioff/time/v2\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Msg {
    /// The first thing each side says. `have`: author -> last `seq` it holds (version vector).
    Hello {
        v: u8,
        profile: String,
        have: BTreeMap<String, u64>,
        time: SignedTime,
    },
    /// age blobs in base64, each one a `SignedEvent` sealed for the receiver.
    Events {
        blobs: Vec<String>,
    },
    Done,
    /// Pairing: the joiner's request (base64), sealed with the ephemeral key from the QR.
    Join {
        token: String,
        blob: String,
    },
}

/// Time of the device that speaks, signed with its device key (not its network key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedTime {
    pub unix_ms: i64,
    pub sig: String,
}

/// Bytes signed for a `SignedTime`.
pub fn time_message(unix_ms: i64) -> Vec<u8> {
    let mut m = TIME_DOMAIN.to_vec();
    m.extend_from_slice(&unix_ms.to_be_bytes());
    m
}

pub fn parse_key(hex_key: &str) -> Option<VerifyingKey> {
    let bytes: [u8; 32] = hex::decode(hex_key).ok()?.try_into().ok()?;
    VerifyingKey::from_bytes(&bytes).ok()
}

pub fn encode(m: &Msg) -> io::Result<Vec<u8>> {
    let body = serde_json::to_vec(m).map_err(io::Error::other)?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::other("message too large"));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode(body: &[u8]) -> io::Result<Msg> {
    serde_json::from_slice(body).map_err(io::Error::other)
}

pub async fn write_msg<W: AsyncWrite + Unpin>(w: &mut W, m: &Msg) -> io::Result<()> {
    w.write_all(&encode(m)?).await
}

pub async fn read_msg<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<Msg> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let n = u32::from_be_bytes(len) as usize;
    // SECURITY: checked before allocating: a peer cannot make us reserve 4 GB.
    if n > MAX_FRAME {
        return Err(io::Error::other("message too large"));
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body).await?;
    decode(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn roundtrip(m: Msg) -> Msg {
        let bytes = encode(&m).unwrap();
        read_msg(&mut bytes.as_slice()).await.unwrap()
    }

    #[tokio::test]
    async fn every_message_roundtrips() {
        let time = SignedTime { unix_ms: 1_700_000_000_000, sig: "ab".repeat(64) };
        let have = BTreeMap::from([("aa".to_string(), 12), ("bb".to_string(), 40)]);
        for m in [
            Msg::Hello { v: PROTOCOL_VERSION, profile: "cc".repeat(32), have, time },
            Msg::Events { blobs: vec!["YWdl".into(), "eA==".into()] },
            Msg::Done,
            Msg::Join { token: "dd".repeat(16), blob: "YWdl".into() },
        ] {
            assert_eq!(roundtrip(m.clone()).await, m);
        }
    }

    #[tokio::test]
    async fn oversized_header_is_rejected_without_allocating() {
        // 2 GB announced, 4 bytes sent: an error, not an allocation of 2 GB.
        let mut wire: &[u8] = &(2u32 << 30).to_be_bytes();
        let err = read_msg(&mut wire).await.unwrap_err();
        assert!(err.to_string().contains("too large"));
        let huge = Msg::Events { blobs: vec!["x".repeat(MAX_FRAME)] };
        assert!(encode(&huge).is_err());
    }

    #[test]
    fn versions_in_the_support_window_only() {
        assert!(version_supported(PROTOCOL_VERSION));
        assert!(version_supported(MIN_PROTOCOL_VERSION));
        assert!(!version_supported(MIN_PROTOCOL_VERSION - 1));
        assert!(!version_supported(PROTOCOL_VERSION + 1), "a newer peer: update this app");
    }

    #[tokio::test]
    async fn truncated_or_garbage_frames_fail() {
        let mut short: &[u8] = &[0, 0, 0, 10, b'{'];
        assert!(read_msg(&mut short).await.is_err());
        let mut garbage = 3u32.to_be_bytes().to_vec();
        garbage.extend_from_slice(b"{x}");
        assert!(read_msg(&mut garbage.as_slice()).await.is_err());
        let mut unknown = encode(&Msg::Done).unwrap();
        unknown.splice(4.., br#"{"t":"nope"}"#.iter().copied());
        unknown.splice(0..4, (12u32).to_be_bytes());
        assert!(read_msg(&mut unknown.as_slice()).await.is_err());
    }
}
