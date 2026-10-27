//! Resolve: map the names of a SQL script to the data source model and to the script's own
//! definitions. Every statement is tokenized and walked with scopes, error tolerant (a
//! half-typed query still resolves what it can):
//!
//! - `FROM` / `JOIN` items: tables and views (`schema.table`), CTEs, subqueries and table
//!   functions with their aliases and column lists;
//! - `WITH [RECURSIVE]` CTEs with explicit or derived columns;
//! - columns, qualified (`alias.col`, `schema.table.col`) or not (searched in the sources of
//!   the query, then of the enclosing queries, so correlated subqueries work), select-list
//!   aliases in `ORDER BY` / `GROUP BY` / `HAVING`;
//! - `INSERT` column lists, `UPDATE … SET`, `DELETE … USING`, `ON CONFLICT` / `ON DUPLICATE KEY`,
//!   `RETURNING`, `CREATE VIEW … AS`, `ALTER` / `DROP` / `TRUNCATE` / `CREATE INDEX` targets;
//! - routines of the model (navigation only: built-in functions are not in the model).
//!
//! Names are compared case-insensitively. Schemas that were not introspected and sources
//! with unknown columns (table functions, `SELECT *` of an unknown table) resolve to
//! "unknown", which is never reported. All offsets are bytes of the whole script.

use serde::Serialize;

use crate::catalog::{Catalog, Lookup};
use crate::lang::{is_reserved, SqlDialect};
use crate::lex::{lex, Kind, Token};

/// What a name resolved to.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Schema {
        schema: String,
    },
    #[serde(rename_all = "camelCase")]
    Table {
        schema: String,
        table: String,
        view: bool,
    },
    #[serde(rename_all = "camelCase")]
    Column {
        schema: String,
        table: String,
        column: String,
        data_type: String,
    },
    Routine {
        schema: String,
        name: String,
    },
    /// A definition in the script: table alias, CTE or select-list alias.
    Local {
        def: usize,
    },
    /// A column of a CTE or subquery.
    LocalColumn {
        def: usize,
        column: String,
    },
}

impl Target {
    /// Whether two targets are the same object (names compared case-insensitively).
    pub fn same(&self, other: &Target) -> bool {
        let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        match (self, other) {
            (Target::Schema { schema: a }, Target::Schema { schema: b }) => eq(a, b),
            (
                Target::Table {
                    schema: s1,
                    table: t1,
                    ..
                },
                Target::Table {
                    schema: s2,
                    table: t2,
                    ..
                },
            ) => eq(s1, s2) && eq(t1, t2),
            (
                Target::Column {
                    schema: s1,
                    table: t1,
                    column: c1,
                    ..
                },
                Target::Column {
                    schema: s2,
                    table: t2,
                    column: c2,
                    ..
                },
            ) => eq(s1, s2) && eq(t1, t2) && eq(c1, c2),
            (
                Target::Routine {
                    schema: s1,
                    name: n1,
                },
                Target::Routine {
                    schema: s2,
                    name: n2,
                },
            ) => eq(s1, s2) && eq(n1, n2),
            (Target::Local { def: a }, Target::Local { def: b }) => a == b,
            (
                Target::LocalColumn { def: a, column: c1 },
                Target::LocalColumn { def: b, column: c2 },
            ) => a == b && eq(c1, c2),
            _ => false,
        }
    }
}

/// The role of a name in the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Schema,
    Table,
    Column,
    /// The `u` of `u.name`: an alias, table or CTE name used as a qualifier.
    Qualifier,
    Function,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Resolved,
    /// Known not to exist.
    Unresolved,
    /// Not knowable (no model, schema not loaded, source with unknown columns).
    Unknown,
    /// A column found in several sources.
    Ambiguous,
}

/// One name usage.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ref {
    pub from: usize,
    pub to: usize,
    pub name: String,
    pub role: Role,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Ambiguous columns: the qualifiers of the sources that have it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DefKind {
    /// `FROM users u`
    TableAlias,
    /// `FROM (SELECT …) s`
    SubqueryAlias,
    Cte,
    /// `SELECT count(*) AS total`
    SelectAlias,
}

/// A name defined by the script.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Def {
    pub id: usize,
    pub from: usize,
    pub to: usize,
    pub name: String,
    pub kind: DefKind,
    /// What to delete to remove the definition (the alias with its `AS`, a whole CTE).
    pub remove: (usize, usize),
    pub used: bool,
    /// The aliased table or the expression's column name (redundant alias check).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliased: Option<String>,
}

/// A problem found while walking (not about names).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: &'static str,
    pub from: usize,
    pub to: usize,
    pub message: String,
    /// Where an edit for its fix goes and what it inserts or removes.
    #[serde(skip)]
    pub fix: Option<FindingFix>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FindingFix {
    /// Delete `from..to`.
    Remove(usize, usize),
    /// Insert at `at`; select `text[sel]` afterwards.
    Insert {
        at: usize,
        text: String,
        select: (usize, usize),
    },
    /// Wrap `from..to` in quotes.
    Quote(usize, usize),
}

/// One statement of the script.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementInfo {
    pub from: usize,
    pub to: usize,
    /// Leading keyword (`SELECT`, `UPDATE`, …).
    pub kind: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub refs: Vec<Ref>,
    pub defs: Vec<Def>,
    pub findings: Vec<Finding>,
    pub statements: Vec<StatementInfo>,
    /// Whether names were resolved against a data source model.
    pub has_model: bool,
}

impl Analysis {
    /// The reference or definition at a byte offset (touching its start or end).
    pub fn ref_at(&self, at: usize) -> Option<&Ref> {
        self.refs.iter().find(|r| r.from <= at && at <= r.to)
    }

    pub fn def_at(&self, at: usize) -> Option<&Def> {
        self.defs.iter().find(|d| d.from <= at && at <= d.to)
    }
}

// ---- scopes ------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Col {
    name: String,
    data_type: Option<String>,
}

#[derive(Debug, Clone)]
enum Cols {
    Known(Vec<Col>),
    Unknown,
}

impl Cols {
    fn names(names: &[String]) -> Self {
        Cols::Known(
            names
                .iter()
                .map(|n| Col {
                    name: n.clone(),
                    data_type: None,
                })
                .collect(),
        )
    }
}

#[derive(Debug, Clone)]
enum Origin {
    Table { schema: String, table: String },
    Local(usize),
    Other,
}

#[derive(Debug, Clone)]
struct Source {
    /// The name that qualifies its columns: the alias, else the table or CTE name.
    name: String,
    alias: Option<usize>,
    /// Schema of a table source (for `schema.table.col`).
    schema: Option<String>,
    origin: Origin,
    cols: Cols,
}

#[derive(Debug, Default)]
struct Scope {
    parent: Option<usize>,
    sources: Vec<Source>,
    ctes: Vec<(String, usize, Cols)>,
    aliases: Vec<(String, usize)>,
    /// `USING (id)` columns and NATURAL joins: shared columns are not ambiguous.
    using: Vec<String>,
    natural: bool,
}

/// Where a name stands in an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Plain,
    /// `ORDER BY`: select-list aliases first.
    AliasFirst,
    /// `GROUP BY` / `HAVING`: columns first, then aliases.
    AliasLast,
}

/// Clauses of a `SELECT` after its list (`FROM` first).
const SELECT_CLAUSES: &[&str] = &[
    "FROM", "INTO", "WHERE", "PREWHERE", "GROUP", "HAVING", "WINDOW", "QUALIFY", "ORDER", "LIMIT",
    "OFFSET", "FETCH", "FOR", "SETTINGS", "FORMAT", "OPTION",
];

const JOIN_WORDS: &[&str] = &[
    "JOIN",
    "LEFT",
    "RIGHT",
    "FULL",
    "INNER",
    "OUTER",
    "CROSS",
    "NATURAL",
    "STRAIGHT_JOIN",
    "LATERAL",
    "APPLY",
    "ANY",
    "ALL",
    "ASOF",
    "SEMI",
    "ANTI",
    "GLOBAL",
    "ARRAY",
    "ONLY",
];

/// Words after a table that are not its alias (hints, sampling, clauses).
const NOT_ALIAS: &[&str] = &[
    "ON",
    "USING",
    "WHERE",
    "GROUP",
    "HAVING",
    "ORDER",
    "LIMIT",
    "OFFSET",
    "FETCH",
    "WINDOW",
    "UNION",
    "EXCEPT",
    "INTERSECT",
    "MINUS",
    "SET",
    "RETURNING",
    "WITH",
    "TABLESAMPLE",
    "FINAL",
    "SAMPLE",
    "PREWHERE",
    "PARTITION",
    "FORCE",
    "USE",
    "IGNORE",
    "FOR",
    "INTO",
    "VALUES",
    "SELECT",
    "DEFAULT",
    "OUTPUT",
    "WHEN",
    "QUALIFY",
    "SETTINGS",
    "FORMAT",
    "OPTION",
    "DO",
    "CONNECT",
    "START",
];

