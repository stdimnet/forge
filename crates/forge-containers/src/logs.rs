//! Container log streams. Without a TTY the Engine multiplexes stdout and stderr into
//! frames (`[stream, 0, 0, 0, size: u32 BE]` + payload); with a TTY (and for Kubernetes) the
//! body is the raw text. [`LogDecoder`] turns body chunks into text per stream, keeping
//! incomplete frames and UTF-8 sequences for the next chunk.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogStream {
    Stdout,
    Stderr,
}

impl LogStream {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogChunk {
    pub stream: LogStream,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct LogDecoder {
    /// `None` until the first bytes show whether the stream is multiplexed.
    multiplexed: Option<bool>,
    buf: Vec<u8>,
    /// Incomplete UTF-8 at the end of the previous chunk, per stream.
    pending: [Vec<u8>; 2],
}

impl LogDecoder {
    /// `tty`: the container has a TTY, so the body is raw text. Unknown (`None`): detect from
    /// the first header.
    pub fn new(tty: Option<bool>) -> Self {
        LogDecoder {
            multiplexed: tty.map(|t| !t),
            ..Default::default()
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<LogChunk> {
        self.buf.extend_from_slice(bytes);
        if self.multiplexed.is_none() {
            if self.buf.len() < 8 {
                return Vec::new();
            }
            let b = &self.buf;
            self.multiplexed = Some(matches!(b[0], 0..=2) && b[1] == 0 && b[2] == 0 && b[3] == 0);
        }
        let mut out = Vec::new();
        if self.multiplexed == Some(false) {
            let bytes = std::mem::take(&mut self.buf);
            self.text(LogStream::Stdout, &bytes, &mut out);
            return out;
        }
        let mut pos = 0;
        while self.buf.len() - pos >= 8 {
            let h = &self.buf[pos..pos + 8];
            let size = u32::from_be_bytes([h[4], h[5], h[6], h[7]]) as usize;
            if self.buf.len() - pos - 8 < size {
                break;
            }
            let stream = if h[0] == 2 {
                LogStream::Stderr
            } else {
                LogStream::Stdout
            };
            let payload = self.buf[pos + 8..pos + 8 + size].to_vec();
            self.text(stream, &payload, &mut out);
            pos += 8 + size;
        }
        self.buf.drain(..pos);
        out
    }

    /// Flush what is left (end of stream).
    pub fn finish(&mut self) -> Vec<LogChunk> {
        let mut out = Vec::new();
        if !self.buf.is_empty() {
            let rest = std::mem::take(&mut self.buf);
            self.text(LogStream::Stdout, &rest, &mut out);
        }
        for (i, stream) in [LogStream::Stdout, LogStream::Stderr]
            .into_iter()
            .enumerate()
        {
            let rest = std::mem::take(&mut self.pending[i]);
            if !rest.is_empty() {
                push_chunk(
                    &mut out,
                    stream,
                    String::from_utf8_lossy(&rest).into_owned(),
                );
            }
        }
        out
    }

    fn text(&mut self, stream: LogStream, bytes: &[u8], out: &mut Vec<LogChunk>) {
        let idx = stream as usize;
        let mut data = std::mem::take(&mut self.pending[idx]);
        data.extend_from_slice(bytes);
        let valid_up_to = match std::str::from_utf8(&data) {
            Ok(_) => data.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => data.len(),
        };
        let rest = data.split_off(valid_up_to);
        self.pending[idx] = rest;
        if !data.is_empty() {
            push_chunk(out, stream, String::from_utf8_lossy(&data).into_owned());
        }
    }
}

fn push_chunk(out: &mut Vec<LogChunk>, stream: LogStream, text: String) {
    match out.last_mut() {
        Some(last) if last.stream == stream => last.text.push_str(&text),
        _ => out.push(LogChunk { stream, text }),
    }
}

/// Split `2026-10-20T09:12:01.123456789Z message` (logs with `timestamps=1`) into the time
/// and the message.
pub fn split_timestamp(line: &str) -> (Option<&str>, &str) {
    match line.split_once(' ') {
        Some((ts, rest)) if crate::time::parse_rfc3339(ts).is_some() => (Some(ts), rest),
        _ => (None, line),
    }
}

/// Docker's `/containers/{id}/logs` query.
pub fn docker_logs_query(tail: u32, follow: bool, timestamps: bool) -> String {
    format!(
        "stdout=1&stderr=1&tail={tail}{}{}",
        if follow { "&follow=1" } else { "" },
        if timestamps { "&timestamps=1" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(stream: u8, text: &[u8]) -> Vec<u8> {
        let mut f = vec![stream, 0, 0, 0];
        f.extend_from_slice(&(text.len() as u32).to_be_bytes());
        f.extend_from_slice(text);
        f
    }

    #[test]
    fn demultiplexes_frames_across_chunks() {
        let mut bytes = frame(1, b"Starting server\n");
        bytes.extend(frame(2, b"warning: x\n"));
        bytes.extend(frame(1, "caf\u{e9} ok\n".as_bytes()));
        let mut d = LogDecoder::new(None);
        let mut out = Vec::new();
        // Feed in awkward pieces: inside a header, inside a payload, inside a UTF-8 char.
        for piece in bytes.chunks(5) {
            out.extend(d.push(piece));
        }
        out.extend(d.finish());
        let stdout: String = out
            .iter()
            .filter(|c| c.stream == LogStream::Stdout)
            .map(|c| c.text.as_str())
            .collect();
        let stderr: String = out
            .iter()
            .filter(|c| c.stream == LogStream::Stderr)
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(stdout, "Starting server\ncaf\u{e9} ok\n");
        assert_eq!(stderr, "warning: x\n");
    }

    #[test]
    fn raw_tty_stream() {
        let mut d = LogDecoder::new(None);
        let out = d.push(b"plain log line one\nline two\n");
        assert_eq!(
            out,
            vec![LogChunk {
                stream: LogStream::Stdout,
                text: "plain log line one\nline two\n".into()
            }]
        );
        let mut d = LogDecoder::new(Some(true));
        assert_eq!(d.push(b"hi")[0].text, "hi");
    }

    #[test]
    fn timestamps() {
        assert_eq!(
            split_timestamp("2026-10-20T09:12:01.123456789Z GET / 200"),
            (Some("2026-10-20T09:12:01.123456789Z"), "GET / 200")
        );
        assert_eq!(split_timestamp("no time here"), (None, "no time here"));
        assert_eq!(
            docker_logs_query(100, true, false),
            "stdout=1&stderr=1&tail=100&follow=1"
        );
    }
}
