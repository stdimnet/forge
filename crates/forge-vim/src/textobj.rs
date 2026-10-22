//! Text objects: `iw aw iW aW`, quotes, brackets, tags and paragraphs.

use regex::Regex;
use std::sync::OnceLock;

use crate::buffer::{class_of, Buffer, CharClass};
use crate::motions::{enclosing_open, match_bracket_at};

/// A text object's range (`to` exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjRange {
    pub from: usize,
    pub to: usize,
    pub linewise: bool,
    /// An inner block whose braces are on their own lines (`i{` over a multi-line block):
    /// `d` removes the lines, `c` keeps one empty line.
    pub block_lines: bool,
}

impl ObjRange {
    fn chars(from: usize, to: usize) -> Self {
        Self {
            from,
            to,
            linewise: false,
            block_lines: false,
        }
    }
}

/// Resolve text object `obj` (the character after `i`/`a`) at `pos`.
pub fn text_object(
    b: &Buffer,
    pos: usize,
    inner: bool,
    obj: char,
    count: usize,
) -> Option<ObjRange> {
    match obj {
        'w' => word(b, pos, inner, false, count),
        'W' => word(b, pos, inner, true, count),
        '"' | '\'' | '`' => quote(b, pos, inner, obj),
        '(' | ')' | 'b' => bracket(b, pos, inner, '(', ')', count),
        '[' | ']' => bracket(b, pos, inner, '[', ']', count),
        '{' | '}' | 'B' => bracket(b, pos, inner, '{', '}', count),
        '<' | '>' => bracket(b, pos, inner, '<', '>', count),
        't' => tag(b, pos, inner, count),
        'p' => paragraph(b, pos, inner, count),
        _ => None,
    }
}

fn word(b: &Buffer, pos: usize, inner: bool, big: bool, count: usize) -> Option<ObjRange> {
    let line = b.line_of(pos);
    let (ls, le) = (b.line_start(line), b.line_end(line));
    if ls == le {
        return Some(ObjRange::chars(ls, ls));
    }
    let ch = b.chars();
    let pos = pos.min(le - 1);
    let class = |i: usize| class_of(ch[i], big);
    let run = |i: usize| {
        let c = class(i);
        let (mut s, mut e) = (i, i + 1);
        while s > ls && class(s - 1) == c {
            s -= 1;
        }
        while e < le && class(e) == c {
            e += 1;
        }
        (s, e)
    };
    let (mut s, mut e) = run(pos);
    let on_blank = class(pos) == CharClass::Blank;
    for k in 0..count {
        if inner {
            if k > 0 {
                if e >= le {
                    break;
                }
                e = run(e).1;
            }
        } else {
            if k > 0 {
                if e >= le {
                    break;
                }
                let (_, e2) = run(e);
                e = e2;
            }
            if on_blank {
                // `aw` on blanks: the blanks plus the following word.
                if e < le {
                    e = run(e).1;
                }
            } else if e < le && class(e) == CharClass::Blank {
                e = run(e).1;
            } else if k == 0 {
                // No trailing blanks: take the leading ones.
                while s > ls && class(s - 1) == CharClass::Blank {
                    s -= 1;
                }
            }
        }
    }
    Some(ObjRange::chars(s, e))
}

fn quote(b: &Buffer, pos: usize, inner: bool, q: char) -> Option<ObjRange> {
    let line = b.line_of(pos);
    let (ls, le) = (b.line_start(line), b.line_end(line));
    let ch = b.chars();
    let quotes: Vec<usize> = (ls..le)
        .filter(|&i| ch[i] == q && (i == ls || ch[i - 1] != '\\'))
        .collect();
    // Pairs from the start of the line.
    let pairs: Vec<(usize, usize)> = quotes
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| (c[0], c[1]))
        .collect();
    let (open, close) = pairs
        .iter()
        .find(|(o, c)| *o <= pos && pos <= *c)
        .or_else(|| pairs.iter().find(|(o, _)| *o > pos))
        .copied()?;
    if inner {
        return Some(ObjRange::chars(open + 1, close));
    }
    let mut to = close + 1;
    let mut from = open;
    if to < le && matches!(ch[to], ' ' | '\t') {
        while to < le && matches!(ch[to], ' ' | '\t') {
            to += 1;
        }
    } else {
        while from > ls && matches!(ch[from - 1], ' ' | '\t') {
            from -= 1;
        }
    }
    Some(ObjRange::chars(from, to))
}

fn bracket(
    b: &Buffer,
    pos: usize,
    inner: bool,
    open: char,
    close: char,
    count: usize,
) -> Option<ObjRange> {
    let mut o = enclosing_open(b, pos, open, close)?;
    for _ in 1..count {
        o = enclosing_open(b, o.checked_sub(1)?, open, close)?;
    }
    let c = match_bracket_at(b, o).or_else(|| {
        // `<`/`>` are not in the bracket table.
        let mut depth = 0;
        (o..b.len()).find(|&i| {
            if b.at(i) == Some(open) {
                depth += 1;
            } else if b.at(i) == Some(close) {
                depth -= 1;
            }
            depth == 0
        })
    })?;
    if !inner {
        return Some(ObjRange::chars(o, c + 1));
    }
    let (ol, cl) = (b.line_of(o), b.line_of(c));
    let close_on_own_line = b.first_non_blank(cl) == c;
    if cl > ol + 1 && b.at(o + 1) == Some('\n') && close_on_own_line {
        return Some(ObjRange {
            from: b.line_start(ol + 1),
            to: b.line_start(cl),
            linewise: false,
            block_lines: true,
        });
    }
    let mut from = o + 1;
    if b.at(from) == Some('\n') && cl > ol {
        from += 1;
    }
    let to = if close_on_own_line && cl > ol {
        b.line_start(cl).max(from)
    } else {
        c
    };
    Some(ObjRange::chars(from, to))
}

