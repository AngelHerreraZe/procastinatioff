// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Autenticación, anti-repetición y límite de peticiones 🔒.

use std::collections::HashMap;
use std::sync::Mutex;

use axum::http::HeaderMap;
use procastinacioff_proto as proto;

use crate::error::ApiError;

/// Nonces vistos recientemente (solo en RAM).
#[derive(Default)]
pub struct Replay {
    seen: Mutex<HashMap<String, i64>>,
}

impl Replay {
    /// true si el nonce es nuevo. Limpia los antiguos de paso.
    pub fn check_and_store(&self, nonce: &str, now_s: i64) -> bool {
        let mut seen = self.seen.lock().unwrap_or_else(|p| p.into_inner());
        if seen.len() > 50_000 {
            seen.retain(|_, t| now_s - *t <= 2 * proto::MAX_SKEW_S);
        }
        if seen.contains_key(nonce) {
            return false;
        }
        seen.insert(nonce.to_owned(), now_s);
        true
    }
}

/// Límite simple: N peticiones por clave y minuto.
/// ponytail: ventana fija en RAM; si hay varias instancias del relay, mover a Redis o al balanceador.
#[derive(Default)]
pub struct RateLimit {
    windows: Mutex<HashMap<String, (i64, u32)>>,
}

impl RateLimit {
    pub const PER_MINUTE: u32 = 120;

    pub fn allow(&self, key: &str, now_s: i64) -> bool {
        let minute = now_s / 60;
        let mut w = self.windows.lock().unwrap_or_else(|p| p.into_inner());
        if w.len() > 100_000 {
            w.retain(|_, (m, _)| *m == minute);
        }
        let entry = w.entry(key.to_owned()).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        entry.1 += 1;
        entry.1 <= Self::PER_MINUTE
    }
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> Result<&'a str, ApiError> {
    h.get(name).and_then(|v| v.to_str().ok()).ok_or(ApiError::Unauthorized)
}

/// Verifica la firma de la petición y devuelve la clave pública (hex) del dispositivo.
pub fn authenticate(
    headers: &HeaderMap,
    method: &str,
    path_and_query: &str,
    body: &[u8],
    now_s: i64,
    replay: &Replay,
    limit: &RateLimit,
) -> Result<String, ApiError> {
    let auth = proto::AuthHeaders {
        key: header(headers, proto::H_KEY)?.to_owned(),
        ts: header(headers, proto::H_TS)?.parse().map_err(|_| ApiError::Unauthorized)?,
        nonce: header(headers, proto::H_NONCE)?.to_owned(),
        sig: header(headers, proto::H_SIG)?.to_owned(),
    };
    proto::verify_request(&auth, method, path_and_query, body, now_s).map_err(|_| ApiError::Unauthorized)?;
    if !limit.allow(&auth.key, now_s) {
        return Err(ApiError::TooManyRequests);
    }
    if !replay.check_and_store(&auth.nonce, now_s) {
        return Err(ApiError::Unauthorized);
    }
    // Normalizada a minúsculas: la misma clave en mayúsculas no cuenta como otro dispositivo.
    Ok(auth.key.to_ascii_lowercase())
}
