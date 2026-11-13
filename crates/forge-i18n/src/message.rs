//! ICU MessageFormat: parsing and formatting.
//!
//! Supported syntax (the subset of ICU MessageFormat 1 that UI strings need):
//!
//! - `{name}` — an argument, formatted as text (numbers use the locale's number format);
//! - `{name, number}`, `{name, number, integer}`, `{name, number, percent}`;
//! - `{name, date}`, `{name, date, short|medium|long}`, `{name, time}`,
//!   `{name, time, short|medium}` — the argument is milliseconds since the Unix epoch;
//! - `{name, plural, =0 {…} one {…} other {…}}` and `{name, selectordinal, …}` with an
//!   optional `offset:n` and `#` for the (offset) number inside a case;
//! - `{name, select, a {…} other {…}}`;
//! - apostrophe quoting: `''` is one apostrophe, `'{'` / `'#'` quote syntax characters, and a
//!   lone apostrophe elsewhere stays literal ("don't" needs no escaping).

use std::collections::BTreeSet;
use std::fmt;

use serde_json::Value;

use crate::format::{self, DateStyle, NumberStyle};
use crate::plural::{self, PluralCategory};

/// Message arguments by name.
pub type Args = serde_json::Map<String, Value>;

/// A parsed message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    /// `#` inside a plural case.
    Pound,
    Arg(String),
    Number(String, NumberStyle),
    Date(String, DateStyle),
    Time(String, DateStyle),
    Plural {
        arg: String,
        ordinal: bool,
        offset: f64,
        cases: Vec<(PluralKey, Vec<Part>)>,
    },
    Select {
        arg: String,
        cases: Vec<(String, Vec<Part>)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum PluralKey {
    Exact(f64),
    Category(PluralCategory),
}

/// A syntax error with its character offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at offset {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    src: &'a str,
}

impl Message {
    pub fn parse(src: &str) -> Result<Self, ParseError> {
        let mut p = Parser {
            chars: src.chars().collect(),
            pos: 0,
            src,
        };
        let parts = p.parts(false, 0)?;
        if p.pos < p.chars.len() {
            return Err(p.err("unmatched `}`"));
        }
        Ok(Self { parts })
    }

    /// A message without any arguments, whose text is `text` as is.
    pub fn literal(text: &str) -> Self {
        Self {
            parts: vec![Part::Text(text.to_string())],
        }
    }

    /// Names of all arguments the message refers to (including plural and select selectors).
    pub fn arguments(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        collect_args(&self.parts, &mut out);
        out
    }

    /// Whether formatting needs arguments (the text depends on them).
    pub fn has_arguments(&self) -> bool {
        !self.arguments().is_empty()
    }

    /// Format with `args` for `locale` (plural rules, number and date formats).
    /// Missing arguments are shown as `{name}`, so a broken call is visible but harmless.
    pub fn format(&self, locale: &str, args: &Args, tz_offset_minutes: i32) -> String {
        let mut out = String::new();
        let ctx = Ctx {
            locale,
            args,
            tz: tz_offset_minutes,
        };
        ctx.write(&self.parts, None, &mut out);
        out
    }
}

fn collect_args(parts: &[Part], out: &mut BTreeSet<String>) {
    for p in parts {
        match p {
            Part::Text(_) | Part::Pound => {}
            Part::Arg(a) | Part::Number(a, _) | Part::Date(a, _) | Part::Time(a, _) => {
                out.insert(a.clone());
            }
            Part::Plural { arg, cases, .. } => {
                out.insert(arg.clone());
                for (_, c) in cases {
                    collect_args(c, out);
                }
            }
            Part::Select { arg, cases } => {
                out.insert(arg.clone());
                for (_, c) in cases {
                    collect_args(c, out);
                }
            }
        }
    }
}

