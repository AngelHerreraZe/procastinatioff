// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ProcastinaciOFF core: day rules, signed events, crypto,
//! encrypted storage and sync. No platform dependencies.

uniffi::setup_scaffolding!();

pub mod api;
pub mod clock;
pub mod crypto;
pub mod desktop;
pub mod error;
pub mod events;
pub mod ledger;
pub mod model;
pub mod netpolicy;
pub mod pairing;
pub mod rules;
pub mod storage;
pub mod strictness;
pub mod sync;
pub mod time;
