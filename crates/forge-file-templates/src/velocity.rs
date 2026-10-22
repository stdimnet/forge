//! The template language of file and code templates: the subset of Apache Velocity that
//! PHPStorm's templates use.
//!
//! - References: `$NAME`, `${NAME}`, `$!{NAME}` (quiet: nothing when undefined) and the string
//!   methods `toLowerCase()`, `toUpperCase()`, `trim()` and `replace("a", "b")`
//!   (`${NAME.toLowerCase()}`). An undefined reference renders as written, so PHP code such
//!   as `$this->x` passes through.
//! - Directives: `#if (…)`, `#elseif (…)`, `#else`, `#end`, `#set ($X = …)`,
//!   `#parse("PHP File Header.php")` / `#include(…)`; also braced (`#{end}`). Conditions use
//!   `==`, `!=`, `&&`, `||`, `!` (or `and`, `or`, `not`), parentheses, string literals and
//!   `true` / `false`; a reference alone is true when it is defined and not empty.
//! - `## line comments`, `#* block comments *#`, `#[[unparsed text]]#`, and `\$` / `\#` for a
//!   literal `$` / `#`.
//!
//! Like Velocity's "lines" whitespace mode, a line holding only directives (and whitespace)
//! leaves nothing behind, not even its line break; a `#parse` alone on its line outputs the
//! include followed by a line break.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Variable values by name (without `$`).
pub type Vars = BTreeMap<String, String>;

/// A syntax or include error, with its 1-based line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateError {
    pub message: String,
    pub line: usize,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for TemplateError {}

