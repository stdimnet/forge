//! Offsets for the UI: JavaScript strings and CodeMirror count UTF-16 code units, Rust
//! counts bytes.

/// UTF-16 offset of byte offset `byte` in `text` (clamped to the text).
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if i >= byte {
            break;
        }
        units += c.len_utf16();
    }
    units
}

/// Byte offset of UTF-16 offset `units` in `text` (clamped, never inside a character).
pub fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if seen >= units {
            return i;
        }
        seen += c.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_both_ways() {
        let s = "aé😀b";
        assert_eq!(byte_to_utf16(s, 0), 0);
        assert_eq!(byte_to_utf16(s, 3), 2);
        assert_eq!(byte_to_utf16(s, 7), 4);
        assert_eq!(byte_to_utf16(s, 99), 5);
        assert_eq!(utf16_to_byte(s, 2), 3);
        assert_eq!(utf16_to_byte(s, 4), 7);
        assert_eq!(utf16_to_byte(s, 99), s.len());
    }
}
