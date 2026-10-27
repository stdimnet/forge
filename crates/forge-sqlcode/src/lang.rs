//! SQL dialects of files and consoles (Settings | Languages & Frameworks | SQL Dialects): the
//! project default, per-path mappings like PHPStorm's, and what each dialect changes in the
//! lexer, keywords and formatter.

use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

/// The SQL dialect of a file. `generic` is plain SQL without a database's quirks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SqlDialect {
    #[default]
    Generic,
    MySql,
    Postgres,
    Sqlite,
    SqlServer,
    Oracle,
    ClickHouse,
}

impl SqlDialect {
    pub const ALL: [SqlDialect; 7] = [
        SqlDialect::Generic,
        SqlDialect::MySql,
        SqlDialect::Postgres,
        SqlDialect::Sqlite,
        SqlDialect::SqlServer,
        SqlDialect::Oracle,
        SqlDialect::ClickHouse,
    ];

    /// The dialect of a data source's consoles (Redis and MongoDB are not SQL: generic).
    pub fn from_db(d: Dialect) -> Self {
        match d {
            Dialect::MySql => SqlDialect::MySql,
            Dialect::Postgres => SqlDialect::Postgres,
            Dialect::Sqlite => SqlDialect::Sqlite,
            Dialect::SqlServer => SqlDialect::SqlServer,
            Dialect::ClickHouse => SqlDialect::ClickHouse,
            Dialect::Oracle => SqlDialect::Oracle,
            Dialect::Redis | Dialect::Mongo => SqlDialect::Generic,
        }
    }

    /// The console dialect used for statement splitting and quoting.
    pub fn db(self) -> Dialect {
        match self {
            SqlDialect::MySql => Dialect::MySql,
            SqlDialect::Postgres | SqlDialect::Generic => Dialect::Postgres,
            SqlDialect::Sqlite => Dialect::Sqlite,
            SqlDialect::SqlServer => Dialect::SqlServer,
            SqlDialect::Oracle => Dialect::Oracle,
            SqlDialect::ClickHouse => Dialect::ClickHouse,
        }
    }

    /// `"mysql"`, … as stored in settings; unknown names are generic.
    pub fn parse(s: &str) -> Self {
        serde_json::from_value(serde_json::Value::String(s.to_ascii_lowercase()))
            .unwrap_or_default()
    }

