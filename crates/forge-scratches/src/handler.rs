//! Response handler scripts of `.http` requests (`> {% … %}`), PHPStorm's HTTP Client API
//! as a small, safe subset: no JavaScript engine, no loops, no access to anything but the
//! response and the global variables.
//!
//! ```js
//! client.global.set("token", response.body.data.token);
//! client.test("Order was created", function () {
//!     client.assert(response.status === 201, "Expected 201 but got " + response.status);
//!     client.assert(response.body.data.items.length > 0);
//! });
//! client.log("order " + response.body.data.id);
//! ```
//!
//! Supported: `client.global.set/get/clear/clearAll/isEmpty`, `client.test(name, fn)`,
//! `client.assert(condition, message?)`, `client.log(…)`, `response.status`,
//! `response.body` (parsed JSON, else the text) with `.a.b`, `["a"]` and `[0]`,
//! `response.headers.valueOf/valuesOf(name)`, `response.contentType.mimeType/charset`,
//! `JSON.stringify/parse`, a few string and array methods (`length`, `includes`,
//! `startsWith`, `endsWith`, `indexOf`, `toLowerCase`, `toUpperCase`, `trim`), `var`/`let`/
//! `const`, `if`/`else`, `return`, `function () {}` and arrow functions, literals (also
//! arrays and objects), `! - + * / %`, comparisons, `&&`, `||`, `??` and `?:`. Anything
//! else stops the script with an error that names it.

use std::collections::BTreeMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::HttpResponse;

/// Longest script accepted (characters).
const MAX_SCRIPT: usize = 64 * 1024;
/// Evaluation steps before a script is stopped.
const MAX_STEPS: usize = 100_000;
/// Nesting depth of expressions and functions.
const MAX_DEPTH: usize = 64;

/// One line of the handler's report, in execution order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Entry {
    /// `client.log(…)`.
    Log { text: String },
    /// `client.test(…)`, or a failed `client.assert` outside a test.
    Test {
        name: String,
        passed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// `client.global.set` (`cleared: false`) or `clear`/`clearAll`.
    Global { name: String, cleared: bool },
    /// The script stopped: unsupported syntax or a runtime error.
    Error { message: String },
}

/// What a handler did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandlerReport {
    pub entries: Vec<Entry>,
}

impl HandlerReport {
    pub fn passed(&self) -> usize {
        self.count(true)
    }

    pub fn failed(&self) -> usize {
        self.count(false)
    }

    fn count(&self, ok: bool) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e, Entry::Test { passed, .. } if *passed == ok))
            .count()
    }
}

/// Run `script` against `response`; `globals` are read and updated in place.
pub fn run(
    script: &str,
    response: &HttpResponse,
    globals: &mut BTreeMap<String, String>,
) -> HandlerReport {
    let mut report = HandlerReport::default();
    if script.chars().count() > MAX_SCRIPT {
        report.entries.push(Entry::Error {
            message: "The response handler is too long".into(),
        });
        return report;
    }
    let program = match lex(script).and_then(|tokens| Parser::new(tokens).program()) {
        Ok(p) => p,
        Err(message) => {
            report.entries.push(Entry::Error { message });
            return report;
        }
    };
    let body = serde_json::from_str::<Value>(&response.body)
        .unwrap_or_else(|_| Value::String(response.body.clone()));
    let mut interp = Interp {
        response,
        body,
        globals,
        report: &mut report,
        scopes: vec![BTreeMap::new()],
        steps: 0,
        depth: 0,
        in_test: false,
    };
    match interp.block(&program) {
        Ok(()) | Err(Flow::Return) => {}
        Err(Flow::Assert(message)) => interp.report.entries.push(Entry::Test {
            name: message.clone().unwrap_or_else(|| "Assertion".into()),
            passed: false,
            message,
        }),
        Err(Flow::Error(message)) => interp.report.entries.push(Entry::Error { message }),
    }
    report
}

const RESET: &str = "\x1b[0m";

