// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Protocolo entre dispositivos y relay. Sin red, sin estado.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Cabeceras HTTP de autenticación ──────────────────────────────
pub const H_KEY: &str = "x-pco-key"; // clave pública Ed25519 (hex)
pub const H_TS: &str = "x-pco-ts"; // segundos UNIX
pub const H_NONCE: &str = "x-pco-nonce"; // 16 bytes aleatorios (hex)
pub const H_SIG: &str = "x-pco-sig"; // firma (hex)

pub const MAX_SKEW_S: i64 = 60; // tolerancia de reloj
pub const MAX_BLOB: usize = 70 * 1024; // 64 KB de relleno + cabecera age
pub const POW_BITS: u32 = 20; // ≈1 millón de hashes para crear un buzón

const REQ_DOMAIN: &[u8] = b"procastinacioff/request/v1\n";
const POW_DOMAIN: &[u8] = b"procastinacioff/pow/v1\n";
pub const TIME_DOMAIN: &[u8] = b"procastinacioff/time/v1\n";

/// Mensaje exacto que se firma en cada petición. Cambiarlo rompe la compatibilidad.
pub fn request_message(method: &str, path_and_query: &str, body: &[u8], ts: i64, nonce: &str) -> Vec<u8> {
    let mut m = Vec::with_capacity(160);
    m.extend_from_slice(REQ_DOMAIN);
    for part in [method, path_and_query, &hex::encode(Sha256::digest(body)), &ts.to_string(), nonce] {
        m.extend_from_slice(part.as_bytes());
        m.push(b'\n');
    }
    m
}

#[derive(Debug, Clone)]
pub struct AuthHeaders {
    pub key: String,
    pub ts: i64,
    pub nonce: String,
    pub sig: String,
}

pub fn sign_request(key: &SigningKey, method: &str, path: &str, body: &[u8], ts: i64, nonce: String) -> AuthHeaders {
    let sig = key.sign(&request_message(method, path, body, ts, &nonce));
    AuthHeaders { key: hex::encode(key.verifying_key().to_bytes()), ts, nonce, sig: hex::encode(sig.to_bytes()) }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    #[error("cabecera ausente o mal formada")]
    Malformed,
    #[error("reloj fuera de tolerancia")]
    Skew,
    #[error("firma inválida")]
    BadSignature,
}

/// Verifica firma y hora. El control de nonces repetidos lo hace el relay (tiene estado).
pub fn verify_request(
    h: &AuthHeaders,
    method: &str,
    path: &str,
    body: &[u8],
    now_s: i64,
) -> Result<VerifyingKey, AuthError> {
    if (now_s - h.ts).abs() > MAX_SKEW_S {
        return Err(AuthError::Skew);
    }
    if h.nonce.len() != 32 || hex::decode(&h.nonce).is_err() {
        return Err(AuthError::Malformed);
    }
    let key = parse_key(&h.key).ok_or(AuthError::Malformed)?;
    let sig: [u8; 64] = hex::decode(&h.sig).ok().and_then(|v| v.try_into().ok()).ok_or(AuthError::Malformed)?;
    key.verify_strict(&request_message(method, path, body, h.ts, &h.nonce), &Signature::from_bytes(&sig))
        .map_err(|_| AuthError::BadSignature)?;
    Ok(key)
}

pub fn parse_key(hex_key: &str) -> Option<VerifyingKey> {
    let bytes: [u8; 32] = hex::decode(hex_key).ok()?.try_into().ok()?;
    VerifyingKey::from_bytes(&bytes).ok()
}

// ── Prueba de trabajo para crear buzones (anti-spam sin cuentas) ─────
pub fn pow_ok(pubkey_hex: &str, nonce: u64) -> bool {
    let digest = Sha256::new()
        .chain_update(POW_DOMAIN)
        .chain_update(pubkey_hex.as_bytes())
        .chain_update(nonce.to_le_bytes())
        .finalize();
    let mut zeros = 0;
    for byte in digest {
        if byte == 0 {
            zeros += 8;
        } else {
            zeros += byte.leading_zeros();
            break;
        }
    }
    zeros >= POW_BITS
}

pub fn pow_solve(pubkey_hex: &str) -> u64 {
    (0u64..).find(|n| pow_ok(pubkey_hex, *n)).expect("siempre existe solución")
}

// ── Mensajes de la API (JSON) ───────────────────────────────────────
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMailbox {
    pub pow: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MailboxCreated {
    pub mailbox: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddMember {
    pub pubkey: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostEvent {
    /// Mensaje age en base64.
    pub blob: String,
    /// Si es true, el relay pide un aviso visible (genérico) a los demás dispositivos.
    pub urgent: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostedEvent {
    pub seq: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredBlob {
    pub seq: u64,
    pub blob: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventsPage {
    pub events: Vec<StoredBlob>,
    pub next_after: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushToken {
    /// "apns" | "apns-sandbox" | "fcm" | "unifiedpush"
    pub platform: String,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PairingBlob {
    pub blob: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedTime {
    pub unix_ms: i64,
    pub sig: String,
    /// Clave pública de hora (hex). Los clientes la fijan la primera vez (TOFU, ADR 0007).
    pub key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_request_roundtrip_and_tamper() {
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let nonce = hex::encode([7u8; 16]);
        let h = sign_request(&key, "POST", "/v1/mailboxes/abc/events", b"{}", 1_000, nonce);
        assert!(verify_request(&h, "POST", "/v1/mailboxes/abc/events", b"{}", 1_010).is_ok());
        // Cualquier cambio invalida la firma
        assert_eq!(verify_request(&h, "POST", "/v1/mailboxes/xyz/events", b"{}", 1_010), Err(AuthError::BadSignature));
        assert_eq!(verify_request(&h, "POST", "/v1/mailboxes/abc/events", b"{ }", 1_010), Err(AuthError::BadSignature));
        assert_eq!(verify_request(&h, "POST", "/v1/mailboxes/abc/events", b"{}", 1_100), Err(AuthError::Skew));
    }

    #[test]
    fn pow_solution_verifies() {
        let pk = "ab".repeat(32);
        let n = pow_solve(&pk);
        assert!(pow_ok(&pk, n));
    }
}
