//! Database objects IPC: the Create/Modify Table dialogs (`forge_dbmodel::design`: CREATE
//! and rename-aware ALTER previews per dialect), Drop and Rename, scripts run with the
//! read-only / production checks, Generate DDL (`forge_dbmodel::script`, formatted with the
//! SQL code style), data import (`forge_dbtransfer`: CSV/TSV, Excel, JSON with progress
//! events `db://import`), Excel export, and dump / restore with the native tools in the
//! Run tool window (`run://<runId>` events, passwords in the tool's environment, through
//! the data source's SSH tunnel).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use forge_core::{ForgeError, Result};
use forge_db::model::design::{
    self, AlterPlan, CreateOptions, DesignDialect, DesignWarning, DropRequest, TableSpec,
};
use forge_db::model::script::{generate_ddl, DdlObject, DdlOptions};
use forge_db::{CancellationToken, Dialect, Guard, ImportProgress, ImportSummary, ScriptResult};
use forge_dbtransfer::dump::{self, DumpAction, DumpRequest, ToolChoice};
use forge_dbtransfer::{ImportPlan, Preview, ReadOptions};
use forge_process::{CommandLine, RunEvent};
use forge_sqlcode::{SqlDialect, SqlStyle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use super::code_style::code_style_values;
use super::db::DbState;
use super::run::{start_built, Built, RunStarted, RunState};
use crate::state::AppState;

pub const IMPORT_EVENT: &str = "db://import";

/// Running imports by id (for Cancel).
#[derive(Default)]
pub struct DbObjectsState {
    imports: Mutex<HashMap<String, CancellationToken>>,
}

/// The Create/Modify Table preview.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignPreview {
    pub sql: String,
    pub statements: Vec<String>,
    pub warnings: Vec<DesignWarning>,
}

/// Types, options and features of a dialect's table dialogs.
#[tauri::command]
pub fn db_design_info(dialect: Dialect) -> DesignDialect {
    design::dialect_info(dialect)
}

/// Modify Table: the dialog's model of an existing table (every column remembers its
/// name, so renames are renames).
#[tauri::command]
pub async fn db_design_load(
    id: String,
    schema: String,
    table: String,
    state: State<'_, DbState>,
) -> Result<TableSpec> {
    let t = state.service.table_info(&id, &schema, &table).await?;
    Ok(TableSpec::from_table(&schema, &t))
}

/// The script of the table dialogs: `CREATE TABLE` without `old`, else the `ALTER`
/// statements that turn `old` into `table` (with warnings for destructive changes).
#[tauri::command]
pub fn db_design_preview(
    dialect: Dialect,
    old: Option<TableSpec>,
    table: TableSpec,
    options: Option<CreateOptions>,
) -> DesignPreview {
    preview(dialect, old.as_ref(), &table, options.unwrap_or_default())
}

pub fn preview(
    d: Dialect,
    old: Option<&TableSpec>,
    table: &TableSpec,
    options: CreateOptions,
) -> DesignPreview {
    match old {
        Some(old) => {
            let AlterPlan {
                statements,
                warnings,
            } = design::alter_table(d, old, table);
            DesignPreview {
                sql: design::join(&statements),
                statements,
                warnings,
            }
        }
        None => {
            let statements = design::create_statements(d, table, options, &[]);
            DesignPreview {
                sql: design::join(&statements),
                statements,
                warnings: Vec::new(),
            }
        }
    }
}

/// `DROP …` of tree objects (with cascade).
#[tauri::command]
pub fn db_drop_sql(dialect: Dialect, requests: Vec<DropRequest>) -> String {
    let statements: Vec<String> = requests
        .iter()
        .flat_map(|r| design::drop_statements(dialect, r))
        .collect();
    design::join(&statements)
}

/// Rename a table (no `column`) or a column from the tree.
#[tauri::command]
pub fn db_rename_sql(
    dialect: Dialect,
    schema: String,
    table: String,
    column: Option<String>,
    to: String,
) -> String {
    match column {
        Some(c) => design::rename_column(dialect, &schema, &table, &c, &to),
        None => design::rename_table(dialect, &schema, &table, &to),
    }
}

/// What must happen before a script of the object dialogs runs: refused (read-only) or
/// confirmed (any change on a production data source).
#[tauri::command]
pub fn db_objects_guard(id: String, sql: String, state: State<'_, DbState>) -> Result<Guard> {
    Ok(state.service.objects_guard(&id, &sql)?)
}

