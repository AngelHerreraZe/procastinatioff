// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! UnifiedPush con protección SSRF 🔒: solo https, solo IP públicas, sin redirecciones
//! y con la IP fijada tras comprobarla (evita el cambio de DNS entre comprobar y conectar).

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_documentation()
                || (o[0] == 100 && (64..128).contains(&o[1])) // CGNAT
                || o[0] == 0)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // ULA
                || (s[0] & 0xffc0) == 0xfe80) // link-local
        }
    }
}

pub async fn send(endpoint: &str) -> Result<(), String> {
    let url = url::Url::parse(endpoint).map_err(|_| "URL inválida")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("solo https sin credenciales".into());
    }
    let host = url.host_str().ok_or("sin host")?.to_owned();
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port)).await.map_err(|_| "DNS")?.collect();
    let addr = *addrs.first().ok_or("sin direcciones")?;
    if !addrs.iter().all(|a| is_public(a.ip())) {
        return Err("destino no público".into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&host, addr) // conecta exactamente a la IP comprobada
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.post(url).body("1").send().await.map_err(|e| e.without_url().to_string())?;
    if resp.status().is_success() { Ok(()) } else { Err(format!("UnifiedPush {}", resp.status())) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_addresses_are_rejected() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip} debería rechazarse");
        }
        assert!(is_public("1.1.1.1".parse().unwrap()));
    }

    #[tokio::test]
    async fn loopback_and_metadata_endpoints_are_refused() {
        assert!(send("https://127.0.0.1/push").await.is_err());
        assert!(send("https://169.254.169.254/latest").await.is_err());
        assert!(send("http://example.com/push").await.is_err());
    }
}
