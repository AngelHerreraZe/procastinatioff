// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Núcleo + verificador en UNA librería estática para las apps de Apple. Dos `staticlib` de Rust en
//! el mismo binario duplicarían la biblioteca estándar y los símbolos de UniFFI. Los bindings Swift
//! se generan desde cada crate por separado (`scripts/build_apple.sh`); aquí solo se enlazan.

pub use procastinacioff_core;
pub use procastinacioff_verify;
