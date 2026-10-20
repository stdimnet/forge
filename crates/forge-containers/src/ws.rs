//! WebSocket (RFC 6455) pieces for the Kubernetes streaming subresources: the upgrade
//! handshake headers and `Sec-WebSocket-Accept` check, frame encoding (client frames are
//! masked) and an incremental frame decoder that also joins fragmented messages.

use base64::Engine;
use sha1::{Digest, Sha1};

use crate::{err, Result};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Largest message accepted from the server (port-forward data comes in small frames).
pub const MAX_MESSAGE: usize = 16 * 1024 * 1024;

pub const OP_CONTINUATION: u8 = 0x0;
pub const OP_TEXT: u8 = 0x1;
pub const OP_BINARY: u8 = 0x2;
pub const OP_CLOSE: u8 = 0x8;
pub const OP_PING: u8 = 0x9;
pub const OP_PONG: u8 = 0xA;

/// `Sec-WebSocket-Key` for 16 random bytes.
pub fn client_key(nonce: [u8; 16]) -> String {
    base64::engine::general_purpose::STANDARD.encode(nonce)
}

/// The `Sec-WebSocket-Accept` the server must answer for `key`.
pub fn accept_key(key: &str) -> String {
    let mut h = Sha1::new();
    h.update(key.as_bytes());
    h.update(GUID.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(h.finalize())
}

/// Headers of the upgrade request (besides `Host`).
pub fn upgrade_headers(key: &str, protocols: &[&str]) -> Vec<(String, String)> {
    let mut h = vec![
        ("Connection".to_string(), "Upgrade".to_string()),
        ("Upgrade".to_string(), "websocket".to_string()),
        ("Sec-WebSocket-Version".to_string(), "13".to_string()),
        ("Sec-WebSocket-Key".to_string(), key.to_string()),
    ];
    if !protocols.is_empty() {
        h.push(("Sec-WebSocket-Protocol".to_string(), protocols.join(", ")));
    }
    h
}

/// Check the server's answer to the upgrade request: `101`, the accept key and (when
/// protocols were offered) one of them. Returns the chosen protocol.
pub fn check_upgrade(
    status: u16,
    header: &dyn Fn(&str) -> Option<String>,
    key: &str,
    protocols: &[&str],
) -> Result<Option<String>> {
    if status != 101 {
        return err(format!(
            "The server did not upgrade the connection (HTTP {status})"
        ));
    }
    if header("Sec-WebSocket-Accept").as_deref() != Some(accept_key(key).as_str()) {
        return err("The server answered with a wrong Sec-WebSocket-Accept");
    }
    let chosen = header("Sec-WebSocket-Protocol").filter(|p| !p.is_empty());
    if let Some(p) = &chosen {
        if !protocols.contains(&p.as_str()) {
            return err(format!("The server chose an unknown protocol `{p}`"));
        }
    }
    Ok(chosen)
}

/// One frame; `mask` is set for client frames.
pub fn encode_frame(opcode: u8, payload: &[u8], mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | (opcode & 0x0F));
    let m = if mask.is_some() { 0x80 } else { 0 };
    match payload.len() {
        n if n < 126 => out.push(m | n as u8),
        n if n <= u16::MAX as usize => {
            out.push(m | 126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(m | 127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    match mask {
        Some(key) => {
            out.extend_from_slice(&key);
            out.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
        }
        None => out.extend_from_slice(payload),
    }
    out
}

/// A complete message from the peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Binary(Vec<u8>),
    Text(String),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    /// Close with the status code and reason, if any.
    Close(Option<u16>, String),
}

/// Incremental decoder: push bytes as they arrive, take complete messages.
#[derive(Debug, Default)]
pub struct Decoder {
    buf: Vec<u8>,
    /// Opcode and data of a fragmented message in progress.
    partial: Option<(u8, Vec<u8>)>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete message, `None` until more bytes are needed.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Message>> {
        loop {
            let Some((fin, opcode, payload, used)) = self.frame()? else {
                return Ok(None);
            };
            self.buf.drain(..used);
            match opcode {
                OP_PING => return Ok(Some(Message::Ping(payload))),
                OP_PONG => return Ok(Some(Message::Pong(payload))),
                OP_CLOSE => {
                    let code =
                        (payload.len() >= 2).then(|| u16::from_be_bytes([payload[0], payload[1]]));
                    let reason = payload
                        .get(2..)
                        .map(|r| String::from_utf8_lossy(r).into_owned())
                        .unwrap_or_default();
                    return Ok(Some(Message::Close(code, reason)));
                }
                OP_CONTINUATION => {
                    let Some((_, data)) = self.partial.as_mut() else {
                        return err("WebSocket continuation without a message");
                    };
                    data.extend_from_slice(&payload);
                    if data.len() > MAX_MESSAGE {
                        return err("WebSocket message too large");
                    }
                    if fin {
                        let (op, data) = self.partial.take().unwrap_or_default();
                        return Ok(Some(message(op, data)));
                    }
                }
                OP_TEXT | OP_BINARY => {
                    if fin {
                        return Ok(Some(message(opcode, payload)));
                    }
                    self.partial = Some((opcode, payload));
                }
                other => return err(format!("Unknown WebSocket opcode {other}")),
            }
        }
    }

    /// Parse one frame from the buffer: `(fin, opcode, unmasked payload, bytes used)`.
    fn frame(&self) -> Result<Option<(bool, u8, Vec<u8>, usize)>> {
        let b = &self.buf;
        if b.len() < 2 {
            return Ok(None);
        }
        let fin = b[0] & 0x80 != 0;
        let opcode = b[0] & 0x0F;
        let masked = b[1] & 0x80 != 0;
        let (len, mut at) = match b[1] & 0x7F {
            126 => {
                if b.len() < 4 {
                    return Ok(None);
                }
                (u16::from_be_bytes([b[2], b[3]]) as u64, 4)
            }
            127 => {
                if b.len() < 10 {
                    return Ok(None);
                }
                let mut n = [0u8; 8];
                n.copy_from_slice(&b[2..10]);
                (u64::from_be_bytes(n), 10)
            }
            n => (n as u64, 2),
        };
        if len > MAX_MESSAGE as u64 {
            return err("WebSocket frame too large");
        }
        let key = if masked {
            if b.len() < at + 4 {
                return Ok(None);
            }
            let k = [b[at], b[at + 1], b[at + 2], b[at + 3]];
            at += 4;
            Some(k)
        } else {
            None
        };
        let end = at + len as usize;
        if b.len() < end {
            return Ok(None);
        }
        let mut payload = b[at..end].to_vec();
        if let Some(k) = key {
            for (i, x) in payload.iter_mut().enumerate() {
                *x ^= k[i % 4];
            }
        }
        Ok(Some((fin, opcode, payload, end)))
    }
}

fn message(opcode: u8, data: Vec<u8>) -> Message {
    if opcode == OP_TEXT {
        Message::Text(String::from_utf8_lossy(&data).into_owned())
    } else {
        Message::Binary(data)
    }
}

/// A small xorshift generator for masking keys and handshake nonces (RFC 6455 only needs
/// them to be unpredictable to proxies, not cryptographically strong).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn mask(&mut self) -> [u8; 4] {
        (self.next_u64() as u32).to_le_bytes()
    }

    pub fn nonce(&mut self) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..8].copy_from_slice(&self.next_u64().to_le_bytes());
        n[8..].copy_from_slice(&self.next_u64().to_le_bytes());
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_key_from_the_rfc() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let headers = |name: &str| match name {
            "Sec-WebSocket-Accept" => Some(accept_key(key)),
            "Sec-WebSocket-Protocol" => Some("v4.channel.k8s.io".to_string()),
            _ => None,
        };
        assert_eq!(
            check_upgrade(101, &headers, key, &["v4.channel.k8s.io"]).unwrap(),
            Some("v4.channel.k8s.io".into())
        );
        assert!(check_upgrade(400, &headers, key, &["v4.channel.k8s.io"]).is_err());
        assert!(check_upgrade(101, &headers, "other", &["v4.channel.k8s.io"]).is_err());
        assert!(check_upgrade(101, &headers, key, &["x"]).is_err());
        let h = upgrade_headers(key, &["a", "b"]);
        assert!(h.contains(&("Sec-WebSocket-Protocol".into(), "a, b".into())));
    }

    #[test]
    fn frames_round_trip_with_and_without_mask() {
        for len in [0usize, 5, 125, 126, 300, 70_000] {
            let payload: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            for mask in [None, Some([1, 2, 3, 4])] {
                let frame = encode_frame(OP_BINARY, &payload, mask);
                let mut d = Decoder::new();
                // Byte by byte for small frames, all at once for big ones.
                if len < 400 {
                    for (i, b) in frame.iter().enumerate() {
                        d.push(&[*b]);
                        let m = d.next().unwrap();
                        if i + 1 < frame.len() {
                            assert_eq!(m, None);
                        } else {
                            assert_eq!(m, Some(Message::Binary(payload.clone())));
                        }
                    }
                } else {
                    d.push(&frame);
                    assert_eq!(d.next().unwrap(), Some(Message::Binary(payload.clone())));
                }
            }
        }
    }

    #[test]
    fn control_frames_and_fragments() {
        let mut d = Decoder::new();
        let mut first = encode_frame(OP_TEXT, b"hel", None);
        first[0] &= 0x7F; // not final
        d.push(&first);
        d.push(&encode_frame(OP_PING, b"p", None));
        let mut cont = encode_frame(OP_CONTINUATION, b"lo", None);
        cont[0] |= 0x80;
        d.push(&cont);
        let mut close = vec![0x03, 0xE8];
        close.extend_from_slice(b"bye");
        d.push(&encode_frame(OP_CLOSE, &close, None));
        assert_eq!(d.next().unwrap(), Some(Message::Ping(b"p".to_vec())));
        assert_eq!(d.next().unwrap(), Some(Message::Text("hello".into())));
        assert_eq!(
            d.next().unwrap(),
            Some(Message::Close(Some(1000), "bye".into()))
        );
        assert_eq!(d.next().unwrap(), None);
        let mut bad = Decoder::new();
        bad.push(&encode_frame(OP_CONTINUATION, b"x", None));
        assert!(bad.next().is_err());
    }

    #[test]
    fn rng_varies() {
        let mut r = Rng::new(42);
        let a = r.mask();
        let b = r.mask();
        assert_ne!(a, b);
        assert_eq!(client_key(r.nonce()).len(), 24);
    }
}
