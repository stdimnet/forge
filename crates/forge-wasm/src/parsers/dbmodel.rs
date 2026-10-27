//! The Database Explorer model (`forge-dbmodel`) for the browser-mode database backend:
//! schema filter patterns, DDL data sources, virtual objects, Quick Documentation and the
//! full-text search over the demo rows.

use forge_dbmodel::ddl::{self, DdlFile};
use forge_dbmodel::search::{searchable_columns, table_query};
use forge_dbmodel::{
    apply_patterns, document, CellMatcher, ObjectRef, SchemaInfo, SchemaName, SearchOptions,
    TableInfo, VirtualObjects,
};
use forge_sql::Dialect;
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use super::parse;
use crate::to_json;

fn error(e: impl std::fmt::Display) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
struct MatchRequest {
    patterns: Vec<String>,
    schemas: Vec<SchemaName>,
}

/// Mirrors `db_match_schemas`: `{ patterns, schemas }` → schemas with `visible` set.
#[wasm_bindgen]
pub fn dbmodel_match_schemas(request_json: &str) -> String {
    match parse::<MatchRequest>(request_json) {
        Ok(mut r) => match apply_patterns(&r.patterns, &mut r.schemas) {
            Ok(()) => to_json(&r.schemas),
            Err(e) => error(e),
        },
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DdlRequest {
    dialect: Dialect,
    #[serde(default)]
    default_schema: String,
    files: Vec<DdlFile>,
}

/// `{ dialect, defaultSchema, files: [{ path, text }] }` → `{ schemas, warnings }`.
#[wasm_bindgen]
pub fn dbmodel_parse_ddl(request_json: &str) -> String {
    match parse::<DdlRequest>(request_json) {
        Ok(r) => to_json(&ddl::parse(r.dialect, &r.default_schema, &r.files)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct DocRequest {
    dialect: Dialect,
    schema: SchemaInfo,
    object: ObjectRef,
}

/// Mirrors `db_object_doc`: `{ dialect, schema, object }` → `ObjectDoc | null`.
#[wasm_bindgen]
pub fn dbmodel_object_doc(request_json: &str) -> String {
    match parse::<DocRequest>(request_json) {
        Ok(r) => to_json(&document(r.dialect, &r.schema, &r.object)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VirtualRequest {
    data_source: String,
    objects: VirtualObjects,
    #[serde(default)]
    schema: Option<SchemaInfo>,
}

/// `{ dataSource, objects, schema? }` → the schema with the virtual objects merged in, or
/// `{ ok: true }` when the objects are valid and no schema was given; `{ error }` otherwise.
#[wasm_bindgen]
pub fn dbmodel_apply_virtual(request_json: &str) -> String {
    match parse::<VirtualRequest>(request_json) {
        Ok(r) => {
            if let Err(e) = r.objects.validate() {
                return error(e);
            }
            match r.schema {
                Some(mut s) => {
                    r.objects.apply(&r.data_source, &mut s);
                    to_json(&s)
                }
                None => to_json(&json!({ "ok": true })),
            }
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct SearchRowsRequest {
    dialect: Dialect,
    schema: String,
    table: TableInfo,
    options: SearchOptions,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

/// The full-text search of one table over in-memory rows: the query the backend would run,
/// the matching rows (searched columns only), their matching cells and whether the limit
/// cut them.
#[wasm_bindgen]
pub fn dbmodel_search_rows(request_json: &str) -> String {
    let r = match parse::<SearchRowsRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let sql = match table_query(r.dialect, &r.schema, &r.table, &r.options) {
        Ok(Some(sql)) => sql,
        Ok(None) => {
            return to_json(&json!({ "sql": "", "rows": [], "matches": [], "truncated": false }))
        }
        Err(e) => return error(e),
    };
    let matcher = match CellMatcher::new(&r.options) {
        Ok(m) => m,
        Err(e) => return error(e),
    };
    let searched: Vec<usize> = searchable_columns(&r.table, &r.options)
        .iter()
        .filter_map(|c| r.columns.iter().position(|n| n == &c.name))
        .collect();
    let limit = r.options.limit();
    let mut rows = Vec::new();
    let mut matches = Vec::new();
    let mut truncated = false;
    for row in r.rows {
        let hits: Vec<usize> = matcher
            .matching_cells(&row)
            .into_iter()
            .filter(|i| searched.contains(i))
            .collect();
        if hits.is_empty() {
            continue;
        }
        if rows.len() == limit {
            truncated = true;
            break;
        }
        rows.push(row);
        matches.push(hits);
    }
    to_json(&json!({ "sql": sql, "rows": rows, "matches": matches, "truncated": truncated }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_backend() {
        let out = dbmodel_match_schemas(
            r#"{"patterns":["tenant_*"],"schemas":[{"name":"public","current":true},{"name":"tenant_1","current":false}]}"#,
        );
        assert!(
            out.contains(r#"{"name":"public","current":true,"visible":false}"#),
            "{out}"
        );
        assert!(dbmodel_match_schemas(r#"{"patterns":["/(/"],"schemas":[]}"#).contains("error"));
        let parsed = dbmodel_parse_ddl(
            r#"{"dialect":"postgres","files":[{"path":"a.sql","text":"CREATE TABLE t (id int PRIMARY KEY);"}]}"#,
        );
        assert!(parsed.contains(r#""primaryKey":["id"]"#), "{parsed}");
        let doc = dbmodel_object_doc(
            r#"{"dialect":"mysql","schema":{"name":"s","tables":[{"name":"t","kind":"table","columns":[{"name":"id","dataType":"int","nullable":false}]}]},"object":{"kind":"column","table":"t","name":"id"}}"#,
        );
        assert!(doc.contains("`id` int NOT NULL"), "{doc}");
        let applied = dbmodel_apply_virtual(
            r#"{"dataSource":"d","objects":{"foreignKey":[{"dataSource":"d","schema":"s","table":"t","columns":["u"],"refTable":"users","refColumns":["id"]}]},"schema":{"name":"s","tables":[{"name":"t","kind":"table","columns":[]}]}}"#,
        );
        assert!(applied.contains(r#""virtual":true"#), "{applied}");
        let rows = dbmodel_search_rows(
            r#"{"dialect":"mysql","schema":"shop","table":{"name":"users","kind":"table","columns":[{"name":"id","dataType":"int","nullable":false},{"name":"name","dataType":"varchar(20)","nullable":false}]},"options":{"text":"ada","limit":1},"columns":["id","name"],"rows":[[1,"Ada"],[2,"Bob"],[3,"Adam"]]}"#,
        );
        assert!(
            rows.contains(r#""rows":[[1,"Ada"]]"#) && rows.contains(r#""truncated":true"#),
            "{rows}"
        );
    }
}
