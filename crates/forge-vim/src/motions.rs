//! Cursor motions over a [`Buffer`]. Positions are character indices; a position on an empty
//! line is the line's start.

use crate::buffer::{class_of, Buffer, CharClass};

fn blankish(c: char) -> bool {
    matches!(class_of(c, false), CharClass::Blank | CharClass::Newline)
}

/// Whether `i` is an empty line (its `\n`, or the end of an empty last line).
fn empty_line_at(b: &Buffer, i: usize) -> bool {
    let line = b.line_of(i);
    b.line_len(line) == 0 && b.line_start(line) == i
}

/// `w` / `W`: start of the next word (empty lines count as words); the text end if none.
pub fn word_forward(b: &Buffer, pos: usize, big: bool) -> usize {
    let n = b.len();
    if pos >= n {
        return n;
    }
    let mut i = pos;
    let c0 = class_of(b.chars()[i], big);
    if !matches!(c0, CharClass::Blank | CharClass::Newline) {
        while i < n && class_of(b.chars()[i], big) == c0 {
            i += 1;
        }
    }
    while i < n {
        let c = b.chars()[i];
        if c == '\n' {
            i += 1;
            if i < n && empty_line_at(b, i) {
                return i;
            }
            continue;
        }
        if blankish(c) {
            i += 1;
            continue;
        }
        return i;
    }
    n
}

/// `e` / `E`: end of the current or next word.
pub fn word_end(b: &Buffer, pos: usize, big: bool) -> usize {
    let n = b.len();
    if n == 0 {
        return 0;
    }
    let mut i = pos + 1;
    while i < n && blankish(b.chars()[i]) {
        i += 1;
    }
    if i >= n {
        return n - 1;
    }
    let c = class_of(b.chars()[i], big);
    while i + 1 < n && class_of(b.chars()[i + 1], big) == c {
        i += 1;
    }
    i
}

/// `b` / `B`: start of the current or previous word.
pub fn word_back(b: &Buffer, pos: usize, big: bool) -> usize {
    if pos == 0 {
        return 0;
    }
    let ch = b.chars();
    let mut i = pos.min(b.len()) - 1;
    while i > 0 && blankish(ch[i]) {
        if ch[i] == '\n' && ch[i - 1] == '\n' {
            return i;
        }
        i -= 1;
    }
    if blankish(ch[i]) {
        return i;
    }
    let c = class_of(ch[i], big);
    while i > 0 && class_of(ch[i - 1], big) == c {
        i -= 1;
    }
    i
}

/// `ge` / `gE`: end of the previous word.
pub fn word_end_back(b: &Buffer, pos: usize, big: bool) -> usize {
    let ch = b.chars();
    let mut i = pos.min(b.len());
    if i < b.len() && !blankish(ch[i]) {
        let c = class_of(ch[i], big);
        while i > 0 && class_of(ch[i - 1], big) == c {
            i -= 1;
        }
    }
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && blankish(ch[i]) {
        if ch[i] == '\n' && ch[i - 1] == '\n' {
            return i;
        }
        i -= 1;
    }
    i
}

/// `}`: the next empty line after the paragraph (or the end of the text).
pub fn paragraph_forward(b: &Buffer, pos: usize) -> usize {
    let last = b.line_count() - 1;
    let mut l = b.line_of(pos);
    while l < last && b.line_len(l) == 0 {
        l += 1;
    }
    while l < last && b.line_len(l) != 0 {
        l += 1;
    }
    if b.line_len(l) == 0 {
        b.line_start(l)
    } else {
        b.line_last(l)
    }
}

/// `{`: the previous empty line before the paragraph (or the start of the text).
pub fn paragraph_back(b: &Buffer, pos: usize) -> usize {
    let mut l = b.line_of(pos);
    while l > 0 && b.line_len(l) == 0 {
        l -= 1;
    }
    while l > 0 && b.line_len(l) != 0 {
        l -= 1;
    }
    b.line_start(l)
}

