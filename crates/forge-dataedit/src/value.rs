//! The value editor of a cell: which editor fits the column (text, JSON, XML, binary,
//! date/time, enum, boolean, number), JSON and XML formatting with validation, hex dumps
//! and image detection of binary values, and date normalization.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use forge_sql::value::as_binary;

/// Editor for a column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EditorKind {
    Text,
    Json,
    Xml,
    Binary,
    Number,
    Bool,
    Date,
    DateTime,
    Time,
    /// `enum('a','b')` / `set(…)`: the allowed values.
    Enum {
        values: Vec<String>,
        multiple: bool,
    },
}

/// The editor for a column of `data_type` currently holding `value`.
pub fn editor_kind(data_type: &str, value: &Value) -> EditorKind {
    let t = data_type.trim().to_ascii_lowercase();
    let base = t.split(['(', ' ']).next().unwrap_or("");
    if base == "enum" || base == "set" {
        return EditorKind::Enum {
            values: enum_values(data_type),
            multiple: base == "set",
        };
    }
    if as_binary(value).is_some() {
        return EditorKind::Binary;
    }
    match base {
        "json" | "jsonb" => EditorKind::Json,
        "xml" | "xmltype" => EditorKind::Xml,
        "bool" | "boolean" => EditorKind::Bool,
        "tinyint" if t.starts_with("tinyint(1)") => EditorKind::Bool,
        "date" => EditorKind::Date,
        "datetime" | "datetime2" | "timestamp" | "timestamptz" | "smalldatetime"
        | "datetimeoffset" => EditorKind::DateTime,
        "time" | "timetz" => EditorKind::Time,
        "blob" | "tinyblob" | "mediumblob" | "longblob" | "binary" | "varbinary" | "bytea"
        | "image" | "raw" => EditorKind::Binary,
        "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "bigint" | "int2" | "int4"
        | "int8" | "decimal" | "numeric" | "float" | "double" | "real" | "float4" | "float8"
        | "money" | "number" | "serial" | "bigserial" => EditorKind::Number,
        _ => {
            // Text that holds JSON or XML (common in TEXT columns) gets the matching editor.
            match value.as_str().map(str::trim_start) {
                Some(s) if (s.starts_with('{') || s.starts_with('['))
                    && serde_json::from_str::<serde::de::IgnoredAny>(s).is_ok() =>
                {
                    EditorKind::Json
                }
                Some(s) if s.starts_with("<?xml") => EditorKind::Xml,
                _ => EditorKind::Text,
            }
        }
    }
}

/// Values of `enum('a','b''c')` or `set(…)`.
pub fn enum_values(data_type: &str) -> Vec<String> {
    let Some(open) = data_type.find('(') else {
        return Vec::new();
    };
    let body = &data_type[open + 1..data_type.rfind(')').unwrap_or(data_type.len())];
    let mut out = Vec::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            let mut v = String::new();
            while let Some(c) = chars.next() {
                if c == '\'' {
                    if chars.peek() == Some(&'\'') {
                        chars.next();
                        v.push('\'');
                    } else {
                        break;
                    }
                } else {
                    v.push(c);
                }
            }
            out.push(v);
        }
    }
    out
}

/// Where a document is invalid (1-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

/// A formatted (or minified) document, or why it is invalid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Formatted {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ValueError>,
}

/// Validate JSON.
pub fn check_json(text: &str) -> Option<ValueError> {
    serde_json::from_str::<serde::de::IgnoredAny>(text)
        .err()
        .map(|e| ValueError {
            message: e.to_string().split(" at line").next().unwrap_or("").to_string(),
            line: e.line(),
            column: e.column(),
        })
}

