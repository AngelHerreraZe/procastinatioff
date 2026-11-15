// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{MatchedPath, Request};
use axum::middleware::{self, Next};
use axum::response::Response;
use ed25519_dalek::SigningKey;
use procastinacioff_relay::{config, db, push, routes};

/// Log mínimo: método, plantilla de ruta (sin IDs), código y latencia. Sin IP, sin cabeceras.
async fn log_requests(req: Request, next: Next) -> Response {
    let route = req.extensions().get::<MatchedPath>().map(|p| p.as_str().to_owned()).unwrap_or_else(|| "?".into());
    let method = req.method().clone();
    let start = Instant::now();
    let resp = next.run(req).await;
    tracing::info!(%method, route = %route, status = resp.status().as_u16(), ms = start.elapsed().as_millis() as u64);
    resp
}

fn load_or_create_time_key(path: &str) -> SigningKey {
    match std::fs::read(path).ok().and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()) {
        Some(seed) => SigningKey::from_bytes(&seed),
        None => {
            let k = SigningKey::generate(&mut rand::rngs::OsRng);
            std::fs::write(path, k.to_bytes()).expect("no se pudo guardar la clave de hora");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400)).ok();
            }
            k
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().with_env_filter("info").init();
    let cfg = config::Config::from_env();

    let conn = db::open(&cfg.db_path).expect("no se pudo abrir la base de datos");
    let time_key = load_or_create_time_key(&cfg.time_key_path);
    tracing::info!(time_pubkey = %hex::encode(time_key.verifying_key().to_bytes()), "clave pública de hora (fijarla en las apps)");

    let push = match (cfg.push_url, cfg.push_secret) {
        (Some(url), Some(secret)) => Some(Arc::new(push::Push::new(url, secret))),
        _ => None,
    };
    let state = routes::AppState {
        db: Arc::new(Mutex::new(conn)),
        replay: Arc::default(),
        limit: Arc::default(),
        time_key: Arc::new(time_key),
        push,
    };

    // Limpieza cada hora
    let db_for_cleanup = state.db.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
            let conn = db_for_cleanup.lock().unwrap_or_else(|p| p.into_inner());
            if let Err(e) = db::cleanup(&conn, now) {
                tracing::error!(error = %e, "fallo en la limpieza");
            }
        }
    });

    let app = routes::router(state).layer(middleware::from_fn(log_requests));
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await.expect("no se pudo abrir el puerto");
    tracing::info!(bind = %cfg.bind, "relay escuchando");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await
        .expect("servidor");
}
