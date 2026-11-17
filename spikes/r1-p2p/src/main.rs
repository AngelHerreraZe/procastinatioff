// Spike R1 (throwaway): can iroh connect two endpoints by key, with direct addresses only,
// no relay and no third-party address lookup? Measures connect time; checks that an
// unknown key is closed before it can read anything.
use std::time::Instant;

use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey};

const ALPN: &[u8] = b"procastinatioff/sync/1";

async fn ep(seed: u8) -> Endpoint {
    Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::from_bytes(&[seed; 32]))
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .clear_address_lookup()
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap()
}

#[tokio::main]
async fn main() {
    let (pc, phone, stranger) = (ep(1).await, ep(2).await, ep(3).await);
    let allowed = phone.id();
    let pc_addr = EndpointAddr::new(pc.id()).with_ip_addr(pc.bound_sockets()[0]);
    println!("pc listens on {:?}", pc.bound_sockets());

    let server = pc.clone();
    let srv = tokio::spawn(async move {
        for _ in 0..2 {
            let conn = server.accept().await.unwrap().await.unwrap();
            if conn.remote_id() != allowed {
                println!("server: unknown key {} -> closed without reading", conn.remote_id().fmt_short());
                conn.close(1u32.into(), b"");
                continue;
            }
            let (mut send, mut recv) = conn.accept_bi().await.unwrap();
            let got = recv.read_to_end(1024).await.unwrap();
            send.write_all(&got).await.unwrap();
            send.finish().unwrap();
            conn.closed().await;
        }
    });

    let t = Instant::now();
    let conn = phone.connect(pc_addr.clone(), ALPN).await.unwrap();
    println!("phone connected in {:?}", t.elapsed());
    let (mut send, mut recv) = conn.open_bi().await.unwrap();
    send.write_all(b"hello").await.unwrap();
    send.finish().unwrap();
    let echo = recv.read_to_end(1024).await.unwrap();
    println!("echo: {} in {:?}", String::from_utf8_lossy(&echo), t.elapsed());
    conn.close(0u32.into(), b"");

    let conn = stranger.connect(pc_addr, ALPN).await.unwrap();
    let r = match conn.open_bi().await {
        Ok((_, mut recv)) => recv.read_to_end(1024).await.map(|v| v.len()).map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    println!("stranger read: {r:?}");
    srv.await.unwrap();
}
