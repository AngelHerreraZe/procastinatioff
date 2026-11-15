// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Relay ciego: guarda y reenvía mensajes cifrados que no puede leer. Sin cuentas,
//! sin lógica de negocio y con logs sin datos personales.

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod push;
pub mod routes;
