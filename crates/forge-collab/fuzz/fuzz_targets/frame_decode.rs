//! The host reads frames from guests it does not trust, and guests read the host's frames.
//! Decoding arbitrary bytes, split into arbitrary chunks, must never panic or allocate past
//! the frame limit, and must stop at the first bad frame.
//!
//! ```sh
//! cd crates/forge-collab/fuzz && cargo +nightly fuzz run frame_decode
//! ```
#![no_main]

use forge_collab::protocol::{FrameDecoder, GuestMessage, HostMessage, MAX_GUEST_FRAME};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The first byte picks the chunk size, the rest is the stream.
    let Some((&chunk, stream)) = data.split_first() else {
        return;
    };
    let chunk = usize::from(chunk).max(1);
    let mut guest = FrameDecoder::new(MAX_GUEST_FRAME);
    let mut host = FrameDecoder::new(MAX_GUEST_FRAME);
    let (mut guest_ok, mut host_ok) = (true, true);
    for part in stream.chunks(chunk) {
        guest.push(part);
        host.push(part);
        while guest_ok {
            match guest.next_message::<GuestMessage>() {
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => guest_ok = false,
            }
        }
        while host_ok {
            match host.next_message::<HostMessage>() {
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => host_ok = false,
            }
        }
    }
});
