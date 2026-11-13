//! Server-side sort and filter of a table's data editor: the header sort (`ORDER BY`), the
//! typed `WHERE` condition of the filter row and per-column "Filter by value" conditions
//! become one `SELECT` per dialect.
//!
//! The typed condition is user text, so it is checked first: it must be one condition
//! (no `;`, balanced brackets, closed quotes and comments) and must not write (no DML/DDL,
//! no `SELECT … INTO`), using the read-only guard of `forge-sql`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use forge_sql::lex::{self, Tok};
use forge_sql::value::sql_literal;
use forge_sql::{guard, Dialect, GuardPolicy, Refused};

/// One column of the header sort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortKey {
    pub column: String,
    #[serde(default)]
    pub desc: bool,
}

/// Comparison of a column filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    IsNull,
    IsNotNull,
    /// `LIKE` with `%` and `_` wildcards (case-insensitive locally).
    Like,
    NotLike,
    /// Any of `values`.
    In,
}

/// "Filter by value" from the grid's context menu, and the conditions of foreign-key
/// navigation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnFilter {
    pub column: String,
    pub op: FilterOp,
    #[serde(default)]
    pub value: Value,
    /// Values of `In`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<Value>,
}

impl ColumnFilter {
    pub fn eq(column: &str, value: Value) -> Self {
        ColumnFilter {
            column: column.to_string(),
            op: if value.is_null() {
                FilterOp::IsNull
            } else {
                FilterOp::Eq
            },
            value,
            values: Vec::new(),
        }
    }

