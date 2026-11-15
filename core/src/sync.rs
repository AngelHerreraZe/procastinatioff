// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Relay client, being replaced by LAN/P2P sync (security-critical). HTTPS only (except localhost in
//! development), responses capped at 1 MB and requests signed with the device key.

use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use ed25519_dalek::SigningKey;
use procastinacioff_proto as proto;
use rand::{RngCore, rngs::OsRng};

use crate::error::CoreError;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const MAX_RESPONSE: u64 = 1_000_000;

/// Downloads a public file over HTTPS (signed lists): no redirects and a maximum size.
/// Authenticity comes from the signature, not from the server.
pub fn fetch_public(url: &str, max_bytes: u64) -> Result<Vec<u8>, CoreError> {
    if !url.starts_with("https://") {
        return Err(CoreError::protocol("HTTPS only"));
    }
    let agent =
        ureq::AgentBuilder::new().timeout(Duration::from_secs(60)).redirects(0).user_agent("procastinacioff").build();
    let r = agent.get(url).call().map_err(|e| CoreError::network(e.kind()))?;
    let mut buf = Vec::new();
    r.into_reader().take(max_bytes).read_to_end(&mut buf).map_err(CoreError::network)?;
    Ok(buf)
}

pub struct Relay {
    base: String,
    agent: ureq::Agent,
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Vec<u8>, CoreError> {
    serde_json::to_vec(v).map_err(CoreError::protocol)
}

impl Relay {
    pub fn new(base: &str) -> Result<Self, CoreError> {
        let base = base.trim_end_matches('/').to_owned();
        let local = base.starts_with("http://127.0.0.1") || base.starts_with("http://localhost");
        if !base.starts_with("https://") && !local {
            return Err(CoreError::protocol("the relay must use HTTPS"));
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(20))
            .redirects(0) // a relay does not redirect; following redirects could leak signed requests
            .user_agent("procastinacioff") // no version nor platform: smaller fingerprint
            .build();
        Ok(Self { base, agent })
    }

    /// Makes a request. If `key` is Some, signs it.
    fn call(
        &self,
        key: Option<&SigningKey>,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let mut req = self.agent.request(method, &format!("{}{}", self.base, path));
        if let Some(k) = key {
            let mut nonce = [0u8; 16];
            OsRng.fill_bytes(&mut nonce);
            // The HTTP signature uses the system clock; the RULES never do (they receive Clock).
            let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_err(CoreError::network)?.as_secs() as i64;
            let h = proto::sign_request(k, method, path, body, ts, hex::encode(nonce));
            req = req
                .set(proto::H_KEY, &h.key)
                .set(proto::H_TS, &h.ts.to_string())
                .set(proto::H_NONCE, &h.nonce)
                .set(proto::H_SIG, &h.sig);
        }
        let resp = if method == "GET" || method == "DELETE" {
            req.call()
        } else {
            req.set("content-type", "application/json").send_bytes(body)
        };
        match resp {
            Ok(r) => {
                let mut buf = Vec::new();
                r.into_reader().take(MAX_RESPONSE).read_to_end(&mut buf).map_err(CoreError::network)?;
                Ok(Some(buf))
            }
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(ureq::Error::Status(code, _)) => Err(CoreError::network(format!("HTTP {code}"))),
            Err(e) => Err(CoreError::network(e.kind())),
        }
    }

    fn json<T: serde::de::DeserializeOwned>(bytes: Option<Vec<u8>>) -> Result<T, CoreError> {
        let b = bytes.ok_or_else(|| CoreError::network("empty response"))?;
        serde_json::from_slice(&b).map_err(CoreError::protocol)
    }

    pub fn create_mailbox(&self, key: &SigningKey) -> Result<String, CoreError> {
        let pow = proto::pow_solve(&hex::encode(key.verifying_key().to_bytes()));
        let body = to_json(&proto::CreateMailbox { pow })?;
        let r: proto::MailboxCreated = Self::json(self.call(Some(key), "POST", "/v1/mailboxes", &body)?)?;
        Ok(r.mailbox)
    }

    pub fn add_member(&self, key: &SigningKey, mailbox: &str, pubkey: &str) -> Result<(), CoreError> {
        let body = to_json(&proto::AddMember { pubkey: pubkey.into() })?;
        self.call(Some(key), "POST", &format!("/v1/mailboxes/{mailbox}/members"), &body)?;
        Ok(())
    }

    pub fn post_event(&self, key: &SigningKey, mailbox: &str, blob: &[u8], urgent: bool) -> Result<u64, CoreError> {
        let body = to_json(&proto::PostEvent { blob: B64.encode(blob), urgent })?;
        let r: proto::PostedEvent =
            Self::json(self.call(Some(key), "POST", &format!("/v1/mailboxes/{mailbox}/events"), &body)?)?;
        Ok(r.seq)
    }

    pub fn events(&self, key: &SigningKey, mailbox: &str, after: u64) -> Result<proto::EventsPage, CoreError> {
        Self::json(self.call(Some(key), "GET", &format!("/v1/mailboxes/{mailbox}/events?after={after}"), &[])?)
    }

    pub fn put_pairing(&self, token: &str, blob: &[u8]) -> Result<(), CoreError> {
        let body = to_json(&proto::PairingBlob { blob: B64.encode(blob) })?;
        self.call(None, "POST", &format!("/v1/pairing/{token}"), &body)?;
        Ok(())
    }

    pub fn get_pairing(&self, token: &str) -> Result<Option<Vec<u8>>, CoreError> {
        match self.call(None, "GET", &format!("/v1/pairing/{token}"), &[])? {
            None => Ok(None),
            Some(b) => {
                let p: proto::PairingBlob = serde_json::from_slice(&b).map_err(CoreError::protocol)?;
                Ok(Some(B64.decode(p.blob).map_err(CoreError::protocol)?))
            }
        }
    }

    pub fn register_push(&self, key: &SigningKey, platform: &str, token: &str) -> Result<(), CoreError> {
        let id = hex::encode(key.verifying_key().to_bytes());
        let body = to_json(&proto::PushToken { platform: platform.into(), token: token.into() })?;
        self.call(Some(key), "PUT", &format!("/v1/devices/{id}/push"), &body)?;
        Ok(())
    }

    /// The relay's signed time (unauthenticated: it is public).
    pub fn time(&self) -> Result<proto::SignedTime, CoreError> {
        Self::json(self.call(None, "GET", "/v1/time", &[])?)
    }

    pub fn decode_blob(b64: &str) -> Result<Vec<u8>, CoreError> {
        B64.decode(b64).map_err(CoreError::protocol)
    }
}
