//! Positions in file texts: byte offsets and LSP positions (0-based line, UTF-16 column).

/// Line starts (byte offsets) of a text, for repeated conversions.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { text, starts }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The line of a byte offset.
    pub fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset.min(self.text.len())) {
            Ok(l) => l,
            Err(l) => l - 1,
        }
    }

    /// Byte offset -> (line, UTF-16 column).
    pub fn position(&self, offset: usize) -> (u32, u32) {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.line_of(offset);
        let col = self.text[self.starts[line]..offset].encode_utf16().count();
        (line as u32, col as u32)
    }

    /// (line, UTF-16 column) -> byte offset, clamped to the line.
    pub fn offset(&self, line: u32, col: u32) -> usize {
        let Some(&start) = self.starts.get(line as usize) else {
            return self.text.len();
        };
        let end = self.line_end(line as usize);
        let mut units = 0u32;
        for (i, c) in self.text[start..end].char_indices() {
            if units >= col {
                return start + i;
            }
            units += c.len_utf16() as u32;
        }
        end
    }

    /// Byte offset of the start of `line`.
    pub fn line_start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.text.len())
    }

    /// Byte offset of the end of `line` (before its `\n` / `\r\n`).
    pub fn line_end(&self, line: usize) -> usize {
        let end = self.starts.get(line + 1).map_or(self.text.len(), |s| s - 1);
        let start = self.line_start(line);
        if end > start && self.text.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end.max(start)
        }
    }

    /// The text of `line` without its line break.
    pub fn line_text(&self, line: usize) -> &'a str {
        &self.text[self.line_start(line)..self.line_end(line)]
    }
}

/// The leading whitespace of a line.
pub fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_between_bytes_and_utf16() {
        let t = "a\r\nä😀x\nend";
        let idx = LineIndex::new(t);
        assert_eq!(idx.line_count(), 3);
        let x = t.find('x').unwrap();
        assert_eq!(idx.position(x), (1, 3));
        assert_eq!(idx.offset(1, 3), x);
        assert_eq!(idx.line_text(0), "a");
        assert_eq!(idx.line_text(2), "end");
        assert_eq!(idx.offset(9, 0), t.len());
        assert_eq!(idx.offset(0, 99), 1);
    }
}
