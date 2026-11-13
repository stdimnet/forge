//! The SQL editor (`forge-sqlcode`) for browser mode, mirroring the `sql_*` IPC commands:
//! the mock passes the data source model, the settings and the history it keeps in memory.

use std::collections::BTreeMap;

use forge_dbmodel::ddl::DdlFile;
use forge_sqlcode::api::{self, SqlContext};
use forge_sqlcode::history::{History, HistoryEntry, HistoryQuery};
use forge_sqlcode::{
    data_source_for, dialect_for, DialectMapping, ParamPatterns, SqlDialect, SqlStyle,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::to_json;

fn run<T: DeserializeOwned, R: serde::Serialize>(json: &str, f: impl FnOnce(T) -> R) -> String {
    match serde_json::from_str::<T>(json) {
        Ok(v) => to_json(&f(v)),
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

/// `{}` → every dialect with keywords and types.
#[wasm_bindgen]
pub fn sql_dialects(_json: &str) -> String {
    to_json(&forge_sqlcode::lang::dialects())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRequest {
    path: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    global: String,
    #[serde(default)]
    mappings: Vec<DialectMapping>,
}

/// Mirrors `sql_file_info` for a file: `{ path, project, global, mappings }` →
/// `{ dialect, source, dataSourceId? }`.
#[wasm_bindgen]
pub fn sql_file_info(json: &str) -> String {
    run(json, |r: FileRequest| {
        let fallback = if r.project.is_empty() {
            (SqlDialect::parse(&r.global), "global")
        } else {
            (SqlDialect::parse(&r.project), "project")
        };
        let mapped = dialect_for(&r.path, SqlDialect::Oracle, &r.mappings)
            == dialect_for(&r.path, SqlDialect::Generic, &r.mappings);
        let (dialect, source) = if mapped {
            (
                dialect_for(&r.path, SqlDialect::Generic, &r.mappings),
                "mapping",
            )
        } else {
            fallback
        };
        json!({ "dialect": dialect, "source": source, "dataSourceId": data_source_for(&r.path, &r.mappings) })
    })
}

/// `SqlContext` → `AnalyzeAnswer`.
#[wasm_bindgen]
pub fn sql_analyze(json: &str) -> String {
    run(json, |c: SqlContext| api::analyze_text(&c))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AtRequest {
    #[serde(flatten)]
    ctx: SqlContext,
    offset: usize,
    #[serde(default)]
    ddl_files: Vec<DdlFile>,
}

/// `{ …SqlContext, offset, ddlFiles? }` → `DeclarationAnswer | null`.
#[wasm_bindgen]
pub fn sql_declaration(json: &str) -> String {
    run(json, |r: AtRequest| {
        api::declaration(&r.ctx, r.offset, &r.ddl_files)
    })
}

/// `{ …SqlContext, offset }` → `UsagesAnswer | null`.
#[wasm_bindgen]
pub fn sql_usages(json: &str) -> String {
    run(json, |r: AtRequest| api::usages(&r.ctx, r.offset))
}

/// `{ …SqlContext, offset }` → `RenameAnswer`.
#[wasm_bindgen]
pub fn sql_rename(json: &str) -> String {
    run(json, |r: AtRequest| api::rename(&r.ctx, r.offset))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FormatRequest {
    text: String,
    #[serde(default)]
    dialect: SqlDialect,
    /// The code style scheme's values (`sql.case.keywords`, …).
    #[serde(default)]
    values: BTreeMap<String, Value>,
}

/// `{ text, dialect, values }` → the formatted text.
#[wasm_bindgen]
pub fn sql_format(json: &str) -> String {
    run(json, |r: FormatRequest| {
        forge_sqlcode::format(&r.text, r.dialect, &SqlStyle::from_values(&r.values))
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParamsRequest {
    text: String,
    #[serde(default)]
    dialect: SqlDialect,
    #[serde(default)]
    patterns: ParamPatterns,
    #[serde(default)]
    values: BTreeMap<String, String>,
}

/// `{ text, dialect, patterns }` → `ParamsAnswer`.
#[wasm_bindgen]
pub fn sql_params(json: &str) -> String {
    run(json, |r: ParamsRequest| {
        api::params(&r.text, r.dialect, &r.patterns)
    })
}

/// `{ text, dialect, patterns, values }` → the text with values, or `{ error }`.
#[wasm_bindgen]
pub fn sql_substitute(json: &str) -> String {
    match serde_json::from_str::<ParamsRequest>(json) {
        Ok(r) => {
            match forge_sqlcode::params::substitute(&r.text, r.dialect, &r.patterns, &r.values) {
                Ok(s) => to_json(&s),
                Err(missing) => {
                    to_json(&json!({ "error": format!("No value for {}", missing.join(", ")) }))
                }
            }
        }
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryRequest {
    #[serde(default)]
    history: History,
    entry: Option<HistoryEntry>,
    #[serde(default)]
    query: HistoryQuery,
    #[serde(default)]
    limit: Option<usize>,
}

/// `{ history, entry, limit }` → the history with the entry added.
#[wasm_bindgen]
pub fn sql_history_add(json: &str) -> String {
    run(json, |mut r: HistoryRequest| {
        if let Some(e) = r.entry {
            r.history
                .add(e, r.limit.unwrap_or(forge_sqlcode::history::DEFAULT_LIMIT));
        }
        r.history
    })
}

/// `{ history, query }` → the matching entries.
#[wasm_bindgen]
pub fn sql_history_search(json: &str) -> String {
    run(json, |r: HistoryRequest| r.history.search(&r.query))
}

#[derive(Deserialize)]
struct PlanRequest {
    text: String,
    #[serde(default)]
    dialect: SqlDialect,
}

/// `{ text, dialect }` → the statements Run SQL Script executes.
#[wasm_bindgen]
pub fn sql_plan_run(json: &str) -> String {
    run(json, |r: PlanRequest| api::plan_run(&r.text, r.dialect))
}
