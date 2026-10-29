//! The collab protocol: what guests and host send each other over one encrypted stream.
//!
//! Frames have the agent protocol's format (`forge_backend::protocol`): a big-endian `u32`
//! length and a MessagePack map with named fields, so older peers ignore fields they do not
//! know. The message enums are deliberately separate from the agent's `Call`: a guest is
//! untrusted, so its enum contains only requests the host's
//! [`Policy`](crate::permissions::Policy) can check. Later stages add variants (project
//! calls, terminals, runs) without changing the existing ones.
//!
//! Documents use the Yjs update format: `update` fields are opaque Yjs (v1) updates, and
//! awareness fields are `y-protocols/awareness` updates.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::permissions::Permissions;
use crate::presence::{Participant, ParticipantId};
use crate::{Error, Result};

/// Bumped on incompatible changes; the host refuses other versions in the knock.
pub const PROTOCOL_VERSION: u32 = 1;

/// QUIC application protocol of Code With Me connections.
pub const ALPN: &[u8] = b"forge/cwm/1";

/// Largest frame a guest accepts (the initial state of a shared file up to the size limit).
pub const MAX_HOST_FRAME: usize = 32 * 1024 * 1024;
/// Largest frame the host accepts from a guest (one edit, e.g. a large paste).
pub const MAX_GUEST_FRAME: usize = 4 * 1024 * 1024;

/// A shared document, numbered by the host. Documents are keyed by id, not by path, so a
/// rename keeps everyone's tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocId(pub u64);

/// The text as last written to disk, as a hash. A participant's tab is dirty while the hash
/// of its text differs ([`text_hash`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveState {
    pub hash: String,
}

impl SaveState {
    pub fn of(text: &str) -> Self {
        Self {
            hash: text_hash(text),
        }
    }
}

/// `<UTF-16 length>:<FNV-1a 32 of the UTF-16 code units, hex>`. The UI computes the same
/// value in TypeScript (`features/code-with-me/hash.ts`), on the same UTF-16 text it edits.
pub fn text_hash(text: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    let mut len = 0usize;
    for unit in text.encode_utf16() {
        hash ^= u32::from(unit);
        hash = hash.wrapping_mul(0x0100_0193);
        len += 1;
    }
    format!("{len}:{hash:08x}")
}

/// A file the host shares (one the host has open in the editor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedFile {
    pub path: String,
}

/// Guest → host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GuestMessage {
    /// The first message on a connection. `proof` is the join proof
    /// ([`crate::verify::join_proof`]); a wrong one closes the connection silently.
    Knock {
        protocol: u32,
        session: ByteBuf,
        name: String,
        proof: ByteBuf,
    },
    /// Open a shared file -> [`HostMessage::DocOpened`] or [`HostMessage::DocError`].
    DocOpen {
        request: u64,
        path: String,
    },
    /// A local edit (Yjs update).
    DocUpdate {
        doc: DocId,
        update: ByteBuf,
    },
    /// Caret and selection (awareness update).
    Awareness {
        doc: DocId,
        update: ByteBuf,
    },
    /// Write the document to disk -> [`HostMessage::SaveResult`].
    DocSave {
        request: u64,
        doc: DocId,
    },
    DocClose {
        doc: DocId,
    },
    /// A project request of the guest's backend (`guest_backend::HostCall`, MessagePack) ->
    /// [`HostMessage::CallResult`]. The payload stays opaque here so this module is pure.
    Call {
        id: u64,
        payload: ByteBuf,
    },
    Leave,
}

impl GuestMessage {
    /// The request's name for the session log.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Knock { .. } => "knock",
            Self::DocOpen { .. } => "open",
            Self::DocUpdate { .. } => "edit",
            Self::Awareness { .. } => "caret",
            Self::DocSave { .. } => "save",
            Self::DocClose { .. } => "close",
            Self::Call { .. } => "call",
            Self::Leave => "leave",
        }
    }
}

/// Host → guest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HostMessage {
    /// The knock was accepted.
    Welcome {
        protocol: u32,
        you: ParticipantId,
        participants: Vec<Participant>,
        files: Vec<SharedFile>,
        /// Session-wide notice, e.g. that this is a development preview without approval.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notice: Option<String>,
    },
    /// The knock was refused (wrong version, removed guest, ...). Wrong proofs get no answer.
    Refused {
        reason: String,
    },
    Participants {
        participants: Vec<Participant>,
    },
    /// The files guests can open changed.
    SharedFiles {
        files: Vec<SharedFile>,
    },
    /// Your permissions changed.
    Permissions {
        permissions: Permissions,
    },
    DocOpened {
        request: u64,
        doc: DocId,
        path: String,
        /// The whole document as one Yjs update.
        update: ByteBuf,
        saved: SaveState,
        read_only: bool,
    },
    DocError {
        request: u64,
        error: String,
    },
    DocUpdate {
        doc: DocId,
        update: ByteBuf,
    },
    Awareness {
        doc: DocId,
        update: ByteBuf,
    },
    /// The document was written to disk or reloaded from it.
    DocSaved {
        doc: DocId,
        saved: SaveState,
    },
    SaveResult {
        request: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    DocRenamed {
        doc: DocId,
        path: String,
    },
    /// The document is gone for everyone (the file was deleted or is no longer shared).
    DocClosed {
        doc: DocId,
        reason: String,
    },
    /// The answer to a [`GuestMessage::Call`]: the MessagePack encoding of the call's reply
    /// type, or the error to show.
    CallResult {
        id: u64,
        result: std::result::Result<ByteBuf, String>,
    },
    /// Files of the project changed on disk (only those the guest may see), after the guest
    /// asked to watch the project.
    FsChanged {
        changed: Vec<String>,
        removed: Vec<String>,
        git: bool,
    },
    /// Language server diagnostics (JSON `EditorDiagnostic` list) for a file the guest told
    /// the host it has open.
    Diagnostics {
        path: String,
        diagnostics: ByteBuf,
    },
    /// A request was refused by the host's policy.
    Denied {
        reason: String,
    },
    /// The session is over for this guest.
    Ended {
        reason: String,
    },
}