/// Qualifiers of trigger and upsert pseudo rows.
const PSEUDO_ROWS: &[&str] = &["excluded", "new", "old", "inserted", "deleted"];

/// Words that are not reported when they do not resolve: date parts, niladic functions,
/// pseudo columns.
const SOFT_WORDS: &[&str] = &[
    "YEAR",
    "MONTH",
    "DAY",
    "HOUR",
    "MINUTE",
    "SECOND",
    "EPOCH",
    "DOW",
    "DOY",
    "WEEK",
    "QUARTER",
    "MILLISECONDS",
    "MICROSECONDS",
    "ROWID",
    "ROWNUM",
    "OID",
    "CTID",
    "SYSDATE",
    "SYSTIMESTAMP",
    "LEVEL",
    "DUAL",
    "LEADING",
    "TRAILING",
    "BOTH",
    "PRECEDING",
    "FOLLOWING",
    "UNBOUNDED",
    "CURRENT",
    "ROW",
    "RANGE",
    "GROUPS",
    "SEPARATOR",
    "UTC",
    "ZONE",
    "LOCAL",
    "TIME",
    "DATE",
    "TIMESTAMP",
];

struct Walker<'a> {
    src: &'a str,
    d: SqlDialect,
    cat: Option<Catalog<'a>>,
    t: Vec<Token>,
    pair: Vec<usize>,
    scopes: Vec<Scope>,
    out: Analysis,
}

/// Analyze a script: references, definitions and findings of every statement. `model` is
/// the data source's model (with `default_schema`, e.g. the console's schema).
pub fn analyze(
    text: &str,
    d: SqlDialect,
    model: Option<&forge_dbmodel::DbModel>,
    default_schema: Option<&str>,
) -> Analysis {
    let mut out = Analysis {
        has_model: model.is_some(),
        ..Analysis::default()
    };
    for st in forge_sql::split_statements(text, d.db()) {
        let toks: Vec<Token> = lex(&st.text, d)
            .into_iter()
            .filter(Token::is_significant)
            .map(|t| Token {
                kind: t.kind,
                start: t.start + st.start,
                end: t.end + st.start,
            })
            .collect();
        let mut toks = toks;
        while toks.last().is_some_and(|t| t.kind == Kind::Semi) {
            toks.pop();
        }
        if toks.is_empty() {
            continue;
        }
        let pair = pairs(&toks);
        let mut w = Walker {
            src: text,
            d,
            cat: model.map(|m| Catalog::new(m, default_schema)),
            t: toks,
            pair,
            scopes: Vec::new(),
            out: std::mem::take(&mut out),
        };
        let kind = w.t[0].text(text).to_ascii_uppercase();
        let end = w.t.len();
        w.out.statements.push(StatementInfo {
            from: st.start,
            to: st.end,
            kind: kind.clone(),
        });
        w.statement(0, end);
        out = w.out;
    }
    out
}

/// Index of the matching `)` of every `(` (the end of the tokens when unbalanced).
fn pairs(t: &[Token]) -> Vec<usize> {
    let mut pair = vec![usize::MAX; t.len()];
    let mut stack = Vec::new();
    for (i, tok) in t.iter().enumerate() {
        match tok.kind {
            Kind::Open => stack.push(i),
            Kind::Close => {
                if let Some(o) = stack.pop() {
                    pair[o] = i;
                }
            }
            _ => {}
        }
    }
    for o in stack {
        pair[o] = t.len();
    }
    pair
}

impl<'a> Walker<'a> {
    fn kw(&self, i: usize, k: &str) -> bool {
        self.t.get(i).is_some_and(|t| t.is_kw(self.src, k))
    }

    fn kw_any(&self, i: usize, ks: &[&str]) -> bool {
        self.t
            .get(i)
            .is_some_and(|t| t.kind == Kind::Word && ks.iter().any(|k| t.is_kw(self.src, k)))
    }

