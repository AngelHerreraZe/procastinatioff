// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Verification logic (security-critical). Receives NUMBERS (pose points, labels, in-memory OCR text)
//! and returns a fixed-size result. Never sees an image nor touches disk or network.

uniffi::setup_scaffolding!();

pub mod food;
pub mod liveness;
pub mod pages;
pub mod phash;
pub mod pose;
pub mod reading;
pub mod result;
pub mod summary;
