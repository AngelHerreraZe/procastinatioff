// Spike R1: which direct addresses does an endpoint report when bound on 0.0.0.0, with no relay?
use iroh::endpoint::presets;
use iroh::{Endpoint, RelayMode, SecretKey};
#[tokio::main]
async fn main() {
    let ep = Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::from_bytes(&[1; 32]))
        .alpns(vec![b"x".to_vec()])
        .relay_mode(RelayMode::Disabled)
        .clear_address_lookup()
        .bind_addr("0.0.0.0:47800").unwrap()
        .bind().await.unwrap();
    println!("bound: {:?}", ep.bound_sockets());
    for _ in 0..3 {
        let t = std::time::Instant::now();
        println!("addr after {:?}: {:?}", t.elapsed(), ep.addr().addrs); tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
