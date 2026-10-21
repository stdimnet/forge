//! Offset helpers: the UI speaks UTF-16 (CodeMirror), Rust works on bytes.

/// Byte offset of a UTF-16 offset (clamped to the text).
pub(crate) fn byte_of(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= utf16 {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// UTF-16 offset of a byte offset (rounded down to a character boundary).
pub(crate) fn utf16_of(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].encode_utf16().count()
}

/// Start of the line that contains `byte`.
pub(crate) fn line_start(text: &str, byte: usize) -> usize {
    text[..byte].rfind('\n').map_or(0, |i| i + 1)
}

/// End of the line that contains `byte` (before its line break).
pub(crate) fn line_end(text: &str, byte: usize) -> usize {
    text[byte..].find('\n').map_or(text.len(), |i| byte + i)
}

/// Leading whitespace of the line that contains `byte`.
pub(crate) fn line_indent(text: &str, byte: usize) -> &str {
    let start = line_start(text, byte);
    let line = &text[start..line_end(text, start)];
    &line[..line.len() - line.trim_start().len()]
}

/// Remove the common leading whitespace of the non-blank lines and trailing whitespace;
/// leading and trailing blank lines are dropped.
pub(crate) fn dedent(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let common = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                l[common.min(l.len())..].trim_end().to_string()
            }
        })
        .collect();
    while out.first().is_some_and(|l| l.is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_offsets() {
        let t = "aé😀b";
        assert_eq!(byte_of(t, 2), 3);
        assert_eq!(byte_of(t, 4), 7);
        assert_eq!(utf16_of(t, 7), 4);
        assert_eq!(utf16_of(t, 100), 5);
    }

    #[test]
    fn lines() {
        let t = "a\n  bc\nd";
        assert_eq!(line_start(t, 4), 2);
        assert_eq!(line_end(t, 4), 6);
        assert_eq!(line_indent(t, 5), "  ");
        assert_eq!(dedent("\n    a\n      b\n\n"), vec!["a", "  b"]);
    }
}
