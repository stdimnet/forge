//! Offset conversion. The parser works in UTF-8 byte offsets; the UI (CodeMirror / JS
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
    let mut b = byte.min(text.len());
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    text[..b].chars().map(char::len_utf16).sum()
}

/// Line starts of a text, for byte offset -> (line, UTF-16 column).
#[derive(Debug, Clone)]
pub struct LineIndex {
    starts: Vec<usize>,
    /// UTF-16 offset of each line start.
    starts_utf16: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        let mut starts_utf16 = vec![0];
        let mut units = 0;
        for (i, ch) in text.char_indices() {
            units += ch.len_utf16();
            if ch == '\n' {
                starts.push(i + 1);
                starts_utf16.push(units);
            }
        }
        LineIndex {
            starts,
            starts_utf16,
        }
    }

    /// UTF-16 offset, line and UTF-16 column of a byte offset.
    pub fn pos(&self, text: &str, byte: usize) -> crate::model::Pos {
        let (line, column) = self.position(text, byte);
        crate::model::Pos {
            offset: self.starts_utf16[line] + column,
            line,
            column,
        }
    }

    /// 0-based line of a byte offset.
    pub fn line(&self, byte: usize) -> usize {
        match self.starts.binary_search(&byte) {
            Ok(l) => l,
            Err(l) => l - 1,
        }
    }

    /// 0-based line and UTF-16 column of a byte offset.
    pub fn position(&self, text: &str, byte: usize) -> (usize, usize) {
        let line = self.line(byte);
        let start = self.starts[line];
        let col = byte_to_utf16(&text[start..], byte.saturating_sub(start));
        (line, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_ascii_and_multibyte() {
        let s = "a€b😀c";
        assert_eq!(utf16_to_byte(s, 2), 4);
        assert_eq!(utf16_to_byte(s, 5), 9);
        assert_eq!(byte_to_utf16(s, 9), 5);
        for b in [0, 1, 4, 5, 9, 10] {
            assert_eq!(utf16_to_byte(s, byte_to_utf16(s, b)), b);
        }
    }

    #[test]
    fn lines_and_columns() {
        let s = "ab\n€x\n";
        let li = LineIndex::new(s);
        assert_eq!(li.position(s, 0), (0, 0));
        assert_eq!(li.position(s, 3), (1, 0));
        assert_eq!(li.position(s, 6), (1, 1));
        assert_eq!(li.line(s.len()), 2);
    }
}
