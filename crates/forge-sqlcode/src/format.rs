//! The SQL formatter (Reformat Code, Actions on Save, the Code Style preview): keyword and
//! identifier case, clauses on their own lines (optionally right-aligned "river" style),
//! select-list items on separate lines with trailing or leading commas, AND / OR on new
//! lines, indented subqueries and CREATE TABLE column lists, and the spaces of the Code
//! Style page. Dialect quirks: MySQL `#` comments and backticks, PostgreSQL `::` casts and
//! dollar-quoted bodies, SQL Server `[names]` and `GO` batches, Oracle `/` terminators;
//! routines and procedural blocks keep their layout (only the keyword case changes).
//! Comments and string contents are never changed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lang::SqlDialect;
use crate::lex::{lex, Kind, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Case {
    #[default]
    AsIs,
    Upper,
    Lower,
}

impl Case {
    fn parse(s: &str) -> Self {
        match s {
            "upper" => Case::Upper,
            "lower" => Case::Lower,
            _ => Case::AsIs,
        }
    }

    fn apply(self, s: &str) -> String {
        match self {
            Case::AsIs => s.to_string(),
            Case::Upper => s.to_uppercase(),
            Case::Lower => s.to_lowercase(),
        }
    }
}

/// The options of Settings | Editor | Code Style | SQL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SqlStyle {
    pub keyword_case: Case,
    pub identifier_case: Case,
    pub indent: usize,
    pub use_tabs: bool,
    /// Clauses (FROM, WHERE, JOIN, …) start a new line.
    pub clause_newline: bool,
    /// Select-list, SET, GROUP BY and ORDER BY items on separate lines.
    pub items_newline: bool,
    /// Commas at the start of the next item's line.
    pub comma_first: bool,
    /// AND / OR of WHERE, HAVING and ON start a new line.
    pub and_or_newline: bool,
    /// Clause keywords right-aligned ("river").
    pub align_right: bool,
    pub space_after_comma: bool,
    pub space_around_operators: bool,
    pub space_within_parentheses: bool,
    /// Blank lines kept between statements (at most).
    pub blank_lines_between: usize,
}

impl Default for SqlStyle {
    fn default() -> Self {
        Self {
            keyword_case: Case::Upper,
            identifier_case: Case::AsIs,
            indent: 4,
            use_tabs: false,
            clause_newline: true,
            items_newline: false,
            comma_first: false,
            and_or_newline: true,
            align_right: false,
            space_after_comma: true,
            space_around_operators: true,
            space_within_parentheses: false,
            blank_lines_between: 1,
        }
    }
}

impl SqlStyle {
    /// The style of a code style scheme's values (`sql.case.keywords`, `sql.indent`, …).
    pub fn from_values(values: &BTreeMap<String, Value>) -> Self {
        let d = SqlStyle::default();
        let get = |k: &str| values.get(&format!("sql.{k}"));
        let b = |k: &str, def: bool| get(k).and_then(Value::as_bool).unwrap_or(def);
        let s = |k: &str| get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let n = |k: &str, def: usize| {
            get(k)
                .and_then(Value::as_u64)
                .map(|v| v as usize)
                .unwrap_or(def)
        };
        let case = |k: &str, def: Case| match get(k).and_then(Value::as_str) {
            Some(v) => Case::parse(v),
            None => def,
        };
        SqlStyle {
            keyword_case: case("case.keywords", d.keyword_case),
            identifier_case: case("case.identifiers", d.identifier_case),
            indent: n("indent", d.indent).max(1),
            use_tabs: b("useTabs", d.use_tabs),
            clause_newline: b("wrapping.clauseNewline", d.clause_newline),
            items_newline: b("wrapping.itemsNewline", d.items_newline),
            comma_first: s("wrapping.commaPosition") == "start",
            and_or_newline: b("wrapping.andOrNewline", d.and_or_newline),
            align_right: s("wrapping.align") == "right",
            space_after_comma: b("spaces.afterComma", d.space_after_comma),
            space_around_operators: b("spaces.aroundOperators", d.space_around_operators),
            space_within_parentheses: b("spaces.withinParentheses", d.space_within_parentheses),
            blank_lines_between: n("blankLines.keepMax", d.blank_lines_between),
        }
    }

    fn pad(&self, cols: usize) -> String {
        if self.use_tabs {
            let tabs = cols / self.indent;
            "\t".repeat(tabs) + &" ".repeat(cols % self.indent)
        } else {
            " ".repeat(cols)
        }
    }
}

