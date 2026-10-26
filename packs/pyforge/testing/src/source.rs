//! A small scanner of Python source for test discovery: logical lines (brackets, line
//! continuations and triple-quoted strings joined, comments removed), and the classes and
//! functions they declare with their decorators, bases and nesting. No full parser: it
//! reads what test discovery needs and never fails.

/// A logical line: one statement, possibly spanning several physical lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalLine {
    /// 0-based physical line it starts on.
    pub line: u32,
    /// Indentation (columns) of its first physical line.
    pub indent: usize,
    /// The text without comments; newlines of continued lines are kept.
    pub text: String,
}

impl LogicalLine {
    /// The physical line of a byte offset of `text`.
    pub fn line_at(&self, offset: usize) -> u32 {
        self.line
            + self.text[..offset.min(self.text.len())]
                .matches('\n')
                .count() as u32
    }
}

/// Split source into logical lines.
pub fn logical_lines(source: &str) -> Vec<LogicalLine> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut start_line = 0u32;
    let mut line = 0u32;
    let mut indent = 0usize;
    let mut at_line_start = true;
    let mut depth = 0i32;
    // Open string: its quote char and whether it is triple-quoted.
    let mut string: Option<(char, bool)> = None;
    let mut continued = false;
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if at_line_start && text.is_empty() {
            // Indentation of a new logical line.
            let mut col = 0;
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                col += if chars[i] == '\t' { 8 - col % 8 } else { 1 };
                i += 1;
            }
            indent = col;
            start_line = line;
            at_line_start = false;
            continue;
        }
        if let Some((q, triple)) = string {
            text.push(c);
            if c == '\\' && i + 1 < chars.len() {
                text.push(chars[i + 1]);
                if chars[i + 1] == '\n' {
                    line += 1;
                }
                i += 2;
                continue;
            }
            if c == '\n' {
                line += 1;
                if !triple {
                    // An unterminated single-quoted string ends at the line end.
                    string = None;
                    text.pop();
                    flush(&mut out, &mut text, start_line, indent);
                    depth = 0;
                    at_line_start = true;
                }
            } else if c == q {
                if !triple {
                    string = None;
                } else if i + 2 < chars.len() && chars[i + 1] == q && chars[i + 2] == q {
                    text.push(q);
                    text.push(q);
                    i += 2;
                    string = None;
                }
            }
            i += 1;
            continue;
        }
        match c {
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '\'' | '"' => {
                let triple = i + 2 < chars.len() && chars[i + 1] == c && chars[i + 2] == c;
                text.push(c);
                if triple {
                    text.push(c);
                    text.push(c);
                    i += 2;
                }
                string = Some((c, triple));
            }
            '(' | '[' | '{' => {
                depth += 1;
                text.push(c);
            }
            ')' | ']' | '}' => {
                depth = (depth - 1).max(0);
                text.push(c);
            }
            '\\' if chars.get(i + 1) == Some(&'\n') => {
                continued = true;
                text.push(' ');
                i += 2;
                line += 1;
                text.push('\n');
                continue;
            }
            '\n' => {
                line += 1;
                if depth > 0 || continued {
                    text.push('\n');
                    continued = false;
                } else {
                    flush(&mut out, &mut text, start_line, indent);
                    at_line_start = true;
                }
            }
            '\r' => {}
            _ => text.push(c),
        }
        i += 1;
    }
    flush(&mut out, &mut text, start_line, indent);
    out
}

fn flush(out: &mut Vec<LogicalLine>, text: &mut String, line: u32, indent: usize) {
    if !text.trim().is_empty() {
        out.push(LogicalLine {
            line,
            indent,
            text: text.trim_end().to_string(),
        });
    }
    text.clear();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Class,
    Function,
}

/// A decorator: its text after `@` and where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decorator {
    pub line: LogicalLine,
}

impl Decorator {
    /// The decorator's expression (`pytest.mark.parametrize("a", [1, 2])`).
    pub fn expr(&self) -> &str {
        self.line.text.trim_start().trim_start_matches('@').trim()
    }
}

/// A class or function declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    pub kind: DefKind,
    pub name: String,
    /// 0-based line of the `def` / `class` keyword.
    pub line: u32,
    pub indent: usize,
    /// Base classes as written (`unittest.TestCase`).
    pub bases: Vec<String>,
    pub decorators: Vec<Decorator>,
    /// Index of the enclosing declaration.
    pub parent: Option<usize>,
    pub is_async: bool,
}

fn identifier(s: &str) -> &str {
    let end = s
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(s.len(), |(i, _)| i);
    &s[..end]
}

