// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Rutas del relay 🔒.
//! ponytail: `db.lock()` bloquea un hilo de Tokio mientras SQLite trabaja (milisegundos);
//! si la latencia p99 supera 50 ms, mover las llamadas a `spawn_blocking` o a un pool.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, Method, Uri};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use procastinacioff_proto as proto;
use rand::{RngCore, rngs::OsRng};
use rusqlite::Connection;
use serde::Deserialize;

use crate::auth::{RateLimit, Replay, authenticate};
use crate::db;
use crate::error::ApiError;
use crate::push::Push;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const AGE_MAGIC: &[u8] = b"age-encryption.org/v1\n";

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub replay: Arc<Replay>,
    pub limit: Arc<RateLimit>,
    pub time_key: Arc<SigningKey>,
    pub push: Option<Arc<Push>>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn pq(uri: &Uri) -> &str {
    uri.path_and_query().map(|p| p.as_str()).unwrap_or("/")
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

impl AppState {
    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn auth(&self, h: &HeaderMap, m: &Method, uri: &Uri, body: &[u8]) -> Result<String, ApiError> {
        authenticate(h, m.as_str(), pq(uri), body, now_ms() / 1000, &self.replay, &self.limit)
    }

    /// Autentica y exige que la clave sea miembro del buzón.
    fn member(&self, id: &str, h: &HeaderMap, m: &Method, uri: &Uri, body: &[u8]) -> Result<String, ApiError> {
        if !is_hex(id, 32) {
            return Err(ApiError::NotFound);
        }
        let key = self.auth(h, m, uri, body)?;
        if !db::is_member(&self.conn(), id, &key)? {
            return Err(ApiError::Forbidden);
        }
        Ok(key)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/time", get(time))
        .route("/v1/mailboxes", post(create_mailbox))
        .route("/v1/mailboxes/{id}", delete(delete_mailbox))
        .route("/v1/mailboxes/{id}/members", post(add_member))
        .route("/v1/mailboxes/{id}/members/{key}", delete(remove_member))
        .route("/v1/mailboxes/{id}/events", post(post_event).get(get_events))
        .route("/v1/pairing/{token}", post(put_pairing).get(get_pairing))
        .route("/v1/devices/{key}/push", put(put_push))
        .layer(DefaultBodyLimit::max(96 * 1024))
        .with_state(state)
}

async fn time(State(s): State<AppState>) -> Json<proto::SignedTime> {
    let unix_ms = now_ms();
    let mut msg = proto::TIME_DOMAIN.to_vec();
    msg.extend_from_slice(unix_ms.to_string().as_bytes());
    Json(proto::SignedTime {
        unix_ms,
        sig: hex::encode(s.time_key.sign(&msg).to_bytes()),
        key: hex::encode(s.time_key.verifying_key().to_bytes()),
    })
}

async fn create_mailbox(
    State(s): State<AppState>,
    h: HeaderMap,
    m: Method,
    uri: Uri,
    body: Bytes,
) -> Result<Json<proto::MailboxCreated>, ApiError> {
    let key = s.auth(&h, &m, &uri, &body)?;
    let req: proto::CreateMailbox = serde_json::from_slice(&body).map_err(|_| ApiError::BadRequest)?;
    if !proto::pow_ok(&key, req.pow) {
        return Err(ApiError::BadRequest);
    }
    let mut id = [0u8; 16];
    OsRng.fill_bytes(&mut id);
    let id = hex::encode(id);
    db::create_mailbox(&s.conn(), &id, &key, now_ms())?;
    Ok(Json(proto::MailboxCreated { mailbox: id }))
}

async fn delete_mailbox(
    State(s): State<AppState>,
    Path(id): Path<String>,
    h: HeaderMap,
    m: Method,
    uri: Uri,
    body: Bytes,
) -> Result<(), ApiError> {
    s.member(&id, &h, &m, &uri, &body)?;
    db::delete_mailbox(&s.conn(), &id)?;
    Ok(())
}

async fn add_member(
    State(s): State<AppState>,
    Path(id): Path<String>,
    h: HeaderMap,
    m: Method,
    uri: Uri,
    body: Bytes,
) -> Result<(), ApiError> {
    s.member(&id, &h, &m, &uri, &body)?;
    let req: proto::AddMember = serde_json::from_slice(&body).map_err(|_| ApiError::BadRequest)?;
    if proto::parse_key(&req.pubkey).is_none() {
        return Err(ApiError::BadRequest);
    }
    let conn = s.conn();
    if db::member_count(&conn, &id)? >= db::MAX_MEMBERS {
        return Err(ApiError::Conflict);
    }
    db::add_member(&conn, &id, &req.pubkey.to_ascii_lowercase(), now_ms())?;
    Ok(())
}

async fn remove_member(
    State(s): State<AppState>,
    Path((id, key)): Path<(String, String)>,
    h: HeaderMap,
    m: Method,
    uri: Uri,
    body: Bytes,
) -> Result<(), ApiError> {
    s.member(&id, &h, &m, &uri, &body)?;
    db::remove_member(&s.conn(), &id, &key.to_ascii_lowercase())?;
    Ok(())
}

async fn post_event(
    State(s): State<AppState>,
    Path(id): Path<String>,
    h: HeaderMap,
    m: Method,
    uri: Uri,
    body: Bytes,
) -> Result<Json<proto::PostedEvent>, ApiError> {
    let author = s.member(&id, &h, &m, &uri, &body)?;
    let req: proto::PostEvent = serde_json::from_slice(&body).map_err(|_| ApiError::BadRequest)?;
    let blob = B64.decode(&req.blob).map_err(|_| ApiError::BadRequest)?;
    // Un blob age siempre empieza por esta línea; rechazar lo demás evita usar el relay como almacén genérico.
    if blob.len() > proto::MAX_BLOB || !blob.starts_with(AGE_MAGIC) {
        return Err(ApiError::BadRequest);
    }
    let (seq, targets) = {
        let conn = s.conn();
        let seq = db::insert_event(&conn, &id, &blob, now_ms())?.ok_or(ApiError::Full)?;
        (seq, db::other_members_push(&conn, &id, &author)?)
    };
    if let Some(push) = &s.push {
        push.wake(targets, req.urgent); // no bloquea: tarea en segundo plano
    }
    Ok(Json(proto::PostedEvent { seq: seq as u64 }))
}

#[derive(Deserialize)]
struct After {
    after: u64,
}

async fn get_events(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<After>,
    h: HeaderMap,
    m: Method,
    uri: Uri,
    body: Bytes,
) -> Result<Json<proto::EventsPage>, ApiError> {
    s.member(&id, &h, &m, &uri, &body)?;
    let after = i64::try_from(q.after).map_err(|_| ApiError::BadRequest)?;
    let rows = db::events_after(&s.conn(), &id, after)?;
    let next_after = rows.last().map(|(seq, _)| *seq as u64).unwrap_or(q.after);
    let events =
        rows.into_iter().map(|(seq, blob)| proto::StoredBlob { seq: seq as u64, blob: B64.encode(blob) }).collect();
    Ok(Json(proto::EventsPage { events, next_after }))
}

async fn put_pairing(State(s): State<AppState>, Path(token): Path<String>, body: Bytes) -> Result<(), ApiError> {
    if !is_hex(&token, 32) {
        return Err(ApiError::NotFound);
    }
    let req: proto::PairingBlob = serde_json::from_slice(&body).map_err(|_| ApiError::BadRequest)?;
    let blob = B64.decode(&req.blob).map_err(|_| ApiError::BadRequest)?;
    if blob.len() > 4096 || !blob.starts_with(AGE_MAGIC) {
        return Err(ApiError::BadRequest);
    }
    if !db::put_pairing(&s.conn(), &token, &blob, now_ms())? {
        return Err(ApiError::Conflict); // la ranura ya se usó
    }
    Ok(())
}

async fn get_pairing(
    State(s): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<proto::PairingBlob>, ApiError> {
    if !is_hex(&token, 32) {
        return Err(ApiError::NotFound);
    }
    let blob = db::get_pairing(&s.conn(), &token, now_ms())?.ok_or(ApiError::NotFound)?;
    Ok(Json(proto::PairingBlob { blob: B64.encode(blob) }))
}

async fn put_push(
    State(s): State<AppState>,
    Path(key): Path<String>,
    h: HeaderMap,
    m: Method,
    uri: Uri,
    body: Bytes,
) -> Result<(), ApiError> {
    let caller = s.auth(&h, &m, &uri, &body)?;
    if caller != key.to_ascii_lowercase() {
        return Err(ApiError::Forbidden); // cada dispositivo solo registra su propio token
    }
    let req: proto::PushToken = serde_json::from_slice(&body).map_err(|_| ApiError::BadRequest)?;
    let valid_platform = matches!(req.platform.as_str(), "apns" | "apns-sandbox" | "fcm" | "unifiedpush");
    if !valid_platform || req.token.is_empty() || req.token.len() > 512 {
        return Err(ApiError::BadRequest);
    }
    db::put_push(&s.conn(), &caller, &req.platform, &req.token, now_ms())?;
    Ok(())
}
