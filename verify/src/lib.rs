// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Lógica de verificación 🔒. Recibe NÚMEROS (puntos de pose, etiquetas, texto OCR en memoria)
//! y devuelve un resultado de tamaño fijo. Nunca ve una imagen ni toca disco o red.

uniffi::setup_scaffolding!();

pub mod food;
pub mod liveness;
pub mod pages;
pub mod phash;
pub mod pose;
pub mod reading;
pub mod result;
pub mod summary;