impl Parser<'_> {
    fn err(&self, message: &str) -> ParseError {
        ParseError {
            offset: self.pos,
            message: format!("{message} in `{}`", self.src),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    /// Text and arguments up to an unquoted `}` (not consumed) or the end.
    fn parts(&mut self, in_plural: bool, depth: usize) -> Result<Vec<Part>, ParseError> {
        if depth > 16 {
            return Err(self.err("message nested too deeply"));
        }
        let mut parts = Vec::new();
        let mut text = String::new();
        while let Some(c) = self.peek() {
            match c {
                '\'' => {
                    let next = self.chars.get(self.pos + 1).copied();
                    if next == Some('\'') {
                        text.push('\'');
                        self.pos += 2;
                    } else if matches!(next, Some('{' | '}' | '|'))
                        || (in_plural && next == Some('#'))
                    {
                        // Quoted literal up to the next single apostrophe.
                        self.pos += 1;
                        loop {
                            match self.peek() {
                                None => break,
                                Some('\'') if self.chars.get(self.pos + 1) == Some(&'\'') => {
                                    text.push('\'');
                                    self.pos += 2;
                                }
                                Some('\'') => {
                                    self.pos += 1;
                                    break;
                                }
                                Some(ch) => {
                                    text.push(ch);
                                    self.pos += 1;
                                }
                            }
                        }
                    } else {
                        text.push('\'');
                        self.pos += 1;
                    }
                }
                '{' => {
                    if !text.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut text)));
                    }
                    self.pos += 1;
                    parts.push(self.argument(depth)?);
                }
                '}' => break,
                '#' if in_plural => {
                    if !text.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut text)));
                    }
                    parts.push(Part::Pound);
                    self.pos += 1;
                }
                _ => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
        if !text.is_empty() {
            parts.push(Part::Text(text));
        }
        Ok(parts)
    }

    fn ident(&mut self) -> String {
        self.skip_ws();
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn expect(&mut self, c: char) -> Result<(), ParseError> {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected `{c}`")))
        }
    }

    /// After `{`: the argument up to and including its `}`.
    fn argument(&mut self, depth: usize) -> Result<Part, ParseError> {
        let name = self.ident();
        if name.is_empty() {
            return Err(self.err("expected an argument name"));
        }
        self.skip_ws();
        match self.peek() {
            Some('}') => {
                self.pos += 1;
                return Ok(Part::Arg(name));
            }
            Some(',') => self.pos += 1,
            _ => return Err(self.err("expected `,` or `}` after the argument name")),
        }
        let kind = self.ident();
        self.skip_ws();
        let part = match kind.as_str() {
            "number" | "date" | "time" => {
                let style = if self.peek() == Some(',') {
                    self.pos += 1;
                    self.ident()
                } else {
                    String::new()
                };
                match kind.as_str() {
                    "number" => Part::Number(
                        name,
                        match style.as_str() {
                            "" => NumberStyle::Decimal,
                            "integer" => NumberStyle::Integer,
                            "percent" => NumberStyle::Percent,
                            other => {
                                return Err(self.err(&format!("unknown number style `{other}`")))
                            }
                        },
                    ),
                    _ => {
                        let style = match style.as_str() {
                            "" | "medium" => DateStyle::Medium,
                            "short" => DateStyle::Short,
                            "long" | "full" => DateStyle::Long,
                            other => return Err(self.err(&format!("unknown date style `{other}`"))),
                        };
                        if kind == "date" {
                            Part::Date(name, style)
                        } else {
                            Part::Time(name, style)
                        }
                    }
                }
            }
            "plural" | "selectordinal" => {
                self.expect(',')?;
                self.skip_ws();
                let mut offset = 0.0;
                if self.chars[self.pos..].starts_with(&['o', 'f', 'f', 's', 'e', 't', ':']) {
                    self.pos += 7;
                    self.skip_ws();
                    offset = self.number()?;
                }
                let mut cases = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some('}') || self.peek().is_none() {
                        break;
                    }
                    let key = if self.peek() == Some('=') {
                        self.pos += 1;
                        PluralKey::Exact(self.number()?)
                    } else {
                        let k = self.ident();
                        PluralKey::Category(
                            PluralCategory::parse(&k)
                                .ok_or_else(|| self.err(&format!("unknown plural case `{k}`")))?,
                        )
                    };
                    self.expect('{')?;
                    let body = self.parts(true, depth + 1)?;
                    self.expect('}')?;
                    cases.push((key, body));
                }
                if !cases
                    .iter()
                    .any(|(k, _)| *k == PluralKey::Category(PluralCategory::Other))
                {
                    return Err(self.err("a plural needs an `other` case"));
                }
                Part::Plural {
                    arg: name,
                    ordinal: kind == "selectordinal",
                    offset,
                    cases,
                }
            }
            "select" => {
                self.expect(',')?;
                let mut cases = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some('}') || self.peek().is_none() {
                        break;
                    }
                    let k = self.ident();
                    if k.is_empty() {
                        return Err(self.err("expected a select case"));
                    }
                    self.expect('{')?;
                    let body = self.parts(false, depth + 1)?;
                    self.expect('}')?;
                    cases.push((k, body));
                }
                if !cases.iter().any(|(k, _)| k == "other") {
                    return Err(self.err("a select needs an `other` case"));
                }
                Part::Select { arg: name, cases }
            }
            other => return Err(self.err(&format!("unknown argument type `{other}`"))),
        };
        self.expect('}')?;
        Ok(part)
    }

    fn number(&mut self) -> Result<f64, ParseError> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '-')
        {
            self.pos += 1;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse().map_err(|_| self.err("expected a number"))
    }
}

