//! A guest's `Call` payload is decoded by the host before the policy check. Arbitrary bytes
//! must never panic, and a call that decodes must encode and decode to the same call.
//!
//! ```sh
//! cd crates/forge-collab/fuzz && cargo +nightly fuzz run host_call_decode
//! ```
#![no_main]

use forge_collab::guest_backend::calls::HostCall;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(call) = HostCall::decode(data) {
        let bytes = call.encode().expect("a decoded call encodes");
        assert_eq!(HostCall::decode(&bytes).expect("and decodes again"), call);
    }
});
