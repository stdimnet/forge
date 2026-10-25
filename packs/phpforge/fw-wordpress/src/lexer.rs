//! A small, error tolerant PHP tokenizer: enough to find calls, their arguments and string
//! literals while the user types (unclosed strings and parentheses are fine). Inline HTML
//! outside `<?php … ?>`, comments and whitespace produce no tokens.

/// What a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokKind {
    /// A name: `add_action`, `\Foo\bar`, `function`, `true`.
    Ident,
    /// `$name`.
    Variable,
    /// A single or double quoted string, a heredoc / nowdoc or a backtick string; the text
    /// includes the quotes.
    Str,
    Number,
    /// An operator or punctuation: `(`, `,`, `->`, `===`.
    Punct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    pub kind: TokKind,
    pub text: &'a str,
    /// Byte offsets into the source.
    pub start: usize,
    pub end: usize,
}

impl Token<'_> {
    pub fn is(&self, punct: &str) -> bool {
        self.kind == TokKind::Punct && self.text == punct
    }

    pub fn is_ident(&self, name: &str) -> bool {
        self.kind == TokKind::Ident && self.text.eq_ignore_ascii_case(name)
    }
}

const PUNCT3: &[&str] = &[
    "===", "!==", "<=>", "**=", "...", "<<=", ">>=", "??=", "?->",
];
const PUNCT2: &[&str] = &[
    "==", "!=", "<>", "<=", ">=", "&&", "||", "->", "=>", "::", "++", "--", "+=", "-=", "*=", "/=",
    ".=", "%=", "&=", "|=", "^=", "<<", ">>", "??", "**",
];

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// Tokenize `src`. Starts in HTML mode unless `php_only` (for snippets without `<?php`).
pub fn tokenize(src: &str) -> Vec<Token<'_>> {
    tokenize_from(src, false)
}

pub fn tokenize_from<'a>(src: &'a str, php_only: bool) -> Vec<Token<'a>> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    let mut in_php = php_only;
    let push = |out: &mut Vec<Token<'a>>, kind: TokKind, start: usize, end: usize| {
        out.push(Token {
            kind,
            text: &src[start..end],
            start,
            end,
        });
    };
    while i < n {
        if !in_php {
            match src[i..].find("<?") {
                Some(p) => {
                    i += p + 2;
                    if src[i..].len() >= 3 && src[i..i + 3].eq_ignore_ascii_case("php") {
                        i += 3;
                    } else if b.get(i) == Some(&b'=') {
                        i += 1;
                    }
                    in_php = true;
                    continue;
                }
                None => break,
            }
        }
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'?' && b.get(i + 1) == Some(&b'>') {
            i += 2;
            in_php = php_only;
            continue;
        }
        // Comments (`#[` starts an attribute, not a comment).
        if (c == b'#' && b.get(i + 1) != Some(&b'[')) || (c == b'/' && b.get(i + 1) == Some(&b'/'))
        {
            while i < n && b[i] != b'\n' {
                if b[i] == b'?' && b.get(i + 1) == Some(&b'>') {
                    break;
                }
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = match src[i + 2..].find("*/") {
                Some(p) => i + 2 + p + 2,
                None => n,
            };
            continue;
        }
        let start = i;
        if c == b'\'' || c == b'"' || c == b'`' {
            i += 1;
            while i < n && b[i] != c {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(n);
            push(&mut out, TokKind::Str, start, i);
            continue;
        }
        if src[i..].starts_with("<<<") {
            if let Some(end) = heredoc_end(src, i) {
                i = end;
                push(&mut out, TokKind::Str, start, i);
                continue;
            }
        }
        if c == b'$' && b.get(i + 1).is_some_and(|&d| is_name_start(d)) {
            i += 1;
            while i < n && is_name_char(b[i]) {
                i += 1;
            }
            push(&mut out, TokKind::Variable, start, i);
            continue;
        }
        if is_name_start(c) || (c == b'\\' && b.get(i + 1).is_some_and(|&d| is_name_start(d))) {
            i += 1;
            while i < n && (is_name_char(b[i]) || b[i] == b'\\') {
                i += 1;
            }
            push(&mut out, TokKind::Ident, start, i);
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            i += 1;
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            push(&mut out, TokKind::Number, start, i);
            continue;
        }
        let rest = &src[i..];
        let len = PUNCT3
            .iter()
            .chain(PUNCT2)
            .find(|p| rest.starts_with(**p))
            .map(|p| p.len())
            .unwrap_or_else(|| rest.chars().next().map(char::len_utf8).unwrap_or(1));
        i += len;
        push(&mut out, TokKind::Punct, start, i);
    }
    out
}

/// End (exclusive) of a heredoc / nowdoc starting at `at` (`<<<`).
fn heredoc_end(src: &str, at: usize) -> Option<usize> {
    let head = &src[at + 3..];
    let line_end = head.find('\n')?;
    let label = head[..line_end]
        .trim()
        .trim_matches(|c| c == '\'' || c == '"');
    if label.is_empty() || !label.bytes().all(is_name_char) {
        return None;
    }
    let body_start = at + 3 + line_end + 1;
    let mut pos = body_start;
    for line in src[body_start..].split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with(label) && !t[label.len()..].bytes().next().is_some_and(is_name_char) {
            let indent = line.len() - t.len();
            return Some(pos + indent + label.len());
        }
        pos += line.len();
    }
    Some(src.len())
}

