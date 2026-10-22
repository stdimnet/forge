//! Offsets: the engine works on byte offsets; the UI and IPC use UTF-16 offsets
//! (CodeMirror positions).

/// Byte offset of the UTF-16 offset `utf16` in `text` (clamped to the end).
pub fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, ch) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// Converts many byte offsets of one text to UTF-16 offsets.
pub struct Utf16 {
    /// `(byte, utf16)` at the start of each non-ASCII char and after it.
    marks: Vec<(usize, usize)>,
    ascii: bool,
}

impl Utf16 {
    pub fn new(text: &str) -> Utf16 {
        if text.is_ascii() {
            return Utf16 {
                marks: Vec::new(),
                ascii: true,
            };
        }
        let mut marks = Vec::new();
        let mut units = 0;
        for (i, ch) in text.char_indices() {
            if !ch.is_ascii() {
                marks.push((i, units));
                marks.push((i + ch.len_utf8(), units + ch.len_utf16()));
            }
            units += ch.len_utf16();
        }
        Utf16 {
            marks,
            ascii: false,
        }
    }

    pub fn of(&self, byte: usize) -> usize {
        if self.ascii {
            return byte;
        }
        // The last mark at or before `byte`; ASCII bytes after it count one unit each.
        match self.marks.partition_point(|&(b, _)| b <= byte) {
            0 => byte,
            i => {
                let (b, u) = self.marks[i - 1];
                u + (byte - b)
            }
        }
    }
}

/// 0-based line and UTF-16 column of a byte offset.
pub fn line_col(text: &str, byte: usize) -> (usize, usize) {
    let byte = byte.min(text.len());
    let before = &text[..floor_char(text, byte)];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    let col = before[start..].encode_utf16().count();
    (line, col)
}

pub fn floor_char(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_between_utf16_and_bytes() {
        let t = "aé😀b";
        assert_eq!(byte_offset(t, 0), 0);
        assert_eq!(byte_offset(t, 2), 3);
        assert_eq!(byte_offset(t, 4), 7);
        assert_eq!(byte_offset(t, 99), t.len());
        let u = Utf16::new(t);
        assert_eq!(u.of(0), 0);
        assert_eq!(u.of(3), 2);
        assert_eq!(u.of(7), 4);
        assert_eq!(u.of(8), 5);
        assert_eq!(Utf16::new("abc").of(2), 2);
        assert_eq!(line_col("a\nbé c", 6), (1, 3));
    }
}
