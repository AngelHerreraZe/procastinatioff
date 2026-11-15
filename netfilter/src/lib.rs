// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Servidor DNS local (127.0.0.1:53 y [::1]:53) que aplica la política del núcleo 🔒.
//! Solo mueve paquetes: QUÉ se responde lo decide `Core::dns_step` (core/src/netpolicy.rs).
//! 🔒 No registra nombres consultados (ni siquiera en los logs de depuración).
//!
//! ponytail: solo UDP; añadir TCP/53 si aparecen respuestas truncadas que fallan.

use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use procastinacioff_core::api::{Core, DnsAction};
use procastinacioff_core::clock::Clock;
use tokio::net::UdpSocket;

pub type ClockFn = Arc<dyn Fn() -> Clock + Send + Sync>;

/// Pregunta a los DNS reales (los de la red, guardados antes de apuntar el equipo a nosotros), en orden.
async fn ask_upstream(packet: &[u8], upstreams: &[SocketAddr]) -> Option<Vec<u8>> {
    for up in upstreams {
        let bind = if up.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
        let Ok(sock) = UdpSocket::bind(bind).await else { continue };
        if sock.send_to(packet, up).await.is_err() {
            continue;
        }
        let mut buf = vec![0u8; 4096];
        // `connect` no hace falta: `dns_finish` descarta respuestas cuyo id no coincide.
        if let Ok(Ok((n, from))) = tokio::time::timeout(Duration::from_secs(3), sock.recv_from(&mut buf)).await
            && from == *up
        {
            buf.truncate(n);
            return Some(buf);
        }
    }
    None
}

/// Atiende consultas en `sock` hasta que falle. Cada consulta va en su propia tarea.
pub async fn serve(
    sock: UdpSocket,
    core: Arc<Core>,
    upstreams: Vec<SocketAddr>,
    clock: ClockFn,
) -> std::io::Result<()> {
    let sock = Arc::new(sock);
    let upstreams: Arc<[SocketAddr]> = upstreams.into();
    let mut buf = vec![0u8; 4096];
    loop {
        let (n, peer) = match sock.recv_from(&mut buf).await {
            Ok(x) => x,
            // Windows entrega como error en el socket UDP el "puerto inalcanzable" de un envío anterior
            // (WSAECONNRESET) y los datagramas que no caben (WSAEMSGSIZE): se ignoran.
            Err(e) if e.kind() == ErrorKind::ConnectionReset || e.raw_os_error() == Some(10040) => continue,
            Err(e) => return Err(e),
        };
        let query = buf[..n].to_vec();
        let (sock, core, upstreams, clock) = (sock.clone(), core.clone(), upstreams.clone(), clock.clone());
        tokio::spawn(async move {
            let reply = match core.dns_step(clock(), query.clone()) {
                DnsAction::Reply { packet } => Some(packet),
                DnsAction::Upstream { packet, rewritten } => match ask_upstream(&packet, &upstreams).await {
                    Some(r) => core.dns_finish(query, r, rewritten),
                    None => None,
                },
                DnsAction::Drop => None,
            };
            if let Some(r) = reply {
                let _ = sock.send_to(&r, peer).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procastinacioff_core::crypto::SecretStore;
    use procastinacioff_core::error::CoreError;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Mem(Mutex<BTreeMap<String, Vec<u8>>>);
    impl SecretStore for Mem {
        fn load(&self, n: String) -> Result<Option<Vec<u8>>, CoreError> {
            Ok(self.0.lock().unwrap().get(&n).cloned())
        }
        fn save(&self, n: String, v: Vec<u8>) -> Result<(), CoreError> {
            self.0.lock().unwrap().insert(n, v);
            Ok(())
        }
    }

    /// Consulta A mínima para "example.org" con id 0xABCD.
    fn query() -> Vec<u8> {
        let mut q = vec![0xAB, 0xCD, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in ["example", "org"] {
            q.push(label.len() as u8);
            q.extend(label.as_bytes());
        }
        q.extend([0, 0, 1, 0, 1]);
        q
    }

    /// Sin perfil de menor el filtro no actúa: la consulta llega al DNS real y vuelve su respuesta.
    #[tokio::test]
    async fn forwards_to_upstream_and_returns_the_answer() {
        // DNS "real" de pruebas: responde con la misma consulta marcada como respuesta.
        let upstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let up_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            let mut b = [0u8; 512];
            let (n, from) = upstream.recv_from(&mut b).await.unwrap();
            b[2] |= 0x80; // QR = respuesta
            upstream.send_to(&b[..n], from).await.unwrap();
        });

        let dir = std::env::temp_dir().join(format!("pco-netfilter-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let core = Core::open(dir.join("db").to_string_lossy().into(), Arc::new(Mem::default())).unwrap();
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let clock: ClockFn = Arc::new(|| Clock { now_ms: 1_773_046_800_000, utc_offset_min: 0 });
        tokio::spawn(serve(server, core, vec![up_addr], clock));

        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.send_to(&query(), addr).await.unwrap();
        let mut b = [0u8; 512];
        let (n, _) = tokio::time::timeout(Duration::from_secs(5), client.recv_from(&mut b)).await.unwrap().unwrap();
        assert_eq!(&b[..2], &[0xAB, 0xCD], "mismo id");
        assert_ne!(b[2] & 0x80, 0, "es una respuesta");
        assert!(n >= query().len());
    }
}