fn err<T>(line: usize, message: impl Into<String>) -> Result<T, TemplateError> {
    Err(TemplateError {
        message: message.into(),
        line,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct VarRef {
    name: String,
    quiet: bool,
    /// The reference as written (rendered when the variable is undefined).
    raw: String,
    methods: Vec<(String, Vec<Expr>)>,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Var(VarRef),
    /// A string literal; double-quoted ones interpolate references.
    Str(String, bool),
    Bool(bool),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Text(String),
    Var(VarRef),
    If(Expr),
    ElseIf(Expr),
    Else,
    End,
    Set(String, Expr),
    /// `#parse` / `#include`; `own_line` when the directive was alone on its line.
    Parse(Expr, bool),
    /// A comment: only counts as a directive for the "lines" whitespace rule.
    Comment,
}

impl Tok {
    fn is_whitespace_text(&self) -> bool {
        matches!(self, Tok::Text(t) if t.chars().all(|c| c == ' ' || c == '\t'))
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Text(String),
    Var(VarRef),
    If(Vec<(Expr, Vec<Node>)>, Vec<Node>),
    Set(String, Expr),
    Parse(Expr, bool),
}

// ---- Tokenizer

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    line: usize,
}

const DIRECTIVES: [&str; 7] = ["if", "elseif", "else", "end", "set", "parse", "include"];

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

impl<'a> Lexer<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self, n: usize) {
        let taken = &self.src[self.pos..self.pos + n];
        self.line += taken.matches('\n').count();
        self.pos += n;
    }

    fn skip_spaces(&mut self) {
        let n = self.rest().len() - self.rest().trim_start_matches([' ', '\t']).len();
        self.bump(n);
    }

    /// An identifier at the current position (not consumed).
    fn ident_len(&self) -> usize {
        let mut chars = self.rest().char_indices();
        match chars.next() {
            Some((_, c)) if is_ident_start(c) => {}
            _ => return 0,
        }
        chars
            .find(|(_, c)| !is_ident(*c))
            .map_or(self.rest().len(), |(i, _)| i)
    }

    /// A reference at `$`; `None` (nothing consumed) when it is just a dollar sign.
    fn var_ref(&mut self) -> Result<Option<VarRef>, TemplateError> {
        let start = self.pos;
        let save_line = self.line;
        let mut p = 1;
        let quiet = self.rest()[p..].starts_with('!');
        if quiet {
            p += 1;
        }
        let braced = self.rest()[p..].starts_with('{');
        if braced {
            p += 1;
        }
        self.bump(p);
        let n = self.ident_len();
        if n == 0 {
            self.pos = start;
            self.line = save_line;
            return Ok(None);
        }
        let name = self.rest()[..n].to_string();
        self.bump(n);
        let mut methods = Vec::new();
        loop {
            let r = self.rest();
            if !r.starts_with('.') {
                break;
            }
            let save = (self.pos, self.line);
            self.bump(1);
            let m = self.ident_len();
            if m == 0 || !self.rest()[m..].starts_with('(') {
                (self.pos, self.line) = save;
                break;
            }
            let method = self.rest()[..m].to_string();
            self.bump(m);
            let args_src = self.parenthesized(&format!("{method}()"))?;
            let args = if args_src.trim().is_empty() {
                Vec::new()
            } else {
                split_args(&args_src)
                    .iter()
                    .map(|a| parse_expr(a, self.line))
                    .collect::<Result<_, _>>()?
            };
            methods.push((method, args));
        }
        if braced {
            if !self.rest().starts_with('}') {
                self.pos = start;
                self.line = save_line;
                return Ok(None);
            }
            self.bump(1);
        }
        Ok(Some(VarRef {
            name,
            quiet,
            raw: self.src[start..self.pos].to_string(),
            methods,
        }))
    }

    /// The text inside `( … )` at the current position (after optional spaces).
    fn parenthesized(&mut self, what: &str) -> Result<String, TemplateError> {
        self.skip_spaces();
        if !self.rest().starts_with('(') {
            return err(self.line, format!("Expected '(' after {what}"));
        }
        let mut depth = 0;
        let mut quote: Option<char> = None;
        for (i, c) in self.rest().char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '(') => depth += 1,
                (None, ')') => {
                    depth -= 1;
                    if depth == 0 {
                        let inner = self.rest()[1..i].to_string();
                        self.bump(i + 1);
                        return Ok(inner);
                    }
                }
                (None, '\n') => break,
                _ => {}
            }
        }
        err(self.line, format!("Missing ')' in {what}"))
    }

    /// A directive or comment at `#`; `None` (nothing consumed) when it is a plain `#`.
    fn directive(&mut self) -> Result<Option<Tok>, TemplateError> {
        let r = self.rest();
        if r.starts_with("##") {
            let n = r.find('\n').unwrap_or(r.len());
            self.bump(n);
            return Ok(Some(Tok::Comment));
        }
        if r.starts_with("#*") {
            let line = self.line;
            let Some(end) = r.find("*#") else {
                return err(line, "Unclosed comment '#*'");
            };
            self.bump(end + 2);
            return Ok(Some(Tok::Comment));
        }
        if let Some(body) = r.strip_prefix("#[[") {
            let line = self.line;
            let Some(end) = body.find("]]#") else {
                return err(line, "Unclosed '#[['");
            };
            let text = body[..end].to_string();
            self.bump(3 + end + 3);
            return Ok(Some(Tok::Text(text)));
        }
        let braced = r[1..].starts_with('{');
        let skip = if braced { 2 } else { 1 };
        let name_len = r[skip..]
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(r.len() - skip);
        let name = &r[skip..skip + name_len];
        if !DIRECTIVES.contains(&name) || (braced && !r[skip + name_len..].starts_with('}')) {
            return Ok(None);
        }
        let name = name.to_string();
        self.bump(skip + name_len + usize::from(braced));
        let line = self.line;
        let tok = match name.as_str() {
            "if" => Tok::If(parse_expr(&self.parenthesized("#if")?, line)?),
            "elseif" => Tok::ElseIf(parse_expr(&self.parenthesized("#elseif")?, line)?),
            "else" => Tok::Else,
            "end" => Tok::End,
            "set" => {
                let inner = self.parenthesized("#set")?;
                let Some((lhs, rhs)) = inner.split_once('=') else {
                    return err(line, "Expected '$NAME = value' in #set");
                };
                let lhs = lhs.trim();
                let name = lhs
                    .strip_prefix("${")
                    .and_then(|l| l.strip_suffix('}'))
                    .or_else(|| lhs.strip_prefix('$'))
                    .filter(|n| {
                        n.chars().next().is_some_and(is_ident_start) && n.chars().all(is_ident)
                    });
                let Some(name) = name else {
                    return err(line, format!("Invalid variable '{lhs}' in #set"));
                };
                Tok::Set(name.to_string(), parse_expr(rhs, line)?)
            }
            _ => Tok::Parse(
                parse_expr(&self.parenthesized(&format!("#{name}"))?, line)?,
                false,
            ),
        };
        Ok(Some(tok))
    }
}

