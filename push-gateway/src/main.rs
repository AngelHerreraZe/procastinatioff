// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Push gateway: reenvía "despierta" a APNs/FCM/UnifiedPush. No guarda nada.

mod apns;
mod fcm;
mod unifiedpush;

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

#[derive(Deserialize)]
struct Target {
    platform: String,
    token: String,
    urgent: bool,
}

#[derive(Deserialize)]
struct WakeRequest {
    targets: Vec<Target>,
}

struct App {
    /// Un secreto por relay autorizado (PG_SECRETS="secreto1,secreto2").
    secrets: Vec<String>,
    apns: Option<apns::Apns>,
    fcm: Option<fcm::Fcm>,
}

/// Comparación en tiempo constante (no revela cuántos caracteres coinciden).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn wake(State(app): State<Arc<App>>, headers: HeaderMap, Json(req): Json<WakeRequest>) -> StatusCode {
    let presented =
        headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    if !app.secrets.iter().any(|s| ct_eq(s.as_bytes(), presented.as_bytes())) {
        return StatusCode::UNAUTHORIZED;
    }
    if req.targets.len() > 50 {
        return StatusCode::BAD_REQUEST;
    }
    for t in req.targets {
        let app = app.clone();
        tokio::spawn(async move {
            let result = match t.platform.as_str() {
                "apns" | "apns-sandbox" => match &app.apns {
                    Some(a) => a.send(&t.token, t.urgent, t.platform == "apns-sandbox").await,
                    None => Err("APNs no configurado".into()),
                },
                "fcm" => match &app.fcm {
                    Some(f) => f.send(&t.token, t.urgent).await,
                    None => Err("FCM no configurado".into()),
                },
                "unifiedpush" => unifiedpush::send(&t.token).await,
                _ => Err("plataforma desconocida".into()),
            };
            if let Err(e) = result {
                tracing::warn!(platform = %t.platform, error = %e, "envío fallido"); // nunca el token
            }
        });
    }
    StatusCode::ACCEPTED
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().with_env_filter("info").init();
    let secrets: Vec<String> = std::env::var("PG_SECRETS")
        .expect("falta PG_SECRETS")
        .split(',')
        .map(str::trim)
        .filter(|s| s.len() >= 32)
        .map(String::from)
        .collect();
    assert!(!secrets.is_empty(), "PG_SECRETS necesita al menos un secreto de 32+ caracteres");
    let app = Arc::new(App { secrets, apns: apns::Apns::from_env(), fcm: fcm::Fcm::from_env() });
    tracing::info!(apns = app.apns.is_some(), fcm = app.fcm.is_some(), "push gateway listo");
    let router = Router::new().route("/healthz", get(|| async { "ok" })).route("/v1/wake", post(wake)).with_state(app);
    let bind = std::env::var("PG_BIND").unwrap_or_else(|_| "127.0.0.1:8090".into());
    let listener = tokio::net::TcpListener::bind(&bind).await.expect("no se pudo abrir el puerto");
    axum::serve(listener, router).await.expect("servidor");
}

#[cfg(test)]
mod tests {
    use super::ct_eq;

    #[test]
    fn constant_time_compare() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }
}
