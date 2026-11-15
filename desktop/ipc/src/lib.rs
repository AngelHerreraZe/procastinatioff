// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Protocolo UI ↔ servicio de Windows: una línea JSON por petición y una por respuesta.
//! Los tipos y el manejador viven en el núcleo (`core/src/desktop.rs`), compartidos con macOS.

pub use procastinacioff_core::desktop::{AgentStatus, MAX_LINE, Request, Response};
pub use procastinacioff_core::model::Method;

pub const PIPE_NAME: &str = r"\.\pipe\procastinacioff";
