//! Session links come from clipboards, chat messages and `forge://` deep links: untrusted
//! text. Parsing must never panic, and whatever it accepts must survive a round trip through
//! both link forms unchanged.
//!
//! ```sh
//! cd crates/forge-collab/fuzz && cargo +nightly fuzz run link_parse
//! ```
#![no_main]

use forge_collab::link::SessionLink;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(link) = SessionLink::parse(text) {
        let again =
            SessionLink::parse(&link.to_url()).expect("a parsed link prints as a valid link");
        assert_eq!(again, link);
        assert_eq!(SessionLink::parse(&link.to_text()).unwrap(), link);
        let _ = link.summary();
    }
});