/// Split call arguments at top-level commas.
fn split_args(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0;
    for c in src.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => depth -= 1,
            (None, ',') if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out
}

/// Tokens with their lines; whole-line directives already swallowed their line's whitespace
/// and line break.
fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, TemplateError> {
    let mut lx = Lexer {
        src,
        pos: 0,
        line: 1,
    };
    let mut out: Vec<(Tok, usize)> = Vec::new();
    // Tokens of the current line while it holds only whitespace and directives.
    let mut pending: Vec<(Tok, usize)> = Vec::new();
    let mut directive_only = true;
    let mut text = String::new();

    fn flush_text(
        text: &mut String,
        line: usize,
        directive_only: bool,
        pending: &mut Vec<(Tok, usize)>,
        out: &mut Vec<(Tok, usize)>,
    ) {
        if text.is_empty() {
            return;
        }
        let t = Tok::Text(std::mem::take(text));
        if directive_only {
            pending.push((t, line));
        } else {
            out.push((t, line));
        }
    }

    /// End of a line (or the input) that held only whitespace and directives.
    fn finish_line(pending: &mut Vec<(Tok, usize)>, out: &mut Vec<(Tok, usize)>) -> bool {
        let has_directive = pending.iter().any(|(t, _)| !matches!(t, Tok::Text(_)));
        if has_directive {
            for (t, line) in pending.drain(..) {
                match t {
                    t if t.is_whitespace_text() => {}
                    Tok::Parse(e, _) => out.push((Tok::Parse(e, true), line)),
                    t => out.push((t, line)),
                }
            }
        } else {
            out.append(pending);
        }
        has_directive
    }

    while let Some(c) = lx.peek() {
        let line = lx.line;
        match c {
            '\n' => {
                flush_text(&mut text, line, directive_only, &mut pending, &mut out);
                let gobble = directive_only && finish_line(&mut pending, &mut out);
                lx.bump(1);
                if !gobble {
                    out.push((Tok::Text("\n".into()), line));
                }
                directive_only = true;
            }
            '\\' if matches!(lx.rest()[1..].chars().next(), Some('$' | '#')) => {
                let escaped = lx.rest()[1..].chars().next().unwrap_or('$');
                if directive_only {
                    flush_text(&mut text, line, true, &mut pending, &mut out);
                    out.append(&mut pending);
                    directive_only = false;
                }
                text.push(escaped);
                lx.bump(2);
            }
            '$' => match lx.var_ref()? {
                Some(v) => {
                    flush_text(&mut text, line, directive_only, &mut pending, &mut out);
                    out.append(&mut pending);
                    directive_only = false;
                    out.push((Tok::Var(v), line));
                }
                None => {
                    if directive_only {
                        flush_text(&mut text, line, true, &mut pending, &mut out);
                        out.append(&mut pending);
                        directive_only = false;
                    }
                    text.push('$');
                    lx.bump(1);
                }
            },
            '#' => match lx.directive()? {
                Some(Tok::Text(literal)) => {
                    if directive_only {
                        flush_text(&mut text, line, true, &mut pending, &mut out);
                        out.append(&mut pending);
                        directive_only = false;
                    }
                    text.push_str(&literal);
                }
                Some(tok) => {
                    flush_text(&mut text, line, directive_only, &mut pending, &mut out);
                    if directive_only {
                        pending.push((tok, line));
                    } else {
                        out.push((tok, line));
                    }
                }
                None => {
                    if directive_only {
                        flush_text(&mut text, line, true, &mut pending, &mut out);
                        out.append(&mut pending);
                        directive_only = false;
                    }
                    text.push('#');
                    lx.bump(1);
                }
            },
            c => {
                if directive_only && c != ' ' && c != '\t' {
                    flush_text(&mut text, line, true, &mut pending, &mut out);
                    out.append(&mut pending);
                    directive_only = false;
                }
                text.push(c);
                lx.bump(c.len_utf8());
            }
        }
    }
    let line = lx.line;
    flush_text(&mut text, line, directive_only, &mut pending, &mut out);
    if directive_only {
        finish_line(&mut pending, &mut out);
    }
    Ok(out)
}

