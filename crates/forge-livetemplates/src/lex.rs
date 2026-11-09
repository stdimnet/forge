//! Small lexers for PHP, JavaScript/TypeScript and Lua: enough to tell code from strings and
//! comments, find identifiers and match brackets. They never fail; broken code just yields
//! fewer useful tokens.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Ident,
    /// PHP variable (`$name`).
    Var,
    Punct,
    Str,
    Comment,
    Number,
    /// Inline HTML of a PHP file (outside `<?php … ?>`).
    Html,
    OpenTag,
    CloseTag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tok {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

impl Tok {
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }
    pub fn significant(&self) -> bool {
        self.kind != Kind::Comment
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    /// A PHP file: HTML until `<?php`.
    Php,
    Js,
    Lua,
}

fn ident_start(b: u8, syntax: Syntax) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80 || (syntax == Syntax::Js && b == b'$')
}

fn ident_char(b: u8, syntax: Syntax) -> bool {
    ident_start(b, syntax) || b.is_ascii_digit()
}

const TWO_CHAR: [&str; 6] = ["->", "::", "=>", "?->", "==", "~="];

pub(crate) fn lex(src: &str, syntax: Syntax) -> Vec<Tok> {
    if syntax == Syntax::Lua {
        return lex_lua(src);
    }
    let b = src.as_bytes();
    let n = b.len();
    let php = syntax == Syntax::Php;
    let mut toks = Vec::new();
    let mut i = 0;
    let mut in_html = syntax == Syntax::Php;
    let push = |toks: &mut Vec<Tok>, kind, start, end| toks.push(Tok { kind, start, end });
    while i < n {
        if in_html {
            let start = i;
            let open = src[i..].find("<?").map_or(n, |p| i + p);
            if open > start {
                push(&mut toks, Kind::Html, start, open);
            }
            if open >= n {
                break;
            }
            let tag_len = if src[open..].starts_with("<?php") {
                5
            } else if src[open..].starts_with("<?=") {
                3
            } else {
                2
            };
            push(&mut toks, Kind::OpenTag, open, open + tag_len);
            i = open + tag_len;
            in_html = false;
            continue;
        }
        let c = b[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if php && c == b'?' && b.get(i + 1) == Some(&b'>') {
            push(&mut toks, Kind::CloseTag, i, i + 2);
            i += 2;
            in_html = syntax == Syntax::Php;
            continue;
        }
        // Comments.
        if c == b'/' && b.get(i + 1) == Some(&b'/')
            || php && c == b'#' && b.get(i + 1) != Some(&b'[')
        {
            while i < n && b[i] != b'\n' && !(php && b[i] == b'?' && b.get(i + 1) == Some(&b'>')) {
                i += 1;
            }
            push(&mut toks, Kind::Comment, start, i);
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = src[i + 2..].find("*/").map_or(n, |p| i + 2 + p + 2);
            push(&mut toks, Kind::Comment, start, i);
            continue;
        }
        // Strings.
        if c == b'\'' || c == b'"' || (c == b'`') {
            i = if c == b'`' && !php {
                skip_template(b, i + 1)
            } else {
                skip_quoted(b, i + 1, c)
            };
            push(&mut toks, Kind::Str, start, i);
            continue;
        }
        if php && src[i..].starts_with("<<<") {
            if let Some(end) = skip_heredoc(src, i) {
                i = end;
                push(&mut toks, Kind::Str, start, i);
                continue;
            }
        }
        // JavaScript regular expression literals.
        if !php && c == b'/' && regex_allowed(src, &toks) {
            i = skip_regex(b, i + 1);
            push(&mut toks, Kind::Str, start, i);
            continue;
        }
        if php && c == b'$' && b.get(i + 1).is_some_and(|&d| ident_start(d, syntax)) {
            i += 1;
            while i < n && ident_char(b[i], syntax) {
                i += 1;
            }
            push(&mut toks, Kind::Var, start, i);
            continue;
        }
        if ident_start(c, syntax)
            || (php && c == b'\\' && b.get(i + 1).is_some_and(|&d| ident_start(d, syntax)))
        {
            i += 1;
            while i < n && (ident_char(b[i], syntax) || (php && b[i] == b'\\')) {
                i += 1;
            }
            push(&mut toks, Kind::Ident, start, i);
            continue;
        }
        if c.is_ascii_digit() {
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            push(&mut toks, Kind::Number, start, i);
            continue;
        }
        let len = TWO_CHAR
            .iter()
            .find(|p| src[i..].starts_with(**p))
            .map_or(1, |p| p.len());
        // Non-ASCII bytes are identifier characters, so `c` is ASCII here.
        i += len;
        push(&mut toks, Kind::Punct, start, i);
    }
    toks
}

fn skip_quoted(b: &[u8], mut i: usize, quote: u8) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// A JavaScript template literal, with `${ … }` substitutions.
fn skip_template(b: &[u8], mut i: usize) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'`' => return i + 1,
            b'$' if b.get(i + 1) == Some(&b'{') => {
                let mut depth = 0;
                i += 1;
                while i < b.len() {
                    match b[i] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        q @ (b'\'' | b'"') => i = skip_quoted(b, i + 1, q) - 1,
                        b'`' => i = skip_template(b, i + 1) - 1,
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    b.len()
}

fn skip_regex(b: &[u8], mut i: usize) -> usize {
    let mut class = false;
    while i < b.len() && b[i] != b'\n' {
        match b[i] {
            b'\\' => i += 1,
            b'[' => class = true,
            b']' => class = false,
            b'/' if !class => {
                i += 1;
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
                return i;
            }
            _ => {}
        }
        i += 1;
    }
    // A trailing backslash steps past the end.
    i.min(b.len())
}

fn regex_allowed(src: &str, toks: &[Tok]) -> bool {
    let Some(prev) = toks.iter().rev().find(|t| t.significant()) else {
        return true;
    };
    match prev.kind {
        Kind::Punct => !matches!(prev.text(src), ")" | "]" | "}"),
        Kind::Ident => matches!(
            prev.text(src),
            "return"
                | "typeof"
                | "case"
                | "do"
                | "else"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "throw"
                | "yield"
                | "await"
        ),
        _ => false,
    }
}

/// `<<<ID` / `<<<"ID"` / `<<<'ID'` up to the closing identifier.
fn skip_heredoc(src: &str, i: usize) -> Option<usize> {
    let rest = &src[i + 3..];
    let label_line = &rest[..rest.find('\n')?];
    let label = label_line.trim().trim_matches(|c| c == '"' || c == '\'');
    if label.is_empty()
        || !label
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return None;
    }
    let body_start = i + 3 + label_line.len() + 1;
    let mut pos = body_start;
    for line in src[body_start..].split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with(label)
            && !trimmed[label.len()..]
                .bytes()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Some(pos + (line.len() - trimmed.len()) + label.len());
        }
        pos += line.len();
    }
    Some(src.len())
}

