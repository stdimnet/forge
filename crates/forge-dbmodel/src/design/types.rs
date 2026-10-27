//! What the table dialogs offer per dialect: data types for the type completion, table
//! options and which parts of a table the database supports.

use forge_sql::Dialect;
use serde::Serialize;

/// A table option of the Options tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionField {
    /// The [`super::TableOptions`] field (`engine`, `charset`, `unlogged`, …).
    pub key: &'static str,
    pub label: &'static str,
    /// `text` or `bool`.
    pub kind: &'static str,
    /// Suggestions of a text option.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<&'static str>,
}

/// The dialogs' knowledge of a dialect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignDialect {
    pub types: Vec<&'static str>,
    pub options: Vec<OptionField>,
    pub foreign_keys: bool,
    pub checks: bool,
    pub indexes: bool,
    /// Column and table comments.
    pub comments: bool,
    pub auto_increment: bool,
    /// `ON DELETE` / `ON UPDATE` actions.
    pub fk_actions: Vec<&'static str>,
    /// Index methods (`USING …`).
    pub index_methods: Vec<&'static str>,
    /// The type of a new column.
    pub default_type: &'static str,
}

const MYSQL: &[&str] = &[
    "bigint", "binary(16)", "bit(1)", "blob", "bool", "char(36)", "date", "datetime",
    "datetime(6)", "decimal(10,2)", "double", "enum('a','b')", "float", "geometry", "int",
    "json", "longblob", "longtext", "mediumint", "mediumtext", "set('a','b')", "smallint",
    "text", "time", "timestamp", "tinyint", "tinyint(1)", "varbinary(255)", "varchar(255)",
    "year", "bigint unsigned", "int unsigned",
];
const POSTGRES: &[&str] = &[
    "bigint", "bigserial", "boolean", "bytea", "char(1)", "cidr", "date", "double precision",
    "inet", "integer", "interval", "json", "jsonb", "macaddr", "money", "numeric(10,2)",
    "real", "serial", "smallint", "smallserial", "text", "time", "timestamp",
    "timestamp with time zone", "tsvector", "uuid", "varchar(255)", "xml", "integer[]",
    "text[]",
];
const SQLITE: &[&str] = &[
    "INTEGER", "REAL", "TEXT", "BLOB", "NUMERIC", "BOOLEAN", "DATE", "DATETIME",
    "VARCHAR(255)",
];
const SQLSERVER: &[&str] = &[
    "bigint", "binary(16)", "bit", "char(10)", "date", "datetime", "datetime2",
    "datetimeoffset", "decimal(18,2)", "float", "geography", "image", "int", "money",
    "nchar(10)", "ntext", "numeric(18,2)", "nvarchar(255)", "nvarchar(max)", "real",
    "smalldatetime", "smallint", "smallmoney", "text", "time", "tinyint", "uniqueidentifier",
    "varbinary(max)", "varchar(255)", "varchar(max)", "xml",
];
const CLICKHOUSE: &[&str] = &[
    "UInt8", "UInt16", "UInt32", "UInt64", "Int8", "Int16", "Int32", "Int64", "Float32",
    "Float64", "Decimal(18, 2)", "Bool", "String", "FixedString(16)", "UUID", "Date",
    "Date32", "DateTime", "DateTime64(3)", "Enum8('a' = 1, 'b' = 2)", "LowCardinality(String)",
    "Array(String)", "Map(String, String)", "Tuple(String, UInt64)", "JSON", "IPv4", "IPv6",
];
const ORACLE: &[&str] = &[
    "NUMBER", "NUMBER(10)", "NUMBER(10,2)", "INTEGER", "FLOAT", "BINARY_FLOAT",
    "BINARY_DOUBLE", "VARCHAR2(255)", "NVARCHAR2(255)", "CHAR(1)", "NCHAR(1)", "CLOB",
    "NCLOB", "BLOB", "RAW(16)", "DATE", "TIMESTAMP", "TIMESTAMP WITH TIME ZONE",
    "INTERVAL DAY TO SECOND", "LONG", "XMLTYPE",
];