/// Console text of a report, shown below the response in the Run tool window.
pub fn render(report: &HandlerReport) -> String {
    if report.entries.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\x1b[1mResponse handler\x1b[0m\n");
    for e in &report.entries {
        match e {
            Entry::Log { text } => out.push_str(&format!("{text}\n")),
            Entry::Test {
                name, passed: true, ..
            } => out.push_str(&format!("\x1b[32m✓ {name}{RESET}\n")),
            Entry::Test { name, message, .. } => {
                out.push_str(&format!("\x1b[31m✗ {name}{RESET}\n"));
                if let Some(m) = message.as_ref().filter(|m| *m != name) {
                    out.push_str(&format!("    \x1b[31m{m}{RESET}\n"));
                }
            }
            Entry::Global {
                name,
                cleared: false,
            } => out.push_str(&format!("\x1b[90mGlobal variable '{name}' set{RESET}\n")),
            Entry::Global { name, .. } => out.push_str(&format!(
                "\x1b[90mGlobal variable '{name}' cleared{RESET}\n"
            )),
            Entry::Error { message } => out.push_str(&format!("\x1b[31m{message}{RESET}\n")),
        }
    }
    let (passed, failed) = (report.passed(), report.failed());
    if passed + failed > 0 {
        out.push_str(&format!(
            "Tests: \x1b[32m{passed} passed{RESET}, {}{failed} failed{RESET}\n",
            if failed > 0 { "\x1b[31m" } else { "" }
        ));
    }
    out
}

// ---- lexer -------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64),
    Str(String),
    Punct(&'static str),
}

const PUNCTS: &[&str] = &[
    "===", "!==", "=>", "==", "!=", "<=", ">=", "&&", "||", "??", ".", ",", ";", "(", ")", "{",
    "}", "[", "]", "<", ">", "!", "+", "-", "*", "/", "%", "=", "?", ":",
];

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_')
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().filter(|c| **c != '_').collect();
            let n = text
                .parse::<f64>()
                .map_err(|_| format!("Unsupported number '{text}'"))?;
            out.push(Tok::Num(n));
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
        } else if c == '"' || c == '\'' || c == '`' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            loop {
                let Some(&ch) = chars.get(i) else {
                    return Err("Unterminated string".into());
                };
                i += 1;
                if ch == quote {
                    break;
                }
                if quote == '`' && ch == '$' && chars.get(i) == Some(&'{') {
                    return Err("Template literals with ${…} are not supported; use +".into());
                }
                if ch == '\\' {
                    let Some(&esc) = chars.get(i) else {
                        return Err("Unterminated string".into());
                    };
                    i += 1;
                    match esc {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        'u' => {
                            let hex: String = chars.iter().skip(i).take(4).collect();
                            let code = u32::from_str_radix(&hex, 16)
                                .map_err(|_| "Bad \\u escape".to_string())?;
                            s.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                            i += 4;
                        }
                        other => s.push(other),
                    }
                } else {
                    s.push(ch);
                }
            }
            out.push(Tok::Str(s));
        } else {
            let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
            let p = PUNCTS
                .iter()
                .find(|p| rest.starts_with(**p))
                .ok_or_else(|| format!("Unexpected character '{c}'"))?;
            out.push(Tok::Punct(p));
            i += p.chars().count();
        }
    }
    Ok(out)
}

// ---- parser ------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Expr {
    Lit(Value),
    Ident(String),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Func(Rc<Vec<Stmt>>),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
}

