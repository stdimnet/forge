//! The text the engine works on: a copy of the editor document as characters, with line
//! starts and conversion between character indices and the UTF-16 offsets the editor uses.

use std::cell::OnceCell;

/// Character class used by word motions and text objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    Blank,
    Word,
    Punct,
    Newline,
}

/// Class of `c`; with `big` (WORD motions) every non-blank is one class.
pub fn class_of(c: char, big: bool) -> CharClass {
    if c == '\n' {
        CharClass::Newline
    } else if c == ' ' || c == '\t' || c.is_whitespace() {
        CharClass::Blank
    } else if big || c == '_' || c.is_alphanumeric() {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

pub fn is_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

#[derive(Debug, Clone, Default)]
pub struct Buffer {
    chars: Vec<char>,
    line_starts: Vec<usize>,
    /// Characters outside the Basic Multilingual Plane (two UTF-16 units each).
    astral: usize,
    text: OnceCell<String>,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        let mut b = Self {
            chars: text.chars().collect(),
            ..Self::default()
        };
        b.reindex();
        b
    }

    fn reindex(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);
        self.astral = 0;
        for (i, c) in self.chars.iter().enumerate() {
            if *c == '\n' {
                self.line_starts.push(i + 1);
            } else if (*c as u32) > 0xFFFF {
                self.astral += 1;
            }
        }
        self.text = OnceCell::new();
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    /// The character at `i`, `None` past the end.
    pub fn at(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    pub fn text(&self) -> &str {
        self.text.get_or_init(|| self.chars.iter().collect())
    }

    pub fn slice(&self, from: usize, to: usize) -> String {
        let to = to.min(self.len());
        let from = from.min(to);
        self.chars[from..to].iter().collect()
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// 0-based line of character index `pos`.
    pub fn line_of(&self, pos: usize) -> usize {
        match self.line_starts.binary_search(&pos.min(self.len())) {
            Ok(l) => l,
            Err(l) => l - 1,
        }
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts[line.min(self.line_count() - 1)]
    }

    /// Index of the line's `\n` (or the end of the text).
    pub fn line_end(&self, line: usize) -> usize {
        let line = line.min(self.line_count() - 1);
        if line + 1 < self.line_count() {
            self.line_starts[line + 1] - 1
        } else {
            self.len()
        }
    }

    pub fn line_len(&self, line: usize) -> usize {
        self.line_end(line) - self.line_start(line)
    }

    pub fn line_text(&self, line: usize) -> String {
        self.slice(self.line_start(line), self.line_end(line))
    }

    /// Last position a normal-mode cursor may take on `line` (the last character).
    pub fn line_last(&self, line: usize) -> usize {
        let (s, e) = (self.line_start(line), self.line_end(line));
        if e > s {
            e - 1
        } else {
            s
        }
    }

    /// First non-blank character of `line` (or its end).
    pub fn first_non_blank(&self, line: usize) -> usize {
        let (mut i, e) = (self.line_start(line), self.line_end(line));
        while i < e && matches!(self.chars[i], ' ' | '\t') {
            i += 1;
        }
        i
    }

    /// Leading whitespace of `line`.
    pub fn indent_of(&self, line: usize) -> String {
        self.slice(self.line_start(line), self.first_non_blank(line))
    }

    pub fn col_of(&self, pos: usize) -> usize {
        pos - self.line_start(self.line_of(pos))
    }

    /// Position at `col` on `line`, clamped to the line (`allow_end`: may be the `\n`).
    pub fn pos_at(&self, line: usize, col: usize, allow_end: bool) -> usize {
        let s = self.line_start(line);
        let e = if allow_end {
            self.line_end(line)
        } else {
            self.line_last(line)
        };
        (s + col).min(e)
    }

    /// Replace `from..to` with `text`.
    pub fn replace(&mut self, from: usize, to: usize, text: &str) {
        let to = to.min(self.len());
        let from = from.min(to);
        self.chars.splice(from..to, text.chars());
        self.reindex();
    }

    /// UTF-16 offset of character index `pos`.
    pub fn to_utf16(&self, pos: usize) -> usize {
        let pos = pos.min(self.len());
        if self.astral == 0 {
            return pos;
        }
        pos + self.chars[..pos]
            .iter()
            .filter(|c| (**c as u32) > 0xFFFF)
            .count()
    }

    /// Character index of UTF-16 offset `off` (an offset inside a surrogate pair rounds down).
    pub fn from_utf16(&self, off: usize) -> usize {
        if self.astral == 0 {
            return off.min(self.len());
        }
        let mut units = 0;
        for (i, c) in self.chars.iter().enumerate() {
            let w = c.len_utf16();
            if units + w > off {
                return i;
            }
            units += w;
        }
        self.len()
    }

    /// Character index of byte offset `b` in [`Buffer::text`].
    pub fn from_byte(&self, b: usize) -> usize {
        let t = self.text();
        if self.is_ascii() {
            return b;
        }
        t[..b.min(t.len())].chars().count()
    }

    /// Byte offset in [`Buffer::text`] of character index `pos`.
    pub fn to_byte(&self, pos: usize) -> usize {
        if self.is_ascii() {
            return pos;
        }
        self.chars[..pos.min(self.len())]
            .iter()
            .map(|c| c.len_utf8())
            .sum()
    }

    fn is_ascii(&self) -> bool {
        self.text().len() == self.chars.len()
    }

    /// UTF-16 length of the text.
    pub fn utf16_len(&self) -> usize {
        self.len() + self.astral
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_lines() {
        let b = Buffer::new("ab\n\ncd");
        assert_eq!(b.line_count(), 3);
        assert_eq!(b.line_of(0), 0);
        assert_eq!(b.line_of(2), 0);
        assert_eq!(b.line_of(3), 1);
        assert_eq!(b.line_of(6), 2);
        assert_eq!(b.line_end(0), 2);
        assert_eq!(b.line_last(1), 3);
        assert_eq!(b.line_text(2), "cd");
    }

    #[test]
    fn converts_utf16_offsets() {
        let b = Buffer::new("a😀b\nä");
        assert_eq!(b.to_utf16(2), 3);
        assert_eq!(b.from_utf16(3), 2);
        assert_eq!(b.from_utf16(2), 1);
        assert_eq!(b.utf16_len(), 6);
        assert_eq!(b.to_byte(2), 5);
        assert_eq!(b.from_byte(5), 2);
    }

    #[test]
    fn replaces_text() {
        let mut b = Buffer::new("hello world");
        b.replace(0, 5, "bye\nnow");
        assert_eq!(b.text(), "bye\nnow world");
        assert_eq!(b.line_count(), 2);
    }
}
