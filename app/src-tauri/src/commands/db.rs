//! Database tools IPC (`forge_db`): data sources (SSH tunnels, SSL), introspection,
//! consoles and their sessions, the read-only / production guard, table editing, the Redis
//! keys tree and export. Events: `db://dataSources` (the full list after a change),
//! `db://session` (a console's [`SessionInfo`] when it starts/stops running or its
//! transaction changes) and `db://sessions` (all open sessions, for the Sessions view).

use std::path::PathBuf;
use std::sync::Arc;

use forge_core::{ForgeError, Result};
use forge_db::{
    ApplyResult, DataSource, DataSourceInfo, DbError, DbResult, DbService, DbTunnel,
    DetectedDataSource, Dialect, ExportFormat, Guard, KeyringStore, ProjectContext, QueryPage,
    RedisKeys, RedisValue, RowChange, SchemaInfo, SchemaName, SecretUpdate, SessionInfo,
    SessionMode, StatementResult, TableDataRequest, TableInfo, TestResult, TunnelEndpoint,
};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

pub const DATA_SOURCES_EVENT: &str = "db://dataSources";
pub const SESSION_EVENT: &str = "db://session";
pub const SESSIONS_EVENT: &str = "db://sessions";

/// Managed state: one [`DbService`] for the app.
pub struct DbState {
    pub service: Arc<DbService>,
}

impl DbState {
    pub fn load() -> Self {
        Self {
            service: Arc::new(DbService::new(
                forge_db::config::global_file(),
                Arc::new(KeyringStore),
            )),
        }
    }
}

/// Load the data sources of a newly opened project (in the background).
pub fn project_opened(app: &AppHandle, root: PathBuf) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let svc = app.state::<DbState>().service.clone();
        if let Err(e) = svc.set_project(Some(root)).await {
            tracing::warn!("data sources not loaded: {e}");
        }
        emit_sources(&app, &svc);
    });
}

/// Project data sources of a remote project connect through its SSH connection.
struct SshDbTunnel {
    tunnels: Arc<dyn forge_backend::Tunnels>,
    label: String,
}

impl DbTunnel for SshDbTunnel {
    fn open(
        &self,
        host: String,
        port: u16,
    ) -> futures_util::future::BoxFuture<'static, DbResult<TunnelEndpoint>> {
        let forward = self.tunnels.forward_local(host, port);
        Box::pin(async move {
            let forward = forward
                .await
                .map_err(|e| DbError::Io(format!("SSH tunnel: {e}")))?;
            Ok(TunnelEndpoint::new(forward.port, forward))
        })
    }

    fn label(&self) -> String {
        self.label.clone()
    }
}

/// Load the data sources of a newly opened remote project (its `.forge/dataSources.toml`
/// and `.env` are read on the host).
pub fn remote_project_opened(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Ok(backend) = state.backend() else {
            return;
        };
        let ctx = ProjectContext {
            root: backend.info().root.clone(),
            files: Arc::new(forge_backend::BackendFiles(backend.clone())),
            tunnel: backend.tunnels().map(|tunnels| {
                Arc::new(SshDbTunnel {
                    tunnels,
                    label: backend
                        .info()
                        .remote
                        .as_ref()
                        .map(|r| r.label())
                        .unwrap_or_default(),
                }) as Arc<dyn DbTunnel>
            }),
        };
        let svc = app.state::<DbState>().service.clone();
        // Reading the files waits for the host.
        let loaded = tauri::async_runtime::spawn_blocking({
            let svc = svc.clone();
            move || tauri::async_runtime::block_on(svc.set_project_context(ctx))
        })
        .await;
        match loaded {
            Ok(Err(e)) => tracing::warn!("data sources not loaded: {e}"),
            Err(e) => tracing::warn!("data sources not loaded: {e}"),
            Ok(Ok(())) => {}
        }
        emit_sources(&app, &svc);
    });
}

fn emit_sources(app: &AppHandle, svc: &DbService) {
    if let Err(e) = app.emit(DATA_SOURCES_EVENT, svc.list()) {
        tracing::warn!("cannot emit {DATA_SOURCES_EVENT}: {e}");
    }
}

fn emit_session(app: &AppHandle, info: &SessionInfo) {
    if let Err(e) = app.emit(SESSION_EVENT, info) {
        tracing::warn!("cannot emit {SESSION_EVENT}: {e}");
    }
    emit_sessions(app);
}

