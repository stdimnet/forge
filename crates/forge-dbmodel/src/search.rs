//! Full-text search across data: per-dialect, read-only `SELECT`s that find rows of a table
//! containing a text in any of its (text) columns, and the matching of result cells for
//! highlighting. The text only ever appears as a quoted literal.

use forge_sql::Dialect;
use regex_lite::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{ColumnInfo, TableInfo};

pub const DEFAULT_LIMIT: u32 = 100;
pub const MAX_LIMIT: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// The text anywhere in the value (`LIKE '%text%'`).
    #[default]
    Contains,
    /// The whole value equals the text.
    Exact,
    /// A regular expression.
    Regex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    pub text: String,
    #[serde(default)]
    pub mode: SearchMode,
    #[serde(default)]
    pub case_sensitive: bool,
    /// Also search numbers, dates and other non-text columns (cast to text).
    #[serde(default)]
    pub all_columns: bool,
    /// Rows per table (default 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

impl SearchOptions {
    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.text.is_empty() {
            return Err("enter a text to search for".into());
        }
        if self.mode == SearchMode::Regex {
            Regex::new(&self.text).map_err(|e| format!("invalid regular expression: {e}"))?;
        }
        Ok(())
    }
}

const TEXT_TYPES: [&str; 14] = [
    "char",
    "text",
    "string",
    "clob",
    "json",
    "uuid",
    "enum",
    "set",
    "xml",
    "citext",
    "name",
    "sysname",
    "lowcardinality(string",
    "fixedstring",
];
const BINARY_TYPES: [&str; 8] = [
    "blob", "bytea", "binary", "image", "raw", "geometry", "point", "polygon",
];

/// Whether a column holds text (`varchar(255)`, `nvarchar(max)`, `String`, `jsonb`, …).
pub fn is_text_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    if BINARY_TYPES.iter().any(|b| t.contains(b)) {
        return false;
    }
    TEXT_TYPES.iter().any(|x| t.contains(x))
}

fn is_binary_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    BINARY_TYPES.iter().any(|b| t.contains(b))
}

/// The columns searched in a table: stored text columns, or every non-binary column.
pub fn searchable_columns<'a>(t: &'a TableInfo, opts: &SearchOptions) -> Vec<&'a ColumnInfo> {
    t.columns
        .iter()
        .filter(|c| !c.is_virtual)
        .filter(|c| {
            if opts.all_columns {
                !is_binary_type(&c.data_type)
            } else {
                is_text_type(&c.data_type)
            }
        })
        .collect()
}

