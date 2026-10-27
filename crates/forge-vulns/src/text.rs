//! Positions as the editor takes them: 0-based lines and UTF-16 columns.

use serde::{Deserialize, Serialize};

/// A range on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

pub(crate) struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines { text, starts }
    }

    fn line_text(&self, line: usize) -> &'a str {
        let start = self.starts[line];
        let end = self
            .starts
            .get(line + 1)
            .map(|e| e - 1)
            .unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches('\r')
    }

    /// The span of the byte range `[start, end)` (which must not cross a line end).
    pub fn span(&self, start: usize, end: usize) -> Span {
        let line = self.starts.partition_point(|s| *s <= start) - 1;
        let base = self.starts[line];
        let col = |b: usize| {
            self.text[base..b.min(self.text.len()).max(base)]
                .encode_utf16()
                .count() as u32
        };
        Span {
            line: line as u32,
            start: col(start),
            end: col(end.max(start)),
        }
    }

    /// The byte offset of a line and UTF-16 column (clamped to the line).
    pub fn offset(&self, line: u32, col: u32) -> Option<usize> {
        let line = line as usize;
        let base = *self.starts.get(line)?;
        let text = self.line_text(line);
        let mut units = 0u32;
        for (i, c) in text.char_indices() {
            if units >= col {
                return Some(base + i);
            }
            units += c.len_utf16() as u32;
        }
        Some(base + text.len())
    }

    /// Byte range of a span.
    pub fn range(&self, span: Span) -> Option<(usize, usize)> {
        Some((
            self.offset(span.line, span.start)?,
            self.offset(span.line, span.end)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_between_bytes_and_utf16() {
        let text = "a\n  \"é\": \"x\"\r\nz";
        let l = Lines::new(text);
        let x = text.find('x').unwrap();
        let span = l.span(x, x + 1);
        assert_eq!(
            span,
            Span {
                line: 1,
                start: 8,
                end: 9
            }
        );
        assert_eq!(l.range(span), Some((x, x + 1)));
        assert_eq!(l.offset(2, 5), Some(text.len()));
    }
}