fn tag_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<(/?)([A-Za-z][\w:.-]*)(?:[^>]*?)(/?)>").unwrap())
}

fn tag(b: &Buffer, pos: usize, inner: bool, count: usize) -> Option<ObjRange> {
    let text = b.text();
    // (open start, open end, close start, close end)
    let mut pairs = Vec::new();
    let mut stack: Vec<(String, usize, usize)> = Vec::new();
    for caps in tag_regex().captures_iter(text) {
        let m = caps.get(0).unwrap();
        let (s, e) = (b.from_byte(m.start()), b.from_byte(m.end()));
        let name = caps[2].to_string();
        if !caps[3].is_empty() {
            continue;
        }
        if caps[1].is_empty() {
            stack.push((name, s, e));
        } else if let Some(k) = stack
            .iter()
            .rposition(|(n, _, _)| n.eq_ignore_ascii_case(&name))
        {
            let (_, os, oe) = stack[k].clone();
            stack.truncate(k);
            pairs.push((os, oe, s, e));
        }
    }
    let mut around: Vec<_> = pairs
        .into_iter()
        .filter(|(os, _, _, ce)| *os <= pos && pos < *ce)
        .collect();
    around.sort_by_key(|(os, _, _, ce)| ce - os);
    let (os, oe, cs, ce) = *around.get(count.max(1) - 1)?;
    Some(if inner {
        ObjRange::chars(oe, cs)
    } else {
        ObjRange::chars(os, ce)
    })
}

fn paragraph(b: &Buffer, pos: usize, inner: bool, count: usize) -> Option<ObjRange> {
    let last = b.line_count() - 1;
    let line = b.line_of(pos);
    let blank = |l: usize| b.line_text(l).trim().is_empty();
    let kind = blank(line);
    let mut s = line;
    while s > 0 && blank(s - 1) == kind {
        s -= 1;
    }
    let mut e = line;
    let mut runs = 0;
    let mut cur = kind;
    loop {
        while e < last && blank(e + 1) == cur {
            e += 1;
        }
        runs += 1;
        let needed = if inner { count } else { count * 2 };
        if runs >= needed || e >= last {
            break;
        }
        e += 1;
        cur = !cur;
    }
    if !inner && runs < count * 2 && !kind {
        // No blank lines after the paragraph: take the ones before it.
        while s > 0 && blank(s - 1) {
            s -= 1;
        }
    }
    let to = if e < last {
        b.line_start(e + 1)
    } else {
        b.len()
    };
    Some(ObjRange {
        from: b.line_start(s),
        to,
        linewise: true,
        block_lines: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(text: &str, pos: usize, spec: &str) -> String {
        let b = Buffer::new(text);
        let mut c = spec.chars();
        let inner = c.next() == Some('i');
        let r = text_object(&b, pos, inner, c.next().unwrap(), 1).unwrap();
        b.slice(r.from, r.to)
    }

    #[test]
    fn selects_words() {
        assert_eq!(obj("foo bar baz", 5, "iw"), "bar");
        assert_eq!(obj("foo bar baz", 5, "aw"), "bar ");
        assert_eq!(obj("foo bar", 5, "aw"), " bar");
        assert_eq!(obj("a.b c", 0, "iW"), "a.b");
        assert_eq!(obj("foo   bar", 4, "iw"), "   ");
    }

    #[test]
    fn selects_quotes_and_brackets() {
        assert_eq!(obj(r#"x = "a\"b" + 'c'"#, 6, "i\""), r#"a\"b"#);
        assert_eq!(obj(r#"say "hi" now"#, 0, "a\""), r#""hi" "#);
        assert_eq!(obj("f(a, (b))", 3, "i("), "a, (b)");
        assert_eq!(obj("f(a, (b))", 6, "a("), "(b)");
        assert_eq!(obj("x[1]", 2, "i]"), "1");
        assert_eq!(obj("if (x) {\n    foo;\n}\n", 12, "i{"), "    foo;\n");
        assert_eq!(obj("{ a }", 2, "iB"), " a ");
    }

    #[test]
    fn selects_tags_and_paragraphs() {
        assert_eq!(obj("<div><b>x</b> y</div>", 9, "it"), "x");
        assert_eq!(obj("<div><b>x</b> y</div>", 14, "it"), "<b>x</b> y");
        assert_eq!(obj("<div><b>x</b> y</div>", 9, "at"), "<b>x</b>");
        assert_eq!(obj("a\nb\n\nc", 0, "ip"), "a\nb\n");
        assert_eq!(obj("a\nb\n\nc", 0, "ap"), "a\nb\n\n");
    }
}
