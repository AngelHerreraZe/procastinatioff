// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Direct sync between paired devices (security-critical): QUIC (iroh) authenticated by each
//! device's network key, LAN only, no relay and no third-party discovery.
//!
//! - Only keys listed in the profile's `MembersChanged` are accepted; any other is closed
//!   right after the handshake, before reading anything.
//! - A session is symmetric: both sides send `Hello` (version vector + signed time), then the
//!   events the other lacks (sealed for it with `age`), then `Done`.
//! - Every device blocks with its local copy; sync only speeds up convergence.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, Weak};
use std::time::Duration;

use base64::Engine;
use hmac::{Hmac, Mac};
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, PublicKey, RelayMode, SecretKey};
use procastinacioff_proto::{self as proto, Msg};
use sha2::Sha256;

use crate::api::Core;
use crate::error::CoreError;
use crate::events::SignedEvent;
use crate::storage::Db;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
pub const SESSION_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Incoming connections handled at once (one per peer is the normal case).
const MAX_INCOMING: usize = 8;
/// Room for the JSON around the blobs of an `Events` frame.
const FRAME_SLACK: usize = 4096;

/// Events a peer lacks, in (time, author, seq) order: the same order the ledgers accept them in,
/// so a member's events never arrive before the `MembersChanged` that added it.
pub fn missing_for(
    db: &Db,
    mine: &BTreeMap<String, u64>,
    theirs: &BTreeMap<String, u64>,
) -> Result<Vec<SignedEvent>, CoreError> {
    let mut out = Vec::new();
    for (author, &my_seq) in mine {
        let their_seq = theirs.get(author).copied().unwrap_or(0); // the first seq is 1
        if my_seq > their_seq {
            out.extend(db.all_by(author)?.into_iter().filter(|e| e.body.seq > their_seq));
        }
    }
    sort_for_ledger(&mut out);
    out.truncate(proto::MAX_EVENTS_PER_SESSION);
    Ok(out)
}

pub fn sort_for_ledger(evs: &mut [SignedEvent]) {
    evs.sort_by(|a, b| (a.body.at_ms, &a.body.author, a.body.seq).cmp(&(b.body.at_ms, &b.body.author, b.body.seq)));
}

/// Rotating mDNS instance name of one device of a profile on one UTC day. Without the profile's
/// discovery key it says nothing and it never repeats from one day to the next.
pub fn discovery_tag(key: &[u8; 32], node_id: &str, day: i64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(b"pco/mdns/v1\0");
    mac.update(node_id.as_bytes());
    mac.update(&day.to_be_bytes());
    hex::encode(&mac.finalize().into_bytes()[..8])
}

/// Splits blobs into frames under `MAX_FRAME` bytes and `MAX_EVENTS_PER_FRAME` events.
pub fn frames(blobs: Vec<String>) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut size = 0;
    for b in blobs {
        let full = out
            .last()
            .is_none_or(|f| f.len() >= proto::MAX_EVENTS_PER_FRAME || size + b.len() + FRAME_SLACK > proto::MAX_FRAME);
        if full {
            out.push(Vec::new());
            size = 0;
        }
        size += b.len() + 3;
        out.last_mut().expect("just pushed").push(b);
    }
    out
}

pub fn node_key(node_id: &str) -> Option<PublicKey> {
    let bytes: [u8; 32] = hex::decode(node_id).ok()?.try_into().ok()?;
    PublicKey::from_bytes(&bytes).ok()
}

pub fn node_hex(k: &PublicKey) -> String {
    hex::encode(k.as_bytes())
}

/// Who is on the other side, as this profile knows it.
#[derive(Clone)]
pub struct Peer {
    pub node_id: String,
    /// None while joining (the inviter is not in our ledger yet): nothing is sent to it.
    pub recipient: Option<age::x25519::Recipient>,
    pub device_id: Option<String>,
    pub time_source: bool,
}

/// What a finished session leaves.
pub struct Exchange {
    pub sent: u32,
    pub blobs: Vec<Vec<u8>>,
    pub time: proto::SignedTime,
    /// Local monotonic-equivalent moment the peer's `Hello` arrived (ms since the session started).
    pub hello_after_ms: i64,
}