/// Only change the case of keywords and identifiers (the pass after another formatter).
pub fn change_case(text: &str, d: SqlDialect, style: &SqlStyle) -> String {
    let toks = lex(text, d);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (i, t) in toks.iter().enumerate() {
        out.push_str(&text[at..t.start]);
        out.push_str(&cased(text, &toks, i, d, style));
        at = t.end;
    }
    out.push_str(&text[at..]);
    out
}

fn is_keyword(src: &str, toks: &[Token], i: usize, d: SqlDialect) -> bool {
    let t = toks[i];
    if t.kind != Kind::Word {
        return false;
    }
    let w = t.text(src);
    let prev_dot = i > 0 && toks[i - 1].kind == Kind::Dot;
    let next_dot = toks.get(i + 1).is_some_and(|n| n.kind == Kind::Dot);
    !prev_dot
        && !next_dot
        && (d.is_keyword(w) || d.types().iter().any(|k| k.eq_ignore_ascii_case(w)))
}

fn cased(src: &str, toks: &[Token], i: usize, d: SqlDialect, style: &SqlStyle) -> String {
    let t = toks[i];
    let text = t.text(src);
    if t.kind != Kind::Word {
        return text.to_string();
    }
    if is_keyword(src, toks, i, d) {
        style.keyword_case.apply(text)
    } else {
        style.identifier_case.apply(text)
    }
}

/// Words that start a clause line.
const CLAUSE_WORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP",
    "HAVING",
    "ORDER",
    "LIMIT",
    "OFFSET",
    "FETCH",
    "UNION",
    "EXCEPT",
    "INTERSECT",
    "MINUS",
    "INSERT",
    "UPDATE",
    "DELETE",
    "SET",
    "VALUES",
    "RETURNING",
    "WINDOW",
    "QUALIFY",
    "JOIN",
    "LEFT",
    "RIGHT",
    "FULL",
    "INNER",
    "CROSS",
    "NATURAL",
    "STRAIGHT_JOIN",
    "PREWHERE",
    "SETTINGS",
    "FORMAT",
    "OUTPUT",
    "ON",
    "WITH",
    "USING",
    "MERGE",
    "WHEN",
    "INTO",
];

/// Clauses whose items go on separate lines.
const LIST_CLAUSES: &[&str] = &["SELECT", "GROUP", "ORDER", "SET", "RETURNING"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A statement or subquery: clauses start lines at `base`.
    Query,
    /// A parenthesized list with one item per line (CREATE TABLE columns).
    List,
    /// Function arguments, IN lists, expressions.
    Inline,
}

#[derive(Debug, Clone)]
struct Level {
    frame: Frame,
    /// Indent of clause lines (Query) or of the line with `(`.
    base: usize,
    /// The current clause (upper case first word).
    clause: String,
    /// Inside BETWEEN … AND.
    between: bool,
    /// Whether anything was written in this level yet.
    started: bool,
}

struct Printer<'a> {
    src: &'a str,
    d: SqlDialect,
    s: &'a SqlStyle,
    toks: Vec<Token>,
    out: String,
    stack: Vec<Level>,
    /// A line comment ended the line: the next token starts a new one.
    pending_nl: bool,
}

impl<'a> Printer<'a> {
    fn kw(&self, i: usize, k: &str) -> bool {
        self.toks.get(i).is_some_and(|t| t.is_kw(self.src, k))
    }

    fn up(&self, i: usize) -> String {
        self.toks[i].text(self.src).to_ascii_uppercase()
    }