#[derive(Debug, Clone)]
enum Stmt {
    Expr(Expr),
    Let(String, Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    Return,
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn new(toks: Vec<Tok>) -> Self {
        Parser {
            toks,
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n)
    }

    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(q)) if *q == p)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(i)) if i == w)
    }

    fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: &str) -> Result<(), String> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(format!("Expected '{p}' {}", self.near()))
        }
    }

    fn near(&self) -> String {
        match self.peek() {
            None => "at the end of the script".into(),
            Some(Tok::Ident(i)) => format!("near '{i}'"),
            Some(Tok::Str(s)) => format!("near \"{s}\""),
            Some(Tok::Num(n)) => format!("near {n}"),
            Some(Tok::Punct(p)) => format!("near '{p}'"),
        }
    }

    fn ident(&mut self) -> Result<String, String> {
        match self.peek().cloned() {
            Some(Tok::Ident(i)) => {
                self.pos += 1;
                Ok(i)
            }
            _ => Err(format!("Expected a name {}", self.near())),
        }
    }

    fn program(mut self) -> Result<Vec<Stmt>, String> {
        let mut out = Vec::new();
        while self.peek().is_some() {
            if !self.eat(";") {
                out.push(self.statement()?);
            }
        }
        Ok(out)
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.eat("}") {
            if self.peek().is_none() {
                return Err("Expected '}' at the end of the script".into());
            }
            if !self.eat(";") {
                out.push(self.statement()?);
            }
        }
        Ok(out)
    }

    fn body(&mut self) -> Result<Vec<Stmt>, String> {
        if self.is("{") {
            self.block()
        } else {
            Ok(vec![self.statement()?])
        }
    }

    fn statement(&mut self) -> Result<Stmt, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("The response handler is nested too deeply".into());
        }
        let out = self.statement_inner();
        self.depth -= 1;
        out
    }

    fn statement_inner(&mut self) -> Result<Stmt, String> {
        if self.is_word("var") || self.is_word("let") || self.is_word("const") {
            self.pos += 1;
            let name = self.ident()?;
            let value = if self.eat("=") {
                self.expr()?
            } else {
                Expr::Lit(Value::Null)
            };
            self.eat(";");
            return Ok(Stmt::Let(name, value));
        }
        if self.is_word("if") {
            self.pos += 1;
            self.expect("(")?;
            let cond = self.expr()?;
            self.expect(")")?;
            let then = self.body()?;
            let otherwise = if self.is_word("else") {
                self.pos += 1;
                self.body()?
            } else {
                Vec::new()
            };
            return Ok(Stmt::If(cond, then, otherwise));
        }
        if self.is_word("return") {
            self.pos += 1;
            if !self.is(";") && !self.is("}") && self.peek().is_some() {
                // The value is not used: handlers and tests return nothing.
                self.expr()?;
            }
            self.eat(";");
            return Ok(Stmt::Return);
        }
        for word in [
            "for", "while", "do", "switch", "try", "throw", "class", "new",
        ] {
            if self.is_word(word) {
                return Err(format!("'{word}' is not supported in response handlers"));
            }
        }
        if self.is("{") {
            let block = self.block()?;
            return Ok(Stmt::If(Expr::Lit(Value::Bool(true)), block, Vec::new()));
        }
        let e = self.expr()?;
        if !self.eat(";") && !self.is("}") && self.peek().is_some() {
            return Err(format!("Expected ';' {}", self.near()));
        }
        Ok(Stmt::Expr(e))
    }

    fn expr(&mut self) -> Result<Expr, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("The response handler is nested too deeply".into());
        }
        let out = self.conditional();
        self.depth -= 1;
        out
    }

    fn conditional(&mut self) -> Result<Expr, String> {
        let cond = self.binary(0)?;
        if self.is("=") {
            return Err("Assignments are only supported in var/let/const declarations".into());
        }
        if !self.eat("?") {
            return Ok(cond);
        }
        let a = self.expr()?;
        self.expect(":")?;
        let b = self.expr()?;
        Ok(Expr::Cond(Box::new(cond), Box::new(a), Box::new(b)))
    }

    fn binary(&mut self, level: usize) -> Result<Expr, String> {
        const LEVELS: &[&[&str]] = &[
            &["||", "??"],
            &["&&"],
            &["===", "!==", "==", "!="],
            &["<", ">", "<=", ">="],
            &["+", "-"],
            &["*", "/", "%"],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        loop {
            let op = match self.peek() {
                Some(Tok::Punct(p)) if LEVELS[level].contains(p) => *p,
                _ => return Ok(left),
            };
            self.pos += 1;
            let right = self.binary(level + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        for op in ["!", "-", "+"] {
            if self.eat(op) {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err("The response handler is nested too deeply".into());
                }
                let e = self.unary();
                self.depth -= 1;
                return Ok(Expr::Unary(op, Box::new(e?)));
            }
        }
        if self.is_word("typeof") {
            return Err("'typeof' is not supported in response handlers".into());
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.eat(".") {
                e = Expr::Member(Box::new(e), self.ident()?);
            } else if self.eat("[") {
                let index = self.expr()?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(index));
            } else if self.eat("(") {
                let mut args = Vec::new();
                while !self.eat(")") {
                    args.push(self.expr()?);
                    if !self.eat(",") {
                        self.expect(")")?;
                        break;
                    }
                }
                e = Expr::Call(Box::new(e), args);
            } else {
                return Ok(e);
            }
        }
    }

    /// Whether an arrow function starts here: `() =>`, `(a, b) =>` or `a =>`.
    fn arrow_ahead(&self) -> bool {
        if matches!(self.peek(), Some(Tok::Ident(_))) {
            return matches!(self.peek_at(1), Some(Tok::Punct("=>")));
        }
        if !self.is("(") {
            return false;
        }
        let mut n = 1;
        loop {
            match self.peek_at(n) {
                Some(Tok::Ident(_)) | Some(Tok::Punct(",")) => n += 1,
                Some(Tok::Punct(")")) => {
                    return matches!(self.peek_at(n + 1), Some(Tok::Punct("=>")))
                }
                _ => return false,
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        if self.arrow_ahead() {
            if !self.eat("(") {
                self.pos += 1;
            } else {
                while !self.eat(")") {
                    self.pos += 1;
                }
            }
            self.expect("=>")?;
            let body = if self.is("{") {
                self.block()?
            } else {
                vec![Stmt::Expr(self.expr()?)]
            };
            return Ok(Expr::Func(Rc::new(body)));
        }
        if self.is_word("function") {
            self.pos += 1;
            if matches!(self.peek(), Some(Tok::Ident(_))) {
                self.pos += 1;
            }
            self.expect("(")?;
            while !self.eat(")") {
                if self.peek().is_none() {
                    return Err("Expected ')' at the end of the script".into());
                }
                self.pos += 1;
            }
            return Ok(Expr::Func(Rc::new(self.block()?)));
        }
        if self.eat("(") {
            let e = self.expr()?;
            self.expect(")")?;
            return Ok(e);
        }
        if self.eat("[") {
            let mut items = Vec::new();
            while !self.eat("]") {
                items.push(self.expr()?);
                if !self.eat(",") {
                    self.expect("]")?;
                    break;
                }
            }
            return Ok(Expr::Array(items));
        }
        if self.eat("{") {
            let mut fields = Vec::new();
            while !self.eat("}") {
                let key = match self.peek().cloned() {
                    Some(Tok::Ident(k)) | Some(Tok::Str(k)) => {
                        self.pos += 1;
                        k
                    }
                    _ => return Err(format!("Expected a property name {}", self.near())),
                };
                self.expect(":")?;
                fields.push((key, self.expr()?));
                if !self.eat(",") {
                    self.expect("}")?;
                    break;
                }
            }
            return Ok(Expr::Object(fields));
        }
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Ok(Expr::Lit(num(n)))
            }
            Some(Tok::Str(s)) => {
                self.pos += 1;
                Ok(Expr::Lit(Value::String(s)))
            }
            Some(Tok::Ident(i)) => {
                self.pos += 1;
                Ok(match i.as_str() {
                    "true" => Expr::Lit(Value::Bool(true)),
                    "false" => Expr::Lit(Value::Bool(false)),
                    "null" | "undefined" => Expr::Lit(Value::Null),
                    _ => Expr::Ident(i),
                })
            }
            _ => Err(format!(
                "Unexpected {}",
                self.near().trim_start_matches("near ")
            )),
        }
    }
}

