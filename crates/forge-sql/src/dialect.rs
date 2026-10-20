//! SQL dialects: quoting of identifiers and literals.

use serde::{Deserialize, Serialize};

/// SQL flavour used to quote identifiers and literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    MySql,
    Postgres,
    Sqlite,
}

impl Dialect {
    /// Quote an identifier (`users` → `` `users` `` or `"users"`).
    pub fn quote_ident(self, name: &str) -> String {
        match self {
            Dialect::MySql => format!("`{}`", name.replace('`', "``")),
            Dialect::Postgres | Dialect::Sqlite => format!("\"{}\"", name.replace('"', "\"\"")),
        }
    }

    /// Quote a string literal.
    pub fn quote_str(self, s: &str) -> String {
        match self {
            Dialect::MySql => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''")),
            Dialect::Postgres | Dialect::Sqlite => format!("'{}'", s.replace('\'', "''")),
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
            Dialect::Postgres | Dialect::Sqlite => "BEGIN",
        }
    }
}
