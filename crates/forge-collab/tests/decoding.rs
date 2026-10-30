//! Property tests for everything a peer controls before the host's policy sees it: session
//! links, collab frames and host calls. The cheap, always-run sibling of the cargo-fuzz
//! targets in `crates/forge-collab/fuzz` (which run on demand with a nightly toolchain).

use std::net::SocketAddr;

use forge_collab::guest_backend::calls::{HostCall, MAX_CALL};
use forge_collab::link::SessionLink;
use forge_collab::protocol::{
    encode, DocId, FrameDecoder, GuestMessage, HostMessage, MAX_GUEST_FRAME, MAX_HOST_FRAME,
};
use proptest::prelude::*;
use serde_bytes::ByteBuf;

fn link() -> impl Strategy<Value = SessionLink> {
    (
        "\\PC{1,40}",
        proptest::option::of("[a-zA-Z0-9 .ü-]{1,30}"),
        any::<[u8; 16]>(),
        any::<[u8; 32]>(),
        any::<[u8; 32]>(),
        proptest::option::of("https://relay\\.[a-z]{1,10}\\.example(:[0-9]{2,4})?/?"),
        // Links carry address and port only: no IPv6 flow label or scope id.
        prop::collection::vec(
            any::<SocketAddr>().prop_map(|mut a| {
                if let SocketAddr::V6(v6) = &mut a {
                    v6.set_flowinfo(0);
                    v6.set_scope_id(0);
                }
                a
            }),
            0..4,
        ),
    )
        .prop_map(
            |(host_name, machine, session_id, host_id, join_secret, relay, addrs)| SessionLink {
                host_name: host_name.trim().to_string(),
                machine: machine
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty()),
                session_id,
                host_id,
                join_secret,
                relay,
                addrs,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Valid links print and parse back unchanged, in both forms.
    #[test]
    fn links_round_trip(link in link()) {
        // Some generated names are not valid (empty after trimming, control characters);
        // those must be refused when parsing, never accepted in a different shape.
        match SessionLink::parse(&link.to_url()) {
            Ok(parsed) => {
                prop_assert_eq!(&parsed, &link);
                prop_assert_eq!(SessionLink::parse(&link.to_text()).unwrap(), link);
            }
            Err(_) => prop_assert!(SessionLink::parse(&link.to_text()).is_err()),
        }
    }

    /// Arbitrary text never panics the parser, and whatever it accepts round-trips.
    #[test]
    fn any_text_parses_or_fails_cleanly(text in "(forge://cwm/join\\?|cwm1:)?\\PC{0,200}") {
        if let Ok(link) = SessionLink::parse(&text) {
            prop_assert_eq!(SessionLink::parse(&link.to_url()).unwrap(), link);
        }
    }

    /// Arbitrary bytes in arbitrary chunks never panic the frame decoder; after the first
    /// error the stream is over.
    #[test]
    fn garbage_frames_fail_cleanly(stream in prop::collection::vec(any::<u8>(), 0..512), chunk in 1usize..64) {
        let mut dec = FrameDecoder::new(MAX_GUEST_FRAME);
        let mut failed = false;
        for part in stream.chunks(chunk) {
            dec.push(part);
            while !failed {
                match dec.next_message::<GuestMessage>() {
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(_) => failed = true,
                }
            }
        }
    }

    /// Valid frames split at any points decode to the same messages, in order.
    #[test]
    fn frames_survive_any_split(
        docs in prop::collection::vec((any::<u64>(), prop::collection::vec(any::<u8>(), 0..64), any::<bool>()), 1..8),
        chunk in 1usize..32,
    ) {
        let msgs: Vec<HostMessage> = docs
            .into_iter()
            .map(|(doc, update, conflict)| {
                if conflict {
                    HostMessage::DocConflict { doc: DocId(doc), conflict: update.len().is_multiple_of(2) }
                } else {
                    HostMessage::DocUpdate { doc: DocId(doc), update: ByteBuf::from(update) }
                }
            })
            .collect();
        let stream: Vec<u8> = msgs.iter().flat_map(|m| encode(m, MAX_HOST_FRAME).unwrap()).collect();
        let mut dec = FrameDecoder::new(MAX_HOST_FRAME);
        let mut got = Vec::new();
        for part in stream.chunks(chunk) {
            dec.push(part);
            while let Some(m) = dec.next_message::<HostMessage>().unwrap() {
                got.push(m);
            }
        }
        prop_assert_eq!(got, msgs);
    }

    /// Arbitrary call payloads never panic the host's decoder; decoded calls round-trip.
    #[test]
    fn any_call_payload_decodes_or_fails_cleanly(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        if let Ok(call) = HostCall::decode(&bytes) {
            prop_assert_eq!(HostCall::decode(&call.encode().unwrap()).unwrap(), call);
        }
    }
}

#[test]
fn oversized_calls_are_refused_before_decoding() {
    let err = HostCall::decode(&vec![0x90; MAX_CALL + 1]).unwrap_err();
    assert!(err.to_string().contains("too large"), "{err}");
}