// ---- values ------------------------------------------------------------------------------

/// Host objects of the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    Client,
    Global,
    Response,
    Headers,
    ContentType,
    Json,
}

#[derive(Debug, Clone)]
enum V {
    J(Value),
    F(Rc<Vec<Stmt>>),
    H(Host),
}

fn num(n: f64) -> Value {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15 {
        Value::from(n as i64)
    } else {
        serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// JavaScript's `String(v)` (objects as JSON, which is what users want to see).
fn to_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

fn to_num(v: &Value) -> f64 {
    match v {
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) if s.trim().is_empty() => 0.0,
        Value::String(s) => s.trim().parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => to_num(a) == to_num(b),
        _ => a == b,
    }
}

fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::String(_), Value::String(_)) => a == b,
        (Value::Array(_) | Value::Object(_), _) | (_, Value::Array(_) | Value::Object(_)) => a == b,
        _ => to_num(a) == to_num(b),
    }
}

enum Flow {
    Error(String),
    /// A failed `client.assert` (its message).
    Assert(Option<String>),
    Return,
}

type R<T> = Result<T, Flow>;

fn err<T>(message: impl Into<String>) -> R<T> {
    Err(Flow::Error(message.into()))
}

struct Interp<'a> {
    response: &'a HttpResponse,
    body: Value,
    globals: &'a mut BTreeMap<String, String>,
    report: &'a mut HandlerReport,
    scopes: Vec<BTreeMap<String, Value>>,
    steps: usize,
    depth: usize,
    in_test: bool,
}