/// Run a script of the object dialogs; `confirmed` after the production warning.
#[tauri::command]
pub async fn db_run_script(
    id: String,
    sql: String,
    confirmed: Option<bool>,
    state: State<'_, DbState>,
) -> Result<ScriptResult> {
    Ok(state
        .service
        .run_script(&id, &sql, confirmed.unwrap_or(false))
        .await?)
}

/// "SQL Scripts › Generate DDL": the selected objects in dependency order, formatted with
/// the SQL code style; with `path` also written to that file.
#[tauri::command]
pub async fn db_generate_ddl(
    id: String,
    objects: Vec<DdlObject>,
    options: Option<DdlOptions>,
    path: Option<String>,
    app: State<'_, AppState>,
    state: State<'_, DbState>,
) -> Result<String> {
    let model = state.service.model(&id, false).await?;
    let script = generate_ddl(&model, &objects, options.unwrap_or_default());
    let style = SqlStyle::from_values(&code_style_values(&app));
    let text = format_ddl(model.dialect, &script, &style);
    if let Some(p) = path {
        tokio::fs::write(&p, &text)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?;
    }
    Ok(text)
}

pub fn format_ddl(d: Dialect, script: &str, style: &SqlStyle) -> String {
    let mut out = forge_sqlcode::format(script, SqlDialect::from_db(d), style);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// A file to import: a path on this machine, or the file's bytes (base64).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSource {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
}

async fn source_bytes(src: &ImportSource) -> Result<Vec<u8>> {
    if let Some(d) = &src.data {
        return base64::engine::general_purpose::STANDARD
            .decode(d)
            .map_err(|e| ForgeError::Other(format!("invalid file data: {e}")));
    }
    let path = src
        .path
        .clone()
        .ok_or_else(|| ForgeError::Other("choose a file to import".into()))?;
    tokio::fs::read(&path)
        .await
        .map_err(|e| ForgeError::io(path.clone(), e))
}

/// The import dialog's preview: the first `rows` rows, the detected column kinds and, for
/// `dialect`, the SQL types of a new table.
#[tauri::command]
pub async fn db_import_preview(
    source: ImportSource,
    options: ReadOptions,
    rows: Option<usize>,
    dialect: Option<Dialect>,
) -> Result<Preview> {
    let bytes = source_bytes(&source).await?;
    tauri::async_runtime::spawn_blocking(move || {
        forge_dbtransfer::preview(&bytes, &options, rows.unwrap_or(50), dialect)
            .map_err(ForgeError::Other)
    })
    .await
    .map_err(|e| ForgeError::Other(e.to_string()))?
}

/// Import a file into a table; progress arrives as `db://import`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn db_import_run(
    id: String,
    import_id: String,
    source: ImportSource,
    options: ReadOptions,
    plan: ImportPlan,
    confirmed: Option<bool>,
    app: AppHandle,
    state: State<'_, DbState>,
    objects: State<'_, DbObjectsState>,
) -> Result<ImportSummary> {
    let bytes = source_bytes(&source).await?;
    let table =
        tauri::async_runtime::spawn_blocking(move || forge_dbtransfer::read(&bytes, &options))
            .await
            .map_err(|e| ForgeError::Other(e.to_string()))?
            .map_err(ForgeError::Other)?;
    let token = CancellationToken::new();
    objects
        .imports
        .lock()
        .expect("imports poisoned")
        .insert(import_id.clone(), token.clone());
    let emitter = app.clone();
    let res = state
        .service
        .import(
            &id,
            &import_id,
            &table,
            &plan,
            confirmed.unwrap_or(false),
            token,
            move |p: ImportProgress| {
                if let Err(e) = emitter.emit(IMPORT_EVENT, p) {
                    tracing::warn!("cannot emit {IMPORT_EVENT}: {e}");
                }
            },
        )
        .await;
    objects
        .imports
        .lock()
        .expect("imports poisoned")
        .remove(&import_id);
    Ok(res?)
}

#[tauri::command]
pub fn db_import_cancel(import_id: String, objects: State<'_, DbObjectsState>) {
    if let Some(t) = objects
        .imports
        .lock()
        .expect("imports poisoned")
        .get(&import_id)
    {
        t.cancel();
    }
}

