//! JSON-RPC message framing used by LSP over stdio: a `Content-Length` header block,
//! an empty line, then exactly that many bytes of UTF-8 JSON.

use std::io;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

/// Serialize a message into a framed byte buffer ready to be written to the server.
pub fn encode(message: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(message).expect("serde_json::Value always serializes");
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out
}

/// Read the next framed message. Returns `Ok(None)` on a clean end of stream.
pub async fn read_message<R: AsyncBufRead + Unpin>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return if content_length.is_none() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "stream ended inside a header",
                ))
            };
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if content_length.is_some() {
                break;
            }
            // Tolerate stray blank lines between messages.
            continue;
        }
        match header.split_once(':') {
            Some((name, value)) => {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    let len = value.trim().parse::<usize>().map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("bad Content-Length: {value}"),
                        )
                    })?;
                    content_length = Some(len);
                }
                // Other headers (Content-Type) are ignored.
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("malformed header line: {header}"),
                ))
            }
        }
    }
    let len = content_length.unwrap_or_default();
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[test]
    fn encode_writes_byte_length_header() {
        let msg = json!({"jsonrpc": "2.0", "method": "ä"});
        let text = String::from_utf8(encode(&msg)).unwrap();
        let (header, body) = text.split_once("\r\n\r\n").unwrap();
        assert_eq!(header, format!("Content-Length: {}", body.len()));
        // "ä" is two bytes, so the byte length differs from the char count.
        assert_ne!(body.len(), body.chars().count());
    }

    #[tokio::test]
    async fn reads_consecutive_messages_and_eof() {
        let mut data = encode(&json!({"id": 1}));
        data.extend(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n");
        data.extend(encode(&json!({"id": 2, "text": "Grüße"})));
        let mut reader = BufReader::new(&data[..]);
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(json!({"id": 1}))
        );
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(json!({"id": 2, "text": "Grüße"}))
        );
        assert_eq!(read_message(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn reads_messages_split_across_small_reads() {
        let data = encode(&json!({"method": "x", "params": [1, 2, 3]}));
        let mut reader = BufReader::with_capacity(3, &data[..]);
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(json!({"method": "x", "params": [1, 2, 3]}))
        );
    }

    #[tokio::test]
    async fn header_names_are_case_insensitive() {
        let data = b"content-length: 2\r\n\r\n{}".to_vec();
        let mut reader = BufReader::new(&data[..]);
        assert_eq!(read_message(&mut reader).await.unwrap(), Some(json!({})));
    }

    #[tokio::test]
    async fn rejects_garbage_and_truncated_bodies() {
        let data = b"hello world\r\n\r\n".to_vec();
        let mut reader = BufReader::new(&data[..]);
        assert!(read_message(&mut reader).await.is_err());

        let data = b"Content-Length: 10\r\n\r\n{}".to_vec();
        let mut reader = BufReader::new(&data[..]);
        assert!(read_message(&mut reader).await.is_err());
    }
}
