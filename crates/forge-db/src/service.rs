//! [`DbService`]: data sources of the open project plus global ones, their pools, console
//! sessions with transaction state, table data paging and row editing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::config::{self, DataSource, Dialect, Scope};
use crate::conn::{cancel_statement, ColumnMeta, Conn, DbPool, RawResult};
use crate::edit::{change_sql, ChangeKind, RowChange};
use crate::env::{self, DetectedDataSource};
use crate::error::{DbError, DbResult};
use crate::introspect::{self, SchemaInfo, SchemaName, TableInfo};
use crate::secrets::SecretStore;
use crate::split::{is_query, split_statements, transaction_control};

pub const DEFAULT_PAGE_SIZE: usize = 500;

/// A data source as the UI sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceInfo {
    #[serde(flatten)]
    pub source: DataSource,
    pub scope: Scope,
    pub has_password: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub driver: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Result of one statement of a console run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResult {
    pub index: usize,
    pub sql: String,
    /// Byte offsets of the statement in the executed text.
    pub start: usize,
    pub end: usize,
    #[serde(flatten)]
    pub result: RawResult,
    pub returns_rows: bool,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A page of rows (table editor or a console result).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPage {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub has_more: bool,
    pub offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    pub elapsed_ms: u64,
    pub sql: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortKey {
    pub column: String,
    #[serde(default)]
    pub desc: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataRequest {
    pub schema: String,
    pub table: String,
    /// Raw `WHERE` condition typed into the filter row.
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub order: Vec<SortKey>,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub statements: Vec<String>,
    pub rows_affected: u64,
}

/// Transaction state of a console.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub data_source_id: String,
    pub auto_commit: bool,
    pub tx_open: bool,
    pub running: bool,
}

/// One console: a dedicated connection, so transactions span several executions.
pub struct Session {
    id: String,
    ds_id: String,
    dialect: Dialect,
    pool: DbPool,
    conn: tokio::sync::Mutex<Conn>,
    backend_id: Option<i64>,
    auto_commit: AtomicBool,
    tx_open: AtomicBool,
    running: Mutex<Option<CancellationToken>>,
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            data_source_id: self.ds_id.clone(),
            auto_commit: self.auto_commit.load(Ordering::SeqCst),
            tx_open: self.tx_open.load(Ordering::SeqCst),
            running: self.running.lock().expect("poisoned").is_some(),
        }
    }
}

#[derive(Default)]
struct Sources {
    root: Option<PathBuf>,
    list: Vec<DataSource>,
    /// Passwords of data sources with "Save: Never", kept for this run only.
    transient: HashMap<String, String>,
}

pub struct DbService {
    global_path: Option<PathBuf>,
    secrets: Arc<dyn SecretStore>,
    sources: Mutex<Sources>,
    pools: tokio::sync::Mutex<HashMap<String, DbPool>>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_session: AtomicU64,
}

fn elapsed_ms(t: Instant) -> u64 {
    t.elapsed().as_millis() as u64
}

fn strip_semicolon(sql: &str) -> &str {
    sql.trim().trim_end_matches(';').trim_end()
}