fn lex_lua(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let n = b.len();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < n {
        let c = b[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'-' && b.get(i + 1) == Some(&b'-') {
            i += 2;
            if let Some(end) = long_bracket_end(src, i) {
                i = end;
            } else {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
            }
            toks.push(Tok {
                kind: Kind::Comment,
                start,
                end: i,
            });
            continue;
        }
        if c == b'[' {
            if let Some(end) = long_bracket_end(src, i) {
                i = end;
                toks.push(Tok {
                    kind: Kind::Str,
                    start,
                    end: i,
                });
                continue;
            }
        }
        if c == b'\'' || c == b'"' {
            i = skip_quoted(b, i + 1, c);
            toks.push(Tok {
                kind: Kind::Str,
                start,
                end: i,
            });
            continue;
        }
        if ident_start(c, Syntax::Lua) {
            while i < n && ident_char(b[i], Syntax::Lua) {
                i += 1;
            }
            toks.push(Tok {
                kind: Kind::Ident,
                start,
                end: i,
            });
            continue;
        }
        if c.is_ascii_digit() {
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'.') {
                i += 1;
            }
            toks.push(Tok {
                kind: Kind::Number,
                start,
                end: i,
            });
            continue;
        }
        let len = TWO_CHAR
            .iter()
            .find(|p| src[i..].starts_with(**p))
            .map_or(1, |p| p.len());
        i += len;
        toks.push(Tok {
            kind: Kind::Punct,
            start,
            end: i,
        });
    }
    toks
}