    pub fn id(self) -> &'static str {
        match self {
            SqlDialect::Generic => "generic",
            SqlDialect::MySql => "mysql",
            SqlDialect::Postgres => "postgres",
            SqlDialect::Sqlite => "sqlite",
            SqlDialect::SqlServer => "sqlserver",
            SqlDialect::Oracle => "oracle",
            SqlDialect::ClickHouse => "clickhouse",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SqlDialect::Generic => "Generic SQL",
            SqlDialect::MySql => "MySQL",
            SqlDialect::Postgres => "PostgreSQL",
            SqlDialect::Sqlite => "SQLite",
            SqlDialect::SqlServer => "SQL Server",
            SqlDialect::Oracle => "Oracle",
            SqlDialect::ClickHouse => "ClickHouse",
        }
    }

    /// `` `name` `` quotes identifiers.
    pub fn backticks(self) -> bool {
        matches!(
            self,
            SqlDialect::MySql | SqlDialect::ClickHouse | SqlDialect::Generic | SqlDialect::Sqlite
        )
    }

    /// `[name]` quotes identifiers.
    pub fn brackets(self) -> bool {
        matches!(self, SqlDialect::SqlServer | SqlDialect::Sqlite)
    }

    /// `"text"` is a string (MySQL without ANSI_QUOTES), not an identifier.
    pub fn double_quoted_strings(self) -> bool {
        self == SqlDialect::MySql
    }

    /// `# comment` lines.
    pub fn hash_comments(self) -> bool {
        matches!(self, SqlDialect::MySql | SqlDialect::ClickHouse)
    }

    /// `'it\'s'` escapes.
    pub fn backslash_escapes(self) -> bool {
        matches!(self, SqlDialect::MySql | SqlDialect::ClickHouse)
    }

    /// `$$ … $$` and `$tag$ … $tag$` strings.
    pub fn dollar_quotes(self) -> bool {
        matches!(self, SqlDialect::Postgres | SqlDialect::Generic)
    }

    /// The schema unqualified names belong to when the model does not say.
    pub fn default_schema(self) -> &'static str {
        forge_dbmodel::ddl::default_schema(self.db())
    }

    /// Keywords of the dialect (upper case): the common ones plus the dialect's own.
    pub fn keywords(self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = COMMON_KEYWORDS.to_vec();
        out.extend_from_slice(match self {
            SqlDialect::Generic => &[],
            SqlDialect::MySql => MYSQL_KEYWORDS,
            SqlDialect::Postgres => POSTGRES_KEYWORDS,
            SqlDialect::Sqlite => SQLITE_KEYWORDS,
            SqlDialect::SqlServer => SQLSERVER_KEYWORDS,
            SqlDialect::Oracle => ORACLE_KEYWORDS,
            SqlDialect::ClickHouse => CLICKHOUSE_KEYWORDS,
        });
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Whether `word` is a keyword of the dialect (any case).
    pub fn is_keyword(self, word: &str) -> bool {
        let up = word.to_ascii_uppercase();
        COMMON_KEYWORDS.contains(&up.as_str())
            || match self {
                SqlDialect::Generic => false,
                SqlDialect::MySql => MYSQL_KEYWORDS.contains(&up.as_str()),
                SqlDialect::Postgres => POSTGRES_KEYWORDS.contains(&up.as_str()),
                SqlDialect::Sqlite => SQLITE_KEYWORDS.contains(&up.as_str()),
                SqlDialect::SqlServer => SQLSERVER_KEYWORDS.contains(&up.as_str()),
                SqlDialect::Oracle => ORACLE_KEYWORDS.contains(&up.as_str()),
                SqlDialect::ClickHouse => CLICKHOUSE_KEYWORDS.contains(&up.as_str()),
            }
    }

    /// Built-in type names (for `CAST(x AS type)` and `x::type`, and highlighting).
    pub fn types(self) -> &'static [&'static str] {
        TYPES
    }
}

/// Words that are never names in expressions (a column called `select` must be quoted).
pub(crate) const RESERVED: &[&str] = &[
    "ALL",
    "AND",
    "ANY",
    "AS",
    "ASC",
    "BETWEEN",
    "BY",
    "CASE",
    "CAST",
    "CROSS",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "CURRENT_USER",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DISTINCT",
    "ELSE",
    "END",
    "EXCEPT",
    "EXISTS",
    "FALSE",
    "FETCH",
    "FOR",
    "FROM",
    "FULL",
    "GROUP",
    "HAVING",
    "ILIKE",
    "IN",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTERVAL",
    "INTO",
    "IS",
    "JOIN",
    "LATERAL",
    "LEFT",
    "LIKE",
    "LIMIT",
    "LOCALTIME",
    "LOCALTIMESTAMP",
    "NATURAL",
    "NOT",
    "NULL",
    "NULLS",
    "OFFSET",
    "ON",
    "OR",
    "ORDER",
    "OUTER",
    "OVER",
    "PARTITION",
    "RECURSIVE",
    "REGEXP",
    "RETURNING",
    "RIGHT",
    "ROWS",
    "SELECT",
    "SESSION_USER",
    "SET",
    "SIMILAR",
    "SOME",
    "THEN",
    "TOP",
    "TRUE",
    "UNION",
    "UNKNOWN",
    "UPDATE",
    "USING",
    "VALUES",
    "WHEN",
    "WHERE",
    "WINDOW",
    "WITH",
];

pub(crate) fn is_reserved(word: &str) -> bool {
    let up = word.to_ascii_uppercase();
    RESERVED.binary_search(&up.as_str()).is_ok()
}

