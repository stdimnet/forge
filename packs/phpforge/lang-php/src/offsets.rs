//! Offset conversion. The index works in UTF-8 byte offsets; the UI (CodeMirror / JS
//! strings) uses UTF-16 code units. Convert at the IPC boundary.

/// UTF-16 offset -> byte offset (clamped to the text, snapped to a char boundary).
pub fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= utf16 {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// Byte offset -> UTF-16 offset.
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    let mut b = byte;
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    text[..b].chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_ascii_and_multibyte() {
        let s = "a€b😀c";
        assert_eq!(utf16_to_byte(s, 0), 0);
        assert_eq!(utf16_to_byte(s, 2), 4); // after €
        assert_eq!(utf16_to_byte(s, 5), 9); // after 😀 (2 units)
        assert_eq!(byte_to_utf16(s, 9), 5);
        assert_eq!(byte_to_utf16(s, 100), 6);
        assert_eq!(utf16_to_byte(s, 100), s.len());
        for b in [0, 1, 4, 5, 9, 10] {
            assert_eq!(utf16_to_byte(s, byte_to_utf16(s, b)), b);
        }
    }
}