// ---- Tree

fn build(tokens: Vec<(Tok, usize)>) -> Result<Vec<Node>, TemplateError> {
    struct Frame {
        line: usize,
        branches: Vec<(Expr, Vec<Node>)>,
        otherwise: Option<Vec<Node>>,
    }
    let mut root: Vec<Node> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();

    fn current<'a>(root: &'a mut Vec<Node>, stack: &'a mut [Frame]) -> &'a mut Vec<Node> {
        match stack.last_mut() {
            None => root,
            Some(f) => match &mut f.otherwise {
                Some(o) => o,
                None => &mut f.branches.last_mut().expect("a branch").1,
            },
        }
    }

    for (tok, line) in tokens {
        match tok {
            Tok::Text(t) => {
                let nodes = current(&mut root, &mut stack);
                if let Some(Node::Text(prev)) = nodes.last_mut() {
                    prev.push_str(&t);
                } else {
                    nodes.push(Node::Text(t));
                }
            }
            Tok::Var(v) => current(&mut root, &mut stack).push(Node::Var(v)),
            Tok::Set(n, e) => current(&mut root, &mut stack).push(Node::Set(n, e)),
            Tok::Parse(e, own) => current(&mut root, &mut stack).push(Node::Parse(e, own)),
            Tok::Comment => {}
            Tok::If(cond) => stack.push(Frame {
                line,
                branches: vec![(cond, Vec::new())],
                otherwise: None,
            }),
            Tok::ElseIf(cond) => match stack.last_mut() {
                Some(f) if f.otherwise.is_none() => f.branches.push((cond, Vec::new())),
                Some(_) => return err(line, "#elseif after #else"),
                None => return err(line, "#elseif without #if"),
            },
            Tok::Else => match stack.last_mut() {
                Some(f) if f.otherwise.is_none() => f.otherwise = Some(Vec::new()),
                Some(_) => return err(line, "Second #else"),
                None => return err(line, "#else without #if"),
            },
            Tok::End => {
                let Some(f) = stack.pop() else {
                    return err(line, "#end without #if");
                };
                current(&mut root, &mut stack)
                    .push(Node::If(f.branches, f.otherwise.unwrap_or_default()));
            }
        }
    }
    if let Some(f) = stack.last() {
        return err(f.line, "Missing #end for #if");
    }
    Ok(root)
}

// ---- Expressions

#[derive(Debug, Clone, PartialEq)]
enum ETok {
    Var(VarRef),
    Str(String, bool),
    Word(String),
    Op(&'static str),
}

fn lex_expr(src: &str, line: usize) -> Result<Vec<ETok>, TemplateError> {
    let mut out = Vec::new();
    let mut lx = Lexer { src, pos: 0, line };
    while let Some(c) = lx.peek() {
        match c {
            ' ' | '\t' | '\r' | '\n' => lx.bump(1),
            '$' => match lx.var_ref()? {
                Some(v) => out.push(ETok::Var(v)),
                None => return err(line, "Invalid reference in condition"),
            },
            '"' | '\'' => {
                let body = &lx.rest()[1..];
                let Some(end) = body.find(c) else {
                    return err(line, "Unclosed string");
                };
                out.push(ETok::Str(body[..end].to_string(), c == '"'));
                lx.bump(end + 2);
            }
            _ => {
                let r = lx.rest();
                if let Some(op) = ["==", "!=", "&&", "||", "!", "(", ")"]
                    .into_iter()
                    .find(|op| r.starts_with(op))
                {
                    out.push(ETok::Op(op));
                    lx.bump(op.len());
                } else {
                    let n = r
                        .find(|c: char| {
                            !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
                        })
                        .unwrap_or(r.len());
                    if n == 0 {
                        return err(line, format!("Unexpected '{c}' in condition"));
                    }
                    out.push(ETok::Word(r[..n].to_string()));
                    lx.bump(n);
                }
            }
        }
    }
    Ok(out)
}

struct ExprParser {
    toks: Vec<ETok>,
    pos: usize,
    line: usize,
}

impl ExprParser {
    fn peek_is(&self, op: &str) -> bool {
        match self.toks.get(self.pos) {
            Some(ETok::Op(o)) => *o == op,
            Some(ETok::Word(w)) => w == op,
            _ => false,
        }
    }

