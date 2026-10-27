//! Statements and blocks of a file, found from braces, semicolons, line breaks or
//! indentation (no parser): what a suppression comment covers and where the Suppress
//! actions put one.

use crate::lang::{Lang, Lexed, Mode};

/// A statement (with the blocks that belong to it: `if {} else {}`, a function body).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stmt {
    pub start: usize,
    /// Exclusive.
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Function,
    Class,
    Other,
}

/// A declaration with a body: from its header (attributes and modifiers included) to the
/// end of its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub start: usize,
    pub end: usize,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, Default)]
pub struct Structure {
    pub stmts: Vec<Stmt>,
    pub blocks: Vec<Block>,
}

impl Structure {
    /// The innermost statement containing `offset`.
    pub fn statement_at(&self, offset: usize) -> Option<Stmt> {
        self.stmts
            .iter()
            .filter(|s| s.start <= offset && offset < s.end.max(s.start + 1))
            .min_by_key(|s| s.end - s.start)
            .copied()
    }

    /// The longest statement starting at `offset`.
    pub fn statement_from(&self, offset: usize) -> Option<Stmt> {
        self.stmts
            .iter()
            .filter(|s| s.start == offset)
            .max_by_key(|s| s.end)
            .copied()
    }

    /// The innermost block of `kind` containing `offset`.
    pub fn block_at(&self, offset: usize, kind: BlockKind) -> Option<Block> {
        self.blocks
            .iter()
            .filter(|b| b.kind == kind && b.start <= offset && offset < b.end)
            .min_by_key(|b| b.end - b.start)
            .copied()
    }
}

const FUNCTION_WORDS: &[&str] = &["function", "fn", "func", "fun", "def", "sub"];
const CLASS_WORDS: &[&str] = &[
    "class",
    "interface",
    "trait",
    "enum",
    "struct",
    "impl",
    "object",
    "record",
];
const CONTROL_WORDS: &[&str] = &[
    "if",
    "else",
    "elseif",
    "for",
    "foreach",
    "while",
    "do",
    "switch",
    "match",
    "catch",
    "try",
    "finally",
    "synchronized",
    "using",
    "lock",
    "return",
    "new",
    "when",
    "with",
    "loop",
    "unsafe",
    "namespace",
    "module",
    "declare",
];
const CONTINUE_WORDS: &[&str] = &["else", "elseif", "catch", "finally", "except", "elif"];

fn words(header: &str) -> Vec<&str> {
    header
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect()
}

fn classify(header: &str) -> BlockKind {
    let ws = words(header);
    // `class Foo` (not `Foo::class`), `struct`, `impl ... for`.
    let paren = header.find('(').unwrap_or(header.len());
    let before_paren = words(&header[..paren]);
    if before_paren.iter().any(|w| CLASS_WORDS.contains(w)) && !header.contains("::class") {
        return BlockKind::Class;
    }
    if ws.iter().any(|w| FUNCTION_WORDS.contains(w)) {
        return BlockKind::Function;
    }
    // `public void run(...) throws X`, `int main(void)`: a call-like header that is not a
    // control statement.
    let tail = header.trim_end();
    let callish = tail.ends_with([')', '>'])
        || header.contains("throws")
        || header.contains("->")
        || header.contains(" const");
    if paren < header.len()
        && !before_paren.is_empty()
        && !before_paren.iter().any(|w| CONTROL_WORDS.contains(w))
        && !header[..paren].contains('=')
        && callish
    {
        return BlockKind::Function;
    }
    BlockKind::Other
}

/// The code text of `start..end` (comments and strings blanked).
fn code_text(text: &str, lexed: &Lexed, start: usize, end: usize) -> String {
    text[start..end]
        .char_indices()
        .map(|(i, c)| if lexed.is_code(start + i) { c } else { ' ' })
        .collect()
}

fn word_at(text: &str, i: usize) -> &str {
    let rest = &text[i..];
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    &rest[..end]
}

