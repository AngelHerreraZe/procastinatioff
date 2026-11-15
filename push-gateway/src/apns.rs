// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use serde_json::json;

pub struct Apns {
    key: EncodingKey,
    key_id: String,
    team_id: String,
    topic: String, // bundle id de la app iOS
    client: reqwest::Client,
    cached: Mutex<Option<(String, Instant)>>,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    iat: u64,
}

impl Apns {
    /// Lee APNS_KEY_PATH (.p8), APNS_KEY_ID, APNS_TEAM_ID, APNS_TOPIC. None si falta algo.
    pub fn from_env() -> Option<Self> {
        let pem = std::fs::read(std::env::var("APNS_KEY_PATH").ok()?).ok()?;
        Some(Self {
            key: EncodingKey::from_ec_pem(&pem).ok()?,
            key_id: std::env::var("APNS_KEY_ID").ok()?,
            team_id: std::env::var("APNS_TEAM_ID").ok()?,
            topic: std::env::var("APNS_TOPIC").ok()?,
            client: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().ok()?,
            cached: Mutex::new(None),
        })
    }

    /// Apple pide renovar el JWT entre 20 y 60 minutos.
    fn jwt(&self) -> Result<String, String> {
        let mut cached = self.cached.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((t, at)) = cached.as_ref()
            && at.elapsed() < Duration::from_secs(40 * 60)
        {
            return Ok(t.clone());
        }
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.key_id.clone());
        let iat = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let t = encode(&header, &Claims { iss: &self.team_id, iat }, &self.key).map_err(|e| e.to_string())?;
        *cached = Some((t.clone(), Instant::now()));
        Ok(t)
    }

    pub async fn send(&self, token: &str, urgent: bool, sandbox: bool) -> Result<(), String> {
        if token.is_empty() || token.len() > 200 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("token APNs inválido".into());
        }
        let host = if sandbox { "api.sandbox.push.apple.com" } else { "api.push.apple.com" };
        // 🔒 El texto visible es genérico: el contenido real lo descifra la
        // Notification Service Extension en el iPhone (Fase 4).
        let (push_type, priority, payload) = if urgent {
            (
                "alert",
                "10",
                json!({"aps": {"alert": {"title": "ProcastinaciOFF", "body": "Tienes una novedad"}, "mutable-content": 1}}),
            )
        } else {
            ("background", "5", json!({"aps": {"content-available": 1}}))
        };
        let resp = self
            .client
            .post(format!("https://{host}/3/device/{token}"))
            .header("authorization", format!("bearer {}", self.jwt()?))
            .header("apns-topic", &self.topic)
            .header("apns-push-type", push_type)
            .header("apns-priority", priority)
            .header("apns-expiration", "0")
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if resp.status().is_success() { Ok(()) } else { Err(format!("APNs {}", resp.status())) }
    }
}