    fn eat(&mut self, ops: &[&str]) -> bool {
        if ops.iter().any(|o| self.peek_is(o)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Result<Expr, TemplateError> {
        let mut e = self.and()?;
        while self.eat(&["||", "or"]) {
            e = Expr::Or(Box::new(e), Box::new(self.and()?));
        }
        Ok(e)
    }

    fn and(&mut self) -> Result<Expr, TemplateError> {
        let mut e = self.cmp()?;
        while self.eat(&["&&", "and"]) {
            e = Expr::And(Box::new(e), Box::new(self.cmp()?));
        }
        Ok(e)
    }

    fn cmp(&mut self) -> Result<Expr, TemplateError> {
        let e = self.unary()?;
        if self.eat(&["==", "eq"]) {
            return Ok(Expr::Eq(Box::new(e), Box::new(self.unary()?)));
        }
        if self.eat(&["!=", "ne"]) {
            return Ok(Expr::Ne(Box::new(e), Box::new(self.unary()?)));
        }
        Ok(e)
    }

    fn unary(&mut self) -> Result<Expr, TemplateError> {
        if self.eat(&["!", "not"]) {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.eat(&["("]) {
            let e = self.or()?;
            if !self.eat(&[")"]) {
                return err(self.line, "Missing ')' in condition");
            }
            return Ok(e);
        }
        let tok = self.toks.get(self.pos).cloned();
        self.pos += 1;
        match tok {
            Some(ETok::Var(v)) => Ok(Expr::Var(v)),
            Some(ETok::Str(s, interpolate)) => Ok(Expr::Str(s, interpolate)),
            Some(ETok::Word(w)) if w == "true" || w == "false" => Ok(Expr::Bool(w == "true")),
            Some(ETok::Word(w))
                if w.chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '-') =>
            {
                Ok(Expr::Str(w, false))
            }
            Some(ETok::Word(w)) => err(self.line, format!("Unexpected '{w}' in condition")),
            Some(ETok::Op(o)) => err(self.line, format!("Unexpected '{o}' in condition")),
            None => err(self.line, "Incomplete condition"),
        }
    }
}

fn parse_expr(src: &str, line: usize) -> Result<Expr, TemplateError> {
    let toks = lex_expr(src, line)?;
    if toks.is_empty() {
        return err(line, "Empty condition");
    }
    let mut p = ExprParser { toks, pos: 0, line };
    let e = p.or()?;
    if p.pos < p.toks.len() {
        return err(line, "Unexpected text after the condition");
    }
    Ok(e)
}

// ---- Evaluation

/// Looks up the text of an include (`#parse("PHP File Header.php")`) by name.
pub type Includes<'a> = &'a dyn Fn(&str) -> Option<String>;

struct Ctx<'a> {
    vars: Vars,
    includes: Includes<'a>,
    depth: usize,
}

const MAX_INCLUDE_DEPTH: usize = 8;

impl Ctx<'_> {
    fn var(&self, v: &VarRef) -> Result<Option<String>, TemplateError> {
        let Some(mut value) = self.vars.get(&v.name).cloned() else {
            return Ok(None);
        };
        for (method, args) in &v.methods {
            let args: Vec<String> = args
                .iter()
                .map(|a| self.string(a))
                .collect::<Result<_, _>>()?;
            value = match (method.as_str(), args.as_slice()) {
                ("toLowerCase", []) => value.to_lowercase(),
                ("toUpperCase", []) => value.to_uppercase(),
                ("trim", []) => value.trim().to_string(),
                ("replace", [a, b]) => value.replace(a.as_str(), b),
                _ => return Ok(None),
            };
        }
        Ok(Some(value))
    }

