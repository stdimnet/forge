//! SQL text (`forge-sql`) for the browser-mode database backend and console.

use std::collections::BTreeMap;

use forge_sql::{change_sql, export, leading_keyword, split_statements, statement_at};
use forge_sql::{Dialect, ExportFormat, RowChange};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use super::parse;
use crate::to_json;

#[derive(Deserialize)]
struct SplitRequest {
    sql: String,
    dialect: Dialect,
}

/// `{ sql, dialect }` → `Statement[]` (byte offsets, like `db_execute`).
#[wasm_bindgen]
pub fn sql_split(request_json: &str) -> String {
    match parse::<SplitRequest>(request_json) {
        Ok(r) => to_json(&split_statements(&r.sql, r.dialect)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct StatementAtRequest {
    sql: String,
    from: usize,
    to: usize,
    dialect: Dialect,
}

/// Mirrors `db_statement_at`: `{ sql, from, to, dialect }` → `Statement | null`.
#[wasm_bindgen]
pub fn sql_statement_at(request_json: &str) -> String {
    match parse::<StatementAtRequest>(request_json) {
        Ok(r) => to_json(&statement_at(&r.sql, r.from, r.to, r.dialect)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ChangeRequest {
    dialect: Dialect,
    schema: String,
    table: String,
    pk: Vec<String>,
    changes: Vec<RowChange>,
}

/// `{ dialect, schema, table, pk, changes }` → the statements, or `{ error }`.
#[wasm_bindgen]
pub fn sql_change_statements(request_json: &str) -> String {
    match parse::<ChangeRequest>(request_json) {
        Ok(r) => match r
            .changes
            .iter()
            .map(|c| change_sql(r.dialect, &r.schema, &r.table, &r.pk, c))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(statements) => to_json(&statements),
            Err(e) => to_json(&json!({ "error": e.0 })),
        },
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ExportRequest {
    format: ExportFormat,
    dialect: Dialect,
    table: Option<String>,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

/// Mirrors `db_export`: `{ format, dialect, table, columns, rows }` → the text.
#[wasm_bindgen]
pub fn sql_export(request_json: &str) -> String {
    match parse::<ExportRequest>(request_json) {
        Ok(r) => to_json(&export(
            r.format,
            r.dialect,
            r.table.as_deref(),
            &r.columns,
            &r.rows,
        )),
        Err(e) => e,
    }
}

/// `{ statements: string[] }` → their leading keywords, upper-cased.
#[wasm_bindgen]
pub fn sql_leading_keywords(request_json: &str) -> String {
    match parse::<BTreeMap<String, Vec<String>>>(request_json) {
        Ok(r) => to_json(
            &r.get("statements")
                .map(|s| s.iter().map(|x| leading_keyword(x)).collect::<Vec<_>>())
                .unwrap_or_default(),
        ),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_backend() {
        assert!(
            sql_split(r#"{"sql":"select 1; select 2","dialect":"mysql"}"#).contains("select 2")
        );
        assert!(sql_statement_at(
            r#"{"sql":"select 1;\nselect 2","from":12,"to":12,"dialect":"mysql"}"#
        )
        .contains(r#""start":10"#));
        let err = sql_change_statements(
            r#"{"dialect":"mysql","schema":"s","table":"t","pk":[],"changes":[{"kind":"delete"}]}"#,
        );
        assert!(err.contains("no primary key"), "{err}");
        assert_eq!(
            sql_leading_keywords(r#"{"statements":["-- x\n select 1"]}"#),
            r#"["SELECT"]"#
        );
    }
}
