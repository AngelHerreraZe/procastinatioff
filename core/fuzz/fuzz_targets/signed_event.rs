#![no_main]
// Signed event JSON (after decrypting a blob from another member).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(ev) = serde_json::from_slice::<procastinatioff_core::events::SignedEvent>(data) {
        let _ = ev.verify();
        let _ = ev.id();
    }
});