    fn string(&self, e: &Expr) -> Result<String, TemplateError> {
        Ok(match e {
            Expr::Var(v) => self.var(v)?.unwrap_or_default(),
            Expr::Str(s, true) => self.interpolate(s)?,
            Expr::Str(s, false) => s.clone(),
            other => self.truthy(other)?.to_string(),
        })
    }

    fn interpolate(&self, s: &str) -> Result<String, TemplateError> {
        let mut out = String::new();
        let mut lx = Lexer {
            src: s,
            pos: 0,
            line: 0,
        };
        while let Some(c) = lx.peek() {
            if c == '$' {
                if let Some(v) = lx.var_ref()? {
                    match self.var(&v)? {
                        Some(val) => out.push_str(&val),
                        None if v.quiet => {}
                        None => out.push_str(&v.raw),
                    }
                    continue;
                }
            }
            out.push(c);
            lx.bump(c.len_utf8());
        }
        Ok(out)
    }

    fn truthy(&self, e: &Expr) -> Result<bool, TemplateError> {
        Ok(match e {
            Expr::Var(v) => self.var(v)?.is_some_and(|s| !s.is_empty() && s != "false"),
            Expr::Str(..) => !self.string(e)?.is_empty(),
            Expr::Bool(b) => *b,
            Expr::Not(a) => !self.truthy(a)?,
            Expr::And(a, b) => self.truthy(a)? && self.truthy(b)?,
            Expr::Or(a, b) => self.truthy(a)? || self.truthy(b)?,
            Expr::Eq(a, b) => self.string(a)? == self.string(b)?,
            Expr::Ne(a, b) => self.string(a)? != self.string(b)?,
        })
    }