pub struct PeerNet {
    rt: Option<tokio::runtime::Runtime>,
    pub ep: Endpoint,
    pub listening: bool,
}

impl Drop for PeerNet {
    fn drop(&mut self) {
        // May run inside the runtime (last reference dropped by a connection task).
        if let Some(rt) = self.rt.take() {
            rt.shutdown_background();
        }
    }
}

impl PeerNet {
    /// `listen`: accept connections on `port` (0 = any). Without it nothing incoming is accepted.
    pub fn bind(net_key: &[u8; 32], port: u16, listen: bool) -> Result<Self, CoreError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(CoreError::network)?;
        let alpns = if listen { vec![proto::SYNC_ALPN.to_vec(), proto::PAIR_ALPN.to_vec()] } else { vec![] };
        let ep = rt
            .block_on(
                Endpoint::builder(presets::Minimal)
                    .secret_key(SecretKey::from_bytes(net_key))
                    .alpns(alpns)
                    // SECURITY: no relay and no DNS/pkarr lookups: addresses come from mDNS or the QR.
                    .relay_mode(RelayMode::Disabled)
                    .clear_address_lookup()
                    .bind_addr(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))
                    .map_err(CoreError::network)?
                    .bind(),
            )
            .map_err(CoreError::network)?;
        Ok(Self { rt: Some(rt), ep, listening: listen })
    }

    pub fn block_on<F: Future>(&self, f: F) -> F::Output {
        self.rt.as_ref().expect("runtime lives as long as PeerNet").block_on(f)
    }

    pub fn spawn_accept_loop(&self, core: Weak<Core>) {
        let ep = self.ep.clone();
        let slots = Arc::new(tokio::sync::Semaphore::new(MAX_INCOMING));
        self.rt.as_ref().expect("runtime lives as long as PeerNet").spawn(async move {
            while let Some(incoming) = ep.accept().await {
                let Ok(slot) = slots.clone().try_acquire_owned() else { continue }; // busy: dropped
                let core = core.clone();
                tokio::spawn(async move {
                    let _slot = slot;
                    let _ = tokio::time::timeout(SESSION_TIMEOUT, async move {
                        let Ok(conn) = incoming.await else { return };
                        let Some(core) = core.upgrade() else { return };
                        core.serve(conn).await;
                    })
                    .await;
                });
            }
        });
    }

    /// Private addresses this device listens on (for the QR), at most `MAX_ADDRS`.
    /// ponytail: takes the first 4 in address order; with many virtual adapters the real LAN may
    /// be left out. Prefer the default-route interface if that shows up in testing.
    pub fn local_addrs(&self) -> Vec<String> {
        self.ep
            .addr()
            .ip_addrs()
            .filter(|a| crate::pairing::is_local(a.ip()) && !a.ip().is_loopback())
            .take(crate::pairing::MAX_ADDRS)
            .map(|a| a.to_string())
            .collect()
    }

    pub async fn connect(&self, node_id: &str, addrs: &[SocketAddr], alpn: &[u8]) -> Result<Connection, CoreError> {
        let key = node_key(node_id).ok_or_else(|| CoreError::protocol("invalid network key"))?;
        let addr = EndpointAddr::new(key).with_addrs(addrs.iter().map(|a| iroh::TransportAddr::Ip(*a)));
        tokio::time::timeout(CONNECT_TIMEOUT, self.ep.connect(addr, alpn))
            .await
            .map_err(|_| CoreError::network("timeout"))?
            .map_err(CoreError::network)
    }
}

fn net_err(e: impl std::fmt::Display) -> CoreError {
    CoreError::network(e)
}