/// One frame: length and MessagePack body.
pub fn encode<T: Serialize>(msg: &T, limit: usize) -> Result<Vec<u8>> {
    let body = rmp_serde::to_vec_named(msg).map_err(|e| Error::Protocol(e.to_string()))?;
    if body.len() > limit {
        return Err(Error::Protocol(format!(
            "message of {} bytes is too large",
            body.len()
        )));
    }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decode a frame body (without the length prefix).
pub fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T> {
    rmp_serde::from_slice(body).map_err(|e| Error::Protocol(e.to_string()))
}

/// Splits a byte stream into messages; feed it whatever arrives.
#[derive(Debug)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    limit: usize,
}

impl FrameDecoder {
    pub fn new(limit: usize) -> Self {
        Self {
            buf: Vec::new(),
            limit,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete message, `Ok(None)` when more bytes are needed. Errors are final:
    /// the stream is not trusted after garbage.
    pub fn next_message<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if len > self.limit {
            return Err(Error::Protocol(format!(
                "message of {len} bytes is too large"
            )));
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let msg = decode(&self.buf[4..4 + len]);
        self.buf.drain(..4 + len);
        msg.map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presence::Role;

    fn bytes(b: &[u8]) -> ByteBuf {
        ByteBuf::from(b.to_vec())
    }

    #[test]
    fn guest_messages_round_trip() {
        let msgs = vec![
            GuestMessage::Knock {
                protocol: PROTOCOL_VERSION,
                session: bytes(&[1; 16]),
                name: "Alex".into(),
                proof: bytes(&[2; 32]),
            },
            GuestMessage::DocOpen {
                request: 7,
                path: "app/Http/Kernel.php".into(),
            },
            GuestMessage::DocUpdate {
                doc: DocId(3),
                update: bytes(&[1, 2, 3]),
            },
            GuestMessage::Awareness {
                doc: DocId(3),
                update: bytes(&[4]),
            },
            GuestMessage::DocSave {
                request: 8,
                doc: DocId(3),
            },
            GuestMessage::DocClose { doc: DocId(3) },
            GuestMessage::Call {
                id: 9,
                payload: bytes(&[0x81, 0xa4]),
            },
            GuestMessage::Leave,
        ];
        let mut dec = FrameDecoder::new(MAX_GUEST_FRAME);
        for m in &msgs {
            dec.push(&encode(m, MAX_GUEST_FRAME).unwrap());
        }
        for m in msgs {
            assert_eq!(dec.next_message::<GuestMessage>().unwrap(), Some(m));
        }
        assert_eq!(dec.next_message::<GuestMessage>().unwrap(), None);
    }

    #[test]
    fn host_messages_round_trip_byte_by_byte() {
        let msg = HostMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            you: ParticipantId(2),
            participants: vec![Participant {
                id: ParticipantId::HOST,
                name: "Robert".into(),
                role: Role::Host,
                color: 1,
                permissions: Permissions::FULL_ACCESS,
            }],
            files: vec![SharedFile {
                path: "a.php".into(),
            }],
            notice: None,
        };
        let frame = encode(&msg, MAX_HOST_FRAME).unwrap();
        let mut dec = FrameDecoder::new(MAX_HOST_FRAME);
        for (i, b) in frame.iter().enumerate() {
            dec.push(&[*b]);
            let got = dec.next_message::<HostMessage>().unwrap();
            if i + 1 < frame.len() {
                assert_eq!(got, None);
            } else {
                assert_eq!(got, Some(msg.clone()));
            }
        }
    }

    #[test]
    fn oversized_and_garbage_frames_are_refused() {
        let big = GuestMessage::DocUpdate {
            doc: DocId(1),
            update: bytes(&vec![0; 1024]),
        };
        assert!(encode(&big, 100).is_err());
        let mut dec = FrameDecoder::new(100);
        dec.push(&(1000u32).to_be_bytes());
        assert!(dec.next_message::<GuestMessage>().is_err());
        let mut dec = FrameDecoder::new(100);
        dec.push(&[0, 0, 0, 3, 0xc1, 0xc1, 0xc1]);
        assert!(dec.next_message::<GuestMessage>().is_err());
    }

    #[test]
    fn hash_counts_utf16_units() {
        assert_eq!(text_hash(""), "0:811c9dc5");
        assert_eq!(text_hash("a"), "1:e40c292c");
        // A surrogate pair is two UTF-16 units, like JavaScript's `length`.
        assert!(text_hash("😀").starts_with("2:"));
        assert_ne!(text_hash("ab"), text_hash("ba"));
        assert_eq!(SaveState::of("x").hash, text_hash("x"));
    }
}
