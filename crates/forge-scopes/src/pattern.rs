//! PHPStorm's scope language (Settings | Appearance & Behavior | Scopes):
//!
//! - `file:app//*.php` — files by path. `/` separates folders, `*` matches within one name,
//!   `?` one character, `//` any number of folders (`app//*` = everything below `app`).
//!   A pattern without `/` matches the file name in every folder (`file:*.php`).
//! - `src:`, `test:` and `lib:` — the same patterns, limited to production files, test files
//!   and libraries (`vendor/`, `node_modules/`). Package patterns like `src:*..*` mean all.
//! - `file[module]:…` — the module name is accepted and ignored (one module per project).
//! - `&&`, `||`, `!` and parentheses; `!` binds tightest, then `&&`, then `||`.

use serde::{Deserialize, Serialize};

use crate::context::ScopeContext;

/// What a pattern atom looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Any file.
    File,
    /// Production files (not tests, not libraries).
    Src,
    /// Test files.
    Test,
    /// Libraries.
    Lib,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Char(char),
    /// `*`: any characters except `/`.
    Star,
    /// `?`: one character except `/`.
    Any,
    /// `//`: one or more `/` with any folders between (`a//b` = `a/b`, `a/x/b`, …).
    Deep,
}

/// A compiled path pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPattern {
    tokens: Vec<Token>,
    /// No `/` in the pattern: it matches the file name.
    name_only: bool,
    /// `*..*`: everything.
    all: bool,
}

impl PathPattern {
    pub fn new(text: &str) -> Self {
        let text = text.trim();
        // No `/`, or a leading `//` (any folder) before a plain name: match file names.
        let name_only = !text.trim_start_matches("//").contains('/');
        // Java package patterns (`*..*`, `com.acme..*`) have no meaning for files; `*..*` is
        // everything and other ones become folder patterns.
        if text == "*..*" {
            return PathPattern {
                tokens: Vec::new(),
                name_only: false,
                all: true,
            };
        }
        let text = if !text.contains('/') && text.contains("..") {
            text.replace("..", "//").replace('.', "/")
        } else {
            text.to_string()
        };
        let text = text.trim_start_matches('/');
        let text = match text.strip_suffix('/') {
            // `file:app/` is the folder with everything below it.
            Some(dir) if !dir.ends_with('/') => format!("{dir}//*"),
            _ => text.to_string(),
        };
        let chars: Vec<char> = text.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '/' if chars.get(i + 1) == Some(&'/') => {
                    while chars.get(i + 1) == Some(&'/') {
                        i += 1;
                    }
                    tokens.push(Token::Deep);
                }
                '*' => {
                    // `**` behaves like `*` followed by `//*` would: any depth.
                    if chars.get(i + 1) == Some(&'*') {
                        while chars.get(i + 1) == Some(&'*') {
                            i += 1;
                        }
                        tokens.push(Token::Star);
                        tokens.push(Token::Deep);
                        tokens.push(Token::Star);
                        // `**/x` → the `/` is part of the deep separator.
                        if chars.get(i + 1) == Some(&'/') {
                            i += 1;
                            tokens.pop();
                        }
                    } else {
                        tokens.push(Token::Star);
                    }
                }
                '?' => tokens.push(Token::Any),
                c => tokens.push(Token::Char(c)),
            }
            i += 1;
        }
        PathPattern {
            name_only: name_only && !text.contains('/'),
            tokens,
            all: false,
        }
    }

    /// Whether a project-relative file path matches.
    pub fn matches(&self, path: &str) -> bool {
        if self.all {
            return true;
        }
        let target = if self.name_only {
            path.rsplit('/').next().unwrap_or(path)
        } else {
            path
        };
        let chars: Vec<char> = target.chars().collect();
        matches_tokens(&self.tokens, &chars)
    }
}

fn matches_tokens(p: &[Token], s: &[char]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(Token::Char(c)) => s.first() == Some(c) && matches_tokens(&p[1..], &s[1..]),
        Some(Token::Any) => !s.is_empty() && s[0] != '/' && matches_tokens(&p[1..], &s[1..]),
        Some(Token::Star) => {
            for i in 0..=s.len() {
                if matches_tokens(&p[1..], &s[i..]) {
                    return true;
                }
                if i < s.len() && s[i] == '/' {
                    return false;
                }
            }
            false
        }
        Some(Token::Deep) => {
            // `/`, then any folders each followed by `/`.
            if s.first() != Some(&'/') {
                return false;
            }
            (1..=s.len())
                .filter(|&i| s[i - 1] == '/')
                .any(|i| matches_tokens(&p[1..], &s[i..]))
        }
    }
}

