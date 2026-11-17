// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Core + verifier in ONE static library for the Apple apps. Two Rust `staticlib`s in
//! the same binary would duplicate the standard library and the UniFFI symbols. The Swift bindings
//! are generated from each crate separately (`scripts/build_apple.sh`); here they are only linked.

pub use procastinatioff_core;
pub use procastinatioff_verify;
