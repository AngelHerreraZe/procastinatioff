// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    token_uri: String,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

pub struct Fcm {
    sa: ServiceAccount,
    key: EncodingKey,
    project: String,
    client: reqwest::Client,
    cached: Mutex<Option<(String, Instant)>>,
}

impl Fcm {
    /// Lee FCM_SA_PATH (JSON de la cuenta de servicio) y FCM_PROJECT_ID.
    pub fn from_env() -> Option<Self> {
        let sa: ServiceAccount =
            serde_json::from_slice(&std::fs::read(std::env::var("FCM_SA_PATH").ok()?).ok()?).ok()?;
        let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes()).ok()?;
        Some(Self {
            sa,
            key,
            project: std::env::var("FCM_PROJECT_ID").ok()?,
            client: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().ok()?,
            cached: Mutex::new(None),
        })
    }

    async fn access_token(&self) -> Result<String, String> {
        if let Some((t, valid_until)) = self.cached.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
            && Instant::now() < *valid_until
        {
            return Ok(t.clone());
        }
        let iat = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let claims = Claims {
            iss: &self.sa.client_email,
            scope: "https://www.googleapis.com/auth/firebase.messaging",
            aud: &self.sa.token_uri,
            iat,
            exp: iat + 3600,
        };
        let assertion = encode(&Header::new(Algorithm::RS256), &claims, &self.key).map_err(|e| e.to_string())?;
        let body = format!("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}");
        let resp: TokenResponse = self
            .client
            .post(&self.sa.token_uri)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let valid_until = Instant::now() + Duration::from_secs(resp.expires_in.saturating_sub(300));
        *self.cached.lock().unwrap_or_else(|p| p.into_inner()) = Some((resp.access_token.clone(), valid_until));
        Ok(resp.access_token)
    }

    pub async fn send(&self, token: &str, urgent: bool) -> Result<(), String> {
        if token.is_empty() || token.len() > 512 {
            return Err("token FCM inválido".into());
        }
        // Mensaje SOLO de datos: la app sincroniza, descifra y crea su propia notificación local.
        let msg = json!({"message": {
            "token": token,
            "data": {"t": "sync"},
            "android": {"priority": if urgent { "HIGH" } else { "NORMAL" }}
        }});
        let resp = self
            .client
            .post(format!("https://fcm.googleapis.com/v1/projects/{}/messages:send", self.project))
            .bearer_auth(self.access_token().await?)
            .json(&msg)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if resp.status().is_success() { Ok(()) } else { Err(format!("FCM {}", resp.status())) }
    }
}