/// Split at top-level commas (outside brackets and strings).
pub fn split_top_level(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut string: Option<char> = None;
    let mut start = 0;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if let Some(q) = string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                string = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => string = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push((start, &s[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    if !s[start..].trim().is_empty() {
        out.push((start, &s[start..]));
    }
    out
}

/// The text between the bracket at `open` and its closing partner.
pub fn bracket_contents(s: &str, open: usize) -> Option<&str> {
    let bytes = s.as_bytes();
    let (o, c) = match bytes.get(open)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0i32;
    let mut string: Option<u8> = None;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if let Some(q) = string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                string = None;
            }
            continue;
        }
        if b == b'\'' || b == b'"' {
            string = Some(b);
        } else if b == o {
            depth += 1;
        } else if b == c {
            depth -= 1;
            if depth == 0 {
                return Some(&s[open + 1..i]);
            }
        }
    }
    None
}

/// The classes and functions of a module, in source order.
pub fn declarations(source: &str) -> Vec<Def> {
    let mut defs: Vec<Def> = Vec::new();
    // Open blocks: (indent, index of the def).
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut decorators: Vec<Decorator> = Vec::new();
    for l in logical_lines(source) {
        while stack.last().is_some_and(|(ind, _)| *ind >= l.indent) {
            stack.pop();
        }
        let t = l.text.trim_start();
        if t.starts_with('@') {
            decorators.push(Decorator { line: l.clone() });
            continue;
        }
        let (is_async, rest) = match t.strip_prefix("async ") {
            Some(r) => (true, r.trim_start()),
            None => (false, t),
        };
        let parsed = if let Some(r) = rest.strip_prefix("def ") {
            Some((DefKind::Function, r.trim_start()))
        } else {
            rest.strip_prefix("class ")
                .filter(|_| !is_async)
                .map(|r| (DefKind::Class, r.trim_start()))
        };
        let Some((kind, r)) = parsed else {
            decorators.clear();
            continue;
        };
        let name = identifier(r);
        if name.is_empty() {
            decorators.clear();
            continue;
        }
        let after = &r[name.len()..];
        let bases = if kind == DefKind::Class && after.trim_start().starts_with('(') {
            let open = r.len() - after.trim_start().len();
            bracket_contents(r, open)
                .map(|inner| {
                    split_top_level(inner)
                        .into_iter()
                        .map(|(_, b)| b.split_whitespace().collect::<Vec<_>>().join(" "))
                        .filter(|b| !b.is_empty() && !b.contains('='))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let def_line = l.line_at(l.text.len() - t.len());
        defs.push(Def {
            kind,
            name: name.to_string(),
            line: def_line,
            indent: l.indent,
            bases,
            decorators: std::mem::take(&mut decorators),
            parent: stack.last().map(|(_, i)| *i),
            is_async,
        });
        stack.push((l.indent, defs.len() - 1));
    }
    defs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_lines_join_brackets_and_skip_strings_and_comments() {
        let src = "import x  # c\n\"\"\"doc\ndef nope():\n\"\"\"\n@deco(\n    1,  # one\n    2,\n)\ndef f(a,\n      b):\n    s = 'it''s'\n    return \\\n        a\n";
        let lines = logical_lines(src);
        let texts: Vec<_> = lines.iter().map(|l| l.text.trim()).collect();
        assert_eq!(texts[0], "import x");
        assert!(texts[1].starts_with("\"\"\"doc"));
        assert_eq!(lines[2].line, 4);
        assert!(texts[2].contains("2,") && !texts[2].contains("one"));
        assert_eq!(lines[3].line, 8);
        assert_eq!(lines[4].indent, 4);
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[2].line_at(lines[2].text.find("2,").unwrap()), 6);
    }

    #[test]
    fn declarations_nest_by_indentation() {
        let src = "\
import unittest

class TestCart(unittest.TestCase, Mixin):
    \"\"\"Cart tests.\"\"\"

    @classmethod
    def setUpClass(cls): ...

    async def test_total(self):
        def helper(): pass

    class TestInner:
        def test_x(self): pass

def test_free():
    pass
";
        let defs = declarations(src);
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "TestCart",
                "setUpClass",
                "test_total",
                "helper",
                "TestInner",
                "test_x",
                "test_free"
            ]
        );
        assert_eq!(defs[0].bases, ["unittest.TestCase", "Mixin"]);
        assert_eq!(defs[1].decorators[0].expr(), "classmethod");
        assert_eq!(defs[2].parent, Some(0));
        assert!(defs[2].is_async);
        assert_eq!(defs[3].parent, Some(2));
        assert_eq!(defs[5].parent, Some(4));
        assert_eq!(defs[6].parent, None);
        assert_eq!(defs[6].line, 14);
    }

    #[test]
    fn splits_and_brackets() {
        let s = "\"a,b\", [(1, 2), (3, 4)], ids=['x', 'y']";
        let parts: Vec<_> = split_top_level(s)
            .into_iter()
            .map(|(_, p)| p.trim())
            .collect();
        assert_eq!(parts, ["\"a,b\"", "[(1, 2), (3, 4)]", "ids=['x', 'y']"]);
        assert_eq!(bracket_contents("f(a, (b))x", 1), Some("a, (b)"));
    }
}