    /// The previous non-comment token.
    fn prev(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&k| self.toks[k].kind != Kind::Comment)
    }

    fn next(&self, i: usize) -> Option<usize> {
        (i + 1..self.toks.len()).find(|&k| self.toks[k].kind != Kind::Comment)
    }

    fn level(&self) -> &Level {
        self.stack.last().expect("level")
    }

    fn level_mut(&mut self) -> &mut Level {
        self.stack.last_mut().expect("level")
    }

    fn trim_end(&mut self) {
        while self.out.ends_with(' ') || self.out.ends_with('\t') {
            self.out.pop();
        }
    }

    fn newline(&mut self, cols: usize) {
        self.trim_end();
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        let pad = self.s.pad(cols);
        self.out.push_str(&pad);
    }

    fn at_line_start(&self) -> bool {
        let line = self.out.rsplit('\n').next().unwrap_or("");
        line.trim().is_empty()
    }

    /// The indent of the current line.
    fn line_indent(&self) -> usize {
        let line = self.out.rsplit('\n').next().unwrap_or("");
        let mut cols = 0;
        for c in line.chars() {
            match c {
                ' ' => cols += 1,
                '\t' => cols += self.s.indent,
                _ => break,
            }
        }
        cols
    }

    /// River alignment: the padding that right-aligns a clause word to SELECT.
    fn river(&self, word: &str) -> usize {
        if self.s.align_right {
            6usize.saturating_sub(word.len())
        } else {
            0
        }
    }

    /// Indent of the items of the current clause.
    fn item_indent(&self) -> usize {
        let l = self.level();
        match l.frame {
            Frame::Query if self.s.align_right => l.base + 7,
            Frame::Query => l.base + self.s.indent,
            Frame::List => l.base + self.s.indent,
            Frame::Inline => l.base,
        }
    }

    fn space(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with([' ', '\n', '\t']) {
            self.out.push(' ');
        }
    }

    /// Whether token `i` starts a clause of the current query level.
    fn starts_clause(&self, i: usize) -> bool {
        if self.level().frame != Frame::Query || self.toks[i].kind != Kind::Word {
            return false;
        }
        let up = self.up(i);
        if !CLAUSE_WORDS.contains(&up.as_str()) {
            return false;
        }
        let prev = self.prev(i).map(|p| self.up(p)).unwrap_or_default();
        let next = self.next(i).map(|n| self.up(n)).unwrap_or_default();
        match up.as_str() {
            // The second word of a join phrase, DELETE FROM, INSERT INTO, …
            "JOIN" | "OUTER" => !matches!(
                prev.as_str(),
                "LEFT"
                    | "RIGHT"
                    | "FULL"
                    | "INNER"
                    | "CROSS"
                    | "NATURAL"
                    | "OUTER"
                    | "SEMI"
                    | "ANTI"
                    | "ANY"
                    | "ALL"
                    | "ASOF"
                    | "GLOBAL"
            ),
            "INNER" | "CROSS" | "FULL" | "RIGHT" | "LEFT" => matches!(
                next.as_str(),
                "JOIN" | "OUTER" | "APPLY" | "SEMI" | "ANTI" | "ANY" | "ALL" | "ASOF"
            ),
            "NATURAL" => true,
            "FROM" => prev != "DELETE" && !self.level().clause.starts_with("DELETE_HEAD"),
            "INTO" => false,
            "UPDATE" => !matches!(prev.as_str(), "KEY" | "DO" | "FOR" | "THEN") && prev != "(",
            "SET" => {
                !matches!(prev.as_str(), "CHARACTER" | "CHARSET" | "UPDATE")
                    || self.level().clause == "UPDATE"
            }
            "SELECT" => true,
            "ON" => matches!(next.as_str(), "CONFLICT" | "DUPLICATE"),
            "USING" => self.level().clause == "DELETE",
            "WITH" => !self.level().started,
            "VALUES" => self.level().clause == "INSERT",
            "WHEN" => self.level().clause == "MERGE",
            "OUTPUT" => true,
            "LIMIT" | "OFFSET" | "FETCH" => true,
            "ORDER" | "GROUP" => next == "BY",
            "FORMAT" | "SETTINGS" => self.d == SqlDialect::ClickHouse,
            _ => true,
        }
    }

    fn emit_token(&mut self, i: usize) {
        let text = cased(self.src, &self.toks, i, self.d, self.s);
        self.out.push_str(&text);
    }

    /// Space before token `i` on the same line.
    fn spacing(&mut self, i: usize) {
        if self.at_line_start() {
            return;
        }
        let Some(p) = (i > 0).then(|| i - 1) else {
            return;
        };
        let (pt, t) = (self.toks[p], self.toks[i]);
        let ptxt = pt.text(self.src);
        let ttxt = t.text(self.src);
        let op = |tok: Token, s: &str| tok.kind == Kind::Op && !matches!(s, "::" | "." | "*");
        let space = match (pt.kind, t.kind) {
            (_, Kind::Comment) => true,
            (Kind::Comment, _) => true,
            (_, Kind::Comma) | (_, Kind::Semi) | (Kind::Dot, _) | (_, Kind::Dot) => false,
            (Kind::Comma, _) => self.s.space_after_comma,
            (Kind::Open, Kind::Close) => false,
            (Kind::Open, _) => self.s.space_within_parentheses,
            (_, Kind::Close) => self.s.space_within_parentheses,
            (_, Kind::Open) => {
                // Calls stick to their name; lists after keywords and table names do not.
                let call = matches!(pt.kind, Kind::Word | Kind::Quoted)
                    && !is_keyword(self.src, &self.toks, p, self.d)
                    && !self.prev(p).is_some_and(|pp| {
                        [
                            "INTO",
                            "TABLE",
                            "VIEW",
                            "EXISTS",
                            "INDEX",
                            "ON",
                            "AS",
                            "USING",
                            "IN",
                            "REFERENCES",
                        ]
                        .iter()
                        .any(|k| self.kw(pp, k))
                            || self.toks[pp].kind == Kind::Close
                    });
                let kw_call = pt.kind == Kind::Word
                    && is_keyword(self.src, &self.toks, p, self.d)
                    && matches!(
                        ptxt.to_ascii_uppercase().as_str(),
                        "COUNT"
                            | "CAST"
                            | "COALESCE"
                            | "REPLACE"
                            | "LEFT"
                            | "RIGHT"
                            | "SUBSTRING"
                            | "TRIM"
                            | "EXTRACT"
                            | "CHAR"
                            | "VARCHAR"
                            | "DECIMAL"
                            | "NUMERIC"
                            | "NUMBER"
                            | "VARCHAR2"
                            | "NVARCHAR"
                            | "NCHAR"
                            | "FLOAT"
                            | "DOUBLE"
                            | "TIMESTAMP"
                            | "TIME"
                            | "BINARY"
                            | "VARBINARY"
                            | "INT"
                            | "BIT"
                            | "IF"
                            | "ROW"
                            | "ARRAY"
                    );
                !(call || kw_call || matches!(pt.kind, Kind::Param | Kind::Variable))
            }
            (Kind::Op, _) if ptxt == "::" => false,
            (_, Kind::Op) if ttxt == "::" => false,
            (Kind::Op, _) if matches!(ptxt, "-" | "+" | "~") && self.unary(p) => false,
            (Kind::Op, _) if ptxt == "*" => self.star_space(p, true),
            (_, Kind::Op) if ttxt == "*" => self.star_space(i, false),
            (Kind::Op, _) if op(pt, ptxt) => self.s.space_around_operators,
            (_, Kind::Op) if op(t, ttxt) => self.s.space_around_operators,
            _ => true,
        };
        if space {
            self.out.push(' ');
        }
    }

    /// `-x` after an operator, `(`, `,` or keyword.
    fn unary(&self, i: usize) -> bool {
        match self.prev(i) {
            None => true,
            Some(p) => {
                let pt = self.toks[p];
                matches!(pt.kind, Kind::Op | Kind::Open | Kind::Comma)
                    || (pt.kind == Kind::Word
                        && is_keyword(self.src, &self.toks, p, self.d)
                        && !self.kw(p, "END"))
            }
        }
    }

    /// `*` as "all columns" (`SELECT *`, `t.*`, `count(*)`) or multiplication.
    fn star_space(&self, star: usize, after: bool) -> bool {
        let p = self.prev(star).map(|p| self.toks[p]);
        let n = self.next(star).map(|n| self.toks[n]);
        let all = p.is_some_and(|p| {
            matches!(p.kind, Kind::Dot | Kind::Open | Kind::Comma)
                || (p.kind == Kind::Word && is_keyword(self.src, &self.toks, star - 1, self.d))
        }) || p.is_none();
        if all {
            // `count(*)`, `t.*`: no spaces; `SELECT *` keeps the keyword's space.
            if after {
                !matches!(n.map(|n| n.kind), Some(Kind::Close | Kind::Comma))
            } else {
                !matches!(p.map(|p| p.kind), Some(Kind::Dot | Kind::Open))
            }
        } else {
            self.s.space_around_operators
        }
    }

    fn print(&mut self) {
        let n = self.toks.len();
        let mut i = 0;
        while i < n {
            let t = self.toks[i];
            let up = if t.kind == Kind::Word {
                self.up(i)
            } else {
                String::new()
            };
            // Line breaks before the token.
            let mut broke = false;
            if self.pending_nl {
                self.pending_nl = false;
                let cols = if self.level().started {
                    self.item_indent()
                } else {
                    self.level().base
                };
                let cols = if self.starts_clause(i) && self.s.clause_newline {
                    self.level().base + self.river(&up)
                } else {
                    cols
                };
                self.newline(cols);
                broke = true;
            }
            if t.kind != Kind::Comment && self.starts_clause(i) {
                let base = self.level().base;
                let started = self.level().started;
                if self.s.clause_newline && started && !broke {
                    self.newline(base + self.river(&up));
                } else if started && !broke {
                    self.space();
                } else if !started && self.s.align_right && !broke && self.at_line_start() {
                    let pad = self.river(&up);
                    self.out.push_str(&" ".repeat(pad));
                }
                let clause = match up.as_str() {
                    "LEFT" | "RIGHT" | "FULL" | "INNER" | "CROSS" | "NATURAL" | "JOIN"
                    | "STRAIGHT_JOIN" => "JOIN".to_string(),
                    _ => up.clone(),
                };
                let lvl = self.level_mut();
                lvl.clause = clause;
                lvl.between = false;
                lvl.started = true;
                self.emit_token(i);
                i = self.after_clause_word(i);
                continue;
            }
            match t.kind {
                Kind::Comma if self.level().frame == Frame::List => {
                    self.trim_end();
                    if self.s.comma_first {
                        let cols = self.item_indent();
                        self.newline(cols.saturating_sub(2));
                        self.out.push(',');
                    } else {
                        self.out.push(',');
                        let cols = self.item_indent();
                        self.newline(cols);
                    }
                }
                Kind::Comma
                    if self.level().frame == Frame::Query
                        && self.s.items_newline
                        && LIST_CLAUSES.contains(&self.level().clause.as_str()) =>
                {
                    let cols = self.item_indent();
                    if self.s.comma_first {
                        self.newline(cols.saturating_sub(2));
                        self.out.push(',');
                    } else {
                        self.out.push(',');
                        self.newline(cols);
                    }
                }
                Kind::Open => {
                    self.spacing(i);
                    self.out.push('(');
                    let next_up = self.next(i).map(|k| self.up(k)).unwrap_or_default();
                    let line = self.line_indent();
                    let sub =
                        matches!(next_up.as_str(), "SELECT" | "WITH") && self.s.clause_newline;
                    let list = !sub && self.is_column_list(i);
                    let (frame, base) = if sub {
                        (Frame::Query, line + self.s.indent)
                    } else if list {
                        (Frame::List, line)
                    } else {
                        (Frame::Inline, self.item_indent())
                    };
                    self.stack.push(Level {
                        frame,
                        base,
                        clause: String::new(),
                        between: false,
                        started: false,
                    });
                    if sub {
                        self.newline(base);
                    } else if list {
                        let cols = self.item_indent();
                        self.newline(cols);
                    }
                }
                Kind::Close => {
                    let lvl = if self.stack.len() > 1 {
                        self.stack.pop()
                    } else {
                        None
                    };
                    match lvl.map(|l| (l.frame, l.base)) {
                        Some((Frame::Query, base)) => {
                            self.newline(base.saturating_sub(self.s.indent))
                        }
                        Some((Frame::List, base)) => self.newline(base),
                        _ => self.spacing(i),
                    }
                    self.out.push(')');
                }
                Kind::Comment => {
                    let text = t.text(self.src);
                    let own_line = i > 0 && self.src[self.toks[i - 1].end..t.start].contains('\n');
                    if own_line && self.level().started {
                        let cols = self.item_indent();
                        self.newline(cols);
                    } else if own_line && !self.out.is_empty() {
                        let cols = self.level().base;
                        self.newline(cols);
                    } else {
                        self.space();
                    }
                    self.out.push_str(text.trim_end());
                    let line_comment = !text.starts_with("/*");
                    if line_comment || (own_line && text.contains('\n')) {
                        self.pending_nl = true;
                    }
                }
                Kind::Word if (up == "AND" || up == "OR") && self.level().frame == Frame::Query => {
                    let between = self.level().between;
                    if up == "AND" && between {
                        self.level_mut().between = false;
                        self.spacing(i);
                        self.emit_token(i);
                    } else if self.s.and_or_newline
                        && matches!(
                            self.level().clause.as_str(),
                            "WHERE" | "HAVING" | "JOIN" | "QUALIFY" | "PREWHERE"
                        )
                    {
                        let cols = if self.s.align_right {
                            self.level().base + self.river(&up)
                        } else {
                            self.item_indent()
                        };
                        self.newline(cols);
                        self.emit_token(i);
                    } else {
                        self.spacing(i);
                        self.emit_token(i);
                    }
                }
                _ => {
                    if t.kind == Kind::Word && up == "BETWEEN" {
                        self.level_mut().between = true;
                    }
                    self.spacing(i);
                    self.emit_token(i);
                    self.level_mut().started = true;
                }
            }
            i += 1;
        }
    }

    /// Emit the rest of a clause's head (`GROUP BY`, `LEFT OUTER JOIN`, `SELECT DISTINCT`,
    /// `INSERT INTO`, `DELETE FROM`) and the line break before list items.
    fn after_clause_word(&mut self, i: usize) -> usize {
        let up = self.up(i);
        let mut j = i + 1;
        let take = |p: &mut Self, j: &mut usize| {
            p.spacing(*j);
            p.emit_token(*j);
            *j += 1;
        };
        match up.as_str() {
            "GROUP" | "ORDER" if self.kw(j, "BY") => take(self, &mut j),
            "LEFT" | "RIGHT" | "FULL" | "INNER" | "CROSS" | "NATURAL" => {
                while j < self.toks.len()
                    && [
                        "OUTER", "INNER", "LEFT", "RIGHT", "FULL", "JOIN", "APPLY", "SEMI", "ANTI",
                        "ANY", "ALL", "ASOF",
                    ]
                    .iter()
                    .any(|k| self.kw(j, k))
                {
                    take(self, &mut j);
                }
            }
            "UNION" | "EXCEPT" | "INTERSECT" | "MINUS" => {
                while self.kw(j, "ALL") || self.kw(j, "DISTINCT") {
                    take(self, &mut j);
                }
            }
            "SELECT" => {
                while ["DISTINCT", "ALL", "TOP"].iter().any(|k| self.kw(j, k)) {
                    let top = self.kw(j, "TOP");
                    take(self, &mut j);
                    if top && j < self.toks.len() {
                        take(self, &mut j);
                    }
                }
            }
            "INSERT" | "REPLACE" => {
                while ["INTO", "IGNORE", "OR"].iter().any(|k| self.kw(j, k)) {
                    take(self, &mut j);
                }
                self.level_mut().clause = "INSERT".into();
            }
            "DELETE" => {
                if self.kw(j, "FROM") {
                    take(self, &mut j);
                }
            }
            "MERGE" => {
                if self.kw(j, "INTO") {
                    take(self, &mut j);
                }
            }
            "FETCH" => {}
            _ => {}
        }
        let clause = self.level().clause.clone();
        if self.s.items_newline
            && LIST_CLAUSES.contains(&clause.as_str())
            && j < self.toks.len()
            && !matches!(self.toks[j].kind, Kind::Semi)
        {
            let cols = self.item_indent();
            self.newline(cols);
        }
        j
    }

    /// `(` of a CREATE TABLE column list.
    fn is_column_list(&self, open: usize) -> bool {
        let Some(mut k) = self.prev(open) else {
            return false;
        };
        if !self.toks[k].is_name() {
            return false;
        }
        while k >= 2 && self.toks[k - 1].kind == Kind::Dot && self.toks[k - 2].is_name() {
            k -= 2;
        }
        while k > 0 && ["EXISTS", "NOT", "IF"].iter().any(|w| self.kw(k - 1, w)) {
            k -= 1;
        }
        k > 0 && self.kw(k - 1, "TABLE") && (0..k - 1).any(|c| self.kw(c, "CREATE"))
    }
}

