// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Eventos firmados y sellados 🔒.
//!
//! EventBody ──firma Ed25519──► SignedEvent ──JSON + relleno──► bytes ──age (N dispositivos)──► blob

use std::io::{Read, Write};

use ed25519_dalek::{Signature, Signer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::crypto::DeviceKeys;
use crate::error::CoreError;
use crate::model::EventKind;

/// Separación de dominio: una firma de evento nunca vale como firma de petición HTTP.
const EVENT_DOMAIN: &[u8] = b"procastinacioff/event/v1\0";
/// Relleno: el tamaño del blob no delata el tipo de evento.
const BUCKETS: [usize; 4] = [1024, 4096, 16 * 1024, 64 * 1024];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBody {
    pub author: String,
    pub seq: u64,
    pub prev: Option<String>,
    pub at_ms: i64,
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEvent {
    pub body: EventBody,
    pub sig: String,
}

fn signing_bytes(body: &EventBody) -> Vec<u8> {
    let mut v = EVENT_DOMAIN.to_vec();
    // EventBody solo contiene tipos que siempre serializan (sin mapas con claves no textuales).
    v.extend(serde_json::to_vec(body).unwrap_or_default());
    v
}

impl SignedEvent {
    pub fn sign(keys: &DeviceKeys, body: EventBody) -> Self {
        debug_assert_eq!(body.author, keys.device_id(), "el autor debe ser este dispositivo");
        let sig = keys.signing.sign(&signing_bytes(&body));
        Self { body, sig: hex::encode(sig.to_bytes()) }
    }

    /// Huella única del evento (SHA-256 del cuerpo firmado, en hex).
    pub fn id(&self) -> String {
        hex::encode(Sha256::digest(signing_bytes(&self.body)))
    }

    pub fn verify(&self) -> Result<(), CoreError> {
        let key =
            procastinacioff_proto::parse_key(&self.body.author).ok_or_else(|| CoreError::protocol("autor inválido"))?;
        let sig: [u8; 64] = hex::decode(&self.sig)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or_else(|| CoreError::protocol("firma mal formada"))?;
        key.verify_strict(&signing_bytes(&self.body), &Signature::from_bytes(&sig))
            .map_err(|_| CoreError::protocol("firma de evento inválida"))
    }
}

/// Serializa, rellena y cifra un evento para todos los destinatarios.
pub fn seal(ev: &SignedEvent, recipients: &[age::x25519::Recipient]) -> Result<Vec<u8>, CoreError> {
    if recipients.is_empty() {
        return Err(CoreError::crypto("sin destinatarios"));
    }
    let json = Zeroizing::new(serde_json::to_vec(ev).map_err(CoreError::crypto)?);
    let needed = json.len() + 4;
    let bucket =
        BUCKETS.iter().copied().find(|b| *b >= needed).ok_or_else(|| CoreError::protocol("evento demasiado grande"))?;

    let mut plain = Zeroizing::new(Vec::with_capacity(bucket));
    plain.extend_from_slice(&(json.len() as u32).to_le_bytes());
    plain.extend_from_slice(&json);
    plain.resize(bucket, 0);

    let encryptor = age::Encryptor::with_recipients(recipients.iter().map(|r| r as &dyn age::Recipient))
        .map_err(CoreError::crypto)?;
    let mut out = Vec::with_capacity(bucket + 512);
    let mut writer = encryptor.wrap_output(&mut out).map_err(CoreError::crypto)?;
    writer.write_all(&plain).map_err(CoreError::crypto)?;
    writer.finish().map_err(CoreError::crypto)?;
    Ok(out)
}

/// Descifra y deserializa un blob. NO verifica la firma: eso lo hace el Ledger.
pub fn open(blob: &[u8], identity: &age::x25519::Identity) -> Result<SignedEvent, CoreError> {
    if blob.len() > procastinacioff_proto::MAX_BLOB {
        return Err(CoreError::protocol("blob demasiado grande"));
    }
    let decryptor = age::Decryptor::new(blob).map_err(CoreError::crypto)?;
    let mut reader = decryptor.decrypt(std::iter::once(identity as &dyn age::Identity)).map_err(CoreError::crypto)?;
    let mut plain = Zeroizing::new(Vec::new());
    reader.by_ref().take(64 * 1024 + 1).read_to_end(&mut plain).map_err(CoreError::crypto)?;
    if plain.len() < 4 {
        return Err(CoreError::protocol("blob vacío"));
    }
    let len = u32::from_le_bytes([plain[0], plain[1], plain[2], plain[3]]) as usize;
    let body = plain.get(4..4usize.saturating_add(len)).ok_or_else(|| CoreError::protocol("longitud inválida"))?;
    serde_json::from_slice(body).map_err(CoreError::protocol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::testing::MemStore;

    fn keys() -> DeviceKeys {
        DeviceKeys::load_or_create(&MemStore::default()).unwrap()
    }

    fn body(k: &DeviceKeys, seq: u64) -> EventBody {
        EventBody { author: k.device_id(), seq, prev: None, at_ms: 1, kind: EventKind::Heartbeat }
    }

    #[test]
    fn sign_verify_and_detect_tampering() {
        let k = keys();
        let mut ev = SignedEvent::sign(&k, body(&k, 1));
        assert!(ev.verify().is_ok());
        ev.body.at_ms = 2; // alguien cambia la hora
        assert!(ev.verify().is_err());
    }

    #[test]
    fn seal_open_roundtrip_and_padding_hides_size() {
        let (a, b) = (keys(), keys());
        let ev = SignedEvent::sign(&a, body(&a, 1));
        let recipients = vec![a.age.to_public(), b.age.to_public()];
        let blob = seal(&ev, &recipients).unwrap();
        assert_eq!(open(&blob, &b.age).unwrap(), ev);
        // Un evento distinto pero pequeño cae en la misma franja de tamaño (cubo de 1 KB + cabecera age).
        // age añade "grease" aleatorio a la cabecera: el tamaño exacto varía, pero no depende del evento.
        let ev2 = SignedEvent::sign(&a, EventBody { kind: EventKind::EmergencyStarted, ..body(&a, 2) });
        for len in [blob.len(), seal(&ev2, &recipients).unwrap().len()] {
            assert!((1024..2048).contains(&len), "tamaño fuera de la franja del cubo de 1 KB: {len}");
        }
    }

    #[test]
    fn outsider_cannot_open() {
        let (a, outsider) = (keys(), keys());
        let blob = seal(&SignedEvent::sign(&a, body(&a, 1)), &[a.age.to_public()]).unwrap();
        assert!(open(&blob, &outsider.age).is_err());
    }

    #[test]
    fn keys_survive_reload() {
        let store = MemStore::default();
        let first = DeviceKeys::load_or_create(&store).unwrap();
        let again = DeviceKeys::load_or_create(&store).unwrap();
        assert_eq!(first.device_id(), again.device_id());
        assert_eq!(first.age_recipient(), again.age_recipient());
    }
}