fn like_escape(text: &str, d: Dialect) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        let special = matches!(ch, '%' | '_' | '\\') || (d == Dialect::SqlServer && ch == '[');
        if special {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// The column as text.
fn as_text(d: Dialect, col: &str, c: &ColumnInfo) -> String {
    if is_text_type(&c.data_type) && !c.data_type.to_ascii_lowercase().contains("json") {
        return match d {
            // LowCardinality/Nullable strings still need toString for some functions.
            Dialect::ClickHouse => format!("toString({col})"),
            _ => col.to_string(),
        };
    }
    match d {
        Dialect::Postgres => format!("CAST({col} AS text)"),
        Dialect::MySql => format!("CAST({col} AS CHAR)"),
        Dialect::Sqlite => format!("CAST({col} AS TEXT)"),
        Dialect::SqlServer => format!("CAST({col} AS nvarchar(max))"),
        Dialect::Oracle => format!("TO_CHAR({col})"),
        Dialect::ClickHouse => format!("toString({col})"),
        Dialect::Redis | Dialect::Mongo => col.to_string(),
    }
}

fn predicate(d: Dialect, x: &str, opts: &SearchOptions) -> Result<String, String> {
    let cs = opts.case_sensitive;
    let text = &opts.text;
    let lower = text.to_lowercase();
    let q = |s: &str| d.quote_str(s);
    let pat = |s: &str| q(&format!("%{}%", like_escape(s, d)));
    Ok(match opts.mode {
        SearchMode::Contains => match d {
            Dialect::Postgres if cs => format!("{x} LIKE {} ESCAPE '\\'", pat(text)),
            Dialect::Postgres => format!("{x} ILIKE {} ESCAPE '\\'", pat(text)),
            // MySQL's LIKE escapes with a backslash by default.
            Dialect::MySql if cs => format!("{x} LIKE BINARY {}", pat(text)),
            Dialect::MySql => format!("LOWER({x}) LIKE {}", pat(&lower)),
            Dialect::Sqlite if cs => format!("instr({x}, {}) > 0", q(text)),
            Dialect::Sqlite => format!("LOWER({x}) LIKE {} ESCAPE '\\'", pat(&lower)),
            Dialect::SqlServer if cs => format!(
                "{x} COLLATE Latin1_General_CS_AS LIKE {} ESCAPE '\\'",
                pat(text)
            ),
            Dialect::SqlServer => format!("LOWER({x}) LIKE {} ESCAPE '\\'", pat(&lower)),
            Dialect::Oracle if cs => format!("{x} LIKE {} ESCAPE '\\'", pat(text)),
            Dialect::Oracle => format!("UPPER({x}) LIKE {} ESCAPE '\\'", pat(&text.to_uppercase())),
            Dialect::ClickHouse if cs => format!("position({x}, {}) > 0", q(text)),
            Dialect::ClickHouse => format!("positionCaseInsensitiveUTF8({x}, {}) > 0", q(text)),
            Dialect::Redis | Dialect::Mongo => return Err(unsupported(d)),
        },
        SearchMode::Exact => match d {
            _ if cs => format!("{x} = {}", q(text)),
            Dialect::ClickHouse => format!("lowerUTF8({x}) = {}", q(&lower)),
            Dialect::Redis | Dialect::Mongo => return Err(unsupported(d)),
            _ => format!("LOWER({x}) = {}", q(&lower)),
        },
        SearchMode::Regex => {
            let flag = if cs { "c" } else { "i" };
            match d {
                Dialect::Postgres => {
                    format!("{x} {} {}", if cs { "~" } else { "~*" }, q(text))
                }
                Dialect::MySql | Dialect::Oracle => {
                    format!("REGEXP_LIKE({x}, {}, '{flag}')", q(text))
                }
                Dialect::ClickHouse => format!(
                    "match({x}, {})",
                    q(&if cs {
                        text.clone()
                    } else {
                        format!("(?i){text}")
                    })
                ),
                Dialect::Sqlite | Dialect::SqlServer => {
                    return Err(format!(
                        "{} has no regular expressions; use Contains or Exact",
                        dialect_name(d)
                    ))
                }
                Dialect::Redis | Dialect::Mongo => return Err(unsupported(d)),
            }
        }
    })
}

fn dialect_name(d: Dialect) -> &'static str {
    match d {
        Dialect::MySql => "MySQL",
        Dialect::Postgres => "PostgreSQL",
        Dialect::Sqlite => "SQLite",
        Dialect::SqlServer => "SQL Server",
        Dialect::ClickHouse => "ClickHouse",
        Dialect::Oracle => "Oracle",
        Dialect::Redis => "Redis",
        Dialect::Mongo => "MongoDB",
    }
}

fn unsupported(d: Dialect) -> String {
    format!("full-text search is not available for {}", dialect_name(d))
}

/// The query searching one table, `Ok(None)` when it has no searchable columns. It returns
/// at most `limit + 1` rows, so callers know when results were cut.
pub fn table_query(
    d: Dialect,
    schema: &str,
    t: &TableInfo,
    opts: &SearchOptions,
) -> Result<Option<String>, String> {
    if !d.is_sql() {
        return Err(unsupported(d));
    }
    opts.validate()?;
    let cols = searchable_columns(t, opts);
    if cols.is_empty() {
        return Ok(None);
    }
    let preds = cols
        .iter()
        .map(|c| predicate(d, &as_text(d, &d.quote_ident(&c.name), c), opts))
        .collect::<Result<Vec<_>, _>>()?;
    let select = format!(
        "SELECT * FROM {} WHERE {}",
        d.qualified(schema, &t.name),
        preds.join(" OR ")
    );
    Ok(Some(d.page(&select, "", opts.limit() + 1, 0)))
}

/// Matches cell values the way the queries do (for highlighting and in-memory data).
pub struct CellMatcher {
    mode: SearchMode,
    case_sensitive: bool,
    text: String,
    regex: Option<Regex>,
}