/// A parsed scope expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Atom(Source, PathPattern),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

/// Why a pattern does not parse, with the character offset of the problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.message, self.offset + 1)
    }
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    _text: &'a str,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn peek_op(&mut self, op: &str) -> bool {
        self.skip_ws();
        let n = op.chars().count();
        if self.pos + n > self.chars.len() {
            return false;
        }
        if self.chars[self.pos..self.pos + n]
            .iter()
            .copied()
            .eq(op.chars())
        {
            self.pos += n;
            true
        } else {
            false
        }
    }

    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            offset: self.pos.min(self.chars.len()),
        }
    }

    fn or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.and()?;
        while self.peek_op("||") {
            let right = self.and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        while self.peek_op("&&") {
            let right = self.unary()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws();
        if self.peek_op("!") {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.peek_op("(") {
            let inner = self.or()?;
            if !self.peek_op(")") {
                return Err(self.err("')' expected"));
            }
            return Ok(inner);
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws();
        let start = self.pos;
        while self
            .chars
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            self.pos += 1;
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        if word.is_empty() {
            return Err(self.err(if self.pos >= self.chars.len() {
                "Pattern expected"
            } else {
                "Unexpected character"
            }));
        }
        let source = match word.as_str() {
            "file" => Source::File,
            "src" => Source::Src,
            "test" => Source::Test,
            "lib" => Source::Lib,
            _ => {
                self.pos = start;
                return Err(self.err(format!(
                    "Unknown pattern type '{word}', expected file:, src:, test: or lib:"
                )));
            }
        };
        if self.chars.get(self.pos) == Some(&'[') {
            while self.chars.get(self.pos).is_some_and(|&c| c != ']') {
                self.pos += 1;
            }
            if self.chars.get(self.pos) != Some(&']') {
                return Err(self.err("']' expected"));
            }
            self.pos += 1;
        }
        if self.chars.get(self.pos) != Some(&':') {
            return Err(self.err("':' expected"));
        }
        self.pos += 1;
        let from = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            let op = (c == '&' || c == '|') && self.chars.get(self.pos + 1) == Some(&c);
            if c.is_whitespace() || c == ')' || op {
                break;
            }
            self.pos += 1;
        }
        let text: String = self.chars[from..self.pos].iter().collect();
        if text.is_empty() {
            return Err(self.err("File pattern expected"));
        }
        Ok(Expr::Atom(source, PathPattern::new(&text)))
    }
}

/// Parse a scope pattern. An empty text is `Ok(None)`: a scope without files.
pub fn parse(text: &str) -> Result<Option<Expr>, ParseError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let mut p = Parser {
        chars: text.chars().collect(),
        pos: 0,
        _text: text,
    };
    let expr = p.or()?;
    p.skip_ws();
    if p.pos < p.chars.len() {
        return Err(p.err(if p.chars[p.pos] == ')' {
            "Unbalanced ')'"
        } else {
            "'&&' or '||' expected"
        }));
    }
    Ok(Some(expr))
}

/// A placeholder name below a folder: no file name pattern matches it, `//*` does.
const DIR_PROBE: &str = "\u{1}";

impl Expr {
    /// Whether a project-relative file is in the scope.
    pub fn matches(&self, path: &str, ctx: &ScopeContext) -> bool {
        match self {
            Expr::Atom(source, pattern) => {
                let kind_ok = match source {
                    Source::File => true,
                    Source::Lib => ctx.is_library(path),
                    Source::Test => !ctx.is_library(path) && ctx.is_test(path),
                    Source::Src => !ctx.is_library(path) && !ctx.is_test(path),
                };
                kind_ok && pattern.matches(path)
            }
            Expr::Not(e) => !e.matches(path, ctx),
            Expr::And(a, b) => a.matches(path, ctx) && b.matches(path, ctx),
            Expr::Or(a, b) => a.matches(path, ctx) || b.matches(path, ctx),
        }
    }

