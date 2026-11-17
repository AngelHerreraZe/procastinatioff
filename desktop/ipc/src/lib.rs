// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! UI ↔ service protocol (Windows pipe, Linux socket): one JSON line per request and one per response.
//! The types and the handler live in the core (`core/src/desktop.rs`), shared with macOS.

pub use procastinatioff_core::desktop::{AgentStatus, MAX_LINE, Request, Response};
pub use procastinatioff_core::model::Method;
/// Pure functions the UI calls without the service: goal check suggestions and place QR codes.
pub use procastinatioff_core::goals;

pub const PIPE_NAME: &str = r"\.\pipe\procastinatioff";
/// Linux: created by the service in its systemd `RuntimeDirectory`.
pub const SOCKET_PATH: &str = "/run/procastinatioff/agent.sock";