impl DbService {
    /// `global_path` is the global data sources file (`None` keeps them in memory).
    pub fn new(global_path: Option<PathBuf>, secrets: Arc<dyn SecretStore>) -> Self {
        let svc = Self {
            global_path,
            secrets,
            sources: Mutex::default(),
            pools: tokio::sync::Mutex::default(),
            sessions: Mutex::default(),
            next_session: AtomicU64::new(1),
        };
        if let Err(e) = svc.reload() {
            tracing::warn!("{e}");
        }
        svc
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Sources> {
        self.sources.lock().expect("data sources lock poisoned")
    }

    /// Switch to another project (or none) and load its data sources.
    pub async fn set_project(&self, root: Option<PathBuf>) -> DbResult<()> {
        self.lock().root = root;
        self.close_all().await;
        self.reload()
    }

    pub fn project_root(&self) -> Option<PathBuf> {
        self.lock().root.clone()
    }

    /// Re-read the project and global files.
    pub fn reload(&self) -> DbResult<()> {
        let root = self.project_root();
        let mut list = Vec::new();
        let mut first_err = None;
        if let Some(root) = &root {
            match config::load_file(&config::project_file(root), Scope::Project) {
                Ok(l) => list.extend(l),
                Err(e) => first_err = Some(e),
            }
        }
        if let Some(g) = &self.global_path {
            match config::load_file(g, Scope::Global) {
                Ok(l) => list.extend(l),
                Err(e) => first_err = first_err.or(Some(e)),
            }
        }
        self.lock().list = list;
        first_err.map_or(Ok(()), Err)
    }

    fn has_password(&self, ds: &DataSource) -> bool {
        self.lock().transient.contains_key(&ds.id)
            || matches!(self.secrets.get(&ds.id), Ok(Some(p)) if !p.is_empty())
    }

    fn info(&self, ds: DataSource) -> DataSourceInfo {
        DataSourceInfo {
            scope: ds.scope,
            has_password: ds.driver != config::Driver::Sqlite && self.has_password(&ds),
            summary: ds.summary(),
            source: ds,
        }
    }

    pub fn list(&self) -> Vec<DataSourceInfo> {
        let list = self.lock().list.clone();
        list.into_iter().map(|d| self.info(d)).collect()
    }

    pub fn get(&self, id: &str) -> DbResult<DataSource> {
        self.lock()
            .list
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| DbError::UnknownDataSource(id.into()))
    }

    fn persist(&self, scope: Scope) -> DbResult<()> {
        let (path, items) = {
            let s = self.lock();
            let items: Vec<_> = s
                .list
                .iter()
                .filter(|d| d.scope == scope)
                .cloned()
                .collect();
            let path = match scope {
                Scope::Project => s.root.as_deref().map(config::project_file),
                Scope::Global => self.global_path.clone(),
            };
            (path, items)
        };
        match path {
            Some(p) => config::save_file(&p, &items),
            None if scope == Scope::Project => Err(DbError::Config(
                "open a project to store project data sources".into(),
            )),
            None => Ok(()),
        }
    }

    /// Create or update a data source. `password`: `None` keeps the stored one, `Some("")`
    /// clears it.
    pub async fn save(
        &self,
        mut ds: DataSource,
        password: Option<String>,
    ) -> DbResult<DataSourceInfo> {
        if ds.id.trim().is_empty() {
            ds.id = config::new_id(if ds.name.is_empty() {
                ds.driver.label()
            } else {
                &ds.name
            });
        }
        if ds.name.trim().is_empty() {
            ds.name = env::default_name(&ds);
        }
        let old_scope = {
            let mut s = self.lock();
            let old = s.list.iter().position(|d| d.id == ds.id);
            let old_scope = old.map(|i| s.list[i].scope);
            match old {
                Some(i) => s.list[i] = ds.clone(),
                None => s.list.push(ds.clone()),
            }
            old_scope
        };
        self.persist(ds.scope)?;
        if let Some(prev) = old_scope.filter(|p| *p != ds.scope) {
            self.persist(prev)?;
        }
        if let Some(p) = password {
            if ds.options.save_password {
                self.lock().transient.remove(&ds.id);
                let stored = if p.is_empty() {
                    self.secrets.delete(&ds.id)
                } else {
                    self.secrets.set(&ds.id, &p)
                };
                if let Err(e) = stored {
                    // No keychain (e.g. headless Linux): keep it for this run.
                    tracing::warn!("{e}; password kept in memory only");
                    self.lock().transient.insert(ds.id.clone(), p);
                }
            } else {
                let _ = self.secrets.delete(&ds.id);
                self.lock().transient.insert(ds.id.clone(), p);
            }
        } else if !ds.options.save_password {
            let _ = self.secrets.delete(&ds.id);
        }
        self.drop_pool(&ds.id).await;
        Ok(self.info(ds))
    }

