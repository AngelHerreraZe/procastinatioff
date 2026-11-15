// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A single error type that crosses into Kotlin/Swift. Messages **never** include personal data.
//!
//! The core never returns user-facing text: `CoreError::code()` is a stable key that each UI
//! translates with its local resources. The English `Display` text is for logs and developers.

use crate::rules::Denied;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    /// `reason` is a stable code (`Denied::code`); UIs show the `denied.<reason>` translation.
    #[error("denied: {reason}")]
    Denied { reason: String, cooldown_minutes: u16 },
    #[error("storage: {msg}")]
    Storage { msg: String },
    #[error("crypto: {msg}")]
    Crypto { msg: String },
    #[error("network: {msg}")]
    Network { msg: String },
    #[error("protocol: {msg}")]
    Protocol { msg: String },
    #[error("system keystore: {msg}")]
    Secret { msg: String },
}

impl CoreError {
    /// Translation key for the UIs: `denied.<code>` or `error.<kind>`.
    pub fn code(&self) -> String {
        match self {
            Self::Denied { reason, .. } => format!("denied.{reason}"),
            Self::Storage { .. } => "error.storage".into(),
            Self::Crypto { .. } => "error.crypto".into(),
            Self::Network { .. } => "error.network".into(),
            Self::Protocol { .. } => "error.protocol".into(),
            Self::Secret { .. } => "error.secret".into(),
        }
    }

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
        // The UI receives the code, never a sentence: every language translates it locally.
        Self::Denied { reason: d.code().into(), cooldown_minutes }
    }
}

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::storage(e)
    }
}

/// Required for traits implemented in Kotlin/Swift: if foreign code throws an unexpected
/// exception, UniFFI turns it into this error.
impl From<uniffi::UnexpectedUniFFICallbackError> for CoreError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Secret { msg: e.reason }
    }
}
