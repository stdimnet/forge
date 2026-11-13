//! SQL editor IPC (`forge_sqlcode`): dialects per file and folder, the SQL inspections,
//! Go to Declaration / Find Usages / alias rename, the formatter, user parameters, the query
//! history (one JSON file per data source in the user data dir) and Run SQL Script, which
//! executes a file's statements in order on its own session of a data source and streams
//! `sql://runFile` events. Names resolve against the data source's cached model (or a DDL
//! data source's files), never by connecting while the user types.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use forge_core::{ForgeError, Result};
use forge_sqlcode::api::{
    self, AnalyzeAnswer, DeclarationAnswer, ParamsAnswer, RenameAnswer, SqlContext, UsagesAnswer,
};
use forge_sqlcode::history::{self, History, HistoryEntry, HistoryQuery};
use forge_sqlcode::lang::{dialects, DialectInfo};
use forge_sqlcode::{
    data_source_for, dialect_for, DialectMapping, ParamPatterns, SqlDialect, SqlStyle,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use super::code_style::code_style_values;
use super::db::DbState;
use crate::state::AppState;

pub const RUN_FILE_EVENT: &str = "sql://runFile";

fn fail(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn setting<T: serde::de::DeserializeOwned>(state: &AppState, key: &str) -> Option<T> {
    state.read_settings().get_as::<T>(key)
}

fn mappings(state: &AppState) -> Vec<DialectMapping> {
    let raw: Option<Value> = setting(state, "sql.dialect.mappings");
    raw.and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// The project default: the project's dialect, else the global one.
fn default_dialect(state: &AppState) -> SqlDialect {
    let project: String = setting(state, "sql.dialect.project").unwrap_or_default();
    if !project.is_empty() {
        return SqlDialect::parse(&project);
    }
    SqlDialect::parse(&setting::<String>(state, "sql.dialect.default").unwrap_or_default())
}

/// What a file or console resolves against.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SqlTarget {
    pub text: String,
    /// Project-relative path of a file (consoles have none).
    pub path: Option<String>,
    /// A console's data source, or a file's chosen one.
    pub data_source_id: Option<String>,
    /// Overrides the file's dialect (consoles: their data source's).
    pub dialect: Option<SqlDialect>,
    pub default_schema: Option<String>,
}

/// The dialect and data source of a file or console.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSql {
    pub dialect: SqlDialect,
    /// Where the dialect comes from: `dataSource`, `mapping`, `project` or `global`.
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,
}

fn file_sql(state: &AppState, db: &DbState, t: &SqlTarget) -> FileSql {
    let maps = mappings(state);
    let ds = t
        .data_source_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| t.path.as_deref().and_then(|p| data_source_for(p, &maps)));
    if let Some(d) = t.dialect {
        return FileSql {
            dialect: d,
            source: "dataSource",
            data_source_id: ds,
        };
    }
    if t.path.is_none() {
        if let Some(src) = ds.as_deref().and_then(|id| db.service.get(id).ok()) {
            return FileSql {
                dialect: SqlDialect::from_db(src.dialect()),
                source: "dataSource",
                data_source_id: ds,
            };
        }
    }
    let project: String = setting(state, "sql.dialect.project").unwrap_or_default();
    let fallback = (
        default_dialect(state),
        if project.is_empty() {
            "global"
        } else {
            "project"
        },
    );
    let (dialect, source) = match t.path.as_deref() {
        // A mapping applies when the answer does not depend on the fallback.
        Some(p)
            if dialect_for(p, SqlDialect::Oracle, &maps)
                == dialect_for(p, SqlDialect::Generic, &maps) =>
        {
            (dialect_for(p, SqlDialect::Generic, &maps), "mapping")
        }
        _ => fallback,
    };
    FileSql {
        dialect,
        source,
        data_source_id: ds,
    }
}

fn context(state: &AppState, db: &DbState, t: SqlTarget) -> (SqlContext, FileSql) {
    let info = file_sql(state, db, &t);
    let model = info
        .data_source_id
        .as_deref()
        .and_then(|id| db.service.model_if_cached(id).ok().flatten());
    (
        SqlContext {
            text: t.text,
            dialect: info.dialect,
            model,
            default_schema: t.default_schema,
        },
        info,
    )
}

/// Every dialect with its keywords and types.
#[tauri::command]
pub fn sql_dialects() -> Vec<DialectInfo> {
    dialects()
}

