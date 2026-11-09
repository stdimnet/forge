//! Offsets and indentation helpers.

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

/// UTF-16 length of a string.
pub(crate) fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
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

/// Remove the common leading whitespace of the non-blank lines; blank lines become empty.
/// Counted in characters, so indentation with multi-byte spaces (NBSP) is not cut inside one.
pub(crate) fn dedent(lines: &[&str]) -> Vec<String> {
    let lead = |l: &str| l.chars().take_while(|c| c.is_whitespace()).count();
    let common = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| lead(l))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                let at = l.char_indices().nth(common).map_or(l.len(), |(i, _)| i);
                l[at..].trim_end().to_string()
            }
        })
        .collect()
}

/// Template indentation (tabs or groups of four spaces) in the file's indent unit.
pub(crate) fn normalize_indent(template: &str, unit: &str) -> String {
    template
        .split('\n')
        .map(|line| {
            let rest = line.trim_start_matches([' ', '\t']);
            let lead = &line[..line.len() - rest.len()];
            let (mut levels, mut spaces) = (0, 0);
            for c in lead.chars() {
                if c == '\t' {
                    levels += 1;
                    spaces = 0;
                } else {
                    spaces += 1;
                    if spaces == 4 {
                        levels += 1;
                        spaces = 0;
                    }
                }
            }
            format!("{}{}{rest}", unit.repeat(levels), " ".repeat(spaces))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedent_counts_characters() {
        assert_eq!(dedent(&["\u{a0}a", " b", ""]), ["a", "b", ""]);
        assert_eq!(dedent(&["    x", "      y"]), ["x", "  y"]);
    }

    #[test]
    fn converts_utf16_offsets() {
        let t = "aé😀b";
        assert_eq!(byte_of(t, 0), 0);
        assert_eq!(byte_of(t, 2), 3);
        assert_eq!(byte_of(t, 4), 7);
        assert_eq!(byte_of(t, 99), t.len());
        assert_eq!(utf16_of(t, 7), 4);
        assert_eq!(utf16_of(t, 5), 2);
        assert_eq!(utf16_len(t), 5);
    }

    #[test]
    fn finds_lines_and_indents() {
        let t = "a\n    bc\nd";
        assert_eq!(line_start(t, 6), 2);
        assert_eq!(line_end(t, 6), 8);
        assert_eq!(line_indent(t, 7), "    ");
        assert_eq!(dedent(&["    a", "", "      b  "]), ["a", "", "  b"]);
    }

    #[test]
    fn normalizes_template_indentation() {
        assert_eq!(
            normalize_indent("a\n    b\n\t\tc\n      d", "\t"),
            "a\n\tb\n\t\tc\n\t  d"
        );
        assert_eq!(normalize_indent("x\n    y", "  "), "x\n  y");
    }
}