struct Ctx<'a> {
    locale: &'a str,
    args: &'a Args,
    tz: i32,
}

fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

impl Ctx<'_> {
    fn missing(name: &str, out: &mut String) {
        out.push('{');
        out.push_str(name);
        out.push('}');
    }

    fn write(&self, parts: &[Part], pound: Option<f64>, out: &mut String) {
        for p in parts {
            match p {
                Part::Text(t) => out.push_str(t),
                Part::Pound => match pound {
                    Some(n) => out.push_str(&format::number(self.locale, n, NumberStyle::Decimal)),
                    None => out.push('#'),
                },
                Part::Arg(name) => match self.args.get(name) {
                    Some(Value::String(s)) => out.push_str(s),
                    Some(Value::Number(n)) => out.push_str(&format::number(
                        self.locale,
                        n.as_f64().unwrap_or(0.0),
                        NumberStyle::Decimal,
                    )),
                    Some(Value::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
                    Some(Value::Null) | None => Self::missing(name, out),
                    Some(other) => out.push_str(&other.to_string()),
                },
                Part::Number(name, style) => match self.args.get(name).and_then(as_number) {
                    Some(n) => out.push_str(&format::number(self.locale, n, *style)),
                    None => Self::missing(name, out),
                },
                Part::Date(name, style) | Part::Time(name, style) => {
                    match self.args.get(name).and_then(as_number) {
                        Some(ms) => {
                            let time = matches!(p, Part::Time(..));
                            out.push_str(&format::date_time(
                                self.locale,
                                ms as i64,
                                self.tz,
                                *style,
                                time,
                            ))
                        }
                        None => Self::missing(name, out),
                    }
                }
                Part::Plural {
                    arg,
                    ordinal,
                    offset,
                    cases,
                } => {
                    let Some(n) = self.args.get(arg).and_then(as_number) else {
                        Self::missing(arg, out);
                        continue;
                    };
                    let exact = cases
                        .iter()
                        .find(|(k, _)| matches!(k, PluralKey::Exact(e) if *e == n));
                    let shown = n - offset;
                    let case = exact.or_else(|| {
                        let cat = if *ordinal {
                            plural::ordinal(self.locale, shown)
                        } else {
                            plural::cardinal(self.locale, shown)
                        };
                        cases
                            .iter()
                            .find(|(k, _)| *k == PluralKey::Category(cat))
                            .or_else(|| {
                                cases
                                    .iter()
                                    .find(|(k, _)| *k == PluralKey::Category(PluralCategory::Other))
                            })
                    });
                    if let Some((_, body)) = case {
                        self.write(body, Some(shown), out);
                    }
                }
                Part::Select { arg, cases } => {
                    let key = match self.args.get(arg) {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Bool(b)) => b.to_string(),
                        Some(Value::Number(n)) => n.to_string(),
                        _ => String::new(),
                    };
                    let case = cases
                        .iter()
                        .find(|(k, _)| *k == key)
                        .or_else(|| cases.iter().find(|(k, _)| k == "other"));
                    if let Some((_, body)) = case {
                        self.write(body, pound, out);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fmt(src: &str, locale: &str, args: Value) -> String {
        let args = args.as_object().cloned().unwrap_or_default();
        Message::parse(src).unwrap().format(locale, &args, 0)
    }

    #[test]
    fn plain_text_and_arguments() {
        assert_eq!(fmt("Project", "en", json!({})), "Project");
        assert_eq!(
            fmt("Open {name}?", "en", json!({ "name": "app.php" })),
            "Open app.php?"
        );
        assert_eq!(fmt("Missing {name}", "en", json!({})), "Missing {name}");
    }

    #[test]
    fn apostrophes() {
        assert_eq!(fmt("don't", "en", json!({})), "don't");
        assert_eq!(fmt("it''s", "en", json!({})), "it's");
        assert_eq!(fmt("'{braces}'", "en", json!({})), "{braces}");
        assert_eq!(
            fmt("{n, plural, other {'#' is #}}", "en", json!({ "n": 3 })),
            "# is 3"
        );
    }

    #[test]
    fn plurals_english_and_german() {
        let src = "{count, plural, =0 {No files} one {# file} other {# files}}";
        assert_eq!(fmt(src, "en", json!({ "count": 0 })), "No files");
        assert_eq!(fmt(src, "en", json!({ "count": 1 })), "1 file");
        assert_eq!(fmt(src, "en", json!({ "count": 1234 })), "1,234 files");
        let de = "{count, plural, =0 {Keine Dateien} one {# Datei} other {# Dateien}}";
        assert_eq!(fmt(de, "de", json!({ "count": 1 })), "1 Datei");
        assert_eq!(fmt(de, "de", json!({ "count": 1234 })), "1.234 Dateien");
        assert_eq!(fmt(de, "de", json!({ "count": 1.5 })), "1,5 Dateien");
    }

    #[test]
    fn plural_offset_and_ordinal() {
        let src = "{n, plural, offset:1 =0 {nobody} =1 {you} one {you and # other} other {you and # others}}";
        assert_eq!(fmt(src, "en", json!({ "n": 1 })), "you");
        assert_eq!(fmt(src, "en", json!({ "n": 2 })), "you and 1 other");
        assert_eq!(fmt(src, "en", json!({ "n": 5 })), "you and 4 others");
        let ord = "{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}";
        assert_eq!(fmt(ord, "en", json!({ "n": 1 })), "1st");
        assert_eq!(fmt(ord, "en", json!({ "n": 22 })), "22nd");
        assert_eq!(fmt(ord, "en", json!({ "n": 13 })), "13th");
        assert_eq!(
            fmt("{n, selectordinal, other {#.}}", "de", json!({ "n": 3 })),
            "3."
        );
    }

    #[test]
    fn select_and_nesting() {
        let src =
            "{kind, select, file {{count, plural, one {# file} other {# files}}} other {# items}}";
        assert_eq!(
            fmt(src, "en", json!({ "kind": "file", "count": 2 })),
            "2 files"
        );
        assert_eq!(
            fmt(src, "en", json!({ "kind": "x", "count": 2 })),
            "# items"
        );
    }

    #[test]
    fn numbers_and_dates() {
        assert_eq!(
            fmt("{n, number}", "de", json!({ "n": 1234567.891 })),
            "1.234.567,891"
        );
        assert_eq!(fmt("{n, number, integer}", "en", json!({ "n": 2.6 })), "3");
        assert_eq!(
            fmt("{n, number, percent}", "en", json!({ "n": 0.25 })),
            "25%"
        );
        assert_eq!(
            fmt("{n, number, percent}", "de", json!({ "n": 0.25 })),
            "25\u{a0}%"
        );
        // 2026-11-04T15:04:05Z
        let ms = 1_793_804_645_000_i64;
        assert_eq!(
            fmt("{d, date, medium}", "en", json!({ "d": ms })),
            "Nov 4, 2026"
        );
        assert_eq!(
            fmt("{d, date, medium}", "de", json!({ "d": ms })),
            "04.11.2026"
        );
        assert_eq!(
            fmt("{d, date, long}", "de", json!({ "d": ms })),
            "4. November 2026"
        );
        assert_eq!(fmt("{d, time, short}", "en", json!({ "d": ms })), "3:04 PM");
        assert_eq!(fmt("{d, time, short}", "de", json!({ "d": ms })), "15:04");
    }

    #[test]
    fn arguments_and_errors() {
        let m =
            Message::parse("{a} {n, plural, one {{b}} other {#}} {s, select, x {{c}} other {}}")
                .unwrap();
        let names: Vec<_> = m.arguments().into_iter().collect();
        assert_eq!(names, ["a", "b", "c", "n", "s"]);
        assert!(Message::parse("{unclosed").is_err());
        assert!(Message::parse("{n, plural, one {x}}").is_err());
        assert!(Message::parse("{n, bogus}").is_err());
        assert!(Message::parse("stray }").is_err());
        assert!(!Message::parse("plain").unwrap().has_arguments());
    }
}