impl CellMatcher {
    pub fn new(opts: &SearchOptions) -> Result<Self, String> {
        opts.validate()?;
        let regex = if opts.mode == SearchMode::Regex {
            Some(
                RegexBuilder::new(&opts.text)
                    .case_insensitive(!opts.case_sensitive)
                    .build()
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        Ok(Self {
            mode: opts.mode,
            case_sensitive: opts.case_sensitive,
            text: if opts.case_sensitive {
                opts.text.clone()
            } else {
                opts.text.to_lowercase()
            },
            regex,
        })
    }

    pub fn matches_text(&self, s: &str) -> bool {
        if let Some(re) = &self.regex {
            return re.is_match(s);
        }
        let s = if self.case_sensitive {
            s.to_string()
        } else {
            s.to_lowercase()
        };
        match self.mode {
            SearchMode::Exact => s == self.text,
            _ => s.contains(&self.text),
        }
    }

    pub fn matches(&self, v: &Value) -> bool {
        match v {
            Value::String(s) => self.matches_text(s),
            Value::Number(n) => self.matches_text(&n.to_string()),
            Value::Bool(b) => self.matches_text(if *b { "true" } else { "false" }),
            _ => false,
        }
    }

    /// Indexes of the matching cells of a row.
    pub fn matching_cells(&self, row: &[Value]) -> Vec<usize> {
        row.iter()
            .enumerate()
            .filter(|(_, v)| self.matches(v))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn table() -> TableInfo {
        TableInfo {
            name: "users".into(),
            columns: vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "bigint".into(),
                    ..Default::default()
                },
                ColumnInfo {
                    name: "email".into(),
                    data_type: "varchar(255)".into(),
                    ..Default::default()
                },
                ColumnInfo {
                    name: "avatar".into(),
                    data_type: "blob".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn opts(text: &str, mode: SearchMode, cs: bool) -> SearchOptions {
        SearchOptions {
            text: text.into(),
            mode,
            case_sensitive: cs,
            all_columns: false,
            limit: Some(50),
        }
    }

    #[test]
    fn builds_per_dialect_queries() {
        let t = table();
        let o = opts("50%_off's", SearchMode::Contains, false);
        assert_eq!(
            table_query(Dialect::Postgres, "public", &t, &o).unwrap().unwrap(),
            "SELECT * FROM \"public\".\"users\" WHERE \"email\" ILIKE '%50\\%\\_off''s%' ESCAPE '\\' LIMIT 51 OFFSET 0"
        );
        assert_eq!(
            table_query(Dialect::MySql, "shop", &t, &o).unwrap().unwrap(),
            "SELECT * FROM `shop`.`users` WHERE LOWER(`email`) LIKE '%50\\\\%\\\\_off''s%' LIMIT 51 OFFSET 0"
        );
        assert_eq!(
            table_query(Dialect::SqlServer, "dbo", &t, &opts("[x]", SearchMode::Contains, false))
                .unwrap()
                .unwrap(),
            "SELECT * FROM [dbo].[users] WHERE LOWER([email]) LIKE N'%\\[x]%' ESCAPE '\\' ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 51 ROWS ONLY"
        );
        assert_eq!(
            table_query(Dialect::ClickHouse, "default", &t, &o)
                .unwrap()
                .unwrap(),
            "SELECT * FROM `default`.`users` WHERE positionCaseInsensitiveUTF8(toString(`email`), '50%_off''s') > 0 LIMIT 51 OFFSET 0"
        );
        assert!(table_query(Dialect::Oracle, "SHOP", &t, &o)
            .unwrap()
            .unwrap()
            .contains("UPPER(\"email\") LIKE '%50\\%\\_OFF''S%' ESCAPE '\\'"));
        assert!(table_query(
            Dialect::Sqlite,
            "main",
            &t,
            &opts("Ada", SearchMode::Contains, true)
        )
        .unwrap()
        .unwrap()
        .contains("instr(\"email\", 'Ada') > 0"));
    }

    #[test]
    fn all_columns_regex_and_errors() {
        let t = table();
        let mut o = opts("^ada@", SearchMode::Regex, false);
        o.all_columns = true;
        let sql = table_query(Dialect::Postgres, "public", &t, &o)
            .unwrap()
            .unwrap();
        assert!(sql.contains("CAST(\"id\" AS text) ~* '^ada@'"), "{sql}");
        assert!(!sql.contains("avatar"), "{sql}");
        assert!(table_query(Dialect::Sqlite, "main", &t, &o).is_err());
        assert!(table_query(Dialect::Mongo, "x", &t, &o).is_err());
        assert!(table_query(
            Dialect::Postgres,
            "public",
            &t,
            &opts("(", SearchMode::Regex, false)
        )
        .is_err());
        let no_text = TableInfo {
            columns: vec![t.columns[0].clone()],
            ..t
        };
        assert_eq!(
            table_query(
                Dialect::Postgres,
                "public",
                &no_text,
                &opts("1", SearchMode::Exact, true)
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn matches_cells() {
        let m = CellMatcher::new(&opts("ADA", SearchMode::Contains, false)).unwrap();
        assert_eq!(
            m.matching_cells(&[
                json!(1),
                json!("ada@example.com"),
                json!(null),
                json!("Bob")
            ]),
            [1]
        );
        let m = CellMatcher::new(&opts("42", SearchMode::Exact, true)).unwrap();
        assert!(m.matches(&json!(42)));
        assert!(!m.matches(&json!("420")));
        let m = CellMatcher::new(&opts("^b.b$", SearchMode::Regex, false)).unwrap();
        assert!(m.matches(&json!("BOB")));
    }

    #[test]
    fn text_types() {
        for t in [
            "varchar(20)",
            "nvarchar(max)",
            "String",
            "LowCardinality(String)",
            "jsonb",
            "CLOB",
            "character varying",
        ] {
            assert!(is_text_type(t), "{t}");
        }
        for t in ["bigint", "bytea", "varbinary(16)", "timestamp", "BLOB"] {
            assert!(!is_text_type(t), "{t}");
        }
    }
}