impl Interp<'_> {
    fn tick(&mut self) -> R<()> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return err("The response handler takes too long");
        }
        Ok(())
    }

    fn block(&mut self, stmts: &[Stmt]) -> R<()> {
        for s in stmts {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        self.tick()?;
        match s {
            Stmt::Expr(e) => {
                self.eval(e)?;
            }
            Stmt::Let(name, e) => {
                let v = self.value(e)?;
                self.scopes
                    .last_mut()
                    .expect("a scope")
                    .insert(name.clone(), v);
            }
            Stmt::If(cond, then, otherwise) => {
                let branch = if truthy(&self.value(cond)?) {
                    then
                } else {
                    otherwise
                };
                self.block(branch)?;
            }
            Stmt::Return => return Err(Flow::Return),
        }
        Ok(())
    }

    /// Run a function literal's body in a new scope.
    fn call_fn(&mut self, body: &Rc<Vec<Stmt>>) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return err("Functions are nested too deeply");
        }
        self.scopes.push(BTreeMap::new());
        let out = match self.block(body) {
            Ok(()) | Err(Flow::Return) => Ok(()),
            Err(e) => Err(e),
        };
        self.scopes.pop();
        self.depth -= 1;
        out
    }

    /// Evaluate to a JSON value (host objects and functions are not values).
    fn value(&mut self, e: &Expr) -> R<Value> {
        match self.eval(e)? {
            V::J(v) => Ok(v),
            V::F(_) => err("A function cannot be used as a value here"),
            V::H(h) => err(format!("'{}' cannot be used as a value", host_name(h))),
        }
    }

    fn eval(&mut self, e: &Expr) -> R<V> {
        self.tick()?;
        Ok(match e {
            Expr::Lit(v) => V::J(v.clone()),
            Expr::Func(body) => V::F(body.clone()),
            Expr::Ident(name) => {
                if let Some(v) = self.scopes.iter().rev().find_map(|s| s.get(name)) {
                    return Ok(V::J(v.clone()));
                }
                match name.as_str() {
                    "client" => V::H(Host::Client),
                    "response" => V::H(Host::Response),
                    "JSON" => V::H(Host::Json),
                    "request" => return err("'request' is not supported in response handlers"),
                    _ => return err(format!("'{name}' is not defined")),
                }
            }
            Expr::Member(obj, name) => {
                let o = self.eval(obj)?;
                self.member(o, name)?
            }
            Expr::Index(obj, index) => {
                let o = self.eval(obj)?;
                let key = self.value(index)?;
                self.member(o, &to_text(&key))?
            }
            Expr::Call(callee, args) => self.call(callee, args)?,
            Expr::Unary(op, x) => {
                let v = self.value(x)?;
                V::J(match *op {
                    "!" => Value::Bool(!truthy(&v)),
                    "-" => num(-to_num(&v)),
                    _ => num(to_num(&v)),
                })
            }
            Expr::Binary(op, a, b) => V::J(self.binary(op, a, b)?),
            Expr::Cond(c, a, b) => {
                if truthy(&self.value(c)?) {
                    self.eval(a)?
                } else {
                    self.eval(b)?
                }
            }
            Expr::Array(items) => {
                let mut out = Vec::new();
                for i in items {
                    out.push(self.value(i)?);
                }
                V::J(Value::Array(out))
            }
            Expr::Object(fields) => {
                let mut out = serde_json::Map::new();
                for (k, v) in fields {
                    let v = self.value(v)?;
                    out.insert(k.clone(), v);
                }
                V::J(Value::Object(out))
            }
        })
    }

    fn binary(&mut self, op: &str, a: &Expr, b: &Expr) -> R<Value> {
        let left = self.value(a)?;
        match op {
            "&&" if !truthy(&left) => return Ok(left),
            "||" if truthy(&left) => return Ok(left),
            "??" if !left.is_null() => return Ok(left),
            "&&" | "||" | "??" => return self.value(b),
            _ => {}
        }
        let right = self.value(b)?;
        Ok(match op {
            "===" => Value::Bool(strict_eq(&left, &right)),
            "!==" => Value::Bool(!strict_eq(&left, &right)),
            "==" => Value::Bool(loose_eq(&left, &right)),
            "!=" => Value::Bool(!loose_eq(&left, &right)),
            "<" | ">" | "<=" | ">=" => {
                let ord = match (&left, &right) {
                    (Value::String(x), Value::String(y)) => x.partial_cmp(y),
                    _ => to_num(&left).partial_cmp(&to_num(&right)),
                };
                Value::Bool(ord.is_some_and(|o| match op {
                    "<" => o.is_lt(),
                    ">" => o.is_gt(),
                    "<=" => o.is_le(),
                    _ => o.is_ge(),
                }))
            }
            "+" if left.is_string() || right.is_string() => {
                Value::String(to_text(&left) + &to_text(&right))
            }
            "+" => num(to_num(&left) + to_num(&right)),
            "-" => num(to_num(&left) - to_num(&right)),
            "*" => num(to_num(&left) * to_num(&right)),
            "/" => num(to_num(&left) / to_num(&right)),
            _ => num(to_num(&left) % to_num(&right)),
        })
    }

    fn member(&mut self, obj: V, name: &str) -> R<V> {
        Ok(match obj {
            V::H(Host::Client) if name == "global" => V::H(Host::Global),
            V::H(Host::Response) => match name {
                "body" => V::J(self.body.clone()),
                "status" => V::J(Value::from(self.response.status)),
                "headers" => V::H(Host::Headers),
                "contentType" => V::H(Host::ContentType),
                _ => return err(format!("'response.{name}' is not supported")),
            },
            V::H(Host::ContentType) => {
                let ct = self.header("content-type").unwrap_or_default();
                let mut parts = ct.split(';');
                let mime = parts.next().unwrap_or("").trim().to_string();
                match name {
                    "mimeType" => V::J(Value::String(mime)),
                    "charset" => V::J(
                        parts
                            .find_map(|p| p.trim().strip_prefix("charset="))
                            .map_or(Value::Null, |c| Value::String(c.trim().to_string())),
                    ),
                    _ => return err(format!("'contentType.{name}' is not supported")),
                }
            }
            V::H(h) => {
                return err(format!(
                    "'{}.{name}' is not supported (call it as a function?)",
                    host_name(h)
                ))
            }
            V::F(_) => return err(format!("Functions have no property '{name}'")),
            V::J(v) => V::J(match (&v, name) {
                (Value::Array(a), "length") => Value::from(a.len()),
                (Value::String(s), "length") => Value::from(s.encode_utf16().count()),
                (Value::Object(o), _) => o.get(name).cloned().unwrap_or(Value::Null),
                (Value::Array(a), _) => name
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| a.get(i).cloned())
                    .unwrap_or(Value::Null),
                (Value::Null, _) => return err(format!("Cannot read property '{name}' of null")),
                _ => Value::Null,
            }),
        })
    }

    fn header(&self, name: &str) -> Option<String> {
        self.response
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.clone())
    }

    fn args(&mut self, args: &[Expr]) -> R<Vec<Value>> {
        let mut out = Vec::new();
        for a in args {
            out.push(self.value(a)?);
        }
        Ok(out)
    }

    fn call(&mut self, callee: &Expr, args: &[Expr]) -> R<V> {
        let Expr::Member(obj, method) = callee else {
            return match self.eval(callee)? {
                V::F(body) => {
                    self.call_fn(&body)?;
                    Ok(V::J(Value::Null))
                }
                _ => err("Only functions can be called"),
            };
        };
        let target = self.eval(obj)?;
        let arg = |vals: &[Value], i: usize| vals.get(i).cloned().unwrap_or(Value::Null);
        match (target, method.as_str()) {
            (V::H(Host::Client), "test") => {
                let name = match args.first() {
                    Some(e) => to_text(&self.value(e)?),
                    None => return err("client.test needs a name and a function"),
                };
                let body = match args.get(1).map(|e| self.eval(e)).transpose()? {
                    Some(V::F(body)) => body,
                    _ => return err("client.test needs a function as its second argument"),
                };
                let was = std::mem::replace(&mut self.in_test, true);
                let outcome = self.call_fn(&body);
                self.in_test = was;
                let (passed, message) = match outcome {
                    Ok(()) | Err(Flow::Return) => (true, None),
                    Err(Flow::Assert(m)) => {
                        (false, Some(m.unwrap_or_else(|| "Assertion failed".into())))
                    }
                    Err(Flow::Error(m)) => (false, Some(m)),
                };
                self.report.entries.push(Entry::Test {
                    name,
                    passed,
                    message,
                });
                Ok(V::J(Value::Null))
            }
            (V::H(Host::Client), "assert") => {
                let vals = self.args(args)?;
                if truthy(&arg(&vals, 0)) {
                    Ok(V::J(Value::Null))
                } else {
                    Err(Flow::Assert(vals.get(1).map(to_text)))
                }
            }
            (V::H(Host::Client), "log") => {
                let vals = self.args(args)?;
                let text = vals.iter().map(to_text).collect::<Vec<_>>().join(" ");
                self.report.entries.push(Entry::Log { text });
                Ok(V::J(Value::Null))
            }
            (V::H(Host::Client), "exit") => Err(Flow::Return),
            (V::H(Host::Global), m) => {
                let vals = self.args(args)?;
                let name = to_text(&arg(&vals, 0));
                Ok(V::J(match m {
                    "set" => {
                        let value = to_text(&arg(&vals, 1));
                        self.globals.insert(name.clone(), value);
                        self.report.entries.push(Entry::Global {
                            name,
                            cleared: false,
                        });
                        Value::Null
                    }
                    "get" => self
                        .globals
                        .get(&name)
                        .map_or(Value::Null, |v| Value::String(v.clone())),
                    "clear" => {
                        self.globals.remove(&name);
                        self.report.entries.push(Entry::Global {
                            name,
                            cleared: true,
                        });
                        Value::Null
                    }
                    "clearAll" => {
                        for name in std::mem::take(self.globals).into_keys() {
                            self.report.entries.push(Entry::Global {
                                name,
                                cleared: true,
                            });
                        }
                        Value::Null
                    }
                    "isEmpty" => Value::Bool(self.globals.is_empty()),
                    _ => return err(format!("'client.global.{m}' is not supported")),
                }))
            }
            (V::H(Host::Headers), m @ ("valueOf" | "valuesOf")) => {
                let vals = self.args(args)?;
                let name = to_text(&arg(&vals, 0));
                let values: Vec<Value> = self
                    .response
                    .headers
                    .iter()
                    .filter(|h| h.name.eq_ignore_ascii_case(&name))
                    .map(|h| Value::String(h.value.clone()))
                    .collect();
                Ok(V::J(if m == "valueOf" {
                    values.into_iter().next().unwrap_or(Value::Null)
                } else {
                    Value::Array(values)
                }))
            }
            (V::H(Host::Json), "stringify") => {
                let vals = self.args(args)?;
                let v = arg(&vals, 0);
                let pretty = vals.len() > 2 && !arg(&vals, 2).is_null();
                let text = if pretty {
                    serde_json::to_string_pretty(&v)
                } else {
                    serde_json::to_string(&v)
                };
                Ok(V::J(Value::String(text.unwrap_or_default())))
            }
            (V::H(Host::Json), "parse") => {
                let vals = self.args(args)?;
                match serde_json::from_str(&to_text(&arg(&vals, 0))) {
                    Ok(v) => Ok(V::J(v)),
                    Err(e) => err(format!("JSON.parse: {e}")),
                }
            }
            (V::H(h), m) => err(format!("'{}.{m}()' is not supported", host_name(h))),
            (V::F(_), m) => err(format!("'{m}()' is not supported on functions")),
            (V::J(v), m) => {
                let vals = self.args(args)?;
                let a0 = arg(&vals, 0);
                Ok(V::J(match (&v, m) {
                    (Value::String(s), "includes") => Value::Bool(s.contains(&to_text(&a0))),
                    (Value::String(s), "startsWith") => Value::Bool(s.starts_with(&to_text(&a0))),
                    (Value::String(s), "endsWith") => Value::Bool(s.ends_with(&to_text(&a0))),
                    (Value::String(s), "indexOf") => {
                        let needle = to_text(&a0);
                        match s.find(&needle) {
                            Some(b) => Value::from(s[..b].encode_utf16().count()),
                            None => Value::from(-1),
                        }
                    }
                    (Value::String(s), "toLowerCase") => Value::String(s.to_lowercase()),
                    (Value::String(s), "toUpperCase") => Value::String(s.to_uppercase()),
                    (Value::String(s), "trim") => Value::String(s.trim().to_string()),
                    (Value::Array(a), "includes") => {
                        Value::Bool(a.iter().any(|x| strict_eq(x, &a0)))
                    }
                    (Value::Array(a), "indexOf") => Value::from(
                        a.iter()
                            .position(|x| strict_eq(x, &a0))
                            .map_or(-1, |i| i as i64),
                    ),
                    (_, "toString") => Value::String(to_text(&v)),
                    (Value::Null, _) => return err(format!("Cannot call '{m}' of null")),
                    _ => return err(format!("'{m}()' is not supported in response handlers")),
                }))
            }
        }
    }
}