/// "Export to Excel": writes `path`, or returns the workbook as base64 without one.
#[tauri::command]
pub async fn db_export_xlsx(
    sheet: String,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    path: Option<String>,
) -> Result<Option<String>> {
    let bytes = forge_dbtransfer::xlsx::workbook(&sheet, &columns, &rows);
    match path {
        Some(p) => {
            tokio::fs::write(&p, &bytes)
                .await
                .map_err(|e| ForgeError::io(p.clone(), e))?;
            Ok(None)
        }
        None => Ok(Some(
            base64::engine::general_purpose::STANDARD.encode(bytes),
        )),
    }
}

/// The dump / restore tools of a data source.
#[tauri::command]
pub fn db_dump_tools(id: String, state: State<'_, DbState>) -> Result<Vec<ToolChoice>> {
    let ds = state.service.get(&id)?;
    let driver = serde_json::to_value(ds.driver)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    Ok(dump::tools_for(&driver))
}

/// The command line a dump / restore would run (the dialog's preview; through an SSH
/// tunnel the tool connects to the tunnel's local port).
#[tauri::command]
pub fn db_dump_command(
    id: String,
    request: DumpRequest,
    state: State<'_, DbState>,
) -> Result<String> {
    let target = state.service.dump_target_settings(&id)?;
    let cmd = dump::command(&request, &target).map_err(ForgeError::Other)?;
    Ok(cmd.display())
}

/// Before a restore: an error on read-only data sources, the warning to confirm on
/// production ones.
#[tauri::command]
pub fn db_restore_guard(id: String, state: State<'_, DbState>) -> Result<Option<String>> {
    Ok(state.service.restore_guard(&id)?)
}

/// Start a dump or restore in the Run tool window as run `run_id`. Restores are refused
/// on read-only data sources and need `confirmed` on production ones.
#[tauri::command]
pub async fn db_dump_start(
    id: String,
    request: DumpRequest,
    run_id: String,
    confirmed: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
    runs: State<'_, RunState>,
) -> Result<RunStarted> {
    if request.action == DumpAction::Restore {
        if let (Some(confirm), false) = (db.service.restore_guard(&id)?, confirmed.unwrap_or(false))
        {
            return Err(forge_db::DbError::NeedsConfirmation(confirm).into());
        }
    }
    let (target, tunnel) = db.service.dump_target(&id).await?;
    let tool = dump::command(&request, &target).map_err(ForgeError::Other)?;
    let cwd = state.backend()?.info().root.clone();
    let cmd = CommandLine {
        program: tool.program,
        args: tool.args,
        cwd,
        env: tool.env,
    };
    // The SSH tunnel stays open until the tool exits.
    let tunnel = Arc::new(Mutex::new(tunnel));
    runs.observers.lock().expect("observers poisoned").insert(
        run_id.clone(),
        Arc::new(move |e: &RunEvent| {
            if matches!(e, RunEvent::Exited { .. }) {
                tunnel.lock().expect("tunnel poisoned").take();
            }
        }),
    );
    let built = Built {
        cmd,
        path_mappings: Vec::new(),
        debug_server: None,
        output_filters: Vec::new(),
        tests: Default::default(),
    };
    let started = start_built(run_id.clone(), built, false, app, &state, &runs).await;
    if started.is_err() {
        runs.observers
            .lock()
            .expect("observers poisoned")
            .remove(&run_id);
    }
    started
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_create_and_alter() {
        let table = TableSpec {
            schema: "public".into(),
            name: "t".into(),
            columns: vec![design::ColumnSpec {
                name: "id".into(),
                data_type: "bigint".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let p = preview(Dialect::Postgres, None, &table, CreateOptions::default());
        assert_eq!(
            p.sql,
            "CREATE TABLE \"public\".\"t\"\n(\n    \"id\" bigint NOT NULL\n);\n"
        );
        let mut renamed = table.clone();
        renamed.name = "u".into();
        let p = preview(
            Dialect::Postgres,
            Some(&table),
            &renamed,
            CreateOptions::default(),
        );
        assert_eq!(
            p.statements,
            ["ALTER TABLE \"public\".\"t\" RENAME TO \"u\";"]
        );
    }

    #[test]
    fn formats_generated_ddl() {
        let out = format_ddl(
            Dialect::Postgres,
            "create table \"t\" (\"id\" bigint not null);",
            &SqlStyle::default(),
        );
        assert!(out.starts_with("CREATE TABLE"), "{out}");
        assert!(out.ends_with('\n'));
    }
}
