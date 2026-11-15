// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Un solo tipo de error que cruza a Kotlin/Swift. Los mensajes **no** incluyen datos personales.

use crate::rules::Denied;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{reason}")]
    Denied { reason: String, cooldown_minutes: u16 },
    #[error("almacenamiento: {msg}")]
    Storage { msg: String },
    #[error("criptografía: {msg}")]
    Crypto { msg: String },
    #[error("red: {msg}")]
    Network { msg: String },
    #[error("protocolo: {msg}")]
    Protocol { msg: String },
    #[error("llavero del sistema: {msg}")]
    Secret { msg: String },
}

impl CoreError {
    pub fn crypto(e: impl std::fmt::Display) -> Self {
        Self::Crypto { msg: e.to_string() }
    }
    pub fn storage(e: impl std::fmt::Display) -> Self {
        Self::Storage { msg: e.to_string() }
    }
    pub fn network(e: impl std::fmt::Display) -> Self {
        Self::Network { msg: e.to_string() }
    }
    pub fn protocol(e: impl std::fmt::Display) -> Self {
        Self::Protocol { msg: e.to_string() }
    }
}

impl From<Denied> for CoreError {
    fn from(d: Denied) -> Self {
        let cooldown_minutes = if let Denied::Cooldown { minutes } = d { minutes } else { 0 };
        // El texto del motivo (en español, sin datos personales) llega tal cual a la UI.
        Self::Denied { reason: d.to_string(), cooldown_minutes }
    }
}

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::storage(e)
    }
}

/// Obligatorio para traits implementados en Kotlin/Swift: si el código extranjero
/// lanza una excepción inesperada, UniFFI la convierte en este error.
impl From<uniffi::UnexpectedUniFFICallbackError> for CoreError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Secret { msg: e.reason }
    }
}
