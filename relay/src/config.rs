// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Todo por variables de entorno. Nada de secretos en archivos del repositorio.

use std::env;

pub struct Config {
    pub bind: String,
    pub db_path: String,
    /// Archivo con 32 bytes (semilla Ed25519) para firmar la hora.
    pub time_key_path: String,
    /// URL del push gateway (vacía = sin push, útil en desarrollo y self-hosting básico).
    pub push_url: Option<String>,
    /// Secreto compartido relay→gateway (cabecera Authorization: Bearer).
    pub push_secret: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bind: env::var("PCO_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()),
            db_path: env::var("PCO_DB").unwrap_or_else(|_| "relay.db".into()),
            time_key_path: env::var("PCO_TIME_KEY").unwrap_or_else(|_| "time.key".into()),
            push_url: env::var("PCO_PUSH_URL").ok().filter(|s| !s.is_empty()),
            push_secret: env::var("PCO_PUSH_SECRET").ok().filter(|s| !s.is_empty()),
        }
    }
}
