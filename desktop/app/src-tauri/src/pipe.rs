// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Client of the service pipe.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::time::Duration;

use procastinacioff_ipc::{MAX_LINE, PIPE_NAME, Request, Response};

/// One connection per request. ponytail: simple and enough at 1–2 requests per second.
pub fn call(req: &Request) -> Result<Response, String> {
    let mut last = String::new();
    for _ in 0..5 {
        match OpenOptions::new().read(true).write(true).open(PIPE_NAME) {
            Ok(mut pipe) => {
                let mut line = serde_json::to_string(req).map_err(|e| e.to_string())?;
                line.push('\n');
                pipe.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                let mut resp = String::new();
                BufReader::new(pipe.take(MAX_LINE as u64 * 4)).read_line(&mut resp).map_err(|e| e.to_string())?;
                return serde_json::from_str(&resp).map_err(|e| e.to_string());
            }
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(100)); // pipe busy: retry
            }
        }
    }
    Err(format!("el servicio no responde: {last}"))
}