    fn render(&mut self, nodes: &[Node], out: &mut String) -> Result<(), TemplateError> {
        for n in nodes {
            match n {
                Node::Text(t) => out.push_str(t),
                Node::Var(v) => match self.var(v)? {
                    Some(val) => out.push_str(&val),
                    None if v.quiet => {}
                    None => out.push_str(&v.raw),
                },
                Node::If(branches, otherwise) => {
                    let mut taken = false;
                    for (cond, body) in branches {
                        if self.truthy(cond)? {
                            self.render(body, out)?;
                            taken = true;
                            break;
                        }
                    }
                    if !taken {
                        self.render(otherwise, out)?;
                    }
                }
                Node::Set(name, e) => {
                    let v = self.string(e)?;
                    self.vars.insert(name.clone(), v);
                }
                Node::Parse(e, own_line) => {
                    let name = self.string(e)?;
                    let Some(text) = (self.includes)(&name) else {
                        return err(0, format!("Include '{name}' not found"));
                    };
                    if self.depth >= MAX_INCLUDE_DEPTH {
                        return err(0, format!("Includes nested too deeply at '{name}'"));
                    }
                    let nodes = build(tokenize(&text)?).map_err(|e| TemplateError {
                        message: format!("{} (in include '{name}')", e.message),
                        line: e.line,
                    })?;
                    self.depth += 1;
                    let mut inner = String::new();
                    self.render(&nodes, &mut inner)?;
                    self.depth -= 1;
                    if *own_line && !inner.is_empty() && !inner.ends_with('\n') {
                        inner.push('\n');
                    }
                    out.push_str(&inner);
                }
            }
        }
        Ok(())
    }
}

/// Check a template's syntax.
pub fn validate(template: &str) -> Result<(), TemplateError> {
    build(tokenize(template)?).map(|_| ())
}

/// Render `template` with `vars`; `#parse` looks includes up with `includes`.
pub fn render(
    template: &str,
    vars: &Vars,
    includes: Includes<'_>,
) -> Result<String, TemplateError> {
    let nodes = build(tokenize(template)?)?;
    let mut ctx = Ctx {
        vars: vars.clone(),
        includes,
        depth: 0,
    };
    let mut out = String::new();
    ctx.render(&nodes, &mut out)?;
    Ok(out)
}

/// Names of the variables a template references, in order of first use, without those it
/// assigns itself with `#set`.
pub fn referenced_variables(template: &str) -> Vec<String> {
    let Ok(tokens) = tokenize(template) else {
        return Vec::new();
    };
    let mut seen: Vec<String> = Vec::new();
    let mut set: Vec<String> = Vec::new();
    fn add_expr(e: &Expr, seen: &mut Vec<String>) {
        match e {
            Expr::Var(v) => {
                if !seen.contains(&v.name) {
                    seen.push(v.name.clone());
                }
            }
            Expr::Not(a) => add_expr(a, seen),
            Expr::And(a, b) | Expr::Or(a, b) | Expr::Eq(a, b) | Expr::Ne(a, b) => {
                add_expr(a, seen);
                add_expr(b, seen);
            }
            _ => {}
        }
    }
    for (tok, _) in &tokens {
        match tok {
            Tok::Var(v) => add_expr(&Expr::Var(v.clone()), &mut seen),
            Tok::If(e) | Tok::ElseIf(e) | Tok::Parse(e, _) => add_expr(e, &mut seen),
            Tok::Set(name, e) => {
                add_expr(e, &mut seen);
                set.push(name.clone());
            }
            _ => {}
        }
    }
    seen.retain(|v| !set.contains(v));
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn no_includes(_: &str) -> Option<String> {
        None
    }

    fn r(t: &str, v: &[(&str, &str)]) -> String {
        render(t, &vars(v), &no_includes).unwrap()
    }

    #[test]
    fn substitutes_references() {
        assert_eq!(
            r("class ${NAME} {}", &[("NAME", "Order")]),
            "class Order {}"
        );
        assert_eq!(r("$NAME.php", &[("NAME", "Order")]), "Order.php");
        assert_eq!(r("a$!{MISSING}b", &[]), "ab");
        assert_eq!(r("$this->x = ${X};", &[]), "$this->x = ${X};");
        assert_eq!(
            r("price: 5$ \\$NAME \\#if", &[("NAME", "x")]),
            "price: 5$ $NAME #if"
        );
        assert_eq!(
            r(
                "${NAME.toLowerCase()}-${NAME.toUpperCase()}",
                &[("NAME", "Ab")]
            ),
            "ab-AB"
        );
        assert_eq!(
            r(
                "${NAME.replace(\"Controller\", \"\")}",
                &[("NAME", "OrderController")]
            ),
            "Order"
        );
        assert_eq!(
            r("${NAME.unknown()}", &[("NAME", "x")]),
            "${NAME.unknown()}"
        );
    }

    #[test]
    fn whole_line_directives_leave_no_trace() {
        let t =
            "<?php\n\n#if (${NAMESPACE})\nnamespace ${NAMESPACE};\n\n#end\nclass ${NAME}\n{\n}\n";
        assert_eq!(
            r(t, &[("NAMESPACE", "App\\Models"), ("NAME", "Order")]),
            "<?php\n\nnamespace App\\Models;\n\nclass Order\n{\n}\n"
        );
        assert_eq!(
            r(t, &[("NAMESPACE", ""), ("NAME", "Order")]),
            "<?php\n\nclass Order\n{\n}\n"
        );
        assert_eq!(r(t, &[("NAME", "Order")]), "<?php\n\nclass Order\n{\n}\n");
        // Indented directives and several directives on one line.
        assert_eq!(r("a\n  #if (true)#end  \nb", &[]), "a\nb");
        assert_eq!(r("## comment\nx\n#* block *#\ny", &[]), "x\ny");
    }

    #[test]
    fn inline_directives() {
        let t = "class ${NAME}#if (${EXTENDS}) extends ${EXTENDS}#end#if (${IMPLEMENTS}) implements ${IMPLEMENTS}#end\n{";
        assert_eq!(r(t, &[("NAME", "A")]), "class A\n{");
        assert_eq!(
            r(
                t,
                &[("NAME", "A"), ("EXTENDS", "B"), ("IMPLEMENTS", "C, D")]
            ),
            "class A extends B implements C, D\n{"
        );
        assert_eq!(r("#{if}(true)yes#{else}no#{end}", &[]), "yes");
    }

    #[test]
    fn conditions() {
        let t = "#if ($T == \"void\")\nv\n#elseif (${T} != 'int' && !$X)\nnot int\n#elseif ($T eq \"int\" or false)\nint\n#else\nother\n#end\n";
        assert_eq!(r(t, &[("T", "void")]), "v\n");
        assert_eq!(r(t, &[("T", "string")]), "not int\n");
        assert_eq!(r(t, &[("T", "string"), ("X", "1")]), "other\n");
        assert_eq!(r(t, &[("T", "int")]), "int\n");
        assert_eq!(r("#if (\"${A}x\" == \"bx\")ok#end", &[("A", "b")]), "ok");
        assert_eq!(r("#if (!(${A} || ${B}))none#end", &[]), "none");
        assert_eq!(
            r("#if ($FLAG)on#{else}off#end", &[("FLAG", "false")]),
            "off"
        );
    }

    #[test]
    fn set_and_parse() {
        let includes = |name: &str| match name {
            "PHP File Header.php" => Some("/**\n * @author ${USER}\n */".to_string()),
            "Empty" => Some(String::new()),
            "Loop" => Some("#parse(\"Loop\")".to_string()),
            _ => None,
        };
        let t = "<?php\n#set ($USER = \"Ada ${SURNAME}\")\n#parse(\"PHP File Header.php\")\n\nclass X\n";
        assert_eq!(
            render(t, &vars(&[("SURNAME", "L")]), &includes).unwrap(),
            "<?php\n/**\n * @author Ada L\n */\n\nclass X\n"
        );
        assert_eq!(
            render("<?php\n#parse(\"Empty\")\n\nx", &Vars::new(), &includes).unwrap(),
            "<?php\n\nx"
        );
        assert!(render("#parse(\"Nope\")", &Vars::new(), &includes).is_err());
        assert!(render("#parse(\"Loop\")", &Vars::new(), &includes)
            .unwrap_err()
            .message
            .contains("deeply"));
    }

    #[test]
    fn passes_php_and_markup_through() {
        let t = "#[Route('/x')]\n#[[#if literal $X]]#\n<div id=\"#main\">${A}</div>\n# heading";
        assert_eq!(
            r(t, &[("A", "a")]),
            "#[Route('/x')]\n#if literal $X\n<div id=\"#main\">a</div>\n# heading"
        );
        assert_eq!(r("  x\n\n  y\n", &[]), "  x\n\n  y\n");
        assert_eq!(r("äö ${N} ü", &[("N", "€")]), "äö € ü");
    }

    #[test]
    fn reports_syntax_errors_with_lines() {
        assert_eq!(
            validate("a\n#end").unwrap_err(),
            TemplateError {
                message: "#end without #if".into(),
                line: 2
            }
        );
        assert_eq!(validate("x\n#if ($A)\ny").unwrap_err().line, 2);
        assert!(validate("#if ($A")
            .unwrap_err()
            .message
            .contains("Missing ')'"));
        assert!(validate("#if ($A ==)\n#end").is_err());
        assert!(validate("#else").is_err());
        assert!(validate("#if ($A)#else#else#end").is_err());
        assert!(validate("#set ($ = 1)").is_err());
        assert!(validate("#* open").is_err());
        assert!(validate("#if ($A)\n#elseif ($B)\n#else\n#end").is_ok());
    }

    #[test]
    fn lists_referenced_variables() {
        assert_eq!(
            referenced_variables("#set ($Y = $Z)\n${NAME} $X #if ($NAMESPACE)$Y#end $NAME"),
            vec!["Z", "NAME", "X", "NAMESPACE"]
        );
    }
}