const COMMON_KEYWORDS: &[&str] = &[
    "ADD",
    "ALL",
    "ALTER",
    "AND",
    "ANY",
    "AS",
    "ASC",
    "BEGIN",
    "BETWEEN",
    "BY",
    "CASCADE",
    "CASE",
    "CAST",
    "CHECK",
    "COLUMN",
    "COMMIT",
    "CONSTRAINT",
    "CREATE",
    "CROSS",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "DATABASE",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DISTINCT",
    "DROP",
    "ELSE",
    "END",
    "ESCAPE",
    "EXCEPT",
    "EXISTS",
    "EXPLAIN",
    "FALSE",
    "FETCH",
    "FIRST",
    "FOREIGN",
    "FROM",
    "FULL",
    "FUNCTION",
    "GRANT",
    "GROUP",
    "HAVING",
    "IF",
    "IN",
    "INDEX",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTERVAL",
    "INTO",
    "IS",
    "JOIN",
    "KEY",
    "LAST",
    "LATERAL",
    "LEFT",
    "LIKE",
    "LIMIT",
    "NATURAL",
    "NEXT",
    "NOT",
    "NULL",
    "NULLS",
    "OFFSET",
    "ON",
    "ONLY",
    "OR",
    "ORDER",
    "OUTER",
    "OVER",
    "PARTITION",
    "PRIMARY",
    "PROCEDURE",
    "RECURSIVE",
    "REFERENCES",
    "RENAME",
    "REPLACE",
    "RESTRICT",
    "RETURNING",
    "REVOKE",
    "RIGHT",
    "ROLLBACK",
    "ROW",
    "ROWS",
    "SCHEMA",
    "SELECT",
    "SEQUENCE",
    "SET",
    "TABLE",
    "TEMPORARY",
    "THEN",
    "TO",
    "TRANSACTION",
    "TRIGGER",
    "TRUE",
    "TRUNCATE",
    "UNION",
    "UNIQUE",
    "UPDATE",
    "USING",
    "VALUES",
    "VIEW",
    "WHEN",
    "WHERE",
    "WINDOW",
    "WITH",
];

const MYSQL_KEYWORDS: &[&str] = &[
    "AUTO_INCREMENT",
    "CHANGE",
    "DELAYED",
    "DUPLICATE",
    "ENGINE",
    "FORCE",
    "IGNORE",
    "LOW_PRIORITY",
    "MODIFY",
    "REGEXP",
    "SHOW",
    "STRAIGHT_JOIN",
    "UNSIGNED",
    "USE",
    "ZEROFILL",
];

const POSTGRES_KEYWORDS: &[&str] = &[
    "CONCURRENTLY",
    "CONFLICT",
    "DO",
    "ILIKE",
    "MATERIALIZED",
    "NOTHING",
    "OWNER",
    "RETURNING",
    "SIMILAR",
    "VACUUM",
    "VERBOSE",
];

const SQLITE_KEYWORDS: &[&str] = &[
    "ABORT",
    "AUTOINCREMENT",
    "CONFLICT",
    "GLOB",
    "PRAGMA",
    "RETURNING",
    "ROWID",
    "VACUUM",
    "WITHOUT",
];

const SQLSERVER_KEYWORDS: &[&str] = &[
    "APPLY",
    "CLUSTERED",
    "DECLARE",
    "EXEC",
    "EXECUTE",
    "GO",
    "IDENTITY",
    "MERGE",
    "NOCOUNT",
    "NONCLUSTERED",
    "OUTPUT",
    "PRINT",
    "TOP",
    "TRAN",
];

const ORACLE_KEYWORDS: &[&str] = &[
    "CONNECT", "DECLARE", "DUAL", "LOOP", "MERGE", "MINUS", "NOCYCLE", "PACKAGE", "PRIOR",
    "ROWNUM", "START", "SYSDATE",
];

const CLICKHOUSE_KEYWORDS: &[&str] = &[
    "ARRAY", "ENGINE", "FINAL", "FORMAT", "GLOBAL", "OPTIMIZE", "PREWHERE", "SAMPLE", "SETTINGS",
    "TTL",
];

