//! Database objects for the browser-mode backend, mirroring `commands/db_objects.rs`: the
//! table dialogs' previews, Drop / Rename, Generate DDL (formatted), import previews and
//! batches, Excel export and the dump command lines.

use base64::Engine;
use forge_dbmodel::design::{self, CreateOptions, DropRequest, TableSpec};
use forge_dbmodel::script::{generate_ddl, DdlObject, DdlOptions};
use forge_dbmodel::DbModel;
use forge_dbtransfer::dump::{self, DumpRequest, DumpTarget};
use forge_dbtransfer::insert::{batches, ImportPlan};
use forge_dbtransfer::ReadOptions;
use forge_sql::Dialect;
use forge_sqlcode::{SqlDialect, SqlStyle};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn run<T: DeserializeOwned, R: serde::Serialize>(
    json: &str,
    f: impl FnOnce(T) -> Result<R, String>,
) -> String {
    match serde_json::from_str::<T>(json) {
        Ok(v) => match f(v) {
            Ok(r) => to_json(&r),
            Err(e) => to_json(&json!({ "error": e })),
        },
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

fn bytes(data: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("invalid file data: {e}"))
}

#[derive(Deserialize)]
struct DialectRequest {
    dialect: Dialect,
}

/// Mirrors `db_design_info`: `{ dialect }` → `DesignDialect`.
#[wasm_bindgen]
pub fn dbobjects_design_info(json: &str) -> String {
    run(json, |r: DialectRequest| {
        Ok(design::dialect_info(r.dialect))
    })
}

#[derive(Deserialize)]
struct SpecRequest {
    schema: String,
    table: forge_dbmodel::TableInfo,
}

/// Mirrors `db_design_load` with the mock's table: `{ schema, table }` → `TableSpec`.
#[wasm_bindgen]
pub fn dbobjects_design_spec(json: &str) -> String {
    run(json, |r: SpecRequest| {
        Ok(TableSpec::from_table(&r.schema, &r.table))
    })
}

#[derive(Deserialize)]
struct PreviewRequest {
    dialect: Dialect,
    #[serde(default)]
    old: Option<TableSpec>,
    table: TableSpec,
    #[serde(default)]
    options: CreateOptions,
}

/// Mirrors `db_design_preview`: `{ dialect, old?, table, options? }` → `{ sql, statements, warnings }`.
#[wasm_bindgen]
pub fn dbobjects_design_preview(json: &str) -> String {
    run(json, |r: PreviewRequest| {
        Ok(match r.old {
            Some(old) => {
                let plan = design::alter_table(r.dialect, &old, &r.table);
                json!({ "sql": plan.script(), "statements": plan.statements, "warnings": plan.warnings })
            }
            None => {
                let st = design::create_statements(r.dialect, &r.table, r.options, &[]);
                json!({ "sql": design::join(&st), "statements": st, "warnings": [] })
            }
        })
    })
}

#[derive(Deserialize)]
struct DropSqlRequest {
    dialect: Dialect,
    requests: Vec<DropRequest>,
}

/// Mirrors `db_drop_sql`.
#[wasm_bindgen]
pub fn dbobjects_drop_sql(json: &str) -> String {
    run(json, |r: DropSqlRequest| {
        let st: Vec<String> = r
            .requests
            .iter()
            .flat_map(|q| design::drop_statements(r.dialect, q))
            .collect();
        Ok(design::join(&st))
    })
}

#[derive(Deserialize)]
struct RenameRequest {
    dialect: Dialect,
    schema: String,
    table: String,
    #[serde(default)]
    column: Option<String>,
    to: String,
}

/// Mirrors `db_rename_sql`.
#[wasm_bindgen]
pub fn dbobjects_rename_sql(json: &str) -> String {
    run(json, |r: RenameRequest| {
        Ok(match r.column {
            Some(c) => design::rename_column(r.dialect, &r.schema, &r.table, &c, &r.to),
            None => design::rename_table(r.dialect, &r.schema, &r.table, &r.to),
        })
    })
}

#[derive(Deserialize)]
struct DdlRequest {
    model: DbModel,
    objects: Vec<DdlObject>,
    #[serde(default)]
    options: Option<DdlOptions>,
    /// Code style values (`sql.case.keywords`, …).
    #[serde(default)]
    values: BTreeMap<String, Value>,
}

/// Mirrors `db_generate_ddl` with the mock's model: `{ model, objects, options?, values }`.
#[wasm_bindgen]
pub fn dbobjects_generate_ddl(json: &str) -> String {
    run(json, |r: DdlRequest| {
        let script = generate_ddl(&r.model, &r.objects, r.options.unwrap_or_default());
        let mut out = forge_sqlcode::format(
            &script,
            SqlDialect::from_db(r.model.dialect),
            &SqlStyle::from_values(&r.values),
        );
        if !out.ends_with('\n') {
            out.push('\n');
        }
        Ok(out)
    })
}

#[derive(Deserialize)]
struct ImportPreviewRequest {
    data: String,
    options: ReadOptions,
    #[serde(default)]
    rows: Option<usize>,
    #[serde(default)]
    dialect: Option<Dialect>,
}

/// Mirrors `db_import_preview` for file bytes: `{ data (base64), options, rows?, dialect? }`.
#[wasm_bindgen]
pub fn dbobjects_import_preview(json: &str) -> String {
    run(json, |r: ImportPreviewRequest| {
        forge_dbtransfer::preview(
            &bytes(&r.data)?,
            &r.options,
            r.rows.unwrap_or(50),
            r.dialect,
        )
    })
}

#[derive(Deserialize)]
struct ImportBatchesRequest {
    dialect: Dialect,
    data: String,
    options: ReadOptions,
    plan: ImportPlan,
}

/// The statements of an import (the mock runs them on the demo database):
/// `{ dialect, data, options, plan }` → `{ create: string[], batches: Batch[], rows }`.
#[wasm_bindgen]
pub fn dbobjects_import_batches(json: &str) -> String {
    run(json, |r: ImportBatchesRequest| {
        let t = forge_dbtransfer::read(&bytes(&r.data)?, &r.options)?;
        let create = r
            .plan
            .create
            .as_ref()
            .map(|c| design::create_statements(r.dialect, c, CreateOptions::default(), &[]))
            .unwrap_or_default();
        Ok(json!({
            "create": create,
            "batches": batches(r.dialect, &r.plan, &t.rows),
            "rows": t.rows.len(),
        }))
    })
}

#[derive(Deserialize)]
struct XlsxRequest {
    sheet: String,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

/// Mirrors `db_export_xlsx` without a path: the workbook as base64.
#[wasm_bindgen]
pub fn dbobjects_export_xlsx(json: &str) -> String {
    run(json, |r: XlsxRequest| {
        Ok(
            base64::engine::general_purpose::STANDARD.encode(forge_dbtransfer::xlsx::workbook(
                &r.sheet, &r.columns, &r.rows,
            )),
        )
    })
}

#[derive(Deserialize)]
struct ToolsRequest {
    driver: String,
}

/// Mirrors `db_dump_tools`: `{ driver }`.
#[wasm_bindgen]
pub fn dbobjects_dump_tools(json: &str) -> String {
    run(json, |r: ToolsRequest| Ok(dump::tools_for(&r.driver)))
}

#[derive(Deserialize)]
struct DumpCommandRequest {
    request: DumpRequest,
    target: DumpTarget,
}

/// Mirrors `db_dump_command`: `{ request, target }` → `{ display, program, args }`.
#[wasm_bindgen]
pub fn dbobjects_dump_command(json: &str) -> String {
    run(json, |r: DumpCommandRequest| {
        let c = dump::command(&r.request, &r.target)?;
        Ok(json!({ "display": c.display(), "program": c.program, "args": c.args }))
    })
}
