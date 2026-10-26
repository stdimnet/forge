//! The Python Console's protocol (`helpers/forge_console.py`): the console process writes
//! its replies as JSON lines that start with the record separator (`\x1e`) on stdout,
//! between the user's own output. [`Splitter`] separates the two in a stream of output
//! chunks.

use serde::{Deserialize, Serialize};
use serde_json::Value;

const RS: char = '\u{1e}';

/// A piece of console output: text for the console, or a reply of the helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Piece {
    Text { text: String },
    Message { message: Value },
}

/// Splits output chunks into text and replies; a reply may arrive in several chunks.
#[derive(Debug, Default)]
pub struct Splitter {
    /// A reply that started but has no line end yet.
    pending: Option<String>,
}

impl Splitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, chunk: &str) -> Vec<Piece> {
        let mut out = Vec::new();
        let mut rest = chunk;
        let mut text = String::new();
        loop {
            if let Some(p) = self.pending.as_mut() {
                match rest.find('\n') {
                    Some(i) => {
                        p.push_str(&rest[..i]);
                        let raw = self.pending.take().unwrap_or_default();
                        match serde_json::from_str(&raw) {
                            Ok(message) => out.push(Piece::Message { message }),
                            Err(_) => text.push_str(&format!("{raw}\n")),
                        }
                        rest = &rest[i + 1..];
                    }
                    None => {
                        p.push_str(rest);
                        break;
                    }
                }
                continue;
            }
            match rest.find(RS) {
                Some(i) => {
                    text.push_str(&rest[..i]);
                    if !text.is_empty() {
                        out.push(Piece::Text {
                            text: std::mem::take(&mut text),
                        });
                    }
                    self.pending = Some(String::new());
                    rest = &rest[i + RS.len_utf8()..];
                }
                None => {
                    text.push_str(rest);
                    break;
                }
            }
        }
        if !text.is_empty() {
            out.push(Piece::Text { text });
        }
        out
    }
}

/// A request line for the console's stdin.
pub fn request(op: &str, fields: Value) -> String {
    let mut obj = match fields {
        Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    obj.insert("op".into(), Value::String(op.into()));
    format!("{}\n", Value::Object(obj))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn separates_replies_from_output() {
        let mut s = Splitter::new();
        let a = s.feed("hello\n\u{1e}{\"type\":\"do");
        assert_eq!(
            a,
            [Piece::Text {
                text: "hello\n".into()
            }]
        );
        let b = s.feed("ne\"}\nworld");
        assert_eq!(
            b,
            [
                Piece::Message {
                    message: json!({"type": "done"})
                },
                Piece::Text {
                    text: "world".into()
                }
            ]
        );
        let c = s.feed("no newline\u{1e}{\"type\":\"vars\",\"vars\":[]}\n\u{1e}broken\n");
        assert_eq!(
            c[0],
            Piece::Text {
                text: "no newline".into()
            }
        );
        assert_eq!(
            c[1],
            Piece::Message {
                message: json!({"type": "vars", "vars": []})
            }
        );
        assert_eq!(
            c[2],
            Piece::Text {
                text: "broken\n".into()
            }
        );
    }

    #[test]
    fn requests_are_json_lines() {
        assert_eq!(
            request("exec", json!({"code": "1+1"})),
            "{\"code\":\"1+1\",\"op\":\"exec\"}\n"
        );
        assert_eq!(request("vars", Value::Null), "{\"op\":\"vars\"}\n");
    }
}
