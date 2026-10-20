//! Export a result set as CSV, JSON or SQL `INSERT` statements.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Dialect;
use crate::value::{display, row_object, sql_literal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
    Sql,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
            ExportFormat::Sql => "sql",
        }
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Render rows. `table` names the target of SQL inserts (`MY_TABLE` when unknown).
pub fn export(
    format: ExportFormat,
    dialect: Dialect,
    table: Option<&str>,
    columns: &[String],
    rows: &[Vec<Value>],
) -> String {
    match format {
        ExportFormat::Csv => {
            let mut out = columns
                .iter()
                .map(|c| csv_field(c))
                .collect::<Vec<_>>()
                .join(",");
            out.push('\n');
            for r in rows {
                out.push_str(
                    &r.iter()
                        .map(|v| csv_field(&display(v)))
                        .collect::<Vec<_>>()
                        .join(","),
                );
                out.push('\n');
            }
            out
        }
        ExportFormat::Json => {
            let arr: Vec<Value> = rows
                .iter()
                .map(|r| Value::Object(row_object(columns, r)))
                .collect();
            let mut s = serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".into());
            s.push('\n');
            s
        }
        ExportFormat::Sql => {
            let target = dialect.quote_ident(table.unwrap_or("MY_TABLE"));
            let cols = columns
                .iter()
                .map(|c| dialect.quote_ident(c))
                .collect::<Vec<_>>()
                .join(", ");
            rows.iter()
                .map(|r| {
                    let vals = r
                        .iter()
                        .map(|v| sql_literal(dialect, v))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("INSERT INTO {target} ({cols}) VALUES ({vals});\n")
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data() -> (Vec<String>, Vec<Vec<Value>>) {
        (
            vec!["id".into(), "name".into(), "avatar".into()],
            vec![
                vec![json!(1), json!("Ada, \"the\" first"), Value::Null],
                vec![
                    json!("9007199254740993"),
                    json!("Bob"),
                    json!({"$hex": "ff00"}),
                ],
            ],
        )
    }

    #[test]
    fn csv_quotes_and_nulls() {
        let (c, r) = data();
        assert_eq!(
            export(ExportFormat::Csv, Dialect::MySql, None, &c, &r),
            "id,name,avatar\n1,\"Ada, \"\"the\"\" first\",\n9007199254740993,Bob,0xFF00\n"
        );
    }

    #[test]
    fn json_objects() {
        let (c, r) = data();
        let v: Value =
            serde_json::from_str(&export(ExportFormat::Json, Dialect::MySql, None, &c, &r))
                .unwrap();
        assert_eq!(v[0]["avatar"], Value::Null);
        assert_eq!(v[1]["avatar"], json!("0xFF00"));
        assert_eq!(v[1]["id"], json!("9007199254740993"));
    }

    #[test]
    fn sql_inserts() {
        let (c, r) = data();
        assert_eq!(
            export(ExportFormat::Sql, Dialect::Postgres, Some("users"), &c, &r[..1]),
            "INSERT INTO \"users\" (\"id\", \"name\", \"avatar\") VALUES (1, 'Ada, \"the\" first', NULL);\n"
        );
    }
}