    pub async fn delete(&self, id: &str) -> DbResult<()> {
        let ds = self.get(id)?;
        self.lock().list.retain(|d| d.id != id);
        self.lock().transient.remove(id);
        self.persist(ds.scope)?;
        if let Err(e) = self.secrets.delete(id) {
            tracing::warn!("{e}");
        }
        self.drop_pool(id).await;
        Ok(())
    }

    fn password(&self, ds: &DataSource) -> Option<String> {
        if let Some(p) = self.lock().transient.get(&ds.id) {
            return Some(p.clone());
        }
        match self.secrets.get(&ds.id) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("{e}");
                None
            }
        }
    }

    /// Connect once with the given settings (the dialog's "Test Connection").
    /// `password: None` uses the stored password of `ds.id`.
    pub async fn test(&self, ds: &DataSource, password: Option<String>) -> TestResult {
        let started = Instant::now();
        let password = password.or_else(|| self.password(ds));
        let root = self.project_root();
        let res = async {
            let pool = DbPool::connect(ds, password.as_deref(), root.as_deref()).await?;
            let mut conn = pool.acquire().await?;
            let v = conn.server_version().await;
            drop(conn);
            pool.close().await;
            v
        }
        .await;
        TestResult {
            ok: res.is_ok(),
            driver: ds.driver.label().into(),
            latency_ms: elapsed_ms(started),
            server_version: res.as_ref().ok().cloned(),
            error: res.err().map(|e| e.to_string()),
        }
    }

    async fn drop_pool(&self, id: &str) {
        let pool = self.pools.lock().await.remove(id);
        if let Some(p) = pool {
            p.close().await;
        }
        self.sessions
            .lock()
            .expect("poisoned")
            .retain(|_, s| s.ds_id != id);
    }

    /// Close every pool and console (project switch, app exit).
    pub async fn close_all(&self) {
        self.sessions.lock().expect("poisoned").clear();
        let pools: Vec<_> = self.pools.lock().await.drain().map(|(_, p)| p).collect();
        for p in pools {
            p.close().await;
        }
    }

    /// The pool of a data source, connecting on first use.
    pub async fn pool(&self, id: &str) -> DbResult<DbPool> {
        let ds = self.get(id)?;
        let mut pools = self.pools.lock().await;
        if let Some(p) = pools.get(id) {
            return Ok(p.clone());
        }
        let root = self.project_root();
        let pool = DbPool::connect(&ds, self.password(&ds).as_deref(), root.as_deref()).await?;
        pools.insert(id.into(), pool.clone());
        Ok(pool)
    }

    /// Whether the data source has an open pool ("connected" in the tree).
    pub async fn is_connected(&self, id: &str) -> bool {
        self.pools.lock().await.contains_key(id)
    }

    /// Close the pool ("Disconnect").
    pub async fn disconnect(&self, id: &str) {
        self.drop_pool(id).await;
    }

    pub async fn schemas(&self, id: &str) -> DbResult<Vec<SchemaName>> {
        let mut conn = self.pool(id).await?.acquire().await?;
        introspect::schemas(&mut conn).await
    }

    pub async fn introspect(&self, id: &str, schema: &str) -> DbResult<SchemaInfo> {
        let mut conn = self.pool(id).await?.acquire().await?;
        introspect::schema(&mut conn, schema, None).await
    }

    pub async fn table_info(&self, id: &str, schema: &str, table: &str) -> DbResult<TableInfo> {
        let mut conn = self.pool(id).await?.acquire().await?;
        introspect::schema(&mut conn, schema, Some(table))
            .await?
            .tables
            .into_iter()
            .next()
            .ok_or_else(|| DbError::Sql(format!("table {schema}.{table} not found")))
    }

    pub async fn table_ddl(&self, id: &str, schema: &str, table: &str) -> DbResult<String> {
        let mut conn = self.pool(id).await?.acquire().await?;
        crate::ddl::table_ddl(&mut conn, schema, table).await
    }

    fn page_size(&self, ds: &DataSource, limit: Option<u32>) -> usize {
        limit
            .or(ds.options.page_size)
            .map(|l| l.clamp(1, 100_000) as usize)
            .unwrap_or(DEFAULT_PAGE_SIZE)
    }

    /// A page of a table with the grid's filter and sort.
    pub async fn table_data(&self, id: &str, req: &TableDataRequest) -> DbResult<QueryPage> {
        let ds = self.get(id)?;
        let limit = self.page_size(&ds, req.limit);
        let pool = self.pool(id).await?;
        let d = pool.dialect();
        let mut from = d.qualified(&req.schema, &req.table);
        if let Some(f) = req
            .filter
            .as_deref()
            .map(strip_semicolon)
            .filter(|f| !f.is_empty())
        {
            from.push_str(&format!(" WHERE {f}"));
        }
        let order = req
            .order
            .iter()
            .map(|k| {
                format!(
                    "{}{}",
                    d.quote_ident(&k.column),
                    if k.desc { " DESC" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = format!("SELECT * FROM {from}");
        if !order.is_empty() {
            sql.push_str(&format!(" ORDER BY {order}"));
        }
        let paged = format!("{sql} LIMIT {} OFFSET {}", limit + 1, req.offset);
        let started = Instant::now();
        let mut conn = pool.acquire().await?;
        let mut raw = conn.run(&paged, 0, limit).await?;
        let elapsed = elapsed_ms(started);
        if raw.columns.is_empty() {
            raw.columns = conn.describe(&sql).await.unwrap_or_default();
        }
        let total = if req.offset == 0 && !raw.has_more {
            Some(raw.rows.len() as u64)
        } else {
            conn.text_rows(&format!("SELECT COUNT(*) FROM {from}"))
                .await
                .ok()
                .and_then(|r| r.first()?.first()?.as_deref()?.parse().ok())
        };
        Ok(QueryPage {
            columns: raw.columns,
            rows: raw.rows,
            has_more: raw.has_more,
            offset: req.offset,
            total,
            elapsed_ms: elapsed,
            sql,
        })
    }

    /// Submit the grid's pending changes in one transaction.
    pub async fn apply_changes(
        &self,
        id: &str,
        schema: &str,
        table: &str,
        changes: &[RowChange],
    ) -> DbResult<ApplyResult> {
        let ds = self.get(id)?;
        if ds.options.read_only {
            return Err(DbError::Refused(format!("{} is read-only", ds.name)));
        }
        let pool = self.pool(id).await?;
        let d = pool.dialect();
        let mut conn = pool.acquire().await?;
        let info = introspect::schema(&mut conn, schema, Some(table))
            .await?
            .tables
            .into_iter()
            .next()
            .ok_or_else(|| DbError::Sql(format!("table {table} not found")))?;
        let statements = changes
            .iter()
            .map(|c| change_sql(d, schema, table, &info.primary_key, c))
            .collect::<DbResult<Vec<_>>>()?;
        conn.execute(d.begin()).await?;
        let mut total = 0;
        for (sql, change) in statements.iter().zip(changes) {
            match conn.execute(sql).await {
                Ok(n) if n > 1 && change.kind != ChangeKind::Insert => {
                    let _ = conn.execute("ROLLBACK").await;
                    return Err(DbError::Refused(format!(
                        "{sql} would change {n} rows; nothing was saved"
                    )));
                }
                Ok(n) => total += n,
                Err(e) => {
                    let _ = conn.execute("ROLLBACK").await;
                    return Err(e);
                }
            }
        }
        conn.execute("COMMIT").await?;
        Ok(ApplyResult {
            statements,
            rows_affected: total,
        })
    }

    // ---- consoles ---------------------------------------------------------------------

    /// Open a console session with its own connection.
    pub async fn open_session(&self, id: &str) -> DbResult<SessionInfo> {
        let ds = self.get(id)?;
        let pool = self.pool(id).await?;
        let mut conn = pool.acquire().await?;
        let backend_id = conn.backend_id().await;
        let n = self.next_session.fetch_add(1, Ordering::SeqCst);
        let session = Arc::new(Session {
            id: format!("{id}#{n}"),
            ds_id: id.into(),
            dialect: pool.dialect(),
            pool,
            conn: tokio::sync::Mutex::new(conn),
            backend_id,
            auto_commit: AtomicBool::new(ds.options.auto_commit),
            tx_open: AtomicBool::new(false),
            running: Mutex::new(None),
        });
        let info = session.info();
        self.sessions
            .lock()
            .expect("poisoned")
            .insert(session.id.clone(), session);
        Ok(info)
    }

    pub fn session(&self, id: &str) -> DbResult<Arc<Session>> {
        self.sessions
            .lock()
            .expect("poisoned")
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::UnknownSession(id.into()))
    }

    /// Close a console; an open transaction is rolled back.
    pub async fn close_session(&self, id: &str) -> DbResult<()> {
        let s = self.sessions.lock().expect("poisoned").remove(id);
        if let Some(s) = s {
            self.cancel(&s.id).await.ok();
            if s.tx_open.load(Ordering::SeqCst) {
                let mut conn = s.conn.lock().await;
                let _ = conn.execute("ROLLBACK").await;
            }
        }
        Ok(())
    }

    /// Execute every statement of `sql` in order; stops at the first error. Rows are
    /// limited to `page_size` per statement (fetch more with [`DbService::fetch`]).
    pub async fn execute(
        &self,
        session_id: &str,
        sql: &str,
        page_size: Option<u32>,
    ) -> DbResult<Vec<StatementResult>> {
        let s = self.session(session_id)?;
        let ds = self.get(&s.ds_id)?;
        let limit = self.page_size(&ds, page_size);
        let token = CancellationToken::new();
        *s.running.lock().expect("poisoned") = Some(token.clone());
        let mut conn = s.conn.lock().await;
        let mut results = Vec::new();
        for (index, st) in split_statements(sql, s.dialect).into_iter().enumerate() {
            let control = transaction_control(&st.text);
            let started = Instant::now();
            let mut res = Ok(());
            if control.is_none()
                && !s.auto_commit.load(Ordering::SeqCst)
                && !s.tx_open.load(Ordering::SeqCst)
            {
                res = conn.execute(s.dialect.begin()).await.map(|_| {
                    s.tx_open.store(true, Ordering::SeqCst);
                });
            }
            let run = match res {
                Ok(()) => tokio::select! {
                    r = conn.run(&st.text, 0, limit) => r,
                    _ = token.cancelled() => Err(DbError::Cancelled),
                },
                Err(e) => Err(e),
            };
            let mut result = StatementResult {
                index,
                returns_rows: false,
                elapsed_ms: 0,
                error: None,
                result: RawResult::default(),
                sql: st.text.clone(),
                start: st.start,
                end: st.end,
            };
            match run {
                Ok(mut raw) => {
                    if raw.columns.is_empty() && is_query(&st.text) {
                        raw.columns = conn.describe(&st.text).await.unwrap_or_default();
                    }
                    match control {
                        Some("begin") => s.tx_open.store(true, Ordering::SeqCst),
                        Some(_) => s.tx_open.store(false, Ordering::SeqCst),
                        None => {}
                    }
                    result.elapsed_ms = elapsed_ms(started);
                    result.returns_rows = !raw.columns.is_empty();
                    result.result = raw;
                    results.push(result);
                }
                Err(e) => {
                    result.elapsed_ms = elapsed_ms(started);
                    result.error = Some(e.to_string());
                    results.push(result);
                    break;
                }
            }
        }
        *s.running.lock().expect("poisoned") = None;
        Ok(results)
    }

    /// Another page of a console query.
    pub async fn fetch(
        &self,
        session_id: &str,
        sql: &str,
        offset: u64,
        limit: Option<u32>,
    ) -> DbResult<QueryPage> {
        let s = self.session(session_id)?;
        let ds = self.get(&s.ds_id)?;
        let limit = self.page_size(&ds, limit);
        let body = strip_semicolon(sql);
        let started = Instant::now();
        let mut conn = s.conn.lock().await;
        let wrapped = format!(
            "SELECT * FROM (\n{body}\n) forge_page LIMIT {} OFFSET {offset}",
            limit + 1
        );
        let raw = match conn.run(&wrapped, 0, limit).await {
            Ok(r) => r,
            // Not wrappable (SHOW, PRAGMA, duplicate column names): skip rows instead.
            Err(_) => conn.run(body, offset as usize, limit).await?,
        };
        Ok(QueryPage {
            columns: raw.columns,
            rows: raw.rows,
            has_more: raw.has_more,
            offset,
            total: None,
            elapsed_ms: elapsed_ms(started),
            sql: body.into(),
        })
    }

    /// Switch between auto-commit and manual transactions; turning auto-commit on commits
    /// an open transaction.
    pub async fn set_auto_commit(&self, session_id: &str, auto: bool) -> DbResult<SessionInfo> {
        let s = self.session(session_id)?;
        if auto && s.tx_open.load(Ordering::SeqCst) {
            self.commit(session_id).await?;
        }
        s.auto_commit.store(auto, Ordering::SeqCst);
        Ok(s.info())
    }

    pub async fn commit(&self, session_id: &str) -> DbResult<SessionInfo> {
        self.end_tx(session_id, "COMMIT").await
    }

    pub async fn rollback(&self, session_id: &str) -> DbResult<SessionInfo> {
        self.end_tx(session_id, "ROLLBACK").await
    }

    async fn end_tx(&self, session_id: &str, sql: &str) -> DbResult<SessionInfo> {
        let s = self.session(session_id)?;
        if s.tx_open.load(Ordering::SeqCst) {
            let mut conn = s.conn.lock().await;
            conn.execute(sql).await?;
            s.tx_open.store(false, Ordering::SeqCst);
        }
        Ok(s.info())
    }

    /// Cancel the running statement of a console.
    pub async fn cancel(&self, session_id: &str) -> DbResult<()> {
        let s = self.session(session_id)?;
        let token = s.running.lock().expect("poisoned").clone();
        let Some(token) = token else {
            return Ok(());
        };
        if let Some(sql) = s.backend_id.and_then(|id| cancel_statement(s.dialect, id)) {
            match s.pool.acquire().await {
                Ok(mut other) => {
                    if let Err(e) = other.execute(&sql).await {
                        tracing::warn!("cancel failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("cancel failed: {e}"),
            }
        }
        token.cancel();
        Ok(())
    }

    // ---- .env detection ----------------------------------------------------------------

    /// Data sources described in the project's `.env` that are not configured yet.
    pub fn detect_env(&self) -> Vec<DetectedDataSource> {
        let Some(root) = self.project_root() else {
            return Vec::new();
        };
        let list = self.lock().list.clone();
        env::detect(&root)
            .into_iter()
            .filter(|d| !list.iter().any(|ds| ds.same_target(&d.source)))
            .collect()
    }

    /// Add the `.env` data sources (with their passwords) to the project.
    pub async fn add_from_env(&self) -> DbResult<Vec<DataSourceInfo>> {
        let mut added = Vec::new();
        for d in self.detect_env() {
            let mut ds = d.source;
            ds.scope = Scope::Project;
            ds.comment = format!("From {}", d.origin);
            added.push(self.save(ds, d.password).await?);
        }
        Ok(added)
    }
}
