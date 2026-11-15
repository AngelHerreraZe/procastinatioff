#![no_main]
// Content of a scanned QR (controlled by whoever prints it).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = procastinacioff_core::pairing::parse_offer(s);
    }
});