/// Whether a statement is a routine or procedural block (kept as written).
fn procedural(src: &str, toks: &[Token]) -> bool {
    let words: Vec<String> = toks
        .iter()
        .filter(|t| t.kind == Kind::Word)
        .take(6)
        .map(|t| t.text(src).to_ascii_uppercase())
        .collect();
    match words.first().map(String::as_str) {
        Some(
            "BEGIN" | "DECLARE" | "DO" | "DELIMITER" | "IF" | "WHILE" | "EXEC" | "EXECUTE" | "CALL",
        ) => true,
        Some("CREATE") => words.iter().any(|w| {
            matches!(
                w.as_str(),
                "FUNCTION" | "PROCEDURE" | "TRIGGER" | "PACKAGE" | "TYPE" | "RULE" | "EVENT"
            )
        }),
        _ => false,
    }
}

/// Format one statement (without its terminator).
fn format_statement(text: &str, d: SqlDialect, s: &SqlStyle) -> String {
    let toks = lex(text, d);
    if procedural(text, &toks) {
        return change_case(text, d, s);
    }
    let mut p = Printer {
        src: text,
        d,
        s,
        toks,
        out: String::new(),
        stack: vec![Level {
            frame: Frame::Query,
            base: 0,
            clause: String::new(),
            between: false,
            started: false,
        }],
        pending_nl: false,
    };
    p.print();
    p.trim_end();
    p.out
}