/// End of a Lua long bracket (`[[ … ]]`, `[==[ … ]==]`) starting at `i`.
fn long_bracket_end(src: &str, i: usize) -> Option<usize> {
    let rest = src[i..].strip_prefix('[')?;
    let eq = rest.bytes().take_while(|&c| c == b'=').count();
    if rest.as_bytes().get(eq) != Some(&b'[') {
        return None;
    }
    let close = format!("]{}]", "=".repeat(eq));
    let body = i + 1 + eq + 1;
    Some(
        src[body..]
            .find(&close)
            .map_or(src.len(), |p| body + p + close.len()),
    )
}

/// Matching bracket index for every `(`, `[`, `{` and its partner (in token indices).
pub(crate) fn match_brackets(src: &str, toks: &[Tok]) -> Vec<Option<usize>> {
    let mut partner = vec![None; toks.len()];
    let mut stack: Vec<(u8, usize)> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if t.kind != Kind::Punct {
            continue;
        }
        let c = t.text(src).as_bytes()[0];
        match c {
            b'(' | b'[' | b'{' => stack.push((c, i)),
            b')' | b']' | b'}' => {
                let open = match c {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                if let Some(pos) = stack.iter().rposition(|(o, _)| *o == open) {
                    let (_, j) = stack[pos];
                    stack.truncate(pos);
                    partner[i] = Some(j);
                    partner[j] = Some(i);
                }
            }
            _ => {}
        }
    }
    partner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str, syntax: Syntax) -> Vec<(Kind, &str)> {
        lex(src, syntax)
            .iter()
            .map(|t| (t.kind, t.text(src)))
            .collect()
    }

    #[test]
    fn a_regex_ending_in_a_backslash_stays_inside_the_text() {
        assert_eq!(skip_regex(b"a\\", 0), 2);
        let src = "x = /a\\";
        assert!(lex(src, Syntax::Js).iter().all(|t| t.end <= src.len()));
    }

    #[test]
    fn lexes_php_with_html_strings_and_comments() {
        let src = "<p><?php $a = 'x}'; // c ?>\n<b><?= $b->c ?>";
        let k = kinds(src, Syntax::Php);
        assert_eq!(k[0], (Kind::Html, "<p>"));
        assert_eq!(k[1], (Kind::OpenTag, "<?php"));
        assert_eq!(k[2], (Kind::Var, "$a"));
        assert_eq!(k[4], (Kind::Str, "'x}'"));
        assert_eq!(k[6], (Kind::Comment, "// c "));
        assert_eq!(k[7], (Kind::CloseTag, "?>"));
        assert_eq!(k[8], (Kind::Html, "\n<b>"));
        assert!(k.contains(&(Kind::Punct, "->")));
        assert!(k.contains(&(Kind::Ident, "c")));
    }

    #[test]
    fn lexes_heredoc_attributes_and_namespaces() {
        let src = "<?php\n#[Attr]\nnew \\App\\X(<<<EOT\n}{\nEOT);";
        let k = kinds(src, Syntax::Php);
        assert!(k.contains(&(Kind::Punct, "#")));
        assert!(k.contains(&(Kind::Ident, "\\App\\X")));
        assert!(k
            .iter()
            .any(|(kind, t)| *kind == Kind::Str && t.ends_with("EOT")));
        assert!(!k.contains(&(Kind::Punct, "{")));
    }

    #[test]
    fn lexes_javascript_templates_and_regexes() {
        let src = "let a = `x${ {b: 1} }y`; const r = /}/g; f(a) / 2";
        let k = kinds(src, Syntax::Js);
        assert!(k.contains(&(Kind::Str, "`x${ {b: 1} }y`")));
        assert!(k.contains(&(Kind::Str, "/}/g")));
        assert!(k.contains(&(Kind::Punct, "/")));
    }

    #[test]
    fn lexes_lua_long_strings_and_comments() {
        let src = "local s = [[a\nend]] --[[ if ]] -- end\nif x then end";
        let k = kinds(src, Syntax::Lua);
        assert!(k.contains(&(Kind::Str, "[[a\nend]]")));
        assert_eq!(
            k.iter()
                .filter(|(kind, t)| *kind == Kind::Ident && *t == "end")
                .count(),
            1
        );
    }

    #[test]
    fn matches_brackets() {
        let src = "f({a: [1]}, (2))";
        let toks = lex(src, Syntax::Js);
        let m = match_brackets(src, &toks);
        let open = toks.iter().position(|t| t.text(src) == "{").unwrap();
        assert_eq!(toks[m[open].unwrap()].text(src), "}");
    }
}
