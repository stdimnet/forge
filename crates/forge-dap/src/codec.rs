//! DAP base protocol: every message is a JSON object preceded by a header block
//! (`Content-Length: <bytes>\r\n\r\n`).

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

use crate::error::{DapError, DapResult};

/// Upper bound for a single message (a disassembly or memory dump is far below this).
const MAX_MESSAGE: usize = 64 * 1024 * 1024;

/// Frame one message.
pub fn encode(message: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(message).unwrap_or_else(|_| b"{}".to_vec());
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out
}

/// Read one message; `None` at a clean end of the stream.
pub async fn read_message<R: AsyncBufRead + Unpin>(r: &mut R) -> DapResult<Option<Value>> {
    let mut length: Option<usize> = None;
    let mut line = String::new();
    let mut started = false;
    loop {
        line.clear();
        let n = r.read_line(&mut line).await?;
        if n == 0 {
            return if started {
                Err(DapError::Protocol("stream ended inside a header".into()))
            } else {
                Ok(None)
            };
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if !started {
                // Tolerate blank lines between messages.
                continue;
            }
            break;
        }
        started = true;
        if let Some((name, value)) = header.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = Some(value.trim().parse().map_err(|_| {
                    DapError::Protocol(format!("bad Content-Length '{}'", value.trim()))
                })?);
            }
        }
    }
    let length = length.ok_or_else(|| DapError::Protocol("missing Content-Length".into()))?;
    if length > MAX_MESSAGE {
        return Err(DapError::Protocol(format!("message of {length} bytes")));
    }
    let mut body = vec![0u8; length];
    r.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}

/// The next line of a program's output without its line ending, invalid UTF-8 replaced by
/// U+FFFD (`None` at the end). Unlike `AsyncBufReadExt::lines`, one invalid byte does not
/// end the reader, which would leave the pipe unread and the program killed by SIGPIPE.
pub async fn next_line_lossy<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> std::io::Result<Option<String>> {
    buf.clear();
    if reader.read_until(b'\n', buf).await? == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
    Ok(Some(String::from_utf8_lossy(buf).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn output_lines_survive_invalid_utf8() {
        let mut r = tokio::io::BufReader::new(&b"ok\r\nbad \xff byte\nlast"[..]);
        let mut buf = Vec::new();
        let mut lines = Vec::new();
        while let Some(l) = next_line_lossy(&mut r, &mut buf).await.unwrap() {
            lines.push(l);
        }
        assert_eq!(lines, ["ok", "bad \u{fffd} byte", "last"]);
    }

    #[tokio::test]
    async fn round_trips_messages_with_extra_headers() {
        let a = json!({"seq": 1, "type": "request", "command": "initialize"});
        let b = json!({"seq": 2, "type": "event", "event": "output", "body": {"output": "ä\n"}});
        let mut bytes = encode(&a);
        bytes.extend(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n");
        bytes.extend(encode(&b));
        let mut r = BufReader::new(&bytes[..]);
        assert_eq!(read_message(&mut r).await.unwrap(), Some(a));
        assert_eq!(read_message(&mut r).await.unwrap(), Some(b));
        assert_eq!(read_message(&mut r).await.unwrap(), None);
    }

    #[tokio::test]
    async fn rejects_missing_length_and_truncated_bodies() {
        let mut r = BufReader::new(&b"X-Other: 1\r\n\r\n{}"[..]);
        assert!(read_message(&mut r).await.is_err());
        let mut r = BufReader::new(&b"Content-Length: 10\r\n\r\n{}"[..]);
        assert!(read_message(&mut r).await.is_err());
    }
}