/// The dialect (and attached data source) of a file or console.
#[tauri::command]
pub fn sql_file_info(
    request: SqlTarget,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> FileSql {
    file_sql(&state, &db, &request)
}

/// The problems of the SQL inspections.
#[tauri::command]
pub fn sql_analyze(
    request: SqlTarget,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> AnalyzeAnswer {
    let (ctx, _) = context(&state, &db, request);
    api::analyze_text(&ctx)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtRequest {
    #[serde(flatten)]
    pub target: SqlTarget,
    /// UTF-16 offset of the caret.
    pub offset: usize,
}

/// Go to Declaration: the symbol at the caret, with its place in a DDL data source's files.
#[tauri::command]
pub fn sql_declaration(
    request: AtRequest,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Option<DeclarationAnswer> {
    let (ctx, info) = context(&state, &db, request.target);
    let ddl = info
        .data_source_id
        .as_deref()
        .and_then(|id| db.service.get(id).ok())
        .filter(|ds| ds.driver == forge_db::Driver::Ddl)
        .and_then(|ds| db.service.ddl_files(&ds).ok())
        .unwrap_or_default();
    api::declaration(&ctx, request.offset, &ddl)
}

/// Find Usages of the symbol at the caret in the file.
#[tauri::command]
pub fn sql_usages(
    request: AtRequest,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Option<UsagesAnswer> {
    let (ctx, _) = context(&state, &db, request.target);
    api::usages(&ctx, request.offset)
}

/// The occurrences an in-place rename of an alias or CTE edits.
#[tauri::command]
pub fn sql_rename(
    request: AtRequest,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> RenameAnswer {
    let (ctx, _) = context(&state, &db, request.target);
    api::rename(&ctx, request.offset)
}

/// Reformat Code with the selected code style scheme's SQL options and the file's dialect.
#[tauri::command]
pub fn sql_format(
    request: SqlTarget,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> String {
    let info = file_sql(&state, &db, &request);
    let style = SqlStyle::from_values(&code_style_values(&state));
    forge_sqlcode::format(&request.text, info.dialect, &style)
}

fn patterns(state: &AppState) -> ParamPatterns {
    let d = ParamPatterns::default();
    let b = |k: &str, def: bool| {
        setting::<bool>(state, &format!("database.parameters.{k}")).unwrap_or(def)
    };
    ParamPatterns {
        question: b("question", d.question),
        colon: b("colon", d.colon),
        dollar: b("dollar", d.dollar),
        dollar_brace: b("dollarBrace", d.dollar_brace),
        hash_brace: b("hashBrace", d.hash_brace),
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamsRequest {
    pub text: String,
    #[serde(default)]
    pub dialect: SqlDialect,
    /// Values by parameter name (for substitution).
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

/// The user parameters of a statement (the rows of the parameters dialog).
#[tauri::command]
pub fn sql_params(request: ParamsRequest, state: State<'_, AppState>) -> ParamsAnswer {
    api::params(&request.text, request.dialect, &patterns(&state))
}

/// The statement with the parameter values filled in.
#[tauri::command]
pub fn sql_substitute(request: ParamsRequest, state: State<'_, AppState>) -> Result<String> {
    forge_sqlcode::params::substitute(
        &request.text,
        request.dialect,
        &patterns(&state),
        &request.values,
    )
    .map_err(|missing| fail(format!("No value for {}", missing.join(", "))))
}

// ---- query history -------------------------------------------------------------------------

fn history_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("sql-history"))
}

fn history_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn load_history(data_source: &str) -> History {
    history_dir()
        .and_then(|d| std::fs::read_to_string(d.join(history::file_name(data_source))).ok())
        .map(|t| History::parse(&t))
        .unwrap_or_default()
}

fn add_history(entry: HistoryEntry, limit: usize) -> Result<()> {
    let Some(dir) = history_dir() else {
        return Ok(());
    };
    let _guard = history_lock().lock().expect("poisoned");
    let mut h = load_history(&entry.data_source);
    let file = dir.join(history::file_name(&entry.data_source));
    h.add(entry, limit);
    std::fs::create_dir_all(&dir).map_err(fail)?;
    std::fs::write(file, h.to_json()).map_err(fail)
}

fn history_limit(state: &AppState) -> usize {
    setting::<usize>(state, "database.history.limit").unwrap_or(history::DEFAULT_LIMIT)
}

/// A data source's query history, newest first, filtered by words and console.
#[tauri::command]
pub fn sql_history_list(data_source_id: String, query: Option<HistoryQuery>) -> Vec<HistoryEntry> {
    load_history(&data_source_id).search(&query.unwrap_or_default())
}

/// Record an execution.
#[tauri::command]
pub fn sql_history_add(entry: HistoryEntry, state: State<'_, AppState>) -> Result<()> {
    add_history(entry, history_limit(&state))
}

/// Forget a data source's history.
#[tauri::command]
pub fn sql_history_clear(data_source_id: String) -> Result<()> {
    let _guard = history_lock().lock().expect("poisoned");
    if let Some(dir) = history_dir() {
        let file = dir.join(history::file_name(&data_source_id));
        if file.exists() {
            std::fs::remove_file(file).map_err(fail)?;
        }
    }
    Ok(())
}

// ---- Run SQL Script ------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFileRequest {
    pub run_id: String,
    /// Project-relative path of the `.sql` file.
    pub path: String,
    pub data_source_id: String,
    #[serde(default = "yes")]
    pub stop_on_error: bool,
    /// The user agreed to the production warning.
    #[serde(default)]
    pub confirmed: bool,
}

fn yes() -> bool {
    true
}

/// One `sql://runFile` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFileEvent {
    pub run_id: String,
    /// `started`, `statement` (one finished) or `finished`.
    pub kind: &'static str,
    pub index: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sql: String,
    pub line: u32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Rows returned (first page) or affected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<i64>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFileSummary {
    pub total: usize,
    pub executed: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub elapsed_ms: u64,
    /// The production guard wants a confirmation first (nothing ran).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_confirmation: Option<String>,
}

fn cancelled_runs() -> &'static Mutex<HashSet<String>> {
    static RUNS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    RUNS.get_or_init(Default::default)
}

fn run_sessions() -> &'static Mutex<HashMap<String, String>> {
    static S: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Run a `.sql` file's statements in order on a new session of a data source.
#[tauri::command]
pub async fn sql_run_file(
    request: RunFileRequest,
    app: AppHandle,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<RunFileSummary> {
    let text = state
        .backend()?
        .read_optional(&request.path)?
        .ok_or_else(|| fail(format!("{} not found", request.path)))?;
    let svc = db.service.clone();
    let ds = svc.get(&request.data_source_id).map_err(fail)?;
    if ds.driver == forge_db::Driver::Ddl {
        return Err(fail("DDL data sources cannot run SQL"));
    }
    let guard = svc.guard(&request.data_source_id, &text).map_err(fail)?;
    if let Some(blocked) = guard.blocked {
        return Err(fail(blocked));
    }
    if let (Some(confirm), false) = (guard.confirm, request.confirmed) {
        return Ok(RunFileSummary {
            needs_confirmation: Some(confirm),
            ..RunFileSummary::default()
        });
    }
    let dialect = SqlDialect::from_db(ds.dialect());
    let plan = api::plan_run(&text, dialect);
    let total = plan.len();
    let limit = history_limit(&state);
    let console = request
        .path
        .rsplit('/')
        .next()
        .unwrap_or(&request.path)
        .to_string();
    let emit = |e: RunFileEvent| {
        if let Err(err) = app.emit(RUN_FILE_EVENT, e) {
            tracing::warn!("run file event not sent: {err}");
        }
    };
    let event = |kind, index, sql: &str, line| RunFileEvent {
        run_id: request.run_id.clone(),
        kind,
        index,
        total,
        sql: sql.to_string(),
        line,
        ok: true,
        error: None,
        rows: None,
        elapsed_ms: 0,
    };
    emit(event("started", 0, "", 0));
    let session = svc
        .open_session(&request.data_source_id)
        .await
        .map_err(fail)?;
    run_sessions()
        .lock()
        .expect("poisoned")
        .insert(request.run_id.clone(), session.id.clone());
    let started = std::time::Instant::now();
    let mut summary = RunFileSummary {
        total,
        ..RunFileSummary::default()
    };
    for st in &plan {
        if cancelled_runs()
            .lock()
            .expect("poisoned")
            .remove(&request.run_id)
        {
            summary.cancelled = true;
            break;
        }
        let res = svc
            .execute_with(&session.id, &st.text, Some(100), true)
            .await;
        let mut ev = event("statement", st.index, &st.text, st.line);
        match res {
            Ok(results) => {
                let failed = results.iter().find_map(|r| r.error.clone());
                ev.elapsed_ms = results.iter().map(|r| r.elapsed_ms).sum();
                ev.rows = results.last().map(|r| {
                    if r.returns_rows {
                        r.result.rows.len() as i64
                    } else {
                        r.result.rows_affected as i64
                    }
                });
                ev.ok = failed.is_none();
                ev.error = failed;
            }
            Err(e) => {
                ev.ok = false;
                ev.error = Some(e.to_string());
            }
        }
        summary.executed += 1;
        let ok = ev.ok;
        if !ok {
            summary.failed += 1;
        }
        let _ = add_history(
            HistoryEntry {
                sql: st.text.clone(),
                time: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                duration_ms: ev.elapsed_ms,
                rows: ev.rows,
                data_source: request.data_source_id.clone(),
                console: console.clone(),
                error: ev.error.clone(),
            },
            limit,
        );
        emit(ev);
        if !ok && request.stop_on_error {
            break;
        }
    }
    run_sessions()
        .lock()
        .expect("poisoned")
        .remove(&request.run_id);
    cancelled_runs()
        .lock()
        .expect("poisoned")
        .remove(&request.run_id);
    let _ = svc.close_session(&session.id).await;
    summary.elapsed_ms = started.elapsed().as_millis() as u64;
    let mut done = event("finished", summary.executed, "", 0);
    done.ok = summary.failed == 0 && !summary.cancelled;
    done.elapsed_ms = summary.elapsed_ms;
    emit(done);
    Ok(summary)
}

/// Stop a Run SQL Script after the running statement (which is cancelled too).
#[tauri::command]
pub async fn sql_run_file_cancel(run_id: String, db: State<'_, DbState>) -> Result<()> {
    cancelled_runs()
        .lock()
        .expect("poisoned")
        .insert(run_id.clone());
    let session = run_sessions()
        .lock()
        .expect("poisoned")
        .get(&run_id)
        .cloned();
    if let Some(s) = session {
        let _ = db.service.cancel(&s).await;
    }
    Ok(())
}