fn emit_sessions(app: &AppHandle) {
    let list = app.state::<DbState>().service.sessions();
    if let Err(e) = app.emit(SESSIONS_EVENT, list) {
        tracing::warn!("cannot emit {SESSIONS_EVENT}: {e}");
    }
}

#[tauri::command]
pub fn db_data_sources_list(state: State<'_, DbState>) -> Vec<DataSourceInfo> {
    state.service.list()
}

/// Create or update a data source. `password` and the SSH/SSL `secrets`: omitted keeps the
/// stored one, `""` clears it.
#[tauri::command]
pub async fn db_data_source_save(
    source: DataSource,
    password: Option<String>,
    secrets: Option<SecretUpdate>,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<DataSourceInfo> {
    let info = state
        .service
        .save_with(source, password, secrets.unwrap_or_default())
        .await?;
    emit_sources(&app, &state.service);
    Ok(info)
}

#[tauri::command]
pub async fn db_data_source_delete(
    id: String,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<()> {
    state.service.delete(&id).await?;
    emit_sources(&app, &state.service);
    Ok(())
}

/// "Test Connection" with the dialog's unsaved settings; `accept_host_key` is the SSH host
/// key fingerprint the user chose to trust.
#[tauri::command]
pub async fn db_test_connection(
    source: DataSource,
    password: Option<String>,
    secrets: Option<SecretUpdate>,
    accept_host_key: Option<String>,
    state: State<'_, DbState>,
) -> Result<TestResult> {
    Ok(state
        .service
        .test_with(
            &source,
            password,
            secrets.unwrap_or_default(),
            accept_host_key,
        )
        .await)
}

#[tauri::command]
pub async fn db_disconnect(id: String, app: AppHandle, state: State<'_, DbState>) -> Result<()> {
    state.service.disconnect(&id).await;
    emit_sessions(&app);
    Ok(())
}

/// Schemas of a data source (connects on first use).
#[tauri::command]
pub async fn db_schemas(id: String, state: State<'_, DbState>) -> Result<Vec<SchemaName>> {
    Ok(state.service.schemas(&id).await?)
}

#[tauri::command]
pub async fn db_introspect(
    id: String,
    schema: String,
    state: State<'_, DbState>,
) -> Result<SchemaInfo> {
    Ok(state.service.introspect(&id, &schema).await?)
}

#[tauri::command]
pub async fn db_table_info(
    id: String,
    schema: String,
    table: String,
    state: State<'_, DbState>,
) -> Result<TableInfo> {
    Ok(state.service.table_info(&id, &schema, &table).await?)
}

#[tauri::command]
pub async fn db_table_ddl(
    id: String,
    schema: String,
    table: String,
    state: State<'_, DbState>,
) -> Result<String> {
    Ok(state.service.table_ddl(&id, &schema, &table).await?)
}

#[tauri::command]
pub async fn db_table_data(
    id: String,
    request: TableDataRequest,
    state: State<'_, DbState>,
) -> Result<QueryPage> {
    Ok(state.service.table_data(&id, &request).await?)
}

/// Submit the data editor's pending changes (one transaction; refused without a PK).
#[tauri::command]
pub async fn db_apply_changes(
    id: String,
    schema: String,
    table: String,
    changes: Vec<RowChange>,
    state: State<'_, DbState>,
) -> Result<ApplyResult> {
    Ok(state
        .service
        .apply_changes(&id, &schema, &table, &changes)
        .await?)
}

/// Open a console session: its own connection, or with `shared` the data source's shared
/// session.
#[tauri::command]
pub async fn db_session_open(
    id: String,
    shared: Option<bool>,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<SessionInfo> {
    let mode = if shared.unwrap_or(false) {
        SessionMode::Shared
    } else {
        SessionMode::Own
    };
    let info = state.service.open_session_with(&id, mode).await?;
    emit_sessions(&app);
    Ok(info)
}

#[tauri::command]
pub async fn db_session_close(
    session_id: String,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<()> {
    state.service.close_session(&session_id).await?;
    emit_sessions(&app);
    Ok(())
}

/// Open sessions for the Sessions view.
#[tauri::command]
pub fn db_sessions_list(state: State<'_, DbState>) -> Vec<SessionInfo> {
    state.service.sessions()
}

/// The Sessions view's "Close Session": closes the connection whatever console uses it.
#[tauri::command]
pub async fn db_session_terminate(
    session_id: String,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<()> {
    state.service.terminate_session(&session_id).await?;
    emit_sessions(&app);
    Ok(())
}

/// What the console must do before running `sql`: refuse it (read-only data source) or ask
/// (UPDATE/DELETE without WHERE on a production data source).
#[tauri::command]
pub fn db_guard(id: String, sql: String, state: State<'_, DbState>) -> Result<Guard> {
    Ok(state.service.guard(&id, &sql)?)
}

/// Run a script in a console; returns one result per statement (stops at an error).
/// `confirmed`: the user agreed to the production warning of `db_guard`.
#[tauri::command]
pub async fn db_execute(
    session_id: String,
    sql: String,
    page_size: Option<u32>,
    confirmed: Option<bool>,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<Vec<StatementResult>> {
    let svc = state.service.clone();
    let session = svc.session(&session_id)?;
    let mut running = session.info();
    running.running = true;
    emit_session(&app, &running);
    let res = svc
        .execute_with(&session_id, &sql, page_size, confirmed.unwrap_or(false))
        .await;
    emit_session(&app, &session.info());
    Ok(res?)
}

/// Keys of a Redis database (`db0`) matching a pattern, with type and TTL.
#[tauri::command]
pub async fn db_redis_keys(
    id: String,
    schema: String,
    pattern: String,
    limit: Option<u32>,
    state: State<'_, DbState>,
) -> Result<RedisKeys> {
    Ok(state
        .service
        .redis_keys(&id, &schema, &pattern, limit)
        .await?)
}

/// A Redis key's value for the value viewer.
#[tauri::command]
pub async fn db_redis_value(
    id: String,
    schema: String,
    key: String,
    state: State<'_, DbState>,
) -> Result<RedisValue> {
    Ok(state.service.redis_value(&id, &schema, &key).await?)
}

#[tauri::command]
pub async fn db_fetch(
    session_id: String,
    sql: String,
    offset: u64,
    limit: Option<u32>,
    state: State<'_, DbState>,
) -> Result<QueryPage> {
    Ok(state
        .service
        .fetch(&session_id, &sql, offset, limit)
        .await?)
}

#[tauri::command]
pub async fn db_set_auto_commit(
    session_id: String,
    auto_commit: bool,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<SessionInfo> {
    let info = state
        .service
        .set_auto_commit(&session_id, auto_commit)
        .await?;
    emit_session(&app, &info);
    Ok(info)
}

#[tauri::command]
pub async fn db_commit(
    session_id: String,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<SessionInfo> {
    let info = state.service.commit(&session_id).await?;
    emit_session(&app, &info);
    Ok(info)
}

#[tauri::command]
pub async fn db_rollback(
    session_id: String,
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<SessionInfo> {
    let info = state.service.rollback(&session_id).await?;
    emit_session(&app, &info);
    Ok(info)
}

#[tauri::command]
pub async fn db_cancel(session_id: String, state: State<'_, DbState>) -> Result<()> {
    Ok(state.service.cancel(&session_id).await?)
}

/// Data sources configured in the project's `.env` that are not added yet.
#[tauri::command]
pub async fn db_detect_env(state: State<'_, DbState>) -> Result<Vec<DetectedDataSource>> {
    // A remote project's `.env` is read on its host.
    let svc = state.service.clone();
    crate::commands::workspace::blocking(move || Ok(svc.detect_env())).await
}

#[tauri::command]
pub async fn db_add_from_env(
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<Vec<DataSourceInfo>> {
    let added = state.service.add_from_env().await?;
    emit_sources(&app, &state.service);
    Ok(added)
}

/// Render rows as CSV/JSON/SQL; with `path` the text is also written to that file.
#[tauri::command]
pub async fn db_export(
    format: ExportFormat,
    dialect: Dialect,
    table: Option<String>,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    path: Option<String>,
) -> Result<String> {
    let text = forge_db::export(format, dialect, table.as_deref(), &columns, &rows);
    if let Some(p) = path {
        tokio::fs::write(&p, &text)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?;
    }
    Ok(text)
}

/// What Ctrl+Enter runs in a console: the selection, or the statement at the caret (or the
/// closest one before it). Offsets in and out are editor positions (UTF-16).
#[tauri::command]
pub fn db_statement_at(
    sql: String,
    from: usize,
    to: usize,
    dialect: Dialect,
) -> Option<forge_db::split::Statement> {
    forge_db::split::statement_at(&sql, from, to, dialect)
}
