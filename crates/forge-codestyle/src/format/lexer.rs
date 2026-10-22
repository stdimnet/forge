//! A lossless tokenizer for the languages Forge formats itself (PHP, JavaScript/TypeScript,
//! CSS, JSON, Lua, SQL): every character of the input belongs to a token or to the
//! whitespace before one, so a formatter that changes nothing gives back the input.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Php,
    Js,
    Ts,
    Css,
    Json,
    Lua,
    Sql,
}

impl Lang {
    pub fn is_js(self) -> bool {
        matches!(self, Lang::Js | Lang::Ts)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Identifiers and keywords (`function`, `App\Models\User`, `font-size`, `#fff`).
    Word,
    /// PHP variables (`$user`), SCSS variables, SQL parameters.
    Var,
    Num,
    /// Strings, including multi-line ones (heredocs, template literals, Lua long strings).
    Str,
    Comment,
    Op,
    Open,
    Close,
    /// PHP type casts (`(int)`).
    Cast,
    /// Text that is never formatted (HTML outside PHP tags).
    Verbatim,
    /// `<?php`, `<?=` and `?>`.
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tok {
    pub kind: Kind,
    pub text: String,
    /// The whitespace before the token, as in the input.
    pub ws: String,
}

impl Tok {
    pub fn new(kind: Kind, text: &str, ws: &str) -> Self {
        Self {
            kind,
            text: text.into(),
            ws: ws.into(),
        }
    }
    /// Line breaks in the whitespace before the token.
    pub fn nl(&self) -> usize {
        self.ws.matches('\n').count()
    }
    pub fn is(&self, text: &str) -> bool {
        self.text == text
    }
    pub fn is_word(&self, word: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(word)
    }
    pub fn is_op(&self, op: &str) -> bool {
        self.kind == Kind::Op && self.text == op
    }
}

const PHP_OPS: &[&str] = &[
    "<=>", "**=", "...", "<<=", ">>=", "===", "!==", "??=", "?->", "->", "=>", "::", "++", "--",
    "&&", "||", "??", "==", "!=", "<>", "<=", ">=", "+=", "-=", "*=", "/=", ".=", "%=", "&=", "|=",
    "^=", "<<", ">>", "**",
];
const JS_OPS: &[&str] = &[
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "=>", "?.", "??",
    "&&", "||", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>",
    "**", "++", "--",
];
const CSS_OPS: &[&str] = &["::", "~=", "|=", "^=", "$=", "*="];
const LUA_OPS: &[&str] = &["...", "..", "==", "~=", "<=", ">=", "//", "::", "<<", ">>"];
const SQL_OPS: &[&str] = &["<>", "!=", "<=", ">=", "||", "::", "->>", "->"];
const CASTS: &[&str] = &[
    "int", "integer", "bool", "boolean", "float", "double", "real", "string", "array", "object",
    "unset", "binary",
];

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    lang: Lang,
    out: Vec<Tok>,
    ws_start: usize,
    /// PHP: inside `<?php … ?>`.
    in_php: bool,
}

fn is_word_start(c: char, lang: Lang) -> bool {
    c.is_alphabetic()
        || c == '_'
        || (lang == Lang::Php && c == '\\')
        || (lang.is_js() && c == '$')
        || (lang == Lang::Css && (c == '-' || c == '@'))
        || (lang == Lang::Sql && c == '@')
        || !c.is_ascii() && !c.is_whitespace()
}

fn is_word_char(c: char, lang: Lang) -> bool {
    c.is_alphanumeric()
        || c == '_'
        || (lang == Lang::Php && c == '\\')
        || (lang.is_js() && c == '$')
        || (lang == Lang::Css && c == '-')
        || !c.is_ascii() && !c.is_whitespace()
}

impl<'a> Lexer<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }
    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }
    fn peek_at(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }
    fn push(&mut self, kind: Kind, end: usize) {
        let ws = &self.src[self.ws_start..self.pos];
        self.out.push(Tok::new(kind, &self.src[self.pos..end], ws));
        self.pos = end;
        self.ws_start = end;
    }
    /// The last token that is not a comment.
    fn prev(&self) -> Option<&Tok> {
        self.out.iter().rev().find(|t| t.kind != Kind::Comment)
    }
    /// After this token a `/` starts a regular expression (JavaScript).
    fn regex_allowed(&self) -> bool {
        match self.prev() {
            None => true,
            Some(t) => match t.kind {
                Kind::Num | Kind::Str | Kind::Var | Kind::Close => false,
                Kind::Word => matches!(
                    t.text.as_str(),
                    "return"
                        | "typeof"
                        | "instanceof"
                        | "in"
                        | "of"
                        | "new"
                        | "delete"
                        | "void"
                        | "throw"
                        | "case"
                        | "do"
                        | "else"
                        | "yield"
                        | "await"
                ),
                _ => !(t.is("++") || t.is("--")),
            },
        }
    }

    fn quoted(&self, quote: char) -> usize {
        let mut chars = self.rest().char_indices().skip(1);
        while let Some((i, c)) = chars.next() {
            if c == '\\' {
                chars.next();
            } else if c == quote {
                return self.pos + i + c.len_utf8();
            } else if c == '\n' && quote != '`' && self.lang != Lang::Php && self.lang != Lang::Sql
            {
                return self.pos + i;
            }
        }
        self.src.len()
    }

    fn line_end(&self, stop_at_php_close: bool) -> usize {
        let rest = self.rest();
        let mut end = rest.find('\n').unwrap_or(rest.len());
        if stop_at_php_close {
            if let Some(close) = rest[..end].find("?>") {
                end = close;
            }
        }
        self.pos + end
    }

    fn block_comment(&self) -> usize {
        self.rest()
            .get(2..)
            .and_then(|r| r.find("*/"))
            .map_or(self.src.len(), |i| self.pos + 2 + i + 2)
    }

    /// PHP heredoc / nowdoc starting at `<<<`.
    fn heredoc(&self) -> Option<usize> {
        let rest = self.rest();
        let header_end = rest.find('\n')?;
        let label = rest[3..header_end]
            .trim()
            .trim_matches(|c| c == '\'' || c == '"');
        if label.is_empty() || !label.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        let mut at = header_end + 1;
        loop {
            let line_end = rest[at..].find('\n').map_or(rest.len(), |i| at + i);
            let line = &rest[at..line_end];
            let trimmed = line.trim_start();
            if let Some(after) = trimmed.strip_prefix(label) {
                if !after
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
                {
                    return Some(self.pos + at + (line.len() - trimmed.len()) + label.len());
                }
            }
            if line_end >= rest.len() {
                return Some(self.src.len());
            }
            at = line_end + 1;
        }
    }

    /// A Lua long bracket (`[[`, `[==[`) at the current position (after `--` for comments).
    fn long_bracket(&self, at: usize) -> Option<usize> {
        let rest = &self.src[at..];
        if !rest.starts_with('[') {
            return None;
        }
        let eq = rest[1..].chars().take_while(|c| *c == '=').count();
        if rest[1 + eq..].starts_with('[') {
            let close = format!("]{}]", "=".repeat(eq));
            Some(
                rest.find(&close)
                    .map_or(self.src.len(), |i| at + i + close.len()),
            )
        } else {
            None
        }
    }

    fn template(&self) -> usize {
        // Backtick string with `${…}` substitutions (nested braces and strings).
        let bytes: Vec<(usize, char)> = self.rest().char_indices().collect();
        let mut i = 1;
        let mut depth = 0usize;
        while i < bytes.len() {
            let c = bytes[i].1;
            match c {
                '\\' => i += 1,
                '`' if depth == 0 => return self.pos + bytes[i].0 + 1,
                '$' if depth == 0 && bytes.get(i + 1).is_some_and(|b| b.1 == '{') => {
                    depth = 1;
                    i += 1;
                }
                '{' if depth > 0 => depth += 1,
                '}' if depth > 0 => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        self.src.len()
    }

    fn number(&self) -> usize {
        let rest = self.rest();
        let mut end = 0;
        let mut prev = ' ';
        for (i, c) in rest.char_indices() {
            let ok = c.is_ascii_alphanumeric()
                || c == '_'
                || (c == '.'
                    && !rest[i..].starts_with("..")
                    && !(self.lang == Lang::Php && prev == '.'))
                || ((c == '+' || c == '-')
                    && (prev == 'e' || prev == 'E')
                    && !rest.starts_with("0x"))
                || (self.lang == Lang::Css && c == '%');
            if !ok {
                break;
            }
            end = i + c.len_utf8();
            prev = c;
        }
        self.pos + end.max(1)
    }

    fn op(&self, ops: &[&str]) -> usize {
        let rest = self.rest();
        for op in ops {
            if rest.starts_with(op) {
                return self.pos + op.len();
            }
        }
        self.pos + self.peek().map_or(1, char::len_utf8)
    }

    fn run(mut self) -> Vec<Tok> {
        if self.lang != Lang::Php {
            self.in_php = true;
        }
        while self.pos < self.src.len() {
            if !self.in_php {
                // Inline HTML up to the next PHP open tag.
                let rest = self.rest();
                let mut at = 0;
                let open = loop {
                    match rest[at..].find("<?") {
                        Some(i) => {
                            let s = &rest[at + i..];
                            if s.starts_with("<?php") || s.starts_with("<?=") {
                                break Some(at + i);
                            }
                            if !s[2..].starts_with("xml") {
                                break Some(at + i);
                            }
                            at += i + 2;
                        }
                        None => break None,
                    }
                };
                match open {
                    Some(0) => {
                        let len = if rest.starts_with("<?php") {
                            5
                        } else if rest.starts_with("<?=") {
                            3
                        } else {
                            2
                        };
                        let end = self.pos + len;
                        self.push(Kind::Tag, end);
                        self.in_php = true;
                    }
                    Some(i) => {
                        let end = self.pos + i;
                        self.push(Kind::Verbatim, end);
                    }
                    None => {
                        let end = self.src.len();
                        self.push(Kind::Verbatim, end);
                    }
                }
                continue;
            }
            let c = self.peek().unwrap();
            if c.is_whitespace() {
                self.pos += c.len_utf8();
                continue;
            }
            let rest = self.rest();
            let lang = self.lang;
            // Comments.
            if (rest.starts_with("//") && lang != Lang::Lua && lang != Lang::Sql)
                || (c == '#' && lang == Lang::Php && !rest.starts_with("#["))
                || (rest.starts_with("--") && matches!(lang, Lang::Lua | Lang::Sql))
                || (c == '#' && lang == Lang::Sql)
            {
                let end = if lang == Lang::Lua {
                    self.long_bracket(self.pos + 2)
                        .unwrap_or_else(|| self.line_end(false))
                } else {
                    self.line_end(lang == Lang::Php)
                };
                self.push(Kind::Comment, end);
                continue;
            }
            if rest.starts_with("/*") && lang != Lang::Lua {
                let end = self.block_comment();
                self.push(Kind::Comment, end);
                continue;
            }
            if lang == Lang::Php && rest.starts_with("?>") {
                let mut end = self.pos + 2;
                if self.src[end..].starts_with('\n') {
                    end += 1;
                }
                self.push(Kind::Tag, end);
                self.in_php = false;
                continue;
            }
            // Strings.
            if c == '\'' || c == '"' || (c == '`' && lang != Lang::Css) {
                let end = if c == '`' && lang.is_js() {
                    self.template()
                } else {
                    self.quoted(c)
                };
                self.push(Kind::Str, end);
                continue;
            }
            if lang == Lang::Php && rest.starts_with("<<<") {
                if let Some(end) = self.heredoc() {
                    self.push(Kind::Str, end);
                    continue;
                }
            }
            if lang == Lang::Lua && c == '[' {
                if let Some(end) = self.long_bracket(self.pos) {
                    self.push(Kind::Str, end);
                    continue;
                }
            }
            if lang.is_js() && c == '/' && self.regex_allowed() {
                let mut in_class = false;
                let mut end = self.src.len();
                let mut chars = rest.char_indices().skip(1);
                while let Some((i, ch)) = chars.next() {
                    match ch {
                        '\\' => {
                            chars.next();
                        }
                        '[' => in_class = true,
                        ']' => in_class = false,
                        '/' if !in_class => {
                            end = self.pos + i + 1;
                            break;
                        }
                        '\n' => {
                            end = self.pos + i;
                            break;
                        }
                        _ => {}
                    }
                }
                while self.src[end..]
                    .chars()
                    .next()
                    .is_some_and(|f| f.is_ascii_alphabetic())
                {
                    end += 1;
                }
                self.push(Kind::Str, end);
                continue;
            }
            // PHP casts.
            if lang == Lang::Php && c == '(' {
                let inner_end = rest.find(')').unwrap_or(0);
                let inner = rest.get(1..inner_end).unwrap_or("").trim();
                if inner_end > 0
                    && inner_end < 12
                    && CASTS.iter().any(|t| t.eq_ignore_ascii_case(inner))
                {
                    let end = self.pos + inner_end + 1;
                    self.push(Kind::Cast, end);
                    continue;
                }
            }
            // Variables.
            if (lang == Lang::Php && c == '$')
                || (lang == Lang::Css && c == '$')
                || (lang == Lang::Sql
                    && c == ':'
                    && self.peek_at(1).is_some_and(char::is_alphabetic))
            {
                let mut end = self.pos + 1;
                while self.src[end..].starts_with('$') {
                    end += 1;
                }
                let tail = self.src[end..]
                    .char_indices()
                    .find(|(_, ch)| {
                        !(ch.is_alphanumeric() || *ch == '_' || (lang == Lang::Css && *ch == '-'))
                    })
                    .map_or(self.src.len() - end, |(i, _)| i);
                if tail > 0 {
                    self.push(Kind::Var, end + tail);
                    continue;
                }
            }
            if lang == Lang::Css
                && rest.starts_with("url(")
                && !rest[4..].trim_start().starts_with(['"', '\''])
            {
                let end = rest.find(')').map_or(self.src.len(), |i| self.pos + i + 1);
                self.push(Kind::Str, end);
                continue;
            }
            if lang == Lang::Css
                && (c == '#' || c == '!' || c == '.')
                && self
                    .peek_at(1)
                    .is_some_and(|n| n.is_alphanumeric() || n == '_' || n == '-')
                && !(c == '.' && self.peek_at(1).is_some_and(|n| n.is_ascii_digit()))
            {
                let tail = rest[1..]
                    .char_indices()
                    .find(|(_, ch)| !is_word_char(*ch, lang))
                    .map_or(rest.len() - 1, |(i, _)| i);
                self.push(Kind::Word, self.pos + 1 + tail);
                continue;
            }
            if c.is_ascii_digit()
                || (c == '.' && self.peek_at(1).is_some_and(|n| n.is_ascii_digit()))
            {
                let end = self.number();
                self.push(Kind::Num, end);
                continue;
            }
            if is_word_start(c, lang) {
                let tail = rest
                    .char_indices()
                    .skip(1)
                    .find(|(_, ch)| !is_word_char(*ch, lang))
                    .map_or(rest.len(), |(i, _)| i);
                self.push(Kind::Word, self.pos + tail);
                continue;
            }
            if lang.is_js() && c == '#' && self.peek_at(1).is_some_and(|n| is_word_start(n, lang)) {
                let tail = rest[1..]
                    .char_indices()
                    .find(|(_, ch)| !is_word_char(*ch, lang))
                    .map_or(rest.len() - 1, |(i, _)| i);
                self.push(Kind::Word, self.pos + 1 + tail);
                continue;
            }
            if lang == Lang::Php && rest.starts_with("#[") {
                let end = self.pos + 2;
                self.push(Kind::Open, end);
                continue;
            }
            if matches!(c, '(' | '[' | '{') {
                let end = self.pos + 1;
                self.push(Kind::Open, end);
                continue;
            }
            if matches!(c, ')' | ']' | '}') {
                let end = self.pos + 1;
                self.push(Kind::Close, end);
                continue;
            }
            let ops = match lang {
                Lang::Php => PHP_OPS,
                Lang::Js | Lang::Ts => JS_OPS,
                Lang::Css => CSS_OPS,
                Lang::Json => &[],
                Lang::Lua => LUA_OPS,
                Lang::Sql => SQL_OPS,
            };
            let end = self.op(ops);
            self.push(Kind::Op, end);
        }
        // Trailing whitespace becomes an empty token so nothing is lost.
        if self.ws_start < self.src.len() || self.out.is_empty() {
            let ws = &self.src[self.ws_start..];
            self.out.push(Tok::new(Kind::Verbatim, "", ws));
        }
        self.out
    }
}

/// Tokenize `src`. Joining every token's `ws` and `text` gives back `src`.
pub fn lex(src: &str, lang: Lang) -> Vec<Tok> {
    Lexer {
        src,
        pos: 0,
        lang,
        out: Vec::new(),
        ws_start: 0,
        in_php: false,
    }
    .run()
}

/// The text of a token list.
pub fn join(tokens: &[Tok]) -> String {
    let mut out = String::new();
    for t in tokens {
        out.push_str(&t.ws);
        out.push_str(&t.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str, lang: Lang) -> Vec<(Kind, String)> {
        lex(src, lang)
            .into_iter()
            .filter(|t| !t.text.is_empty())
            .map(|t| (t.kind, t.text))
            .collect()
    }

    #[test]
    fn php_is_lossless() {
        let src = "<p>x</p>\n<?php\n$a = (int) $b . 'x' ?? \"y\"; # c\n$h = <<<EOT\n  a {$b}\n  EOT;\n#[Attr]\nfn($x) => $x?->y;\n?>\n<b><?= $a ?></b>\n";
        let toks = lex(src, Lang::Php);
        assert_eq!(join(&toks), src);
        let k = kinds(src, Lang::Php);
        assert_eq!(k[0], (Kind::Verbatim, "<p>x</p>\n".into()));
        assert!(k.contains(&(Kind::Cast, "(int)".into())));
        assert!(k.contains(&(Kind::Comment, "# c".into())));
        assert!(k.contains(&(Kind::Open, "#[".into())));
        assert!(k.contains(&(Kind::Op, "?->".into())));
        assert!(k
            .iter()
            .any(|(k, t)| *k == Kind::Str && t.starts_with("<<<EOT") && t.ends_with("EOT")));
    }

    #[test]
    fn js_regex_and_templates() {
        let src = "const a = b / 2 / c;\nconst r = /[/]+/g.test(`x${ {a: 1}.a }`);\n";
        assert_eq!(join(&lex(src, Lang::Js)), src);
        let k = kinds(src, Lang::Js);
        assert!(k.contains(&(Kind::Str, "/[/]+/g".into())));
        assert!(k.contains(&(Kind::Str, "`x${ {a: 1}.a }`".into())));
        assert_eq!(
            k.iter().filter(|(k, t)| *k == Kind::Op && t == "/").count(),
            2
        );
    }

    #[test]
    fn css_lua_sql() {
        let css = ".a:hover > #id { color: #FFF !important; background: url(http://x/y.png); width: 10.5px; }";
        assert_eq!(join(&lex(css, Lang::Css)), css);
        let k = kinds(css, Lang::Css);
        assert!(k.contains(&(Kind::Word, ".a".into())));
        assert!(k.contains(&(Kind::Word, "#FFF".into())));
        assert!(k.contains(&(Kind::Str, "url(http://x/y.png)".into())));
        assert!(k.contains(&(Kind::Num, "10.5px".into())));
        let lua = "local s = [[a\nb]] -- c\n--[[ d ]] x = a..b";
        assert_eq!(join(&lex(lua, Lang::Lua)), lua);
        assert!(kinds(lua, Lang::Lua).contains(&(Kind::Op, "..".into())));
        let sql = "select a, b from t where x = :id -- c\n";
        assert_eq!(join(&lex(sql, Lang::Sql)), sql);
        assert!(kinds(sql, Lang::Sql).contains(&(Kind::Var, ":id".into())));
    }
}