/// Pretty-print JSON with `indent` spaces (0: minify), keeping the key order and the exact
/// number and string spellings.
pub fn format_json(text: &str, indent: usize) -> Formatted {
    if let Some(e) = check_json(text) {
        return Formatted {
            text: None,
            error: Some(e),
        };
    }
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut depth = 0usize;
    let mut chars = text.chars().peekable();
    let newline = |out: &mut String, depth: usize| {
        if indent > 0 {
            out.push('\n');
            out.push_str(&" ".repeat(depth * indent));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                while let Some(c) = chars.next() {
                    out.push(c);
                    if c == '\\' {
                        if let Some(n) = chars.next() {
                            out.push(n);
                        }
                    } else if c == '"' {
                        break;
                    }
                }
            }
            '{' | '[' => {
                // Empty containers stay on one line.
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                let close = if c == '{' { '}' } else { ']' };
                if chars.peek() == Some(&close) {
                    chars.next();
                    out.push(c);
                    out.push(close);
                } else {
                    out.push(c);
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline(&mut out, depth);
                out.push(c);
            }
            ',' => {
                out.push(',');
                newline(&mut out, depth);
            }
            ':' => {
                out.push(':');
                if indent > 0 {
                    out.push(' ');
                }
            }
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    Formatted {
        text: Some(out),
        error: None,
    }
}

/// Indent XML (`indent` spaces per level) after checking that tags are well nested.
pub fn format_xml(text: &str, indent: usize) -> Formatted {
    let tokens = match xml_tokens(text) {
        Ok(t) => t,
        Err(e) => {
            return Formatted {
                text: None,
                error: Some(e),
            }
        }
    };
    let mut out = String::new();
    let mut depth = 0usize;
    let pad = |d: usize| " ".repeat(d * indent);
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        match t.kind {
            XmlTok::Open => {
                // `<a>text</a>` stays on one line.
                if let (Some(txt), Some(close)) = (tokens.get(i + 1), tokens.get(i + 2)) {
                    if txt.kind == XmlTok::Text && close.kind == XmlTok::Close {
                        out.push_str(&format!("{}{}{}{}\n", pad(depth), t.text, txt.text.trim(), close.text));
                        i += 3;
                        continue;
                    }
                }
                out.push_str(&format!("{}{}\n", pad(depth), t.text));
                depth += 1;
            }
            XmlTok::Close => {
                depth = depth.saturating_sub(1);
                out.push_str(&format!("{}{}\n", pad(depth), t.text));
            }
            XmlTok::Text => {
                let s = t.text.trim();
                if !s.is_empty() {
                    out.push_str(&format!("{}{s}\n", pad(depth)));
                }
            }
            XmlTok::Other => out.push_str(&format!("{}{}\n", pad(depth), t.text)),
        }
        i += 1;
    }
    Formatted {
        text: Some(out.trim_end().to_string()),
        error: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum XmlTok {
    Open,
    Close,
    /// Self-closing tags, declarations, comments, CDATA.
    Other,
    Text,
}

struct Tok {
    kind: XmlTok,
    text: String,
}

fn xml_tokens(text: &str) -> Result<Vec<Tok>, ValueError> {
    let pos = |i: usize| {
        let before = &text[..i];
        ValueError {
            message: String::new(),
            line: before.matches('\n').count() + 1,
            column: before.rfind('\n').map(|p| i - p).unwrap_or(i + 1),
        }
    };
    let err = |i: usize, m: String| ValueError {
        message: m,
        ..pos(i)
    };
    let mut out = Vec::new();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if !rest.starts_with('<') {
            let end = rest.find('<').map(|p| i + p).unwrap_or(text.len());
            out.push(Tok {
                kind: XmlTok::Text,
                text: text[i..end].to_string(),
            });
            i = end;
            continue;
        }
        let (end_pat, kind) = if rest.starts_with("<!--") {
            ("-->", XmlTok::Other)
        } else if rest.starts_with("<![CDATA[") {
            ("]]>", XmlTok::Other)
        } else if rest.starts_with("<?") {
            ("?>", XmlTok::Other)
        } else if rest.starts_with("<!") {
            (">", XmlTok::Other)
        } else if rest.starts_with("</") {
            (">", XmlTok::Close)
        } else {
            (">", XmlTok::Open)
        };
        let Some(e) = rest.find(end_pat) else {
            return Err(err(i, "Unclosed tag".into()));
        };
        let tag = &rest[..e + end_pat.len()];
        let name = |t: &str| {
            t.trim_start_matches(['<', '/'])
                .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .next()
                .unwrap_or("")
                .to_string()
        };
        let kind = match kind {
            XmlTok::Open if tag.ends_with("/>") => XmlTok::Other,
            XmlTok::Open => {
                let n = name(tag);
                if n.is_empty() {
                    return Err(err(i, "Missing tag name".into()));
                }
                stack.push((n, i));
                XmlTok::Open
            }
            XmlTok::Close => {
                let n = name(tag);
                match stack.pop() {
                    Some((open, _)) if open == n => {}
                    Some((open, _)) => {
                        return Err(err(i, format!("Expected </{open}>, found </{n}>")))
                    }
                    None => return Err(err(i, format!("Unexpected </{n}>"))),
                }
                XmlTok::Close
            }
            k => k,
        };
        out.push(Tok {
            kind,
            text: tag.to_string(),
        });
        i += e + end_pat.len();
    }
    if let Some((open, at)) = stack.pop() {
        return Err(err(at, format!("<{open}> is not closed")));
    }
    Ok(out)
}

/// One line of a hex dump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HexLine {
    pub offset: String,
    pub hex: String,
    pub ascii: String,
}

/// What the value editor shows for a binary value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryInfo {
    pub size: usize,
    /// `image/png`, … when the bytes are an image the UI can preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    pub lines: Vec<HexLine>,
    /// Lines left out after `max_bytes`.
    pub truncated: bool,
    /// The bytes as UTF-8 text, when they are.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Image type of `bytes` by their signature.
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"BM") && bytes.len() > 26 {
        Some("image/bmp")
    } else {
        None
    }
}

