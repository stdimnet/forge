//! The schema tools (`forge-dbschema`) for the browser-mode database backend: ER diagram
//! layout, Compare Structure with the migration script, and Explain Plan (the dialect's
//! `EXPLAIN` statements and the parsing of their rows).

use forge_dbschema::{
    compare_schemas, explain_script, layout, parse_plan, CompareRequest, DiagramRequest,
};
use forge_sql::Dialect;
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use super::parse;
use crate::to_json;

/// Mirrors `db_diagram`: `DiagramRequest` → `Diagram`.
#[wasm_bindgen]
pub fn dbschema_diagram(request_json: &str) -> String {
    match parse::<DiagramRequest>(request_json) {
        Ok(r) => to_json(&layout(&r)),
        Err(e) => e,
    }
}

/// Mirrors `db_schema_compare`: `CompareRequest` → `CompareResult` or `{ error }`.
#[wasm_bindgen]
pub fn dbschema_compare(request_json: &str) -> String {
    match parse::<CompareRequest>(request_json) {
        Ok(r) => match compare_schemas(&r) {
            Ok(res) => to_json(&res),
            Err(e) => to_json(&json!({ "error": e })),
        },
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ScriptRequest {
    dialect: Dialect,
    sql: String,
    #[serde(default)]
    raw: bool,
}

/// `{ dialect, sql, raw }` → `ExplainScript` or `{ error }`.
#[wasm_bindgen]
pub fn dbschema_explain_script(request_json: &str) -> String {
    match parse::<ScriptRequest>(request_json) {
        Ok(r) => match explain_script(r.dialect, &r.sql, r.raw) {
            Ok(s) => to_json(&s),
            Err(e) => to_json(&json!({ "error": e })),
        },
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct PlanRequest {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

/// `{ columns, rows }` of an explain statement → `Plan` or `{ error }`.
#[wasm_bindgen]
pub fn dbschema_parse_plan(request_json: &str) -> String {
    match parse::<PlanRequest>(request_json) {
        Ok(r) => match parse_plan(&r.columns, &r.rows) {
            Ok(p) => to_json(&p),
            Err(e) => to_json(&json!({ "error": e })),
        },
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_backend() {
        let d = dbschema_diagram(
            r#"{"schema":{"name":"s","tables":[{"name":"t","kind":"table","columns":[{"name":"id","dataType":"int","nullable":false}],"primaryKey":["id"]}]}}"#,
        );
        assert!(d.contains(r#""primary":true"#), "{d}");
        let c = dbschema_compare(
            r#"{"dialect":"sqlite","source":{"ddl":"CREATE TABLE a (id int);"},"target":{"ddl":""}}"#,
        );
        assert!(c.contains(r#"CREATE TABLE \"a\""#), "{c}");
        let s = dbschema_explain_script(r#"{"dialect":"sqlite","sql":"SELECT 1"}"#);
        assert!(s.contains("EXPLAIN QUERY PLAN SELECT 1"), "{s}");
        let p = dbschema_parse_plan(
            r#"{"columns":["EXPLAIN"],"rows":[["-> Table scan on u  (cost=0.5 rows=3)"]]}"#,
        );
        assert!(p.contains(r#""operation":"Table scan""#), "{p}");
        assert!(dbschema_parse_plan(r#"{"columns":[],"rows":[]}"#).contains("error"));
    }
}
