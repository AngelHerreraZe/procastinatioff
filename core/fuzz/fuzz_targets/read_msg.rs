#![no_main]
// A message from a peer, after the length check (untrusted until the ledger accepts its events).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() <= procastinacioff_proto::MAX_FRAME {
        let _ = procastinacioff_proto::decode(data);
    }
});
