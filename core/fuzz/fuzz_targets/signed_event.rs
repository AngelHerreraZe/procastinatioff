#![no_main]
// JSON de eventos firmados (tras descifrar un blob de otro miembro).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(ev) = serde_json::from_slice::<procastinacioff_core::events::SignedEvent>(data) {
        let _ = ev.verify();
        let _ = ev.id();
    }
});
