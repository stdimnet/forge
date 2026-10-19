//! Minimal Server-Sent Events parser (WHATWG event-stream format).
//!
//! Bytes can arrive split anywhere — inside a line, between `\r` and `\n`, or in the
//! middle of a multi-byte UTF-8 sequence — so input is buffered as bytes and only
//! complete lines are decoded.

use bytes::Bytes;
use futures_util::{Stream, StreamExt};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SseEvent {
    /// `event:` field; empty means the default "message" event.
    pub event: String,
    pub data: String,
    pub id: Option<String>,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    /// The previous chunk ended with `\r`; a leading `\n` in the next chunk belongs to it.
    pending_cr: bool,
    event: String,
    data: Vec<String>,
    id: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed bytes; returns every event completed by them.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        let mut out = Vec::new();
        for &b in chunk {
            if std::mem::take(&mut self.pending_cr) && b == b'\n' {
                continue; // second half of `\r\n`
            }
            match b {
                b'\n' => self.end_line(&mut out),
                b'\r' => {
                    self.end_line(&mut out);
                    self.pending_cr = true;
                }
                _ => self.buf.push(b),
            }
        }
        out
    }

    fn end_line(&mut self, out: &mut Vec<SseEvent>) {
        let line = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if line.starts_with(':') {
            return; // comment / keep-alive
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line.as_str(), ""),
        };
        match field {
            "event" => self.event = value.to_string(),
            "data" => self.data.push(value.to_string()),
            "id" => self.id = Some(value.to_string()),
            _ => {} // `retry` and unknown fields are ignored
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseEvent>) {
        if self.data.is_empty() && self.event.is_empty() {
            return;
        }
        out.push(SseEvent {
            event: std::mem::take(&mut self.event),
            data: std::mem::take(&mut self.data).join("\n"),
            id: self.id.clone(),
        });
    }

    /// Flush a final event that was not followed by a blank line (stream closed early).
    pub fn finish(&mut self) -> Option<SseEvent> {
        if !self.buf.is_empty() {
            let mut out = Vec::new();
            self.end_line(&mut out);
        }
        let mut out = Vec::new();
        self.dispatch(&mut out);
        out.pop()
    }
}

/// Turn a byte stream into a stream of SSE events.
pub fn sse_events<S, E>(bytes: S) -> impl Stream<Item = Result<SseEvent, E>>
where
    S: Stream<Item = Result<Bytes, E>>,
{
    async_stream::stream! {
        let mut parser = SseParser::new();
        let mut bytes = std::pin::pin!(bytes);
        while let Some(chunk) = bytes.next().await {
            match chunk {
                Ok(chunk) => {
                    for ev in parser.push(&chunk) {
                        yield Ok(ev);
                    }
                }
                Err(e) => {
                    yield Err(e);
                    return;
                }
            }
        }
        if let Some(ev) = parser.finish() {
            yield Ok(ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = ": keep-alive\r\n\
event: message_start\r\n\
data: {\"a\":1}\r\n\
\r\n\
event: content_block_delta\n\
data: {\"text\":\"Grüße 🔥\"}\n\
\n\
data: line one\r\
data: line two\r\
\r\
id: 7\n\
data:no-space\n\
retry: 100\n\
\n\
data: [DONE]\n\n";

    fn expected() -> Vec<SseEvent> {
        vec![
            SseEvent {
                event: "message_start".into(),
                data: "{\"a\":1}".into(),
                id: None,
            },
            SseEvent {
                event: "content_block_delta".into(),
                data: "{\"text\":\"Grüße 🔥\"}".into(),
                id: None,
            },
            SseEvent {
                event: String::new(),
                data: "line one\nline two".into(),
                id: None,
            },
            SseEvent {
                event: String::new(),
                data: "no-space".into(),
                id: Some("7".into()),
            },
            SseEvent {
                event: String::new(),
                data: "[DONE]".into(),
                id: Some("7".into()),
            },
        ]
    }

    fn parse_chunks(chunks: &[&[u8]]) -> Vec<SseEvent> {
        let mut p = SseParser::new();
        let mut out: Vec<SseEvent> = chunks.iter().flat_map(|c| p.push(c)).collect();
        out.extend(p.finish());
        out
    }

    #[test]
    fn parses_whole_input() {
        assert_eq!(parse_chunks(&[SAMPLE.as_bytes()]), expected());
    }

    #[test]
    fn parses_input_split_at_every_pair_of_positions() {
        let bytes = SAMPLE.as_bytes();
        for i in 0..=bytes.len() {
            for j in i..=bytes.len() {
                let got = parse_chunks(&[&bytes[..i], &bytes[i..j], &bytes[j..]]);
                assert_eq!(got, expected(), "split at {i}/{j}");
            }
        }
    }

    #[test]
    fn parses_byte_by_byte() {
        let chunks: Vec<&[u8]> = SAMPLE.as_bytes().chunks(1).collect();
        assert_eq!(parse_chunks(&chunks), expected());
    }

    #[test]
    fn flushes_unterminated_final_event() {
        assert_eq!(
            parse_chunks(&[b"data: tail"]),
            vec![SseEvent {
                event: String::new(),
                data: "tail".into(),
                id: None
            }]
        );
    }

    #[tokio::test]
    async fn stream_adapter_yields_events() {
        let chunks: Vec<Result<Bytes, ()>> = vec![
            Ok(Bytes::from_static(b"data: a\n")),
            Ok(Bytes::from_static(b"\ndata: b\n\n")),
        ];
        let events: Vec<_> = sse_events(futures_util::stream::iter(chunks))
            .map(|e| e.unwrap().data)
            .collect()
            .await;
        assert_eq!(events, vec!["a", "b"]);
    }
}
