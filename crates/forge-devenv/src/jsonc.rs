//! JSON with comments (JSONC), as `devcontainer.json` is written: `//` and `/* */`
//! comments and trailing commas are removed before the text goes to `serde_json`.
//! Positions stay the same (comments become spaces), so error lines match the file.

/// `text` without comments and trailing commas; line breaks are kept.
pub fn strip(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            out.push(b);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                in_string = true;
                out.push(b);
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    out.push(b' ');
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                out.extend_from_slice(b"  ");
                i += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    out.push(if bytes[i] == b'\n' { b'\n' } else { b' ' });
                    i += 1;
                }
                if i < bytes.len() {
                    out.extend_from_slice(b"  ");
                    i += 2;
                }
            }
            b']' | b'}' => {
                // Drop a trailing comma before the closing bracket.
                if let Some(pos) = out.iter().rposition(|c| !c.is_ascii_whitespace()) {
                    if out[pos] == b',' {
                        out[pos] = b' ';
                    }
                }
                out.push(b);
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    // Only ASCII bytes were replaced, so the text is still valid UTF-8.
    String::from_utf8(out).unwrap_or_default()
}

/// Parse JSONC into a `serde_json::Value`.
pub fn parse(text: &str) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(&strip(text.trim_start_matches('\u{feff}')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_comments_and_trailing_commas() {
        let text = "{\n  // the image\n  \"image\": \"php:8.3\", /* inline */\n  \"forwardPorts\": [8000, 5173,],\n}\n";
        let v = parse(text).unwrap();
        assert_eq!(v["image"], "php:8.3");
        assert_eq!(v["forwardPorts"][1], 5173);
        assert_eq!(strip(text).lines().count(), text.lines().count());
    }

    #[test]
    fn keeps_comment_markers_in_strings() {
        let v = parse(r#"{"url": "http://x//y", "q": "a /* b */ \" c", "e": "\\"}"#).unwrap();
        assert_eq!(v["url"], "http://x//y");
        assert_eq!(v["q"], "a /* b */ \" c");
        assert_eq!(v["e"], "\\");
    }

    #[test]
    fn multi_line_comments_keep_lines_and_bom_is_ignored() {
        let text = "\u{feff}{\n/* one\ntwo */ \"a\": 1\n}";
        assert_eq!(parse(text).unwrap()["a"], 1);
        assert_eq!(strip(text).matches('\n').count(), 3);
    }
}