/// Hex dump (16 bytes per line, at most `max_bytes`), image type and text of a binary
/// value (`{ "$hex": … }`).
pub fn binary_info(value: &Value, max_bytes: usize) -> Option<BinaryInfo> {
    let bytes = as_binary(value)?;
    let shown = &bytes[..bytes.len().min(max_bytes)];
    let lines = shown
        .chunks(16)
        .enumerate()
        .map(|(i, chunk)| HexLine {
            offset: format!("{:08X}", i * 16),
            hex: chunk
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
            ascii: chunk
                .iter()
                .map(|&b| {
                    if (0x20..0x7f).contains(&b) {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect(),
        })
        .collect();
    let text = std::str::from_utf8(&bytes)
        .ok()
        .filter(|s| !s.chars().any(|c| c.is_control() && !c.is_whitespace()))
        .map(str::to_string);
    Some(BinaryInfo {
        size: bytes.len(),
        mime: image_mime(&bytes).map(str::to_string),
        lines,
        truncated: bytes.len() > max_bytes,
        text,
    })
}

/// Normalize a date, date-time or time typed in the value editor (`2025-1-6 9:05` →
/// `2025-01-06 09:05:00`), or `None` when it is not one. Fractions and zones are kept.
pub fn normalize_temporal(kind: &EditorKind, input: &str) -> Option<String> {
    let s = input.trim().replace('T', " ");
    let (date, time) = match kind {
        EditorKind::Date => (Some(s.as_str()), None),
        EditorKind::Time => (None, Some(s.as_str())),
        _ => match s.split_once(' ') {
            Some((d, t)) => (Some(d), Some(t)),
            None => (Some(s.as_str()), Some("00:00:00")),
        },
    };
    let mut parts = Vec::new();
    if let Some(d) = date {
        let f: Vec<&str> = d.split('-').collect();
        let [y, m, dd] = f.as_slice() else {
            return None;
        };
        let (y, m, dd): (u32, u32, u32) = (y.parse().ok()?, m.parse().ok()?, dd.parse().ok()?);
        let days = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
            2 => 28,
            _ => return None,
        };
        if !(1..=days).contains(&dd) || y > 9999 {
            return None;
        }
        parts.push(format!("{y:04}-{m:02}-{dd:02}"));
    }
    if let Some(t) = time {
        // Keep a zone suffix (`+00`, `Z`) and fractions as typed.
        let split = t.find(['+', 'Z']).unwrap_or(t.len());
        let (clock, zone) = t.split_at(split);
        let (clock, frac) = clock.split_once('.').unwrap_or((clock, ""));
        let f: Vec<&str> = clock.trim().split(':').collect();
        let nums: Vec<u32> = f.iter().map(|x| x.parse().ok()).collect::<Option<_>>()?;
        let (h, mi, se) = match nums.as_slice() {
            [h, m] => (*h, *m, 0),
            [h, m, s] => (*h, *m, *s),
            _ => return None,
        };
        if h > 23 || mi > 59 || se > 59 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let frac = if frac.is_empty() {
            String::new()
        } else {
            format!(".{frac}")
        };
        parts.push(format!("{h:02}:{mi:02}:{se:02}{frac}{zone}"));
    }
    Some(parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn picks_editors() {
        assert_eq!(editor_kind("json", &json!("{}")), EditorKind::Json);
        assert_eq!(editor_kind("text", &json!(" {\"a\": 1}")), EditorKind::Json);
        assert_eq!(editor_kind("text", &json!("{not json")), EditorKind::Text);
        assert_eq!(editor_kind("timestamp", &Value::Null), EditorKind::DateTime);
        assert_eq!(editor_kind("tinyint(1)", &json!(true)), EditorKind::Bool);
        assert_eq!(editor_kind("varchar(10)", &json!({"$hex": "00"})), EditorKind::Binary);
        assert_eq!(
            editor_kind("enum('pending','it''s')", &json!("pending")),
            EditorKind::Enum {
                values: vec!["pending".into(), "it's".into()],
                multiple: false
            }
        );
        assert_eq!(editor_kind("decimal(10,2)", &json!("1.00")), EditorKind::Number);
    }

    #[test]
    fn formats_json_keeping_order() {
        let f = format_json(r#"{"z":1,"a":[1, 2.50,{}],"s":"x: {y}, \"q\""}"#, 2);
        assert_eq!(
            f.text.unwrap(),
            "{\n  \"z\": 1,\n  \"a\": [\n    1,\n    2.50,\n    {}\n  ],\n  \"s\": \"x: {y}, \\\"q\\\"\"\n}"
        );
        assert_eq!(
            format_json("{\n \"a\" : [ ] }", 0).text.unwrap(),
            "{\"a\":[]}"
        );
        let e = format_json("{\"a\": 1,\n \"b\": }", 2).error.unwrap();
        assert_eq!((e.line, e.column), (2, 7));
        assert!(e.message.contains("expected value"), "{}", e.message);
    }

    #[test]
    fn formats_and_checks_xml() {
        let f = format_xml("<?xml version=\"1.0\"?><a><b x=\"1\">t</b><c/><!-- n --></a>", 2);
        assert_eq!(
            f.text.unwrap(),
            "<?xml version=\"1.0\"?>\n<a>\n  <b x=\"1\">t</b>\n  <c/>\n  <!-- n -->\n</a>"
        );
        let e = format_xml("<a>\n<b></a>", 2).error.unwrap();
        assert_eq!(e.line, 2);
        assert!(e.message.contains("Expected </b>"));
        assert!(format_xml("<a>", 2).error.unwrap().message.contains("not closed"));
    }

    #[test]
    fn binary_values() {
        let png = json!({"$hex": "89504e470d0a1a0a0000000d494844520000"});
        let b = binary_info(&png, 16).unwrap();
        assert_eq!(b.size, 18);
        assert_eq!(b.mime.as_deref(), Some("image/png"));
        assert_eq!(b.lines.len(), 1);
        assert!(b.truncated);
        assert_eq!(b.lines[0].ascii, ".PNG........IHDR");
        assert_eq!(b.text, None);
        let t = binary_info(&json!({"$hex": "68690a"}), 64).unwrap();
        assert_eq!(t.text.as_deref(), Some("hi\n"));
        assert!(binary_info(&json!("x"), 4).is_none());
    }

    #[test]
    fn normalizes_dates() {
        let dt = EditorKind::DateTime;
        assert_eq!(normalize_temporal(&dt, "2025-1-6 9:05").unwrap(), "2025-01-06 09:05:00");
        assert_eq!(
            normalize_temporal(&dt, "2025-01-06T10:00:00.123+00").unwrap(),
            "2025-01-06 10:00:00.123+00"
        );
        assert_eq!(normalize_temporal(&dt, "2024-02-29").unwrap(), "2024-02-29 00:00:00");
        assert!(normalize_temporal(&EditorKind::Date, "2025-02-29").is_none());
        assert!(normalize_temporal(&EditorKind::Time, "24:00").is_none());
        assert_eq!(normalize_temporal(&EditorKind::Time, "7:30").unwrap(), "07:30:00");
    }
}