/// Format a script: every statement, with its terminator (`;`, `GO`, `/`), comments after
/// a terminator on its line, and at most `blank_lines_between` blank lines between statements.
pub fn format(text: &str, d: SqlDialect, s: &SqlStyle) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }
    let stmts = forge_sql::split_statements(text, d.db());
    let mut out = String::new();
    let mut prev_end = 0;
    for (n, st) in stmts.iter().enumerate() {
        let mut start = st.start;
        if n > 0 {
            let separator = write_terminator(&mut out, &text[prev_end..st.start], d, s);
            // Comments on the terminator's line stay there.
            let mut line_end = prev_end;
            for t in lex(&st.text, d) {
                let abs = st.start + t.start;
                if t.kind != Kind::Comment || text[line_end..abs].contains('\n') {
                    break;
                }
                out.push(' ');
                out.push_str(t.text(&st.text).trim_end());
                line_end = st.start + t.end;
                start = st.start + t.end;
            }
            let rest = text[start..st.end].trim_start();
            let skipped = st.end - start - rest.len();
            let newlines = text[line_end..start + skipped]
                .matches('\n')
                .count()
                .saturating_sub(separator);
            out.push('\n');
            for _ in 0..newlines.saturating_sub(1).min(s.blank_lines_between) {
                out.push('\n');
            }
            start += skipped;
        }
        out.push_str(&format_statement(&text[start..st.end], d, s));
        prev_end = st.end;
    }
    if stmts.is_empty() {
        out.push_str(text.trim());
    } else {
        // The last terminator and trailing comments.
        let tail = &text[prev_end..];
        write_terminator(&mut out, tail, d, s);
        let mut at = 0;
        for t in lex(tail, d) {
            if t.kind == Kind::Comment {
                out.push(if tail[at..t.start].contains('\n') {
                    '\n'
                } else {
                    ' '
                });
                out.push_str(t.text(tail).trim_end());
            }
            at = t.end;
        }
    }
    let mut out = out.trim_end().to_string();
    out.push('\n');
    out
}