fn host_name(h: Host) -> &'static str {
    match h {
        Host::Client => "client",
        Host::Global => "client.global",
        Host::Response => "response",
        Host::Headers => "response.headers",
        Host::ContentType => "response.contentType",
        Host::Json => "JSON",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Header;

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            version: "HTTP/1.1".into(),
            status,
            reason: "OK".into(),
            headers: vec![
                Header {
                    name: "Content-Type".into(),
                    value: "application/json; charset=utf-8".into(),
                },
                Header {
                    name: "Set-Cookie".into(),
                    value: "a=1".into(),
                },
                Header {
                    name: "set-cookie".into(),
                    value: "b=2".into(),
                },
            ],
            body: body.into(),
            size: body.len() as u64,
            elapsed_ms: 1,
        }
    }

    const BODY: &str = r#"{"data":{"token":"abc","id":1043,"items":[{"sku":"MUG-001","quantity":2}],"paid":false}}"#;

    #[test]
    fn sets_globals_and_runs_tests() {
        let script = r#"
            // Remember the token for later requests.
            client.global.set("token", response.body.data.token);
            client.global.set("first", response.body.data.items[0]["sku"]);
            client.global.set("count", response.body.data.items.length);
            client.test("Request executed successfully", function () {
                client.assert(response.status === 200, "Response status is not 200");
            });
            client.test("Order has items", () => {
                client.assert(response.body.data.items.length > 0);
                client.assert(response.body.data.paid, "Order is not paid");
            });
            client.log("created order " + response.body.data.id);
        "#;
        let mut globals = BTreeMap::new();
        let report = run(script, &response(200, BODY), &mut globals);
        assert_eq!(globals["token"], "abc");
        assert_eq!(globals["first"], "MUG-001");
        assert_eq!(globals["count"], "1");
        assert_eq!((report.passed(), report.failed()), (1, 1));
        assert!(report.entries.contains(&Entry::Test {
            name: "Order has items".into(),
            passed: false,
            message: Some("Order is not paid".into()),
        }));
        assert!(report.entries.contains(&Entry::Log {
            text: "created order 1043".into()
        }));
        let text = render(&report);
        assert!(text.contains("✓ Request executed successfully"), "{text}");
        assert!(text.contains("✗ Order has items"), "{text}");
        assert!(text.contains("Global variable 'token' set"), "{text}");
        assert!(text.contains("1 passed"), "{text}");
    }

    #[test]
    fn expressions_headers_and_globals() {
        let mut globals = BTreeMap::from([("old".to_string(), "x".to_string())]);
        let script = r#"
            let mime = response.contentType.mimeType;
            var cookies = response.headers.valuesOf("set-cookie");
            if (cookies.length == 2 && mime === "application/json") {
                client.global.set("ok", true);
            } else client.global.set("ok", false);
            client.global.set("charset", response.contentType.charset);
            client.global.set("math", (1 + 2) * 3 % 4 - -1);
            client.global.set("str", "n=" + 1 + 2);
            client.global.set("loose", "1" == 1);
            client.global.set("strict", "1" === 1);
            client.global.set("tern", response.status >= 400 ? "bad" : "good");
            client.global.set("nullish", response.body.missing ?? "fallback");
            client.global.set("json", JSON.stringify({ a: [1, "b"], c: null }));
            client.global.set("parsed", JSON.parse("{\"x\": 5}").x);
            client.global.set("hdr", response.headers.valueOf("CONTENT-TYPE").includes("json"));
            client.global.set("had", client.global.get("old"));
            client.global.clear("old");
            client.global.set("upper", "MUG".toLowerCase().startsWith("mu"));
        "#;
        let report = run(script, &response(201, BODY), &mut globals);
        assert!(
            !report
                .entries
                .iter()
                .any(|e| matches!(e, Entry::Error { .. })),
            "{report:?}"
        );
        assert_eq!(globals["ok"], "true");
        assert_eq!(globals["charset"], "utf-8");
        assert_eq!(globals["math"], "2");
        assert_eq!(globals["str"], "n=12");
        assert_eq!(globals["loose"], "true");
        assert_eq!(globals["strict"], "false");
        assert_eq!(globals["tern"], "good");
        assert_eq!(globals["nullish"], "fallback");
        assert_eq!(globals["json"], r#"{"a":[1,"b"],"c":null}"#);
        assert_eq!(globals["parsed"], "5");
        assert_eq!(globals["hdr"], "true");
        assert_eq!(globals["had"], "x");
        assert_eq!(globals["upper"], "true");
        assert!(!globals.contains_key("old"));
    }

    #[test]
    fn plain_text_bodies_and_failed_asserts_outside_tests() {
        let mut globals = BTreeMap::new();
        let report = run(
            r#"client.global.set("t", response.body); client.assert(response.status === 200, "Not OK"); client.log("never")"#,
            &response(500, "Internal error"),
            &mut globals,
        );
        assert_eq!(globals["t"], "Internal error");
        assert_eq!(report.failed(), 1);
        assert!(!report
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Log { .. })));
    }

    #[test]
    fn rejects_unsupported_code_safely() {
        let mut globals = BTreeMap::new();
        let cases = [
            ("while (true) {}", "'while' is not supported"),
            ("for (;;) {}", "'for' is not supported"),
            ("fetch('http://evil')", "'fetch' is not defined"),
            ("x = 1", "Assignments are only supported"),
            ("client.global.set(\"a\", `${1}`)", "Template literals"),
            (
                "response.body.nope.deeper",
                "Cannot read property 'deeper' of null",
            ),
            ("client.test('t')", "client.test needs a function"),
            ("client.global.set('a'", "Expected ')'"),
            ("require('fs')", "'require' is not defined"),
            ("response.status.toFixed(2)", "'toFixed()' is not supported"),
        ];
        for (script, expected) in cases {
            let report = run(script, &response(200, BODY), &mut globals);
            let Some(Entry::Error { message }) = report.entries.last() else {
                panic!("{script}: {report:?}");
            };
            assert!(message.contains(expected), "{script}: {message}");
        }
        let deep = format!("{}1{}", "(".repeat(500), ")".repeat(500));
        let report = run(&deep, &response(200, BODY), &mut globals);
        assert!(matches!(report.entries.last(), Some(Entry::Error { .. })));
        assert!(globals.is_empty());
        // Errors inside a test fail only that test.
        let report = run(
            "client.test('a', () => { nope(); }); client.test('b', () => {});",
            &response(200, BODY),
            &mut globals,
        );
        assert_eq!((report.passed(), report.failed()), (1, 1));
        assert_eq!(render(&HandlerReport::default()), "");
    }
}
