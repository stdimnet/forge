//! Console dialects: quoting of identifiers and literals, transactions and paging. Redis
//! and MongoDB consoles are not SQL; they only use the statement splitting and the guard.

use serde::{Deserialize, Serialize};

/// Language of a console: a SQL flavour, Redis commands or MongoDB shell calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    MySql,
    Postgres,
    Sqlite,
    /// Microsoft SQL Server (T-SQL).
    SqlServer,
    ClickHouse,
    Oracle,
    /// Redis commands, one per line.
    Redis,
    /// MongoDB shell calls: `db.users.find({...})`.
    Mongo,
}

impl Dialect {
    /// Whether consoles of this dialect run SQL.
    pub fn is_sql(self) -> bool {
        !matches!(self, Dialect::Redis | Dialect::Mongo)
    }

    /// Quote an identifier (`users` → `` `users` ``, `"users"` or `[users]`).
    pub fn quote_ident(self, name: &str) -> String {
        match self {
            Dialect::MySql | Dialect::ClickHouse => format!("`{}`", name.replace('`', "``")),
            Dialect::SqlServer => format!("[{}]", name.replace(']', "]]")),
            _ => format!("\"{}\"", name.replace('"', "\"\"")),
        }
    }

    /// Quote a string literal.
    pub fn quote_str(self, s: &str) -> String {
        match self {
            Dialect::MySql | Dialect::ClickHouse => {
                format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
            }
            Dialect::SqlServer => format!("N'{}'", s.replace('\'', "''")),
            _ => format!("'{}'", s.replace('\'', "''")),
        }
    }

    /// A string as a value expression (in `VALUES`, `SET` and `WHERE`; not where the grammar
    /// wants a plain literal, like `COMMENT`). Same as [`Dialect::quote_str`], except for MySQL
    /// strings with a backslash: whether `\\` in a literal is one backslash or two depends on
    /// the server's `NO_BACKSLASH_ESCAPES` mode, so the backslashes are written as
    /// `CHAR(92 USING utf8mb4)`, which reads the same in both modes.
    pub fn quote_value(self, s: &str) -> String {
        if self != Dialect::MySql || !s.contains('\\') {
            return self.quote_str(s);
        }
        let mut parts: Vec<String> = Vec::new();
        let mut rest = s;
        while !rest.is_empty() {
            let run = rest.len() - rest.trim_start_matches('\\').len();
            if run > 0 {
                parts.push(format!("CHAR({} USING utf8mb4)", vec!["92"; run].join(",")));
                rest = &rest[run..];
            } else {
                let end = rest.find('\\').unwrap_or(rest.len());
                parts.push(self.quote_str(&rest[..end]));
                rest = &rest[end..];
            }
        }
        match parts.len() {
            1 => parts.remove(0),
            _ => format!("CONCAT({})", parts.join(", ")),
        }
    }

    /// `schema.table`, leaving the schema out for SQLite's `main`.
    pub fn qualified(self, schema: &str, table: &str) -> String {
        if schema.is_empty() || (self == Dialect::Sqlite && schema == "main") {
            self.quote_ident(table)
        } else {
            format!("{}.{}", self.quote_ident(schema), self.quote_ident(table))
        }
    }

    /// Statement that starts a transaction.
    pub fn begin(self) -> &'static str {
        match self {
            Dialect::MySql => "START TRANSACTION",
            Dialect::SqlServer => "BEGIN TRANSACTION",
            // Oracle starts transactions implicitly; this is a harmless statement.
            Dialect::Oracle => "SET TRANSACTION READ WRITE",
            _ => "BEGIN",
        }
    }

    /// `select` (without `ORDER BY`) limited to `limit` rows after skipping `offset`, ordered
    /// by `order` (`"a", "b" DESC`, may be empty).
    pub fn page(self, select: &str, order: &str, limit: usize, offset: u64) -> String {
        let order_by = if order.is_empty() {
            String::new()
        } else {
            format!(" ORDER BY {order}")
        };
        match self {
            // OFFSET … FETCH needs an ORDER BY in T-SQL.
            Dialect::SqlServer => format!(
                "{select}{} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY",
                if order.is_empty() {
                    " ORDER BY (SELECT NULL)".to_string()
                } else {
                    order_by
                }
            ),
            Dialect::Oracle => {
                format!("{select}{order_by} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY")
            }
            _ => format!("{select}{order_by} LIMIT {limit} OFFSET {offset}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_new_dialects() {
        assert_eq!(Dialect::SqlServer.quote_ident("a]b"), "[a]]b]");
        assert_eq!(Dialect::SqlServer.quote_str("it's"), "N'it''s'");
        assert_eq!(Dialect::ClickHouse.quote_ident("t"), "`t`");
        assert_eq!(Dialect::Oracle.qualified("HR", "EMP"), "\"HR\".\"EMP\"");
    }

    #[test]
    fn mysql_values_with_backslashes_do_not_depend_on_the_sql_mode() {
        let q = |s: &str| Dialect::MySql.quote_value(s);
        assert_eq!(q("it's"), "'it''s'");
        assert_eq!(q("\\"), "CHAR(92 USING utf8mb4)");
        assert_eq!(
            q("C:\\dir\\\\it's"),
            "CONCAT('C:', CHAR(92 USING utf8mb4), 'dir', CHAR(92,92 USING utf8mb4), 'it''s')"
        );
        assert_eq!(q("a\\"), "CONCAT('a', CHAR(92 USING utf8mb4))");
        // Other servers have one reading of a literal.
        assert_eq!(Dialect::Postgres.quote_value("a\\b"), "'a\\b'");
        assert_eq!(Dialect::ClickHouse.quote_value("a\\b"), "'a\\\\b'");
        // Printed by the function above and run against MySQL 8.0 with and without
        // NO_BACKSLASH_ESCAPES: every value came back unchanged in both modes.
    }

    #[test]
    fn pages_per_dialect() {
        assert_eq!(
            Dialect::SqlServer.page("SELECT * FROM [t]", "", 11, 20),
            "SELECT * FROM [t] ORDER BY (SELECT NULL) OFFSET 20 ROWS FETCH NEXT 11 ROWS ONLY"
        );
        assert_eq!(
            Dialect::Oracle.page("SELECT * FROM \"T\"", "\"ID\" DESC", 5, 0),
            "SELECT * FROM \"T\" ORDER BY \"ID\" DESC OFFSET 0 ROWS FETCH NEXT 5 ROWS ONLY"
        );
        assert_eq!(
            Dialect::MySql.page("SELECT * FROM `t`", "", 501, 0),
            "SELECT * FROM `t` LIMIT 501 OFFSET 0"
        );
    }

    #[test]
    fn serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&Dialect::SqlServer).unwrap(),
            "\"sqlserver\""
        );
        assert_eq!(
            serde_json::from_str::<Dialect>("\"clickhouse\"").unwrap(),
            Dialect::ClickHouse
        );
    }
}