/// The next code byte at or after `i` that is not whitespace.
fn next_code(text: &str, lexed: &Lexed, mut i: usize) -> Option<usize> {
    let b = text.as_bytes();
    while i < b.len() {
        if lexed.is_code(i) && !b[i].is_ascii_whitespace() {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The last code byte before `i` that is not whitespace, on the same line.
fn prev_code_on_line(text: &str, lexed: &Lexed, i: usize) -> Option<u8> {
    let b = text.as_bytes();
    let mut j = i;
    while j > 0 {
        j -= 1;
        if b[j] == b'\n' {
            return None;
        }
        if lexed.is_code(j) && !b[j].is_ascii_whitespace() {
            return Some(b[j]);
        }
    }
    None
}

struct Frame {
    open: u8,
    /// Where the current statement of this frame started.
    stmt: Option<usize>,
    /// Statement start of the header this frame (a brace block) belongs to.
    header: Option<usize>,
}

pub fn scan(text: &str, lang: &Lang, lexed: &Lexed) -> Structure {
    match lang.mode {
        Mode::Braces { semicolons } => scan_braces(text, lexed, semicolons),
        Mode::Indent => scan_indent(text, lexed),
        Mode::Line => scan_lines(text, lexed),
    }
}

fn scan_braces(text: &str, lexed: &Lexed, semicolons: bool) -> Structure {
    let b = text.as_bytes();
    let mut out = Structure::default();
    let mut frames = vec![Frame {
        open: 0,
        stmt: None,
        header: None,
    }];
    // The statement start of the innermost brace frame (parens inherit it).
    let current = |frames: &Vec<Frame>| -> Option<usize> {
        frames
            .iter()
            .rev()
            .find(|f| f.open == b'{' || f.open == 0)
            .and_then(|f| f.stmt)
    };
    let mut i = 0;
    while i < b.len() {
        if !lexed.is_code(i) {
            i += 1;
            continue;
        }
        let c = b[i];
        let top_is_block = matches!(frames.last().map(|f| f.open), Some(0) | Some(b'{'));
        if top_is_block && !c.is_ascii_whitespace() && c != b'}' {
            if let Some(f) = frames.last_mut() {
                f.stmt.get_or_insert(i);
            }
        }
        match c {
            b'{' => {
                let header = current(&frames);
                frames.push(Frame {
                    open: b'{',
                    stmt: None,
                    header: header.or(Some(i)),
                });
                // Remember where the brace is, to read the header later.
                out.blocks.push(Block {
                    start: header.unwrap_or(i),
                    end: i,
                    kind: BlockKind::Other,
                });
            }
            b'(' | b'[' => frames.push(Frame {
                open: c,
                stmt: None,
                header: None,
            }),
            b')' | b']' => {
                if frames.len() > 1 && frames.last().is_some_and(|f| f.open != b'{') {
                    frames.pop();
                }
            }
            b'}' => {
                while frames.len() > 1 && frames.last().is_some_and(|f| f.open != b'{') {
                    frames.pop();
                }
                if frames.len() > 1 {
                    let f = frames.pop().expect("checked");
                    if let Some(s) = f.stmt {
                        // A last statement without `;`.
                        out.stmts.push(Stmt { start: s, end: i });
                    }
                    let header = f.header.unwrap_or(i);
                    if let Some(block) = out.blocks.iter_mut().rev().find(|bl| {
                        bl.kind == BlockKind::Other
                            && bl.start == header
                            && bl.end < i
                            && text.as_bytes()[bl.end] == b'{'
                    }) {
                        let head = code_text(text, lexed, block.start, block.end);
                        block.kind = classify(&head);
                        block.end = i + 1;
                    }
                    // Does the statement end with this block?
                    let top_is_block =
                        matches!(frames.last().map(|f| f.open), Some(0) | Some(b'{'));
                    if top_is_block {
                        let next = next_code(text, lexed, i + 1);
                        let goes_on = next.is_some_and(|n| {
                            matches!(
                                b[n],
                                b';' | b')' | b',' | b'.' | b'(' | b']' | b'-' | b'?' | b':'
                            ) || CONTINUE_WORDS.contains(&word_at(text, n))
                                || (word_at(text, n) == "while"
                                    && code_text(text, lexed, header, i)
                                        .trim_start()
                                        .starts_with("do"))
                        });
                        if !goes_on {
                            if let Some(f) = frames.last_mut() {
                                if let Some(s) = f.stmt.take() {
                                    out.stmts.push(Stmt {
                                        start: s,
                                        end: i + 1,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            b';' if top_is_block => {
                if let Some(f) = frames.last_mut() {
                    let s = f.stmt.take().unwrap_or(i);
                    out.stmts.push(Stmt {
                        start: s,
                        end: i + 1,
                    });
                }
            }
            b'\n' if top_is_block && !semicolons => {
                if let Some(s) = frames.last().and_then(|f| f.stmt) {
                    let cont_end = continues_line(prev_code_on_line(text, lexed, i));
                    let next = next_code(text, lexed, i + 1);
                    let cont_next = next.is_some_and(|n| {
                        matches!(
                            b[n],
                            b'.' | b'?'
                                | b':'
                                | b')'
                                | b']'
                                | b'{'
                                | b'+'
                                | b'*'
                                | b'&'
                                | b'|'
                                | b','
                                | b'='
                        ) || (b[n] == b'-' && b.get(n + 1) == Some(&b'>'))
                    });
                    if !cont_end && !cont_next {
                        out.stmts.push(Stmt { start: s, end: i });
                        if let Some(f) = frames.last_mut() {
                            f.stmt = None;
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    for f in frames.into_iter().rev() {
        if let Some(s) = f.stmt {
            out.stmts.push(Stmt {
                start: s,
                end: b.len(),
            });
        }
    }
    out.blocks
        .retain(|bl| bl.end > bl.start && b.get(bl.end - 1) == Some(&b'}'));
    out
}

/// No code on the line, or it ends with something that continues on the next line.
fn continues_line(last: Option<u8>) -> bool {
    match last {
        None => true,
        Some(c) => matches!(
            c,
            b',' | b'('
                | b'['
                | b'{'
                | b'='
                | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'.'
                | b'&'
                | b'|'
                | b'?'
                | b':'
                | b'<'
                | b'>'
                | b'!'
                | b'\\'
        ),
    }
}

/// Logical lines (brackets and `\` continue them) with their indentation.
struct LogicalLine {
    start: usize,
    end: usize,
    indent: usize,
    first: usize,
}

fn logical_lines(text: &str, lexed: &Lexed) -> Vec<LogicalLine> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur: Option<LogicalLine> = None;
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let ls = line_start;
        line_start += line.len();
        let trimmed_end = ls + line.trim_end().len();
        let first = (ls..trimmed_end).find(|&i| lexed.is_code(i) && !b[i].is_ascii_whitespace());
        for (i, &c) in b.iter().enumerate().take(trimmed_end).skip(ls) {
            if lexed.is_code(i) {
                match c {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => depth = (depth - 1).max(0),
                    _ => {}
                }
            }
        }
        let continues = depth > 0 || line.trim_end().ends_with('\\');
        match (&mut cur, first) {
            (Some(c), _) => c.end = trimmed_end.max(c.end),
            (None, Some(f)) => {
                cur = Some(LogicalLine {
                    start: f,
                    end: trimmed_end,
                    indent: f - ls,
                    first: f,
                })
            }
            (None, None) => {}
        }
        if !continues {
            if let Some(c) = cur.take() {
                out.push(c);
            }
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}

fn scan_indent(text: &str, lexed: &Lexed) -> Structure {
    let lines = logical_lines(text, lexed);
    let mut out = Structure::default();
    for (k, l) in lines.iter().enumerate() {
        // Decorators belong to the next line at the same indentation.
        if text[l.first..].starts_with('@') {
            continue;
        }
        let mut start = l.start;
        let mut j = k;
        while j > 0
            && lines[j - 1].indent == l.indent
            && text[lines[j - 1].first..].starts_with('@')
        {
            j -= 1;
            start = lines[j].start;
        }
        let end = lines[k + 1..]
            .iter()
            .take_while(|n| n.indent > l.indent)
            .last()
            .map_or(l.end, |n| n.end);
        out.stmts.push(Stmt { start, end });
        let w = word_at(text, l.first);
        let w = if w == "async" {
            word_at(
                text,
                (l.first + 5..l.end)
                    .find(|&i| !text.as_bytes()[i].is_ascii_whitespace())
                    .unwrap_or(l.end),
            )
        } else {
            w
        };
        let kind = match w {
            "def" => BlockKind::Function,
            "class" => BlockKind::Class,
            _ => BlockKind::Other,
        };
        if kind != BlockKind::Other {
            out.blocks.push(Block { start, end, kind });
        }
    }
    out
}

fn scan_lines(text: &str, lexed: &Lexed) -> Structure {
    let mut out = Structure::default();
    for l in logical_lines(text, lexed) {
        out.stmts.push(Stmt {
            start: l.start,
            end: l.end,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{for_path, lex};

    fn structure(path: &str, text: &str) -> Structure {
        let lang = for_path(path);
        let lexed = lex(text, &lang);
        scan(text, &lang, &lexed)
    }

    fn stmt_text(text: &str, s: Option<Stmt>) -> &str {
        let s = s.expect("a statement");
        &text[s.start..s.end]
    }

    #[test]
    fn php_statements_and_blocks() {
        let src = "<?php\nclass A {\n    #[Pure]\n    public function f($a) {\n        if ($a) {\n            return 1;\n        } else {\n            $b = foo(function () { return 2; });\n        }\n    }\n}\n";
        let s = structure("a.php", src);
        let at = |needle: &str| src.find(needle).unwrap();
        assert_eq!(stmt_text(src, s.statement_at(at("return 1"))), "return 1;");
        assert!(stmt_text(src, s.statement_at(at("if ($a)") + 3)).starts_with("if ($a) {"));
        assert!(stmt_text(src, s.statement_at(at("if ($a)") + 3)).ends_with('}'));
        assert_eq!(stmt_text(src, s.statement_at(at("return 2"))), "return 2;");
        assert_eq!(
            stmt_text(src, s.statement_at(at("foo("))),
            "$b = foo(function () { return 2; });"
        );
        let f = s.block_at(at("return 1"), BlockKind::Function).unwrap();
        assert!(
            src[f.start..f.end].starts_with("#[Pure]"),
            "{}",
            &src[f.start..f.end]
        );
        let c = s.block_at(at("return 1"), BlockKind::Class).unwrap();
        assert!(src[c.start..c.end].starts_with("class A {"));
        assert!(src[c.start..c.end].ends_with('}'));
        let stmt = s.statement_from(at("#[Pure]")).unwrap();
        assert_eq!(stmt.end, f.end);
    }

    #[test]
    fn javascript_without_semicolons() {
        let src = "const a = 1\nconst b = foo()\n  .bar()\nfunction f() {\n  return a\n}\n";
        let s = structure("a.js", src);
        let at = |needle: &str| src.find(needle).unwrap();
        assert_eq!(stmt_text(src, s.statement_at(at("a = 1"))), "const a = 1");
        assert_eq!(
            stmt_text(src, s.statement_at(at(".bar"))),
            "const b = foo()\n  .bar()"
        );
        assert_eq!(
            s.block_at(at("return a"), BlockKind::Function)
                .unwrap()
                .start,
            at("function")
        );
    }

    #[test]
    fn java_methods_and_python_blocks() {
        let src =
            "class A {\n  @Override\n  public void run() throws X {\n    if (x) { y(); }\n  }\n}\n";
        let s = structure("A.java", src);
        let f = s
            .block_at(src.find("y()").unwrap(), BlockKind::Function)
            .unwrap();
        assert!(src[f.start..].starts_with("@Override"));
        let py = "import os\n\n@dec\ndef f(a,\n      b):\n    x = 1\n    return x\n\nclass C:\n    pass\n";
        let s = structure("a.py", py);
        let f = s
            .block_at(py.find("x = 1").unwrap(), BlockKind::Function)
            .unwrap();
        assert_eq!(
            &py[f.start..f.end],
            "@dec\ndef f(a,\n      b):\n    x = 1\n    return x"
        );
        assert_eq!(
            stmt_text(py, s.statement_at(py.find("x = 1").unwrap())),
            "x = 1"
        );
        assert!(s
            .block_at(py.find("pass").unwrap(), BlockKind::Class)
            .is_some());
        assert_eq!(
            stmt_text(py, s.statement_from(py.find("@dec").unwrap())),
            &py[f.start..f.end]
        );
    }
}