const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// The matching bracket of the bracket at `pos`.
pub fn match_bracket_at(b: &Buffer, pos: usize) -> Option<usize> {
    let c = b.at(pos)?;
    let ch = b.chars();
    for (open, close) in PAIRS {
        if c == open {
            let mut depth = 0;
            for (i, x) in ch.iter().enumerate().skip(pos) {
                if *x == open {
                    depth += 1;
                } else if *x == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
            return None;
        }
        if c == close {
            let mut depth = 0;
            for i in (0..=pos).rev() {
                if ch[i] == close {
                    depth += 1;
                } else if ch[i] == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
            return None;
        }
    }
    None
}

/// `%`: the match of the first bracket at or after the cursor on its line.
pub fn percent(b: &Buffer, pos: usize) -> Option<usize> {
    let end = b.line_end(b.line_of(pos));
    (pos..end)
        .find(|i| {
            PAIRS
                .iter()
                .any(|(o, c)| b.at(*i) == Some(*o) || b.at(*i) == Some(*c))
        })
        .and_then(|i| match_bracket_at(b, i))
}

/// The unmatched `open` bracket around `pos` (skipping `count - 1` levels).
pub fn enclosing_open(b: &Buffer, pos: usize, open: char, close: char) -> Option<usize> {
    let ch = b.chars();
    let mut depth = 0;
    let start = if b.at(pos) == Some(close) {
        pos.checked_sub(1)?
    } else {
        pos
    };
    if b.at(pos) == Some(open) {
        return Some(pos);
    }
    for i in (0..=start.min(ch.len().saturating_sub(1))).rev() {
        if ch[i] == close && i != pos {
            depth += 1;
        } else if ch[i] == open {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
        }
    }
    None
}

/// `f` / `t` / `F` / `T` on the cursor line. `till_skip`: a repeated `t` does not stop right
/// before the character it is already in front of.
pub fn find_char(
    b: &Buffer,
    pos: usize,
    target: char,
    forward: bool,
    till: bool,
    count: usize,
    till_skip: bool,
) -> Option<usize> {
    let line = b.line_of(pos);
    let (s, e) = (b.line_start(line), b.line_end(line));
    let ch = b.chars();
    let mut i = pos;
    let mut found = 0;
    if forward {
        if till && till_skip && i + 1 < e && ch[i + 1] == target {
            i += 1;
        }
        while i + 1 < e {
            i += 1;
            if ch[i] == target {
                found += 1;
                if found == count {
                    return Some(if till { i - 1 } else { i });
                }
            }
        }
    } else {
        if till && till_skip && i > s && ch[i - 1] == target {
            i -= 1;
        }
        while i > s {
            i -= 1;
            if ch[i] == target {
                found += 1;
                if found == count {
                    return Some(if till { i + 1 } else { i });
                }
            }
        }
    }
    None
}

/// The keyword under or after the cursor on its line (for `*`, `#`): (start, end).
pub fn word_under_cursor(b: &Buffer, pos: usize) -> Option<(usize, usize)> {
    let line = b.line_of(pos);
    let e = b.line_end(line);
    let ch = b.chars();
    let mut i = pos;
    while i < e && !crate::buffer::is_word_char(ch[i]) {
        i += 1;
    }
    if i >= e {
        return None;
    }
    let mut s = i;
    while s > b.line_start(line) && crate::buffer::is_word_char(ch[s - 1]) {
        s -= 1;
    }
    let mut end = i;
    while end < e && crate::buffer::is_word_char(ch[end]) {
        end += 1;
    }
    Some((s, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_by_words() {
        let b = Buffer::new("foo.bar baz\n\n  qux");
        assert_eq!(word_forward(&b, 0, false), 3);
        assert_eq!(word_forward(&b, 3, false), 4);
        assert_eq!(word_forward(&b, 0, true), 8);
        assert_eq!(word_forward(&b, 8, false), 12);
        assert_eq!(word_forward(&b, 12, false), 15);
        assert_eq!(word_forward(&b, 15, false), 18);
        assert_eq!(word_end(&b, 0, false), 2);
        assert_eq!(word_end(&b, 2, false), 3);
        assert_eq!(word_end(&b, 8, false), 10);
        assert_eq!(word_end(&b, 10, false), 17);
        assert_eq!(word_back(&b, 15, false), 12);
        assert_eq!(word_back(&b, 12, false), 8);
        assert_eq!(word_back(&b, 8, true), 0);
        assert_eq!(word_back(&b, 4, false), 3);
        assert_eq!(word_end_back(&b, 8, false), 6);
        assert_eq!(word_end_back(&b, 15, false), 12);
    }

    #[test]
    fn moves_by_paragraphs_and_brackets() {
        let b = Buffer::new("a\nb\n\nc\nd");
        assert_eq!(paragraph_forward(&b, 0), 4);
        assert_eq!(paragraph_forward(&b, 4), 7);
        assert_eq!(paragraph_back(&b, 7), 4);
        assert_eq!(paragraph_back(&b, 4), 0);
        let b = Buffer::new("f(a[1], {x})");
        assert_eq!(percent(&b, 0), Some(11));
        assert_eq!(match_bracket_at(&b, 11), Some(1));
        assert_eq!(enclosing_open(&b, 9, '{', '}'), Some(8));
        assert_eq!(enclosing_open(&b, 5, '(', ')'), Some(1));
    }

    #[test]
    fn finds_characters() {
        let b = Buffer::new("a,b,c,d");
        assert_eq!(find_char(&b, 0, ',', true, false, 2, false), Some(3));
        assert_eq!(find_char(&b, 0, ',', true, true, 1, false), Some(0));
        assert_eq!(find_char(&b, 0, ',', true, true, 1, true), Some(2));
        assert_eq!(find_char(&b, 6, ',', false, false, 1, false), Some(5));
        assert_eq!(find_char(&b, 6, 'x', false, false, 1, false), None);
        assert_eq!(word_under_cursor(&b, 1), Some((2, 3)));
    }
}