const ACTIONS: &[&str] = &["NO ACTION", "RESTRICT", "CASCADE", "SET NULL", "SET DEFAULT"];

fn text(key: &'static str, label: &'static str, choices: &[&'static str]) -> OptionField {
    OptionField {
        key,
        label,
        kind: "text",
        choices: choices.to_vec(),
    }
}

fn flag(key: &'static str, label: &'static str) -> OptionField {
    OptionField {
        key,
        label,
        kind: "bool",
        choices: Vec::new(),
    }
}

/// The types, options and features of `d`'s table dialogs.
pub fn dialect_info(d: Dialect) -> DesignDialect {
    let (types, default_type): (&[&str], &str) = match d {
        Dialect::MySql => (MYSQL, "varchar(255)"),
        Dialect::Postgres => (POSTGRES, "varchar(255)"),
        Dialect::Sqlite => (SQLITE, "TEXT"),
        Dialect::SqlServer => (SQLSERVER, "nvarchar(255)"),
        Dialect::ClickHouse => (CLICKHOUSE, "String"),
        Dialect::Oracle => (ORACLE, "VARCHAR2(255)"),
        Dialect::Redis | Dialect::Mongo => (&[], ""),
    };
    let mut types = types.to_vec();
    types.sort_by_key(|t| t.to_ascii_lowercase());
    let options = match d {
        Dialect::MySql => vec![
            text("engine", "Engine", &["InnoDB", "MyISAM", "MEMORY", "ARCHIVE", "Aria"]),
            text("charset", "Charset", &["utf8mb4", "utf8mb3", "latin1", "ascii", "binary"]),
            text(
                "collation",
                "Collation",
                &["utf8mb4_unicode_ci", "utf8mb4_0900_ai_ci", "utf8mb4_bin", "utf8mb4_general_ci"],
            ),
            flag("temporary", "Temporary"),
        ],
        Dialect::Postgres => vec![
            text("tablespace", "Tablespace", &["pg_default"]),
            flag("unlogged", "Unlogged"),
            flag("temporary", "Temporary"),
        ],
        Dialect::Sqlite => vec![
            flag("withoutRowid", "WITHOUT ROWID"),
            flag("strict", "STRICT"),
            flag("temporary", "Temporary"),
        ],
        Dialect::SqlServer => vec![],
        Dialect::ClickHouse => vec![
            text(
                "engine",
                "Engine",
                &["MergeTree", "ReplacingMergeTree", "SummingMergeTree", "AggregatingMergeTree", "Log", "Memory"],
            ),
            text("orderBy", "Order by", &["tuple()"]),
            text("partitionBy", "Partition by", &["toYYYYMM(created_at)"]),
        ],
        Dialect::Oracle => vec![
            text("tablespace", "Tablespace", &["USERS"]),
            flag("temporary", "Global temporary"),
        ],
        Dialect::Redis | Dialect::Mongo => vec![],
    };
    let relational = !matches!(d, Dialect::ClickHouse);
    DesignDialect {
        types,
        options,
        foreign_keys: relational,
        checks: true,
        indexes: relational,
        comments: d != Dialect::Sqlite,
        auto_increment: relational,
        fk_actions: match d {
            Dialect::Oracle => vec!["NO ACTION", "CASCADE", "SET NULL"],
            Dialect::SqlServer => vec!["NO ACTION", "CASCADE", "SET NULL", "SET DEFAULT"],
            _ => ACTIONS.to_vec(),
        },
        index_methods: match d {
            Dialect::Postgres => vec!["btree", "hash", "gin", "gist", "brin", "spgist"],
            Dialect::MySql => vec!["btree", "hash"],
            _ => Vec::new(),
        },
        default_type,
    }
}