/// One sync session over an open stream pair. `hello`: what we say first. `profile`: the profile
/// both must belong to. `outgoing`: given the peer's version vector, the events to send it.
pub async fn session(
    send: &mut SendStream,
    recv: &mut RecvStream,
    hello: Msg,
    profile: &str,
    outgoing: impl FnOnce(&BTreeMap<String, u64>) -> Result<Vec<String>, CoreError>,
) -> Result<Exchange, CoreError> {
    let started = std::time::Instant::now();
    let (tx, rx) = tokio::sync::oneshot::channel::<BTreeMap<String, u64>>();
    // Never "write everything, then read": with QUIC flow control two sides writing a lot at the
    // same time would block each other. Both directions run at once.
    let sending = async {
        proto::write_msg(send, &hello).await.map_err(net_err)?;
        let theirs = rx.await.map_err(|_| CoreError::protocol("no hello"))?;
        let blobs = outgoing(&theirs)?;
        let sent = blobs.len() as u32;
        for frame in frames(blobs) {
            proto::write_msg(send, &Msg::Events { blobs: frame }).await.map_err(net_err)?;
        }
        proto::write_msg(send, &Msg::Done).await.map_err(net_err)?;
        send.finish().map_err(net_err)?;
        // Until the peer has every byte: closing earlier could lose the tail.
        let _ = send.stopped().await;
        Ok::<_, CoreError>(sent)
    };
    let receiving = async {
        let Msg::Hello { v, profile: theirs_profile, have, time } = proto::read_msg(recv).await.map_err(net_err)?
        else {
            return Err(CoreError::protocol("expected hello"));
        };
        if !proto::version_supported(v) || theirs_profile != profile {
            return Err(CoreError::protocol("different version or profile"));
        }
        let hello_after_ms = started.elapsed().as_millis() as i64;
        let _ = tx.send(have);
        let mut blobs = Vec::new();
        loop {
            match proto::read_msg(recv).await.map_err(net_err)? {
                Msg::Events { blobs: b } => {
                    if b.len() > proto::MAX_EVENTS_PER_FRAME || blobs.len() + b.len() > proto::MAX_EVENTS_PER_SESSION {
                        return Err(CoreError::protocol("too many events"));
                    }
                    for s in b {
                        let blob = B64.decode(s).map_err(CoreError::protocol)?;
                        if blob.len() > proto::MAX_BLOB {
                            return Err(CoreError::protocol("blob too large"));
                        }
                        blobs.push(blob);
                    }
                }
                Msg::Done => break,
                _ => return Err(CoreError::protocol("unexpected message")),
            }
        }
        Ok((time, blobs, hello_after_ms))
    };
    let (sent, (time, blobs, hello_after_ms)) = tokio::try_join!(sending, receiving)?;
    Ok(Exchange { sent, blobs, time, hello_after_ms })
}

pub fn encode_blob(blob: &[u8]) -> String {
    B64.encode(blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_rotate_daily_and_differ_between_devices_and_profiles() {
        let (k1, k2) = ([1u8; 32], [2u8; 32]);
        let a = discovery_tag(&k1, "aa", 20_000);
        assert_eq!(a.len(), 16);
        assert_eq!(a, discovery_tag(&k1, "aa", 20_000));
        for other in
            [discovery_tag(&k1, "aa", 20_001), discovery_tag(&k1, "bb", 20_000), discovery_tag(&k2, "aa", 20_000)]
        {
            assert_ne!(a, other);
        }
    }

    #[test]
    fn frames_respect_count_and_size_limits() {
        let small: Vec<String> = (0..600).map(|_| "x".repeat(10)).collect();
        let f = frames(small);
        assert_eq!(f.iter().map(|x| x.len()).collect::<Vec<_>>(), vec![256, 256, 88]);
        let big: Vec<String> = (0..40).map(|_| "y".repeat(90_000)).collect();
        for frame in frames(big) {
            let bytes = proto::encode(&Msg::Events { blobs: frame }).unwrap();
            assert!(bytes.len() <= proto::MAX_FRAME + 4);
        }
        assert!(frames(vec![]).is_empty());
    }

    #[test]
    fn network_id_matches_iroh() {
        let secret = [7u8; 32];
        let dalek = hex::encode(ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key().to_bytes());
        assert_eq!(node_hex(&SecretKey::from_bytes(&secret).public()), dalek);
        assert!(node_key(&dalek).is_some());
        assert!(node_key("zz").is_none());
    }
}
