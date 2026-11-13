//! Offsets: the editor (CodeMirror) counts UTF-16 code units, Rust slices bytes.

/// Byte offset of the UTF-16 offset `units` in `text` (clamped to the end, never inside a
/// character).
pub fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut n = 0;
    for (i, c) in text.char_indices() {
        if n >= units {
            return i;
        }
        n += c.len_utf16();
    }
    text.len()
}

/// UTF-16 offset of the byte offset `byte` in `text`.
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

/// Start byte of each line (the first is 0).
pub fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    v
}

/// 0-based line of byte offset `byte`.
pub fn line_of(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].matches('\n').count()
}

/// Leading whitespace of `line`.
pub fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// Width of an indentation (a tab counts as 4).
pub fn indent_width(indent: &str) -> usize {
    indent.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// The code of the first fenced block of a Markdown reply, or the reply itself (trimmed of
/// blank lines) when it has no fence. An unclosed fence runs to the end.
pub fn extract_code(reply: &str) -> String {
    let reply = reply.replace("\r\n", "\n");
    let lines: Vec<&str> = reply.split('\n').collect();
    if let Some(start) = lines.iter().position(|l| l.trim_start().starts_with("```")) {
        let fence_indent = indent_of(lines[start]).len();
        let body: Vec<&str> = lines[start + 1..]
            .iter()
            .take_while(|l| !l.trim().starts_with("```"))
            .map(|l| {
                let strip = indent_of(l).len().min(fence_indent);
                &l[strip..]
            })
            .collect();
        return body.join("\n");
    }
    trim_blank_lines(&reply).to_string()
}

/// `text` without leading and trailing blank lines (indentation of the first line kept).
pub fn trim_blank_lines(text: &str) -> &str {
    let start = text
        .char_indices()
        .take_while(|(_, c)| c.is_whitespace())
        .filter(|(_, c)| *c == '\n')
        .map(|(i, _)| i + 1)
        .last()
        .unwrap_or(0);
    text[start..].trim_end()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_offsets() {
        let t = "a😀b";
        assert_eq!(utf16_to_byte(t, 1), 1);
        assert_eq!(utf16_to_byte(t, 3), 5);
        assert_eq!(utf16_to_byte(t, 99), t.len());
        assert_eq!(byte_to_utf16(t, 5), 3);
    }

    #[test]
    fn extracts_fenced_code() {
        assert_eq!(extract_code("Here:\n```php\n$a = 1;\n```\nDone"), "$a = 1;");
        assert_eq!(extract_code("```\nx\ny"), "x\ny");
        assert_eq!(extract_code("\n\n  return 1;\n\n"), "  return 1;");
        assert_eq!(extract_code("  ```ts\n  a\n    b\n  ```"), "a\n  b");
    }

    #[test]
    fn lines_and_indents() {
        let t = "a\n  b\nc";
        assert_eq!(line_starts(t), [0, 2, 6]);
        assert_eq!(line_of(t, 4), 1);
        assert_eq!(indent_of("\t x"), "\t ");
        assert_eq!(indent_width("\t "), 5);
    }
}