    fn text(&self, i: usize) -> &'a str {
        self.t[i].text(self.src)
    }

    fn name(&self, i: usize) -> String {
        self.t[i].name(self.src)
    }

    /// The index after a parenthesized group starting at `i` (or `i + 1`).
    fn skip(&self, i: usize) -> usize {
        if self.t[i].kind == Kind::Open {
            (self.pair[i] + 1).min(self.t.len())
        } else {
            i + 1
        }
    }

    /// First index in `from..to` at depth 0 whose word is one of `ks`.
    fn find(&self, from: usize, to: usize, ks: &[&str]) -> Option<usize> {
        let mut i = from;
        while i < to {
            if self.kw_any(i, ks) {
                return Some(i);
            }
            i = self.skip(i);
        }
        None
    }

    /// Depth-0 comma separated ranges of `from..to`.
    fn split_commas(&self, from: usize, to: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = from;
        let mut i = from;
        while i < to {
            if self.t[i].kind == Kind::Comma {
                out.push((start, i));
                start = i + 1;
            }
            i = self.skip(i).min(to);
        }
        if start < to {
            out.push((start, to));
        }
        out
    }

    fn new_scope(&mut self, parent: Option<usize>) -> usize {
        self.scopes.push(Scope {
            parent,
            ..Scope::default()
        });
        self.scopes.len() - 1
    }

    fn add_def(&mut self, i: usize, kind: DefKind, remove: (usize, usize)) -> usize {
        let id = self.out.defs.len();
        let tok = self.t[i];
        self.out.defs.push(Def {
            id,
            from: tok.start,
            to: tok.end,
            name: self.name(i),
            kind,
            remove,
            used: false,
            aliased: None,
        });
        id
    }

    fn push_ref(&mut self, i: usize, role: Role, state: State, target: Option<Target>) {
        if let Some(Target::Local { def }) = &target {
            if let Some(d) = self.out.defs.get_mut(*def) {
                d.used = true;
            }
        }
        if let Some(Target::LocalColumn { def, .. }) = &target {
            if let Some(d) = self.out.defs.get_mut(*def) {
                if d.kind == DefKind::Cte {
                    d.used = true;
                }
            }
        }
        let tok = self.t[i];
        self.out.refs.push(Ref {
            from: tok.start,
            to: tok.end,
            name: self.name(i),
            role,
            state,
            target,
            candidates: Vec::new(),
        });
    }

    fn finding(
        &mut self,
        id: &'static str,
        from: usize,
        to: usize,
        message: String,
        fix: Option<FindingFix>,
    ) {
        self.out.findings.push(Finding {
            id,
            from,
            to,
            message,
            fix,
        });
    }

    // ---- statements ----------------------------------------------------------------------

    fn statement(&mut self, i: usize, end: usize) {
        if i >= end {
            return;
        }
        let first = self.text(i).to_ascii_uppercase();
        match first.as_str() {
            "SELECT" | "WITH" | "VALUES" | "TABLE" => {
                self.query(i, end, None);
            }
            "(" => {
                self.query(i, end, None);
            }
            "INSERT" | "REPLACE" | "UPSERT" => self.insert(i, end),
            "UPDATE" => self.update(i, end),
            "DELETE" => self.delete(i, end),
            "EXPLAIN" | "DESCRIBE" | "DESC" => {
                let mut j = i + 1;
                while j < end
                    && self.kw_any(
                        j,
                        &[
                            "ANALYZE", "VERBOSE", "QUERY", "PLAN", "FOR", "EXTENDED", "FORMAT",
                        ],
                    )
                {
                    j += 1;
                }
                if j < end
                    && self.kw_any(
                        j,
                        &["SELECT", "WITH", "INSERT", "UPDATE", "DELETE", "REPLACE"],
                    )
                {
                    self.statement(j, end);
                } else if j < end && self.t[j].is_name() {
                    self.table_list(j, end, false);
                }
            }
            "CREATE" => self.create(i, end),
            "ALTER" => self.alter(i, end),
            "DROP" => {
                if let Some(k) = self.find(i + 1, end, &["TABLE", "VIEW"]) {
                    let mut j = k + 1;
                    let mut if_exists = false;
                    if self.kw(j, "IF") && self.kw(j + 1, "EXISTS") {
                        j += 2;
                        if_exists = true;
                    }
                    self.table_list(j, end, if_exists);
                }
            }
            "TRUNCATE" => {
                let mut j = i + 1;
                if self.kw(j, "TABLE") {
                    j += 1;
                }
                if self.kw(j, "ONLY") {
                    j += 1;
                }
                self.table_list(j, end, false);
            }
            _ => {}
        }
    }

    /// `t1, schema.t2 …` (DROP, TRUNCATE): table references up to the first non-name.
    fn table_list(&mut self, from: usize, end: usize, lenient: bool) {
        let scope = self.new_scope(None);
        for (a, b) in self.split_commas(from, end) {
            let (parts, next) = self.chain(a, b);
            if !parts.is_empty() && next <= b {
                self.table_ref(&parts, scope, lenient);
            }
        }
    }

    fn create(&mut self, i: usize, end: usize) {
        let Some(k) = self.find(i + 1, end, &["TABLE", "VIEW", "INDEX"]) else {
            return;
        };
        if self
            .find(i + 1, k, &["FUNCTION", "PROCEDURE", "TRIGGER", "PACKAGE"])
            .is_some()
        {
            return;
        }
        if self.kw(k, "INDEX") {
            // CREATE [UNIQUE] INDEX [name] ON t (cols)
            if let Some(on) = self.find(k + 1, end, &["ON"]) {
                let mut j = on + 1;
                if self.kw(j, "ONLY") {
                    j += 1;
                }
                let (parts, next) = self.chain(j, end);
                if parts.is_empty() {
                    return;
                }
                let scope = self.new_scope(None);
                if let Some(src) = self.table_ref(&parts, scope, false) {
                    self.scopes[scope].sources.push(src);
                }
                let open = (next..end).find(|&x| self.t[x].kind == Kind::Open);
                if let Some(o) = open {
                    let close = self.pair[o].min(end);
                    for (a, b) in self.split_commas(o + 1, close) {
                        self.walk(a, b, scope, Ctx::Plain);
                    }
                }
            }
            return;
        }
        // CREATE VIEW v AS query / CREATE TABLE t AS query
        if let Some(as_) = self.find(k + 1, end, &["AS"]) {
            let q = as_ + 1;
            if q < end && self.kw_any(q, &["SELECT", "WITH", "VALUES"])
                || (q < end && self.t[q].kind == Kind::Open)
            {
                self.query(q, end, None);
            }
        }
    }

    fn alter(&mut self, i: usize, end: usize) {
        if !self.kw(i + 1, "TABLE") {
            return;
        }
        let mut j = i + 2;
        if self.kw(j, "IF") && self.kw(j + 1, "EXISTS") {
            j += 2;
        }
        if self.kw(j, "ONLY") {
            j += 1;
        }
        let (parts, next) = self.chain(j, end);
        if parts.is_empty() {
            return;
        }
        let scope = self.new_scope(None);
        if let Some(src) = self.table_ref(&parts, scope, false) {
            self.scopes[scope].sources.push(src);
        }
        // DROP COLUMN c, RENAME COLUMN c TO d, ALTER COLUMN c …
        let mut k = next;
        while k < end {
            if self.kw_any(k, &["DROP", "RENAME", "ALTER", "MODIFY", "CHANGE"]) {
                let mut c = k + 1;
                if self.kw(c, "COLUMN") {
                    c += 1;
                } else if !self.kw_any(k, &["MODIFY", "CHANGE"]) {
                    k += 1;
                    continue;
                }
                if self.kw(c, "IF") && self.kw(c + 1, "EXISTS") {
                    c += 2;
                }
                if c < end && self.t[c].is_name() && !is_reserved(self.text(c)) {
                    self.column(c, scope, Ctx::Plain, &[c]);
                }
            }
            k = self.skip(k);
        }
    }

    fn insert(&mut self, i: usize, end: usize) {
        let mut j = i + 1;
        while j < end
            && self.kw_any(
                j,
                &[
                    "INTO",
                    "IGNORE",
                    "OR",
                    "REPLACE",
                    "ABORT",
                    "FAIL",
                    "ROLLBACK",
                    "LOW_PRIORITY",
                    "DELAYED",
                    "HIGH_PRIORITY",
                    "TOP",
                    "OVERWRITE",
                    "TABLE",
                ],
            )
        {
            j += 1;
        }
        let (parts, mut next) = self.chain(j, end);
        if parts.is_empty() {
            return;
        }
        let scope = self.new_scope(None);
        let source = self.table_ref(&parts, scope, false);
        if let Some(src) = source {
            self.scopes[scope].sources.push(src);
        }
        // INSERT INTO t AS a (PostgreSQL)
        if self.kw(next, "AS") && next + 1 < end && self.t[next + 1].is_name() {
            let def = self.add_def(
                next + 1,
                DefKind::TableAlias,
                (self.t[next - 1].end, self.t[next + 1].end),
            );
            self.out.defs[def].used = true;
            if let Some(s) = self.scopes[scope].sources.last_mut() {
                s.name = self.out.defs[def].name.clone();
                s.alias = Some(def);
            }
            next += 2;
        }
        if next < end
            && self.t[next].kind == Kind::Open
            && !self.kw_any(next + 1, &["SELECT", "WITH", "VALUES"])
        {
            let close = self.pair[next].min(end);
            for (a, b) in self.split_commas(next + 1, close) {
                if b == a + 1 && self.t[a].is_name() {
                    self.column(a, scope, Ctx::Plain, &[a]);
                } else {
                    self.walk(a, b, scope, Ctx::Plain);
                }
            }
            next = close + 1;
        }
        let on = {
            let mut k = next;
            let mut hit = None;
            while k < end {
                if self.kw(k, "ON") && self.kw_any(k + 1, &["DUPLICATE", "CONFLICT"]) {
                    hit = Some(k);
                    break;
                }
                k = self.skip(k);
            }
            hit
        };
        let returning = self.find(next, end, &["RETURNING", "OUTPUT"]);
        let body_end = [on, returning].into_iter().flatten().min().unwrap_or(end);
        if next < body_end {
            if self.kw_any(next, &["SELECT", "WITH"]) || self.t[next].kind == Kind::Open {
                self.query(next, body_end, None);
            } else if self.kw(next, "SET") {
                // MySQL: INSERT INTO t SET a = 1
                self.assignments(next + 1, body_end, scope);
            } else {
                let empty = self.new_scope(None);
                self.walk(next, body_end, empty, Ctx::Plain);
            }
        }
        if let Some(on) = on {
            let stop = returning.filter(|r| *r > on).unwrap_or(end);
            if let Some(set) = self.find(on, stop, &["SET", "UPDATE"]) {
                // ON CONFLICT (cols) DO UPDATE SET …, ON DUPLICATE KEY UPDATE …
                let mut k = on + 1;
                while k < set {
                    if self.t[k].kind == Kind::Open {
                        let close = self.pair[k].min(set);
                        self.walk(k + 1, close, scope, Ctx::Plain);
                        k = close + 1;
                    } else {
                        k += 1;
                    }
                }
                let where_ = self.find(set + 1, stop, &["WHERE"]);
                self.assignments(set + 1, where_.unwrap_or(stop), scope);
                if let Some(w) = where_ {
                    self.walk(w + 1, stop, scope, Ctx::Plain);
                }
            } else {
                self.walk(on + 1, stop, scope, Ctx::Plain);
            }
        }
        if let Some(r) = returning {
            self.walk(r + 1, end, scope, Ctx::Plain);
        }
    }

    /// `a = expr, b = expr` (SET lists): targets are columns.
    fn assignments(&mut self, from: usize, to: usize, scope: usize) {
        for (a, b) in self.split_commas(from, to) {
            let eq = (a..b).find(|&k| self.t[k].kind == Kind::Op && self.text(k) == "=");
            match eq {
                Some(e) => {
                    self.walk(a, e, scope, Ctx::Plain);
                    self.walk(e + 1, b, scope, Ctx::Plain);
                }
                None => self.walk(a, b, scope, Ctx::Plain),
            }
        }
    }

    fn update(&mut self, i: usize, end: usize) {
        let scope = self.new_scope(None);
        let set = self.find(i + 1, end, &["SET"]);
        let Some(set) = set else {
            self.from_clause(i + 1, end, scope);
            return;
        };
        let from = self.find(set + 1, end, &["FROM"]);
        let where_ = self.find(set + 1, end, &["WHERE"]);
        let returning = self.find(set + 1, end, &["RETURNING", "OUTPUT"]);
        let tail = [
            where_,
            returning,
            self.find(set + 1, end, &["ORDER", "LIMIT"]),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(end);
        let mut j = i + 1;
        while self.kw_any(j, &["ONLY", "LOW_PRIORITY", "IGNORE", "TOP"]) {
            j = if self.kw(j, "TOP") {
                self.skip(j + 1)
            } else {
                j + 1
            };
        }
        // SQL Server: UPDATE a SET … FROM t a JOIN …
        if let Some(f) = from.filter(|f| *f < tail) {
            self.from_clause(f + 1, tail, scope);
            let target_is_alias = set == j + 1
                && self.scopes[scope]
                    .sources
                    .iter()
                    .any(|s| s.name.eq_ignore_ascii_case(&self.name(j)));
            if target_is_alias {
                self.qualifier(j, scope);
            } else {
                self.from_clause(j, set, scope);
            }
        } else {
            self.from_clause(j, set, scope);
        }
        let set_end = [from, Some(tail)]
            .into_iter()
            .flatten()
            .filter(|x| *x > set)
            .min()
            .unwrap_or(end);
        self.assignments(set + 1, set_end, scope);
        if let Some(w) = where_ {
            let stop = [returning, self.find(w + 1, end, &["ORDER", "LIMIT"])]
                .into_iter()
                .flatten()
                .filter(|x| *x > w)
                .min()
                .unwrap_or(end);
            self.walk(w + 1, stop, scope, Ctx::Plain);
        } else {
            self.without_where(i, end, "Update", "updates all rows of the table");
        }
        if let Some(r) = returning {
            self.walk(r + 1, end, scope, Ctx::Plain);
        }
    }

    fn delete(&mut self, i: usize, end: usize) {
        let scope = self.new_scope(None);
        let mut j = i + 1;
        while self.kw_any(j, &["LOW_PRIORITY", "QUICK", "IGNORE", "TOP"]) {
            j = if self.kw(j, "TOP") {
                self.skip(j + 1)
            } else {
                j + 1
            };
        }
        let where_ = self.find(j, end, &["WHERE"]);
        let returning = self.find(j, end, &["RETURNING", "OUTPUT"]);
        let tail = [where_, returning, self.find(j, end, &["ORDER", "LIMIT"])]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(end);
        if self.kw(j, "FROM") {
            let using = self.find(j + 1, tail, &["USING"]);
            let from_end = using.unwrap_or(tail);
            if let Some(u) = using {
                // MySQL: DELETE FROM a USING t a JOIN …; PostgreSQL: DELETE FROM t USING u
                self.from_clause(u + 1, tail, scope);
                let names: Vec<(usize, usize)> = self.split_commas(j + 1, from_end);
                let all_aliases = names.iter().all(|(a, b)| {
                    b - a == 1
                        && self.scopes[scope]
                            .sources
                            .iter()
                            .any(|s| s.name.eq_ignore_ascii_case(&self.name(*a)))
                });
                if all_aliases {
                    for (a, _) in names {
                        self.qualifier(a, scope);
                    }
                } else {
                    self.from_clause(j + 1, from_end, scope);
                }
            } else {
                self.from_clause(j + 1, from_end, scope);
            }
        } else if let Some(f) = self.find(j, tail, &["FROM"]) {
            // MySQL multi-table: DELETE a, b FROM t a JOIN u b …
            self.from_clause(f + 1, tail, scope);
            for (a, b) in self.split_commas(j, f) {
                if b > a && self.t[a].is_name() {
                    let (parts, _) = self.chain(a, b);
                    if parts.len() == 1 {
                        self.qualifier(a, scope);
                    }
                }
            }
        } else {
            self.from_clause(j, tail, scope);
        }
        if let Some(w) = where_ {
            let stop = [returning, self.find(w + 1, end, &["ORDER", "LIMIT"])]
                .into_iter()
                .flatten()
                .filter(|x| *x > w)
                .min()
                .unwrap_or(end);
            self.walk(w + 1, stop, scope, Ctx::Plain);
        } else {
            self.without_where(i, end, "Delete", "deletes all rows of the table");
        }
        if let Some(r) = returning {
            self.walk(r + 1, end, scope, Ctx::Plain);
        }
    }

    /// `DELETE` / `UPDATE` without `WHERE`: the finding with an "Add WHERE clause" fix.
    fn without_where(&mut self, i: usize, end: usize, verb: &str, what: &str) {
        let at = self
            .find(i + 1, end, &["RETURNING", "OUTPUT", "ORDER", "LIMIT"])
            .map(|k| self.t[k].start)
            .unwrap_or(self.t[end - 1].end);
        let (text, select) = if at == self.t[end - 1].end {
            (" WHERE FALSE".to_string(), (7, 12))
        } else {
            ("WHERE FALSE ".to_string(), (6, 11))
        };
        let tok = self.t[i];
        self.finding(
            "SqlWithoutWhere",
            tok.start,
            tok.end,
            format!("Unsafe query: '{verb}' statement without 'WHERE' {what}"),
            Some(FindingFix::Insert { at, text, select }),
        );
    }

    // ---- queries -------------------------------------------------------------------------

    /// A query (`WITH …`, `SELECT …`, set operations, parenthesized, `VALUES`); returns its
    /// output columns.
    fn query(&mut self, i: usize, end: usize, parent: Option<usize>) -> Cols {
        if i >= end {
            return Cols::Unknown;
        }
        let mut scope_parent = parent;
        let mut j = i;
        if self.kw(i, "WITH") {
            let scope = self.new_scope(parent);
            j = self.with_clause(i + 1, end, scope);
            scope_parent = Some(scope);
        }
        // Set operations at depth 0.
        let mut parts = Vec::new();
        let mut start = j;
        let mut k = j;
        while k < end {
            if self.kw_any(k, &["UNION", "EXCEPT", "INTERSECT", "MINUS"]) {
                parts.push((start, k));
                k += 1;
                while self.kw_any(k, &["ALL", "DISTINCT"]) {
                    k += 1;
                }
                start = k;
                continue;
            }
            k = self.skip(k);
        }
        parts.push((start, end));
        let mut first = None;
        for (a, b) in parts {
            if a >= b {
                continue;
            }
            let cols = if self.t[a].kind == Kind::Open && self.pair[a] < b {
                let inner = self.query(a + 1, self.pair[a], scope_parent);
                // ORDER BY / LIMIT after the parentheses.
                let tail = self.pair[a] + 1;
                if tail < b {
                    let s = self.new_scope(scope_parent);
                    self.walk(tail, b, s, Ctx::Plain);
                }
                inner
            } else if self.kw(a, "SELECT") {
                self.select(a, b, scope_parent)
            } else if self.kw(a, "VALUES") {
                let s = self.new_scope(scope_parent);
                self.walk(a + 1, b, s, Ctx::Plain);
                Cols::Unknown
            } else if self.kw(a, "TABLE") {
                let (p, _) = self.chain(a + 1, b);
                let s = self.new_scope(scope_parent);
                match self.table_ref(&p, s, false) {
                    Some(src) => src.cols,
                    None => Cols::Unknown,
                }
            } else {
                let s = self.new_scope(scope_parent);
                self.walk(a, b, s, Ctx::Plain);
                Cols::Unknown
            };
            first.get_or_insert(cols);
        }
        first.unwrap_or(Cols::Unknown)
    }

    /// CTEs after `WITH`; returns the index of the main query.
    fn with_clause(&mut self, mut i: usize, end: usize, scope: usize) -> usize {
        let recursive = self.kw(i, "RECURSIVE");
        if recursive {
            i += 1;
        }
        let first_cte = i;
        loop {
            if i >= end || !self.t[i].is_name() {
                return i;
            }
            let name_at = i;
            let mut j = i + 1;
            let mut explicit: Option<Vec<String>> = None;
            if j < end && self.t[j].kind == Kind::Open {
                let close = self.pair[j].min(end);
                explicit = Some(
                    self.split_commas(j + 1, close)
                        .iter()
                        .filter(|(a, b)| b > a && self.t[*a].is_name())
                        .map(|(a, _)| self.name(*a))
                        .collect(),
                );
                j = close + 1;
            }
            if !self.kw(j, "AS") {
                return j;
            }
            j += 1;
            while self.kw_any(j, &["NOT", "MATERIALIZED"]) {
                j += 1;
            }
            if j >= end || self.t[j].kind != Kind::Open {
                return j;
            }
            let close = self.pair[j].min(end);
            let after = (close + 1).min(end);
            let more = after < end && self.t[after].kind == Kind::Comma;
            // Removing a CTE takes its comma (the one after it, or the one before the last).
            let remove = if more {
                (
                    self.t[name_at].start,
                    self.t[after + 1].start.min(self.src.len()),
                )
            } else if name_at > first_cte {
                (self.t[name_at - 1].start, self.t[close.min(end - 1)].end)
            } else {
                // The only CTE: remove the whole WITH clause.
                let with = first_cte - 1 - usize::from(recursive);
                let next = self
                    .t
                    .get(after)
                    .map(|t| t.start)
                    .unwrap_or(self.t[end - 1].end);
                (self.t[with].start, next)
            };
            let def = self.add_def(name_at, DefKind::Cte, remove);
            let cte_name = self.out.defs[def].name.clone();
            if recursive {
                let cols = explicit
                    .as_deref()
                    .map(Cols::names)
                    .unwrap_or(Cols::Unknown);
                self.scopes[scope].ctes.push((cte_name.clone(), def, cols));
            }
            let body = self.query(j + 1, close, Some(scope));
            let cols = match explicit {
                Some(names) => Cols::names(&names),
                None => body,
            };
            if recursive {
                if let Some(c) = self.scopes[scope].ctes.iter_mut().find(|c| c.1 == def) {
                    c.2 = cols;
                }
            } else {
                self.scopes[scope].ctes.push((cte_name, def, cols));
            }
            if !more {
                return after;
            }
            i = after + 1;
        }
    }

    /// One `SELECT` core; returns its output columns.
    fn select(&mut self, i: usize, end: usize, parent: Option<usize>) -> Cols {
        let scope = self.new_scope(parent);
        let list_end = self.find(i + 1, end, SELECT_CLAUSES).unwrap_or(end);
        let from = Some(list_end).filter(|f| *f < end && self.kw(*f, "FROM"));
        let mut from_end = list_end;
        if let Some(f) = from {
            from_end = self.find(f + 1, end, &SELECT_CLAUSES[1..]).unwrap_or(end);
            self.from_clause(f + 1, from_end, scope);
        }
        // The select list.
        let mut j = i + 1;
        loop {
            if self.kw_any(
                j,
                &[
                    "DISTINCT",
                    "ALL",
                    "SQL_CALC_FOUND_ROWS",
                    "STRAIGHT_JOIN",
                    "HIGH_PRIORITY",
                ],
            ) {
                j += 1;
                if self.kw(j - 1, "DISTINCT") && self.kw(j, "ON") && j + 1 < list_end {
                    let close = self.pair[j + 1].min(list_end);
                    self.walk(j + 2, close, scope, Ctx::Plain);
                    j = close + 1;
                }
            } else if self.kw(j, "TOP") {
                j = self.skip(j + 1);
                if self.kw(j, "PERCENT") {
                    j += 1;
                }
                if self.kw(j, "WITH") && self.kw(j + 1, "TIES") {
                    j += 2;
                }
            } else {
                break;
            }
        }
        let mut out: Vec<Col> = Vec::new();
        let mut unknown = false;
        for (a, b) in self.split_commas(j, list_end) {
            if a >= b {
                continue;
            }
            // `expr AS alias` / `expr alias`
            let mut expr_end = b;
            let mut alias: Option<usize> = None;
            if b - a >= 2 && self.kw(b - 2, "AS") && self.t[b - 1].is_name() {
                alias = Some(b - 1);
                expr_end = b - 2;
            } else if b - a >= 2
                && self.t[b - 1].is_name()
                && !(self.t[b - 1].kind == Kind::Word && is_reserved(self.text(b - 1)))
                && matches!(
                    self.t[b - 2].kind,
                    Kind::Word
                        | Kind::Quoted
                        | Kind::Close
                        | Kind::Str
                        | Kind::Number
                        | Kind::Param
                )
                && !(self.t[b - 2].kind == Kind::Word
                    && is_reserved(self.text(b - 2))
                    && !self.kw_any(b - 2, &["END", "NULL", "TRUE", "FALSE"]))
            {
                alias = Some(b - 1);
                expr_end = b - 1;
            }
            let star = expr_end > a
                && self.t[expr_end - 1].kind == Kind::Op
                && self.text(expr_end - 1) == "*"
                && (expr_end - 1 == a || self.t[expr_end - 2].kind == Kind::Dot);
            if star {
                if expr_end - 1 == a {
                    // *: every source's columns.
                    for s in &self.scopes[scope].sources {
                        match &s.cols {
                            Cols::Known(c) => out.extend(c.iter().cloned()),
                            Cols::Unknown => unknown = true,
                        }
                    }
                    if self.scopes[scope].sources.is_empty() {
                        unknown = true;
                    }
                } else {
                    let (parts, _) = self.chain(a, expr_end - 2);
                    let refs_before = self.out.refs.len();
                    self.chain_ref(&parts, scope, Ctx::Plain, true);
                    let q = self.out.refs[refs_before..]
                        .iter()
                        .rev()
                        .find(|r| r.role == Role::Qualifier || r.role == Role::Table)
                        .map(|r| r.name.clone());
                    let src = q.and_then(|q| {
                        self.find_source(scope, &q, None)
                            .map(|(_, s)| s.cols.clone())
                    });
                    match src {
                        Some(Cols::Known(c)) => out.extend(c),
                        _ => unknown = true,
                    }
                }
                continue;
            }
            let refs_before = self.out.refs.len();
            self.walk(a, expr_end, scope, Ctx::Plain);
            // The column this item shows (when the item is just a column).
            let whole_chain = self.chain(a, expr_end).1 == expr_end;
            let simple = self.out.refs[refs_before..].last().filter(|r| {
                whole_chain && r.role == Role::Column && r.to == self.t[expr_end - 1].end
            });
            let col_name = simple.map(|r| r.name.clone());
            let data_type = simple.and_then(|r| match &r.target {
                Some(Target::Column { data_type, .. }) => Some(data_type.clone()),
                _ => None,
            });
            match alias {
                Some(al) => {
                    let def = self.add_def(
                        al,
                        DefKind::SelectAlias,
                        (self.t[expr_end - 1].end, self.t[al].end),
                    );
                    // Output aliases are part of the result: never "unused".
                    self.out.defs[def].used = true;
                    self.out.defs[def].aliased = col_name.clone();
                    let name = self.out.defs[def].name.clone();
                    if let Some(c) = &col_name {
                        if c.eq_ignore_ascii_case(&name) {
                            let (f, t) = self.out.defs[def].remove;
                            self.finding(
                                "SqlRedundantAlias",
                                self.t[al].start,
                                self.t[al].end,
                                format!(
                                    "Redundant alias '{name}': the column already has this name"
                                ),
                                Some(FindingFix::Remove(f, t)),
                            );
                        }
                    }
                    self.scopes[scope].aliases.push((name.clone(), def));
                    out.push(Col { name, data_type });
                }
                None => match col_name {
                    Some(name) => out.push(Col { name, data_type }),
                    None => out.push(Col {
                        name: String::new(),
                        data_type: None,
                    }),
                },
            }
        }
        // The other clauses.
        let mut k = from_end;
        while k < end {
            let stop = self.find(k + 1, end, &SELECT_CLAUSES[1..]).unwrap_or(end);
            let word = self.text(k).to_ascii_uppercase();
            match word.as_str() {
                "INTO" | "FOR" | "SETTINGS" | "FORMAT" | "OPTION" => {}
                "WINDOW" => {
                    // WINDOW w AS (PARTITION BY … ORDER BY …)
                    let mut x = k + 1;
                    while x < stop {
                        if self.t[x].kind == Kind::Open {
                            let close = self.pair[x].min(stop);
                            self.walk(x + 1, close, scope, Ctx::Plain);
                            x = close + 1;
                        } else {
                            x += 1;
                        }
                    }
                }
                "ORDER" => self.walk(k + 1, stop, scope, Ctx::AliasFirst),
                "GROUP" | "HAVING" | "QUALIFY" => self.walk(k + 1, stop, scope, Ctx::AliasLast),
                _ => self.walk(k + 1, stop, scope, Ctx::Plain),
            }
            k = stop;
        }
        if unknown {
            Cols::Unknown
        } else {
            Cols::Known(out)
        }
    }

    /// `FROM` items with joins: adds sources to `scope`, walks `ON` conditions and `USING`.
    fn from_clause(&mut self, from: usize, to: usize, scope: usize) {
        let mut i = from;
        while i < to {
            let tok = self.t[i];
            if tok.kind == Kind::Comma || self.kw_any(i, JOIN_WORDS) {
                if self.kw(i, "NATURAL") {
                    self.scopes[scope].natural = true;
                }
                i += 1;
                continue;
            }
            if self.kw(i, "ON") {
                let stop = self.next_join(i + 1, to);
                self.walk(i + 1, stop, scope, Ctx::Plain);
                i = stop;
                continue;
            }
            if self.kw(i, "USING") && i + 1 < to && self.t[i + 1].kind == Kind::Open {
                let close = self.pair[i + 1].min(to);
                for k in i + 2..close {
                    if self.t[k].is_name() {
                        let n = self.name(k);
                        self.scopes[scope].using.push(n);
                        self.column(k, scope, Ctx::Plain, &[k]);
                    }
                }
                i = close + 1;
                continue;
            }
            if tok.kind == Kind::Open {
                let close = self.pair[i].min(to);
                let sub = self.kw_any(i + 1, &["SELECT", "WITH", "VALUES"]);
                let cols = if sub {
                    self.query(i + 1, close, Some(scope))
                } else {
                    // A parenthesized join: (a JOIN b ON …)
                    self.from_clause(i + 1, close, scope);
                    i = close + 1;
                    continue;
                };
                let (alias, next) = self.alias(close + 1, to, close);
                let mut cols = cols;
                let next = self.alias_columns(next, to, &mut cols);
                let name = alias
                    .map(|a| self.out.defs[a].name.clone())
                    .unwrap_or_default();
                if let Some(a) = alias {
                    self.out.defs[a].kind = DefKind::SubqueryAlias;
                }
                self.scopes[scope].sources.push(Source {
                    name,
                    alias,
                    schema: None,
                    origin: alias.map(Origin::Local).unwrap_or(Origin::Other),
                    cols,
                });
                i = self.skip_to_separator(next, to);
                continue;
            }
            if !tok.is_name()
                || (tok.kind == Kind::Word
                    && is_reserved(self.text(i))
                    && !self.kw_any(i, &["LEFT", "RIGHT"]))
            {
                i = self.skip(i);
                continue;
            }
            let (parts, next) = self.chain(i, to);
            if parts.is_empty() {
                i += 1;
                continue;
            }
            if next < to && self.t[next].kind == Kind::Open {
                // A table function: generate_series(…), unnest(…), json_table(…)
                let close = self.pair[next].min(to);
                self.function_ref(&parts, scope);
                self.walk(next + 1, close, scope, Ctx::Plain);
                let (alias, after) = self.alias(close + 1, to, close);
                let mut cols = Cols::Unknown;
                let after = self.alias_columns(after, to, &mut cols);
                let name = alias
                    .map(|a| self.out.defs[a].name.clone())
                    .unwrap_or_else(|| self.name(*parts.last().unwrap()));
                if let Some(a) = alias {
                    self.out.defs[a].used = true;
                }
                self.scopes[scope].sources.push(Source {
                    name,
                    alias,
                    schema: None,
                    origin: Origin::Other,
                    cols,
                });
                i = self.skip_to_separator(after, to);
                continue;
            }
            let source = self.table_ref(&parts, scope, false);
            let last = *parts.last().unwrap();
            let (alias, after) = self.alias(next, to, last);
            let mut src = source.unwrap_or(Source {
                name: self.name(last),
                alias: None,
                schema: None,
                origin: Origin::Other,
                cols: Cols::Unknown,
            });
            let mut cols = src.cols.clone();
            let after = self.alias_columns(after, to, &mut cols);
            src.cols = cols;
            if let Some(a) = alias {
                let table_name = self.name(last);
                self.out.defs[a].aliased = Some(table_name.clone());
                let alias_name = self.out.defs[a].name.clone();
                if alias_name.eq_ignore_ascii_case(&table_name) {
                    let (f, t) = self.out.defs[a].remove;
                    self.finding(
                        "SqlRedundantAlias",
                        self.out.defs[a].from,
                        self.out.defs[a].to,
                        format!("Redundant alias '{alias_name}': it is the table's name"),
                        Some(FindingFix::Remove(f, t)),
                    );
                    self.out.defs[a].used = true;
                }
                src.name = alias_name;
                src.alias = Some(a);
            }
            self.scopes[scope].sources.push(src);
            i = self.skip_to_separator(after, to);
        }
    }

    /// The end of an `ON` condition: the next join word or comma at depth 0.
    fn next_join(&self, from: usize, to: usize) -> usize {
        let mut i = from;
        while i < to {
            if self.t[i].kind == Kind::Comma
                || self.kw_any(
                    i,
                    &[
                        "JOIN",
                        "LEFT",
                        "RIGHT",
                        "FULL",
                        "INNER",
                        "CROSS",
                        "NATURAL",
                        "STRAIGHT_JOIN",
                        "OUTER",
                        "APPLY",
                    ],
                )
            {
                return i;
            }
            i = self.skip(i);
        }
        to
    }

    /// Skip table hints and the like up to the next comma, join word, ON or USING.
    fn skip_to_separator(&self, from: usize, to: usize) -> usize {
        let mut i = from;
        while i < to {
            if self.t[i].kind == Kind::Comma
                || self.kw_any(i, JOIN_WORDS)
                || self.kw_any(i, &["ON", "USING"])
            {
                return i;
            }
            i = self.skip(i);
        }
        to
    }

    /// `[AS] alias` after a FROM item that ends at token `prev`.
    fn alias(&mut self, i: usize, to: usize, prev: usize) -> (Option<usize>, usize) {
        let (at, with_as) = if self.kw(i, "AS") {
            (i + 1, true)
        } else {
            (i, false)
        };
        if at >= to || !self.t[at].is_name() {
            return (None, i);
        }
        if self.t[at].kind == Kind::Word
            && !with_as
            && (is_reserved(self.text(at))
                || self.kw_any(at, NOT_ALIAS)
                || self.kw_any(at, JOIN_WORDS))
        {
            return (None, i);
        }
        let def = self.add_def(at, DefKind::TableAlias, (self.t[prev].end, self.t[at].end));
        (Some(def), at + 1)
    }

    /// `alias (c1, c2)`: the column names of a derived table.
    fn alias_columns(&mut self, i: usize, to: usize, cols: &mut Cols) -> usize {
        if i < to
            && self.t[i].kind == Kind::Open
            && !self.kw_any(i + 1, &["SELECT", "WITH"])
            && self.t.get(i + 1).is_some_and(Token::is_name)
        {
            let close = self.pair[i].min(to);
            let names: Vec<String> = self
                .split_commas(i + 1, close)
                .iter()
                .filter(|(a, b)| b > a && self.t[*a].is_name())
                .map(|(a, _)| self.name(*a))
                .collect();
            if !names.is_empty() {
                *cols = Cols::names(&names);
            }
            return close + 1;
        }
        i
    }

    /// A dotted name starting at `i`: the token indexes of its parts and the index after it.
    fn chain(&self, i: usize, to: usize) -> (Vec<usize>, usize) {
        let mut parts = Vec::new();
        let mut k = i;
        while k < to && self.t[k].is_name() {
            parts.push(k);
            if k + 2 < to && self.t[k + 1].kind == Kind::Dot && self.t[k + 2].is_name() {
                k += 2;
            } else {
                k += 1;
                break;
            }
        }
        (parts, k)
    }

    /// A table reference (`t`, `schema.t`, `db.schema.t`, a CTE): refs and the source.
    fn table_ref(&mut self, parts: &[usize], scope: usize, lenient: bool) -> Option<Source> {
        let last = *parts.last()?;
        let name = self.name(last);
        if parts.len() == 1 {
            if let Some((def, cols)) = self.find_cte(scope, &name) {
                self.push_ref(
                    last,
                    Role::Table,
                    State::Resolved,
                    Some(Target::Local { def }),
                );
                return Some(Source {
                    name,
                    alias: None,
                    schema: None,
                    origin: Origin::Local(def),
                    cols,
                });
            }
        }
        let schema_at = (parts.len() >= 2).then(|| parts[parts.len() - 2]);
        let schema = schema_at.map(|s| self.name(s));
        let Some(cat) = &self.cat else {
            if let Some(s) = schema_at {
                self.push_ref(s, Role::Schema, State::Unknown, None);
            }
            self.push_ref(last, Role::Table, State::Unknown, None);
            return None;
        };
        let lookup = cat.table(schema.as_deref(), &name);
        let schema_known = schema
            .as_deref()
            .and_then(|s| cat.schema(s))
            .map(str::to_string);
        if let Some(s) = schema_at {
            match &schema_known {
                Some(real) => self.push_ref(
                    s,
                    Role::Schema,
                    State::Resolved,
                    Some(Target::Schema {
                        schema: real.clone(),
                    }),
                ),
                None => {
                    let state = if lookup == Lookup::Unknown || lenient {
                        State::Unknown
                    } else {
                        State::Unresolved
                    };
                    self.push_ref(s, Role::Schema, state, None);
                }
            }
        }
        match lookup {
            Lookup::Found((sn, t)) => {
                let target = Target::Table {
                    schema: sn.to_string(),
                    table: t.name.clone(),
                    view: t.kind.is_view(),
                };
                let cols = Cols::Known(
                    t.columns
                        .iter()
                        .map(|c| Col {
                            name: c.name.clone(),
                            data_type: Some(c.data_type.clone()),
                        })
                        .collect(),
                );
                self.push_ref(last, Role::Table, State::Resolved, Some(target));
                Some(Source {
                    name: t.name.clone(),
                    alias: None,
                    schema: Some(sn.to_string()),
                    origin: Origin::Table {
                        schema: sn.to_string(),
                        table: t.name.clone(),
                    },
                    cols,
                })
            }
            Lookup::Missing if schema_at.is_some() && schema_known.is_none() => {
                self.push_ref(last, Role::Table, State::Unknown, None);
                None
            }
            Lookup::Missing => {
                let state = if lenient {
                    State::Unknown
                } else {
                    State::Unresolved
                };
                self.push_ref(last, Role::Table, state, None);
                None
            }
            Lookup::Unknown => {
                self.push_ref(last, Role::Table, State::Unknown, None);
                None
            }
        }
    }

    fn find_cte(&self, scope: usize, name: &str) -> Option<(usize, Cols)> {
        let mut s = Some(scope);
        while let Some(i) = s {
            if let Some(c) = self.scopes[i]
                .ctes
                .iter()
                .rev()
                .find(|c| c.0.eq_ignore_ascii_case(name))
            {
                return Some((c.1, c.2.clone()));
            }
            s = self.scopes[i].parent;
        }
        None
    }

    /// A source by qualifier in the scope chain; with `schema` it must be that table.
    fn find_source(
        &self,
        scope: usize,
        name: &str,
        schema: Option<&str>,
    ) -> Option<(usize, &Source)> {
        let mut s = Some(scope);
        while let Some(i) = s {
            let hit = self.scopes[i].sources.iter().find(|src| {
                src.name.eq_ignore_ascii_case(name)
                    && schema.is_none_or(|sc| {
                        src.schema
                            .as_deref()
                            .is_some_and(|x| x.eq_ignore_ascii_case(sc))
                            && src.alias.is_none()
                    })
            });
            if let Some(h) = hit {
                return Some((i, h));
            }
            s = self.scopes[i].parent;
        }
        None
    }

    fn has_sources(&self, scope: usize) -> bool {
        let mut s = Some(scope);
        while let Some(i) = s {
            if !self.scopes[i].sources.is_empty() {
                return true;
            }
            s = self.scopes[i].parent;
        }
        false
    }

    fn column_target(src: &Source, col: &Col) -> Target {
        match &src.origin {
            Origin::Table { schema, table } => Target::Column {
                schema: schema.clone(),
                table: table.clone(),
                column: col.name.clone(),
                data_type: col.data_type.clone().unwrap_or_default(),
            },
            Origin::Local(def) => Target::LocalColumn {
                def: *def,
                column: col.name.clone(),
            },
            Origin::Other => Target::LocalColumn {
                def: usize::MAX,
                column: col.name.clone(),
            },
        }
    }

    /// A qualifier (`u` of `u.name`, the targets of `DELETE a FROM …`).
    fn qualifier(&mut self, i: usize, scope: usize) -> Option<Source> {
        let name = self.name(i);
        match self.find_source(scope, &name, None).map(|(_, s)| s.clone()) {
            Some(src) => {
                let target = match (&src.alias, &src.origin) {
                    (Some(def), _) => Some(Target::Local { def: *def }),
                    (None, Origin::Table { schema, table }) => Some(Target::Table {
                        schema: schema.clone(),
                        table: table.clone(),
                        view: false,
                    }),
                    (None, Origin::Local(def)) => Some(Target::Local { def: *def }),
                    (None, Origin::Other) => None,
                };
                self.push_ref(i, Role::Qualifier, State::Resolved, target);
                Some(src)
            }
            None => {
                let pseudo = PSEUDO_ROWS.iter().any(|p| p.eq_ignore_ascii_case(&name))
                    || self.cat.as_ref().is_some_and(|c| c.is_sequence(&name));
                let state = if pseudo || !self.has_sources(scope) {
                    State::Unknown
                } else {
                    State::Unresolved
                };
                self.push_ref(i, Role::Qualifier, state, None);
                None
            }
        }
    }

    /// A column name at `i` (unqualified).
    fn column(&mut self, i: usize, scope: usize, ctx: Ctx, _parts: &[usize]) {
        let name = self.name(i);
        let alias = |w: &Walker| {
            let mut s = Some(scope);
            while let Some(x) = s {
                if let Some(a) = w.scopes[x]
                    .aliases
                    .iter()
                    .find(|a| a.0.eq_ignore_ascii_case(&name))
                {
                    return Some(a.1);
                }
                s = w.scopes[x].parent;
            }
            None
        };
        if ctx == Ctx::AliasFirst {
            if let Some(def) = alias(self) {
                self.push_ref(
                    i,
                    Role::Column,
                    State::Resolved,
                    Some(Target::Local { def }),
                );
                return;
            }
        }
        let mut s = Some(scope);
        let mut unknown = false;
        while let Some(x) = s {
            let mut hits: Vec<(Source, Col)> = Vec::new();
            for src in &self.scopes[x].sources {
                match &src.cols {
                    Cols::Known(cols) => {
                        if let Some(c) = cols.iter().find(|c| c.name.eq_ignore_ascii_case(&name)) {
                            hits.push((src.clone(), c.clone()));
                        }
                    }
                    Cols::Unknown => unknown = true,
                }
            }
            if !hits.is_empty() {
                let target = Self::column_target(&hits[0].0, &hits[0].1);
                let shared = self.scopes[x].natural
                    || self.scopes[x]
                        .using
                        .iter()
                        .any(|u| u.eq_ignore_ascii_case(&name));
                if hits.len() > 1 && !shared {
                    let candidates: Vec<String> = hits
                        .iter()
                        .map(|h| h.0.name.clone())
                        .filter(|n| !n.is_empty())
                        .collect();
                    self.push_ref(i, Role::Column, State::Ambiguous, Some(target));
                    if let Some(r) = self.out.refs.last_mut() {
                        r.candidates = candidates;
                    }
                } else {
                    self.push_ref(i, Role::Column, State::Resolved, Some(target));
                }
                return;
            }
            if unknown {
                break;
            }
            s = self.scopes[x].parent;
        }
        if ctx == Ctx::AliasLast {
            if let Some(def) = alias(self) {
                self.push_ref(
                    i,
                    Role::Column,
                    State::Resolved,
                    Some(Target::Local { def }),
                );
                return;
            }
        }
        let soft = self.t[i].kind == Kind::Word
            && (SOFT_WORDS.iter().any(|w| w.eq_ignore_ascii_case(&name))
                || self.d.is_keyword(&name)
                || self.d.types().iter().any(|t| t.eq_ignore_ascii_case(&name)));
        let state = if unknown || soft || !self.has_sources(scope) {
            State::Unknown
        } else {
            State::Unresolved
        };
        self.push_ref(i, Role::Column, state, None);
    }

    /// A dotted name in an expression.
    fn chain_ref(&mut self, parts: &[usize], scope: usize, ctx: Ctx, star: bool) {
        match parts.len() {
            0 => {}
            1 if star => {
                self.qualifier(parts[0], scope);
            }
            1 => self.column(parts[0], scope, ctx, parts),
            _ => {
                let n = parts.len();
                let (q_at, c_at) = if star {
                    (parts[n - 1], None)
                } else {
                    (parts[n - 2], Some(parts[n - 1]))
                };
                let schema_at = if star {
                    (n >= 2).then(|| parts[n - 2])
                } else {
                    (n >= 3).then(|| parts[n - 3])
                };
                let src = match schema_at {
                    Some(s_at) => {
                        let schema = self.name(s_at);
                        let qn = self.name(q_at);
                        match self
                            .find_source(scope, &qn, Some(&schema))
                            .map(|(_, s)| s.clone())
                        {
                            Some(src) => {
                                if let Some(Origin::Table { schema, table }) = Some(&src.origin) {
                                    self.push_ref(
                                        s_at,
                                        Role::Schema,
                                        State::Resolved,
                                        Some(Target::Schema {
                                            schema: schema.clone(),
                                        }),
                                    );
                                    self.push_ref(
                                        q_at,
                                        Role::Qualifier,
                                        State::Resolved,
                                        Some(Target::Table {
                                            schema: schema.clone(),
                                            table: table.clone(),
                                            view: false,
                                        }),
                                    );
                                }
                                Some(src)
                            }
                            None => {
                                self.push_ref(s_at, Role::Schema, State::Unknown, None);
                                self.push_ref(q_at, Role::Qualifier, State::Unknown, None);
                                None
                            }
                        }
                    }
                    None => self.qualifier(q_at, scope),
                };
                let Some(c_at) = c_at else { return };
                let cname = self.name(c_at);
                match src {
                    Some(src) => match &src.cols {
                        Cols::Known(cols) => match cols
                            .iter()
                            .find(|c| c.name.eq_ignore_ascii_case(&cname))
                        {
                            Some(c) => {
                                let target = Self::column_target(&src, c);
                                self.push_ref(c_at, Role::Column, State::Resolved, Some(target));
                            }
                            None => self.push_ref(c_at, Role::Column, State::Unresolved, None),
                        },
                        Cols::Unknown => self.push_ref(c_at, Role::Column, State::Unknown, None),
                    },
                    None => self.push_ref(c_at, Role::Column, State::Unknown, None),
                }
            }
        }
    }

    /// A function call `name(` / `schema.name(`: navigates to model routines.
    fn function_ref(&mut self, parts: &[usize], _scope: usize) {
        let last = *parts.last().unwrap();
        let name = self.name(last);
        let schema = (parts.len() >= 2).then(|| self.name(parts[parts.len() - 2]));
        let found = self
            .cat
            .as_ref()
            .and_then(|c| c.routine(schema.as_deref(), &name))
            .map(|(s, r)| (s.to_string(), r.name.clone()));
        if let (Some(s_at), Some(cat)) = (
            (parts.len() >= 2).then(|| parts[parts.len() - 2]),
            &self.cat,
        ) {
            if let Some(real) = cat.schema(&self.name(s_at)).map(str::to_string) {
                self.push_ref(
                    s_at,
                    Role::Schema,
                    State::Resolved,
                    Some(Target::Schema { schema: real }),
                );
            }
        }
        match found {
            Some((schema, name)) => self.push_ref(
                last,
                Role::Function,
                State::Resolved,
                Some(Target::Routine { schema, name }),
            ),
            None => self.push_ref(last, Role::Function, State::Unknown, None),
        }
    }

    // ---- expressions ---------------------------------------------------------------------

    /// Names, subqueries and literal comparisons of an expression range.
    fn walk(&mut self, from: usize, to: usize, scope: usize, ctx: Ctx) {
        let to = to.min(self.t.len());
        let mut operands: Vec<(usize, usize, Operand)> = Vec::new();
        let mut i = from;
        while i < to {
            let tok = self.t[i];
            match tok.kind {
                Kind::Open if self.kw_any(i + 1, &["SELECT", "WITH"]) => {
                    let close = self.pair[i].min(to);
                    self.query(i + 1, close, Some(scope));
                    i = close + 1;
                }
                Kind::Word | Kind::Quoted => {
                    let word = tok.kind == Kind::Word;
                    let up = self.text(i).to_ascii_uppercase();
                    if word && up == "ASC" && i > from {
                        self.finding(
                            "SqlRedundantOrderingDirection",
                            tok.start,
                            tok.end,
                            "Redundant ordering direction: ascending is the default".into(),
                            Some(FindingFix::Remove(self.t[i - 1].end, tok.end)),
                        );
                    }
                    if word && up == "ELSE" && self.kw(i + 1, "NULL") && self.kw(i + 2, "END") {
                        let (f, t) = (self.t[i - 1].end, self.t[i + 1].end);
                        self.finding(
                            "SqlRedundantElseNull",
                            tok.start,
                            self.t[i + 1].end,
                            "Redundant ELSE NULL: a CASE without ELSE gives NULL".into(),
                            Some(FindingFix::Remove(f, t)),
                        );
                    }
                    if word && matches!(up.as_str(), "TRUE" | "FALSE") {
                        operands.push((i, i, Operand::Literal(Lit::Bool)));
                    }
                    let prev = i.checked_sub(1).map(|p| self.t[p]);
                    let after_cast = prev.is_some_and(|p| {
                        (p.kind == Kind::Op && p.text(self.src) == "::")
                            || p.is_kw(self.src, "AS")
                            || p.is_kw(self.src, "INTERVAL")
                            || p.is_kw(self.src, "OVER")
                    });
                    let typed_literal =
                        word && self.t.get(i + 1).is_some_and(|n| n.kind == Kind::Str);
                    let extract_part = word
                        && i >= 2
                        && self.t[i - 1].kind == Kind::Open
                        && self.kw_any(
                            i - 2,
                            &[
                                "EXTRACT",
                                "DATE_PART",
                                "DATEADD",
                                "DATEDIFF",
                                "DATEPART",
                                "DATE_TRUNC",
                                "TIMESTAMPADD",
                                "TIMESTAMPDIFF",
                            ],
                        );
                    if after_cast || typed_literal || extract_part || (word && is_reserved(&up)) {
                        i += 1;
                        continue;
                    }
                    let (parts, next) = self.chain(i, to);
                    let star = next < to
                        && self.t[next].kind == Kind::Dot
                        && self
                            .t
                            .get(next + 1)
                            .is_some_and(|t| t.kind == Kind::Op && t.text(self.src) == "*");
                    if next < to && self.t[next].kind == Kind::Open {
                        self.function_ref(&parts, scope);
                        i = next;
                        continue;
                    }
                    let before = self.out.refs.len();
                    self.chain_ref(&parts, scope, ctx, star);
                    if !star {
                        if let Some(r) = self.out.refs.get(before..).and_then(|r| r.last()) {
                            if let Some(Target::Column { data_type, .. }) = &r.target {
                                operands.push((
                                    i,
                                    next - 1,
                                    Operand::Column(data_type.clone(), r.name.clone()),
                                ));
                            }
                        }
                    }
                    i = if star { next + 2 } else { next };
                }
                Kind::Number => {
                    operands.push((i, i, Operand::Literal(Lit::Number)));
                    i += 1;
                }
                Kind::Str => {
                    operands.push((
                        i,
                        i,
                        Operand::Literal(Lit::Text(self.t[i].text(self.src).to_string())),
                    ));
                    i += 1;
                }
                _ => i += 1,
            }
        }
        self.comparisons(&operands, from, to);
    }

    /// Type mismatches of `operand op operand` where both sides are simple.
    fn comparisons(&mut self, ops: &[(usize, usize, Operand)], from: usize, to: usize) {
        // SQLite compares values of any type (type affinity).
        if self.d == SqlDialect::Sqlite {
            return;
        }
        const CMP: &[&str] = &["=", "<>", "!=", "<", ">", "<=", ">="];
        const ARITH: &[&str] = &[
            "+", "-", "*", "/", "%", "||", "::", "->", "->>", "#>", "#>>",
        ];
        let arith = |w: &Walker, k: usize| {
            w.t.get(k).is_some_and(|t| {
                (t.kind == Kind::Op && ARITH.contains(&t.text(w.src)))
                    || t.kind == Kind::Dot
                    || t.kind == Kind::Open
                    || t.is_kw(w.src, "COLLATE")
            })
        };
        for (ai, a) in ops.iter().enumerate() {
            let Some(b) = ops.get(ai + 1) else { break };
            let op = a.1 + 1;
            if b.0 != op + 1
                || op >= to
                || self.t[op].kind != Kind::Op
                || !CMP.contains(&self.text(op))
            {
                continue;
            }
            if (a.0 > from && arith(self, a.0 - 1)) || arith(self, b.1 + 1) {
                continue;
            }
            let (col, lit, lit_at) = match (&a.2, &b.2) {
                (Operand::Column(t, n), Operand::Literal(l)) => ((t, n), Some(l), b.0),
                (Operand::Literal(l), Operand::Column(t, n)) => ((t, n), Some(l), a.0),
                (Operand::Column(t1, n1), Operand::Column(t2, _)) => {
                    let (c1, c2) = (category(t1), category(t2));
                    if c1 != Cat::Unknown && c2 != Cat::Unknown && c1 != c2 && !(compatible(c1, c2))
                    {
                        let (f, e) = (self.t[a.0].start, self.t[b.1].end);
                        self.finding(
                            "SqlType",
                            f,
                            e,
                            format!("Comparing '{n1}' ({t1}) with a value of type {t2}"),
                            None,
                        );
                    }
                    continue;
                }
                _ => continue,
            };
            let (ty, name) = col;
            let Some(lit) = lit else { continue };
            let cat = category(ty);
            let mismatch = match (cat, lit) {
                (Cat::Number, Lit::Text(s)) => !numeric_text(s),
                (Cat::Number, Lit::Bool) => self.d != SqlDialect::MySql,
                (Cat::Text, Lit::Number) => true,
                (Cat::Bool, Lit::Text(s)) => !matches!(
                    unquote(s).to_ascii_lowercase().as_str(),
                    "t" | "f" | "true" | "false" | "1" | "0" | "y" | "n" | "yes" | "no"
                ),
                (Cat::Date, Lit::Number) | (Cat::Date, Lit::Bool) => true,
                (Cat::Date, Lit::Text(s)) => {
                    !unquote(s).starts_with(|c: char| c.is_ascii_digit())
                        && !unquote(s).eq_ignore_ascii_case("now")
                        && !unquote(s).eq_ignore_ascii_case("today")
                }
                (Cat::Text, Lit::Bool) => true,
                _ => false,
            };
            if mismatch {
                let lt = match lit {
                    Lit::Number => "a number",
                    Lit::Text(_) => "a string",
                    Lit::Bool => "a boolean",
                };
                let fix = (cat == Cat::Text && *lit == Lit::Number)
                    .then(|| FindingFix::Quote(self.t[lit_at].start, self.t[lit_at].end));
                let (f, e) = (self.t[a.0].start, self.t[b.1].end);
                self.finding(
                    "SqlType",
                    f,
                    e,
                    format!("Comparing '{name}' ({ty}) with {lt}"),
                    fix,
                );
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Lit {
    Number,
    Text(String),
    Bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Operand {
    /// A column with its type and name.
    Column(String, String),
    Literal(Lit),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cat {
    Number,
    Text,
    Bool,
    Date,
    Unknown,
}

fn compatible(a: Cat, b: Cat) -> bool {
    matches!((a, b), (Cat::Date, Cat::Text) | (Cat::Text, Cat::Date))
}

/// The type family of a column type (`varchar(255)` → text).
fn category(ty: &str) -> Cat {
    let t = ty.to_ascii_lowercase();
    let base = t.split(['(', ' ']).next().unwrap_or("");
    match base {
        "int" | "integer" | "bigint" | "smallint" | "tinyint" | "mediumint" | "int2" | "int4"
        | "int8" | "serial" | "bigserial" | "smallserial" | "numeric" | "decimal" | "real"
        | "double" | "float" | "float4" | "float8" | "number" | "money" | "uint8" | "uint16"
        | "uint32" | "uint64" | "int16" | "int32" | "int64" | "float32" | "float64" => Cat::Number,
        "char" | "varchar" | "text" | "character" | "nchar" | "nvarchar" | "varchar2"
        | "nvarchar2" | "clob" | "nclob" | "string" | "tinytext" | "mediumtext" | "longtext"
        | "citext" | "ntext" => Cat::Text,
        "bool" | "boolean" => Cat::Bool,
        "date" | "datetime" | "datetime2" | "timestamp" | "timestamptz" | "smalldatetime"
        | "datetimeoffset" => Cat::Date,
        _ if base.starts_with("character") => Cat::Text,
        _ => Cat::Unknown,
    }
}

fn unquote(s: &str) -> &str {
    let s = s.trim_start_matches(['E', 'e', 'N', 'n']);
    s.strip_prefix('\'')
        .and_then(|x| x.strip_suffix('\''))
        .unwrap_or(s)
}

fn numeric_text(s: &str) -> bool {
    let u = unquote(s).trim();
    !u.is_empty() && u.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests;