/// The terminator in the text after a statement: `;`, a `GO` batch line or Oracle's `/`;
/// returns the lines a separator took.
fn write_terminator(out: &mut String, gap: &str, d: SqlDialect, s: &SqlStyle) -> usize {
    let mut semi = false;
    for t in lex(gap, d) {
        let txt = t.text(gap);
        match t.kind {
            Kind::Semi if !semi => {
                out.push(';');
                semi = true;
            }
            Kind::Word if txt.eq_ignore_ascii_case("GO") && d == SqlDialect::SqlServer => {
                out.push('\n');
                out.push_str(&s.keyword_case.apply("GO"));
                return 1;
            }
            Kind::Op if txt == "/" && d == SqlDialect::Oracle => {
                out.push_str("\n/");
                return 1;
            }
            _ => {}
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(sql: &str) -> String {
        format(sql, SqlDialect::Postgres, &SqlStyle::default())
    }

    #[test]
    fn clauses_and_case() {
        assert_eq!(
            fmt("select a, b from t where x = 1 and y in (1,2) order by a"),
            "SELECT a, b\nFROM t\nWHERE x = 1\n    AND y IN (1, 2)\nORDER BY a\n"
        );
    }

    #[test]
    fn keeps_terminators_and_comments() {
        assert_eq!(
            fmt("select 1; -- one\n\n\n\nselect 2;"),
            "SELECT 1; -- one\n\nSELECT 2;\n"
        );
    }

    #[test]
    fn case_only_pass() {
        let s = SqlStyle {
            keyword_case: Case::Lower,
            ..SqlStyle::default()
        };
        assert_eq!(
            change_case(
                "SELECT Name FROM t WHERE 'SELECT' = x",
                SqlDialect::MySql,
                &s
            ),
            "select Name from t where 'SELECT' = x"
        );
    }
}
