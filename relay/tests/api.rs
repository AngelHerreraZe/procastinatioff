// Pruebas del router en memoria, sin abrir puertos. Cubren la puerta de seguridad del relay:
// extraño 403, repetición 401, blob no-age 400, ranura de vinculación de un solo uso 409.
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use ed25519_dalek::SigningKey;
use http_body_util::BodyExt;
use procastinacioff_proto as proto;
use procastinacioff_relay::{db, routes};
use tower::ServiceExt;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn app() -> axum::Router {
    let state = routes::AppState {
        db: Arc::new(Mutex::new(db::open(":memory:").unwrap())),
        replay: Arc::default(),
        limit: Arc::default(),
        time_key: Arc::new(SigningKey::generate(&mut rand::rngs::OsRng)),
        push: None,
    };
    routes::router(state)
}

fn signed(key: &SigningKey, method: &str, path: &str, body: Vec<u8>) -> Request<Body> {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let nonce = hex::encode(rand::random::<[u8; 16]>());
    let h = proto::sign_request(key, method, path, &body, ts, nonce);
    Request::builder()
        .method(method)
        .uri(path)
        .header(proto::H_KEY, h.key)
        .header(proto::H_TS, h.ts.to_string())
        .header(proto::H_NONCE, h.nonce)
        .header(proto::H_SIG, h.sig)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

async fn json<T: serde::de::DeserializeOwned>(resp: axum::response::Response) -> T {
    serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn mailbox(app: &axum::Router, owner: &SigningKey) -> String {
    let pow = proto::pow_solve(&hex::encode(owner.verifying_key().to_bytes()));
    let body = serde_json::to_vec(&proto::CreateMailbox { pow }).unwrap();
    let r = app.clone().oneshot(signed(owner, "POST", "/v1/mailboxes", body)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    json::<proto::MailboxCreated>(r).await.mailbox
}

fn event_body(blob: &[u8]) -> Vec<u8> {
    serde_json::to_vec(&proto::PostEvent { blob: B64.encode(blob), urgent: false }).unwrap()
}

#[tokio::test]
async fn full_flow_and_outsider_forbidden() {
    let app = app();
    let owner = SigningKey::generate(&mut rand::rngs::OsRng);
    let outsider = SigningKey::generate(&mut rand::rngs::OsRng);
    let mb = mailbox(&app, &owner).await;

    let body = event_body(b"age-encryption.org/v1\n-> X25519 fake\n");
    let path = format!("/v1/mailboxes/{mb}/events");
    let r = app.clone().oneshot(signed(&owner, "POST", &path, body.clone())).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);

    // Un extraño no puede escribir ni leer
    let r = app.clone().oneshot(signed(&outsider, "POST", &path, body)).await.unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    let get = format!("{path}?after=0");
    let r = app.clone().oneshot(signed(&outsider, "GET", &get, vec![])).await.unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);

    let r = app.clone().oneshot(signed(&owner, "GET", &get, vec![])).await.unwrap();
    let page: proto::EventsPage = json(r).await;
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.next_after, 1);
}

#[tokio::test]
async fn replayed_request_is_rejected() {
    let app = app();
    let k = SigningKey::generate(&mut rand::rngs::OsRng);
    let pow = proto::pow_solve(&hex::encode(k.verifying_key().to_bytes()));
    let req = signed(&k, "POST", "/v1/mailboxes", serde_json::to_vec(&proto::CreateMailbox { pow }).unwrap());
    let (parts, body) = req.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let first = Request::from_parts(parts.clone(), Body::from(bytes.clone()));
    let replay = Request::from_parts(parts, Body::from(bytes));
    assert_eq!(app.clone().oneshot(first).await.unwrap().status(), StatusCode::OK);
    assert_eq!(app.clone().oneshot(replay).await.unwrap().status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unsigned_and_non_age_requests_are_refused() {
    let app = app();
    let owner = SigningKey::generate(&mut rand::rngs::OsRng);
    let unsigned = Request::builder().method("POST").uri("/v1/mailboxes").body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(unsigned).await.unwrap().status(), StatusCode::UNAUTHORIZED);

    let mb = mailbox(&app, &owner).await;
    let path = format!("/v1/mailboxes/{mb}/events");
    let r = app.clone().oneshot(signed(&owner, "POST", &path, event_body(b"texto plano"))).await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn pairing_slot_is_single_use() {
    let app = app();
    let token = "ab".repeat(16);
    let body = serde_json::to_vec(&proto::PairingBlob { blob: B64.encode(b"age-encryption.org/v1\nx") }).unwrap();
    let put =
        |b: Vec<u8>| Request::builder().method("POST").uri(format!("/v1/pairing/{token}")).body(Body::from(b)).unwrap();
    assert_eq!(app.clone().oneshot(put(body.clone())).await.unwrap().status(), StatusCode::OK);
    assert_eq!(app.clone().oneshot(put(body)).await.unwrap().status(), StatusCode::CONFLICT);
    let get = Request::builder().uri(format!("/v1/pairing/{token}")).body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(get).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn devices_only_register_their_own_push_token() {
    let app = app();
    let (me, other) = (SigningKey::generate(&mut rand::rngs::OsRng), SigningKey::generate(&mut rand::rngs::OsRng));
    let body = serde_json::to_vec(&proto::PushToken { platform: "fcm".into(), token: "t".into() }).unwrap();
    let path = format!("/v1/devices/{}/push", hex::encode(other.verifying_key().to_bytes()));
    let r = app.clone().oneshot(signed(&me, "PUT", &path, body)).await.unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}
