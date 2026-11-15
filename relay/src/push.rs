// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cliente del push gateway: "despierta a estos dispositivos". Nunca registra tokens.

use serde::Serialize;

pub struct Push {
    client: reqwest::Client,
    url: String,
    secret: String,
}

#[derive(Serialize)]
struct Target {
    platform: String,
    token: String,
    urgent: bool,
}

#[derive(Serialize)]
struct WakeRequest {
    targets: Vec<Target>,
}

impl Push {
    pub fn new(url: String, secret: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, url, secret }
    }

    /// Pide al gateway que despierte a los dispositivos. No se espera la respuesta.
    pub fn wake(&self, targets: Vec<(String, String)>, urgent: bool) {
        if targets.is_empty() {
            return;
        }
        let req = WakeRequest {
            targets: targets.into_iter().map(|(platform, token)| Target { platform, token, urgent }).collect(),
        };
        let (client, url, secret) = (self.client.clone(), format!("{}/v1/wake", self.url), self.secret.clone());
        tokio::spawn(async move {
            if let Err(e) = client.post(url).bearer_auth(secret).json(&req).send().await {
                tracing::warn!(error = %e.without_url(), "push gateway no disponible"); // sin tokens en el log
            }
        });
    }
}