const TYPES: &[&str] = &[
    "BIGINT",
    "BIGSERIAL",
    "BINARY",
    "BIT",
    "BLOB",
    "BOOL",
    "BOOLEAN",
    "BYTEA",
    "CHAR",
    "CHARACTER",
    "CLOB",
    "DATE",
    "DATETIME",
    "DATETIME2",
    "DECIMAL",
    "DOUBLE",
    "FLOAT",
    "INT",
    "INTEGER",
    "JSON",
    "JSONB",
    "MONEY",
    "NCHAR",
    "NUMBER",
    "NUMERIC",
    "NVARCHAR",
    "PRECISION",
    "REAL",
    "SERIAL",
    "SMALLINT",
    "SMALLSERIAL",
    "STRING",
    "TEXT",
    "TIME",
    "TIMESTAMP",
    "TIMESTAMPTZ",
    "TINYINT",
    "UUID",
    "VARBINARY",
    "VARCHAR",
    "VARCHAR2",
];

/// One SQL Dialects mapping: a project-relative file or folder and its dialect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialectMapping {
    pub path: String,
    pub dialect: SqlDialect,
}

/// The dialect of a project-relative `path`: the most specific mapping (the file itself, else
/// its nearest mapped folder), else `project`.
pub fn dialect_for(path: &str, project: SqlDialect, mappings: &[DialectMapping]) -> SqlDialect {
    let path = path.trim_start_matches("./").trim_matches('/');
    mappings
        .iter()
        .filter_map(|m| {
            let p = m.path.trim_start_matches("./").trim_matches('/');
            let hit = p.is_empty()
                || path == p
                || (path.starts_with(p) && path.as_bytes().get(p.len()) == Some(&b'/'));
            hit.then_some((p.len(), m.dialect))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, d)| d)
        .unwrap_or(project)
}

/// What the status bar widget and the settings page show of a dialect.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialectInfo {
    pub id: SqlDialect,
    pub label: &'static str,
    pub keywords: Vec<&'static str>,
    pub types: Vec<&'static str>,
}

/// Every dialect with its keywords and types (for highlighting and completion).
pub fn dialects() -> Vec<DialectInfo> {
    SqlDialect::ALL
        .iter()
        .map(|d| DialectInfo {
            id: *d,
            label: d.label(),
            keywords: d.keywords(),
            types: d.types().to_vec(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_is_sorted() {
        let mut sorted = RESERVED.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, RESERVED);
        assert!(is_reserved("select"));
        assert!(!is_reserved("name"));
    }

    #[test]
    fn most_specific_mapping_wins() {
        let m = vec![
            DialectMapping {
                path: "database".into(),
                dialect: SqlDialect::MySql,
            },
            DialectMapping {
                path: "database/pg".into(),
                dialect: SqlDialect::Postgres,
            },
            DialectMapping {
                path: "database/pg/legacy.sql".into(),
                dialect: SqlDialect::Sqlite,
            },
        ];
        let g = SqlDialect::Generic;
        assert_eq!(dialect_for("database/a.sql", g, &m), SqlDialect::MySql);
        assert_eq!(
            dialect_for("database/pg/b.sql", g, &m),
            SqlDialect::Postgres
        );
        assert_eq!(
            dialect_for("database/pg/legacy.sql", g, &m),
            SqlDialect::Sqlite
        );
        assert_eq!(dialect_for("databases/x.sql", g, &m), g);
        assert_eq!(
            dialect_for("other.sql", SqlDialect::Oracle, &m),
            SqlDialect::Oracle
        );
    }

    #[test]
    fn parses_and_maps() {
        assert_eq!(SqlDialect::parse("SQLServer"), SqlDialect::SqlServer);
        assert_eq!(SqlDialect::parse("nope"), SqlDialect::Generic);
        assert_eq!(SqlDialect::from_db(Dialect::Mongo), SqlDialect::Generic);
        assert!(SqlDialect::MySql.is_keyword("duplicate"));
        assert!(!SqlDialect::Postgres.is_keyword("duplicate"));
        assert_eq!(dialects().len(), 7);
    }
}