    /// Whether a folder belongs to the scope as a whole (`file:tests//*` colors `tests` and
    /// every folder below it; `file:*.php` colors no folder).
    pub fn matches_dir(&self, dir: &str, ctx: &ScopeContext) -> bool {
        let dir = dir.trim_matches('/');
        let probe = if dir.is_empty() {
            DIR_PROBE.to_string()
        } else {
            format!("{dir}/{DIR_PROBE}")
        };
        self.matches(&probe, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, path: &str) -> bool {
        parse(pattern)
            .unwrap()
            .unwrap()
            .matches(path, &ScopeContext::default())
    }

    #[test]
    fn path_patterns() {
        assert!(m("file:app//*", "app/User.php"));
        assert!(m("file:app//*", "app/Models/User.php"));
        assert!(!m("file:app//*", "application/User.php"));
        assert!(m("file:app/*", "app/User.php"));
        assert!(!m("file:app/*", "app/Models/User.php"));
        assert!(m(
            "file:app//*.php",
            "app/Http/Controllers/OrderController.php"
        ));
        assert!(!m("file:app//*.php", "app/Http/routes.js"));
        assert!(m("file:*.php", "app/Models/User.php"));
        assert!(m("file:*.php", "index.php"));
        assert!(m("file:/app/Models/User.php", "app/Models/User.php"));
        assert!(m("file:app/Models/Us?r.php", "app/Models/User.php"));
        assert!(m("file:*/Models//*", "app/Models/User.php"));
        assert!(m("file:app/", "app/Models/User.php"));
        assert!(m("file:**/*.php", "app/Models/User.php"));
        assert!(m("file[forge]:app//*", "app/User.php"));
    }

    #[test]
    fn operators_and_precedence() {
        assert!(m("file:*.js||file:*.ts", "src/a.ts"));
        assert!(m("file:*.js&&!file:*.min.*", "public/app.js"));
        assert!(!m("file:*.js&&!file:*.min.*", "public/app.min.js"));
        // `&&` binds tighter than `||`.
        assert!(m("file:a//*||file:b//*&&file:*.php", "a/x.txt"));
        assert!(!m("(file:a//*||file:b//*)&&file:*.php", "a/x.txt"));
        assert!(m(" ! file:vendor//* ", "app/x.php"));
        assert!(m("!!file:x", "x"));
    }

    #[test]
    fn sources() {
        let ctx = ScopeContext {
            test_roots: vec!["tests".into()],
            ..Default::default()
        };
        let e = |p: &str| parse(p).unwrap().unwrap();
        assert!(e("src:*..*").matches("app/User.php", &ctx));
        assert!(!e("src:*..*").matches("tests/Unit/UserTest.php", &ctx));
        assert!(!e("src:*..*").matches("vendor/a/b.php", &ctx));
        assert!(e("test:*..*").matches("tests/Unit/UserTest.php", &ctx));
        assert!(e("test://*").matches("tests/Unit/UserTest.php", &ctx));
        assert!(e("lib:*..*").matches("vendor/laravel/framework/src/Model.php", &ctx));
        assert!(e("lib:vendor/laravel//*").matches("vendor/laravel/framework/src/Model.php", &ctx));
        assert!(!e("lib:*..*").matches("app/User.php", &ctx));
        assert!(e("src:app//*").matches("app/User.php", &ctx));
    }

    #[test]
    fn folders() {
        let ctx = ScopeContext::default();
        let e = |p: &str| parse(p).unwrap().unwrap();
        assert!(e("file:tests//*").matches_dir("tests", &ctx));
        assert!(e("file:tests//*").matches_dir("tests/Unit", &ctx));
        assert!(!e("file:tests//*").matches_dir("app", &ctx));
        assert!(!e("file:*.php").matches_dir("app", &ctx));
        assert!(!e("file:app//*&&!file:app/Legacy//*").matches_dir("app/Legacy", &ctx));
        assert!(e("file:app//*&&!file:app/Legacy//*").matches_dir("app/Models", &ctx));
    }

    #[test]
    fn errors_point_at_the_problem() {
        assert_eq!(parse("  ").unwrap(), None);
        let err = parse("files:app").unwrap_err();
        assert!(err.message.contains("Unknown pattern type"), "{err:?}");
        assert_eq!(err.offset, 0);
        let err = parse("file:a||").unwrap_err();
        assert_eq!(err.message, "Pattern expected");
        let err = parse("(file:a").unwrap_err();
        assert_eq!(err.message, "')' expected");
        let err = parse("file:a)").unwrap_err();
        assert_eq!(err.message, "Unbalanced ')'");
        assert_eq!(err.offset, 6);
        assert_eq!(parse("file:").unwrap_err().message, "File pattern expected");
        assert_eq!(
            parse("file:a file:b").unwrap_err().message,
            "'&&' or '||' expected"
        );
    }
}