/// The value of a string token and whether it interpolates variables (`"{$x}"`, `"$x"`).
/// Heredocs and backtick strings count as interpolated.
pub fn string_value(text: &str) -> (String, bool) {
    if let Some(inner) = text.strip_prefix('\'') {
        let inner = inner.strip_suffix('\'').unwrap_or(inner);
        return (inner.replace("\\'", "'").replace("\\\\", "\\"), false);
    }
    if let Some(inner) = text.strip_prefix('"') {
        let inner = inner.strip_suffix('"').unwrap_or(inner);
        let bytes = inner.as_bytes();
        let interpolated = bytes.iter().enumerate().any(|(i, &c)| {
            c == b'$'
                && bytes
                    .get(i + 1)
                    .is_some_and(|&d| is_name_start(d) || d == b'{')
                && (i == 0 || bytes[i - 1] != b'\\')
        }) || inner.contains("{$");
        return (
            inner.replace("\\\"", "\"").replace("\\\\", "\\"),
            interpolated,
        );
    }
    (text.to_string(), true)
}

/// Byte range of a string token's content (without the quotes).
pub fn string_content(tok: &Token<'_>) -> (usize, usize) {
    let open = tok.text.chars().next().map(char::len_utf8).unwrap_or(0);
    let close = usize::from(tok.text.len() > open && tok.text.ends_with(&tok.text[..open]));
    (tok.start + open, (tok.end - close).max(tok.start + open))
}

/// Index of the token matching the opening bracket at `open` (`(`, `[`, `{`), or the
/// last token when it is unclosed.
pub fn matching(tokens: &[Token<'_>], open: usize) -> usize {
    let mut depth = 0i32;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        if t.kind != TokKind::Punct {
            continue;
        }
        match t.text {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    tokens.len().saturating_sub(1)
}

/// The arguments of a call whose `(` is at `open`: token index ranges, split at top-level
/// commas.
pub fn arguments(tokens: &[Token<'_>], open: usize) -> Vec<std::ops::Range<usize>> {
    let close = matching(tokens, open);
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut start = open + 1;
    let end = if tokens.get(close).is_some_and(|t| t.is(")")) {
        close
    } else {
        tokens.len()
    };
    for (i, t) in tokens.iter().enumerate().take(end).skip(open + 1) {
        if t.kind != TokKind::Punct {
            continue;
        }
        match t.text {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => {
                args.push(start..i);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < end || !args.is_empty() {
        args.push(start..end);
    }
    args
}

/// 0-based line and byte column of `offset`.
pub fn line_col(src: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(src.len());
    let before = &src[..offset];
    let line = before.bytes().filter(|&c| c == b'\n').count() as u32;
    let col = before.rfind('\n').map(|p| offset - p - 1).unwrap_or(offset) as u32;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(TokKind, &str)> {
        tokenize(src)
            .into_iter()
            .map(|t| (t.kind, t.text))
            .collect()
    }

    #[test]
    fn skips_html_and_comments() {
        let toks = kinds("<h1>x</h1><?php // add_action('no')\n/* do_action('no') */ # x\nfoo('a', $b); ?><p>bar(</p>");
        assert_eq!(
            toks,
            vec![
                (TokKind::Ident, "foo"),
                (TokKind::Punct, "("),
                (TokKind::Str, "'a'"),
                (TokKind::Punct, ","),
                (TokKind::Variable, "$b"),
                (TokKind::Punct, ")"),
                (TokKind::Punct, ";"),
            ]
        );
    }

    #[test]
    fn strings_operators_and_heredocs() {
        let toks = kinds("<?php $a === \"x\\\"y\" ?-> <<<EOT\nhello $name\nEOT;\n#[Attr] 1.5");
        assert_eq!(toks[0], (TokKind::Variable, "$a"));
        assert_eq!(toks[1], (TokKind::Punct, "==="));
        assert_eq!(toks[2], (TokKind::Str, "\"x\\\"y\""));
        assert_eq!(toks[3], (TokKind::Punct, "?->"));
        assert_eq!(toks[4], (TokKind::Str, "<<<EOT\nhello $name\nEOT"));
        assert_eq!(toks[5], (TokKind::Punct, ";"));
        assert_eq!(toks[6], (TokKind::Punct, "#"));
        assert_eq!(toks.last().unwrap(), &(TokKind::Number, "1.5"));
    }

    #[test]
    fn string_values_and_interpolation() {
        assert_eq!(string_value("'it\\'s'"), ("it's".into(), false));
        assert_eq!(
            string_value("\"save_{$type}\""),
            ("save_{$type}".into(), true)
        );
        assert_eq!(string_value("\"cost \\$5\""), ("cost \\$5".into(), false));
        assert_eq!(string_value("\"init\""), ("init".into(), false));
        let src = "<?php 'abc";
        let t = tokenize(src)[0];
        assert_eq!(string_content(&t), (7, 10), "unclosed string");
    }

    #[test]
    fn splits_arguments() {
        let src = "<?php f('a', [ $this, 'b' ], fn($x) => g($x, 1), 10);";
        let toks = tokenize(src);
        let args = arguments(&toks, 1);
        assert_eq!(args.len(), 4);
        assert_eq!(toks[args[0].start].text, "'a'");
        assert_eq!(toks[args[3].start].text, "10");
        let unclosed = tokenize("<?php f('a', ");
        assert_eq!(arguments(&unclosed, 1).len(), 2);
    }

    #[test]
    fn line_and_column() {
        assert_eq!(line_col("a\nbc\nd", 4), (1, 2));
        assert_eq!(line_col("abc", 1), (0, 1));
    }
}
