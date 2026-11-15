#![no_main]
// Blobs que llegan del relay (no confiable).
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;

static ID: OnceLock<age::x25519::Identity> = OnceLock::new();

fuzz_target!(|data: &[u8]| {
    let id = ID.get_or_init(age::x25519::Identity::generate);
    let _ = procastinacioff_core::events::open(data, id);
});