    /// The condition in `d`'s SQL.
    pub fn sql(&self, d: Dialect) -> String {
        let col = d.quote_ident(&self.column);
        let lit = |v: &Value| sql_literal(d, v);
        match self.op {
            FilterOp::IsNull => format!("{col} IS NULL"),
            FilterOp::IsNotNull => format!("{col} IS NOT NULL"),
            // `= NULL` never matches; the menu means "is NULL".
            FilterOp::Eq if self.value.is_null() => format!("{col} IS NULL"),
            FilterOp::Ne if self.value.is_null() => format!("{col} IS NOT NULL"),
            FilterOp::Eq => format!("{col} = {}", lit(&self.value)),
            FilterOp::Ne => format!("{col} <> {}", lit(&self.value)),
            FilterOp::Lt => format!("{col} < {}", lit(&self.value)),
            FilterOp::Le => format!("{col} <= {}", lit(&self.value)),
            FilterOp::Gt => format!("{col} > {}", lit(&self.value)),
            FilterOp::Ge => format!("{col} >= {}", lit(&self.value)),
            FilterOp::Like => format!("{col} LIKE {}", lit(&text_value(&self.value))),
            FilterOp::NotLike => format!("{col} NOT LIKE {}", lit(&text_value(&self.value))),
            FilterOp::In if self.values.is_empty() => "1 = 0".to_string(),
            FilterOp::In => format!(
                "{col} IN ({})",
                self.values.iter().map(lit).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

fn text_value(v: &Value) -> Value {
    match v {
        Value::String(_) => v.clone(),
        other => Value::String(crate::text(other)),
    }
}

/// What the data editor of a table shows: filter and sort.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableQuery {
    pub schema: String,
    pub table: String,
    /// The `WHERE` condition typed into the filter row.
    #[serde(default)]
    pub filter: Option<String>,
    /// "Filter by value" conditions, combined with `AND`.
    #[serde(default)]
    pub column_filters: Vec<ColumnFilter>,
    #[serde(default)]
    pub order: Vec<SortKey>,
}

/// The statements of a [`TableQuery`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSql {
    /// `SELECT * FROM t WHERE …` without `ORDER BY` (paged with `Dialect::page`).
    pub select: String,
    /// `"a", "b" DESC` (may be empty).
    pub order: String,
    /// `SELECT COUNT(*) FROM t WHERE …`.
    pub count: String,
    /// The statement as shown to the user (`select` plus `ORDER BY`).
    pub display: String,
    /// The combined condition (empty without filters).
    #[serde(rename = "where")]
    pub where_: String,
}

impl TableQuery {
    /// Statements in `d`, or why the typed condition is refused.
    pub fn sql(&self, d: Dialect) -> Result<TableSql, Refused> {
        if !d.is_sql() {
            return Err(Refused("this data source has no SQL tables".into()));
        }
        let mut parts = Vec::new();
        if let Some(f) = self.filter.as_deref() {
            let checked = check_where(f, d)?;
            if !checked.is_empty() {
                parts.push(if self.column_filters.is_empty() {
                    checked
                } else {
                    format!("({checked})")
                });
            }
        }
        parts.extend(self.column_filters.iter().map(|f| f.sql(d)));
        let where_ = parts.join(" AND ");
        let mut from = d.qualified(&self.schema, &self.table);
        if !where_.is_empty() {
            from.push_str(&format!(" WHERE {where_}"));
        }
        let order = self
            .order
            .iter()
            .map(|k| {
                format!(
                    "{}{}",
                    d.quote_ident(&k.column),
                    if k.desc { " DESC" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let select = format!("SELECT * FROM {from}");
        let display = if order.is_empty() {
            select.clone()
        } else {
            format!("{select} ORDER BY {order}")
        };
        Ok(TableSql {
            count: format!("SELECT COUNT(*) FROM {from}"),
            select,
            order,
            display,
            where_,
        })
    }
}

/// Check the typed `WHERE` condition: trimmed, a trailing `;` dropped, comments kept but
/// closed. Refused when it is more than one condition or could write.
pub fn check_where(text: &str, d: Dialect) -> Result<String, Refused> {
    let mut t = text.trim();
    while let Some(rest) = t.strip_suffix(';') {
        t = rest.trim_end();
    }
    if t.is_empty() {
        return Ok(String::new());
    }
    let words = scan(t, d)?;
    if let Some(w) = words.iter().find(|w| WRITES.contains(&w.as_str())) {
        return Err(Refused(format!(
            "The filter must be a condition; {w} is not allowed in it"
        )));
    }
    let probe = format!("SELECT * FROM t WHERE {t}");
    let verdict = guard(
        &probe,
        d,
        &GuardPolicy {
            read_only: true,
            production: false,
            name: "the filter".into(),
        },
    );
    if verdict.blocked.is_some() {
        return Err(Refused(
            "The filter must be a condition; it cannot change data".into(),
        ));
    }
    // A trailing line comment would swallow what follows the condition.
    Ok(if has_line_comment(t, d) {
        format!("{t}\n")
    } else {
        t.to_string()
    })
}

/// Keywords that never belong in a condition, at any depth (subqueries included).
const WRITES: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "MERGE", "UPSERT", "REPLACE", "DROP", "CREATE", "ALTER",
    "TRUNCATE", "GRANT", "REVOKE", "INTO", "EXEC", "EXECUTE", "CALL",
];

/// Check quotes, comments, brackets and `;` of a condition; its words outside quotes and
/// comments, upper-cased. The condition is read the way the dialect reads it (`E'…'` and
/// `$$…$$` in PostgreSQL, backslash escapes in MySQL, `[…]` in SQL Server, nested and
/// executable comments), in every plausible reading of the server settings.
fn scan(t: &str, d: Dialect) -> Result<Vec<String>, Refused> {
    let mut words = Vec::new();
    for &o in lex::variants(d) {
        let lexed = lex::tokenize(t, d, o);
        if lexed.unterminated {
            return Err(Refused(
                "The filter has an unclosed quote, identifier or comment".into(),
            ));
        }
        let mut depth = 0i32;
        for tok in &lexed.tokens {
            match tok.tok {
                Tok::Semi => {
                    return Err(Refused("The filter must be one condition without ;".into()))
                }
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(Refused("The filter has an unmatched )".into()));
                    }
                }
                Tok::Word(w) => words.push(w.to_ascii_uppercase()),
                _ => {}
            }
        }
        if depth != 0 {
            return Err(Refused("The filter has an unclosed (".into()));
        }
    }
    Ok(words)
}

fn has_line_comment(t: &str, d: Dialect) -> bool {
    let last = t.lines().last().unwrap_or("");
    last.contains("--") || (matches!(d, Dialect::MySql | Dialect::ClickHouse) && last.contains('#'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn q(filter: Option<&str>, cf: Vec<ColumnFilter>, order: Vec<(&str, bool)>) -> TableQuery {
        TableQuery {
            schema: "shop".into(),
            table: "orders".into(),
            filter: filter.map(str::to_string),
            column_filters: cf,
            order: order
                .into_iter()
                .map(|(c, desc)| SortKey {
                    column: c.into(),
                    desc,
                })
                .collect(),
        }
    }

    #[test]
    fn sort_and_filter_per_dialect() {
        let query = q(
            Some("total > 100;"),
            vec![ColumnFilter::eq("status", json!("paid"))],
            vec![("placed_at", true), ("id", false)],
        );
        let my = query.sql(Dialect::MySql).unwrap();
        assert_eq!(
            my.display,
            "SELECT * FROM `shop`.`orders` WHERE (total > 100) AND `status` = 'paid' ORDER BY `placed_at` DESC, `id`"
        );
        assert_eq!(
            my.count,
            "SELECT COUNT(*) FROM `shop`.`orders` WHERE (total > 100) AND `status` = 'paid'"
        );
        let pg = query.sql(Dialect::Postgres).unwrap();
        assert_eq!(
            pg.select,
            "SELECT * FROM \"shop\".\"orders\" WHERE (total > 100) AND \"status\" = 'paid'"
        );
        assert_eq!(pg.order, "\"placed_at\" DESC, \"id\"");
        let ms = query.sql(Dialect::SqlServer).unwrap();
        assert_eq!(
            ms.select,
            "SELECT * FROM [shop].[orders] WHERE (total > 100) AND [status] = N'paid'"
        );
        assert_eq!(
            Dialect::SqlServer.page(&ms.select, &ms.order, 11, 0),
            "SELECT * FROM [shop].[orders] WHERE (total > 100) AND [status] = N'paid' ORDER BY [placed_at] DESC, [id] OFFSET 0 ROWS FETCH NEXT 11 ROWS ONLY"
        );
        let lite = TableQuery {
            schema: "main".into(),
            ..q(None, vec![], vec![])
        };
        assert_eq!(
            lite.sql(Dialect::Sqlite).unwrap().display,
            "SELECT * FROM \"orders\""
        );
        let ora = q(None, vec![], vec![("ID", true)])
            .sql(Dialect::Oracle)
            .unwrap();
        assert_eq!(
            ora.display,
            "SELECT * FROM \"shop\".\"orders\" ORDER BY \"ID\" DESC"
        );
        // Only the typed filter is used without column filters: no extra brackets.
        assert_eq!(
            q(Some("a = 1"), vec![], vec![])
                .sql(Dialect::ClickHouse)
                .unwrap()
                .where_,
            "a = 1"
        );
    }

    #[test]
    fn column_filters() {
        let d = Dialect::MySql;
        let f = |op, value| ColumnFilter {
            column: "status".into(),
            op,
            value,
            values: vec![],
        };
        assert_eq!(f(FilterOp::Eq, Value::Null).sql(d), "`status` IS NULL");
        assert_eq!(f(FilterOp::Ne, Value::Null).sql(d), "`status` IS NOT NULL");
        assert_eq!(f(FilterOp::Ne, json!("x")).sql(d), "`status` <> 'x'");
        assert_eq!(f(FilterOp::Like, json!(5)).sql(d), "`status` LIKE '5'");
        assert_eq!(f(FilterOp::Ge, json!(2)).sql(d), "`status` >= 2");
        let mut inn = f(FilterOp::In, Value::Null);
        inn.values = vec![json!("paid"), json!("shipped")];
        assert_eq!(inn.sql(d), "`status` IN ('paid', 'shipped')");
        inn.values.clear();
        assert_eq!(inn.sql(d), "1 = 0");
        assert_eq!(
            f(FilterOp::Eq, json!("O'Brien")).sql(Dialect::Postgres),
            "\"status\" = 'O''Brien'"
        );
    }

    #[test]
    fn refuses_unsafe_filters() {
        let d = Dialect::MySql;
        assert!(check_where("1=1; DROP TABLE users", d).is_err());
        assert!(check_where("id IN (SELECT id FROM t", d).is_err());
        assert!(check_where("id = 1) OR (1=1", d).is_err());
        assert!(check_where("name = 'x", d).is_err());
        assert!(check_where("a = 1 /* open", d).is_err());
        assert!(check_where("id IN (DELETE FROM t RETURNING id)", Dialect::Postgres).is_err());
        assert!(check_where("exists (select 1 into x)", d).is_err());
        // PostgreSQL `E''` strings: `\'` is an escaped quote, so the `;` is code.
        let pg = Dialect::Postgres;
        assert!(check_where(
            r"name = E'\'' ; WITH d AS (DELETE FROM users RETURNING 1) SELECT * FROM d --'",
            pg
        )
        .is_err());
        assert!(check_where(r"name = E'\'' OR 1 = 1 --'", pg).is_ok());
        assert!(check_where("name = $$ ' $$ ; DELETE FROM users --'", pg).is_err());
        assert!(check_where("a = 1 /* /* */ ' */ ; DELETE FROM users -- '", pg).is_err());
        assert!(check_where(r"name = 'a\' ; DELETE FROM users --'", pg).is_err());
        assert_eq!(
            check_where(r"name = E'it\'s'", pg).unwrap(),
            r"name = E'it\'s'"
        );
        // MySQL: executable comments are code; `NO_BACKSLASH_ESCAPES` changes strings.
        assert!(check_where("a = 1 /*! ; DROP TABLE users */", d).is_err());
        assert!(check_where(r"a = 'x\' ; DROP TABLE users -- '", d).is_err());
        // SQL Server needs no `;` between statements.
        let ms = Dialect::SqlServer;
        assert!(check_where("a = 1 DROP TABLE users", ms).is_err());
        assert!(check_where("a = 1 SET ROLE x", ms).is_err());
        assert!(check_where("[a]]b] = 1", ms).is_ok());
        // Quoted identifiers are not keywords.
        assert!(check_where("\"update\" = 1", Dialect::Postgres).is_ok());
        assert_eq!(check_where("  ", d).unwrap(), "");
        // Semicolons in strings and comments are fine; trailing ones are dropped.
        assert_eq!(check_where("name = 'a;b';", d).unwrap(), "name = 'a;b'");
        assert_eq!(check_where("a = 'it''s'", d).unwrap(), "a = 'it''s'");
        assert_eq!(check_where("a = 1 -- note", d).unwrap(), "a = 1 -- note\n");
        assert_eq!(
            check_where("id IN (SELECT user_id FROM orders)", d).unwrap(),
            "id IN (SELECT user_id FROM orders)"
        );
        let err = q(Some("1; drop table x"), vec![], vec![])
            .sql(d)
            .unwrap_err();
        assert!(err.0.contains("one condition"), "{err}");
        assert!(q(None, vec![], vec![]).sql(Dialect::Redis).is_err());
    }
}
