//! [`DbService`]: data sources of the open project plus global ones, their pools (through
//! the data source's SSH tunnel or the remote project's connection), console sessions with
//! transaction state (own or shared connection), the read-only / production guard, table
//! data paging, row editing, and the Redis keys tree and value viewer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::config::{self, DataModel, DataSource, Dialect, Scope};
use crate::conn::{cancel_statement, ColumnMeta, Conn, DbPool, RawResult};
use crate::edit::{ChangeKind, RowChange};
use crate::engine::redis::{db_number, RedisKeys, RedisPool, RedisValue};
use crate::engine::{Canceller, ConnectSpec};
use crate::env::{self, DetectedDataSource};
use crate::error::{DbError, DbResult};
use crate::guard::{self, Guard, GuardPolicy};
use crate::introspect;

use crate::secrets::{ssh_account, ssl_key_account, SecretStore};
use crate::split::{is_query, split_statements, transaction_control};
use crate::ssh::{host_key, ForgeSsh, SshConnector};
use crate::tunnel::{DbTunnel, ProjectContext, TunnelEndpoint};

mod explorer;
mod objects;
mod search;

pub use explorer::natural_cmp;
pub use objects::{ImportError, ImportProgress, ImportSummary, ScriptResult};
pub use search::{SearchProgress, SearchRequest, SearchScope, SearchSummary, TableMatches};

pub const DEFAULT_PAGE_SIZE: usize = 500;

/// A data source as the UI sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceInfo {
    #[serde(flatten)]
    pub source: DataSource,
    pub scope: Scope,
    pub has_password: bool,
    /// The SSH tunnel's password or key passphrase is stored.
    pub has_ssh_secret: bool,
    /// The SSL client key's passphrase is stored.
    pub has_ssl_passphrase: bool,
    pub summary: String,
    /// Set when the data source is reached through an SSH connection: its own tunnel or the
    /// remote project's (`deploy@shop.example.com`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    pub model: DataModel,
    /// Consoles can use manual transactions.
    pub transactions: bool,
    /// Production colour: unfiltered changes ask first.
    pub production: bool,
}

/// Secrets of the SSH and SSL tabs: `None` keeps the stored one, `Some("")` clears it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretUpdate {
    #[serde(default)]
    pub ssh: Option<String>,
    #[serde(default)]
    pub ssl_key: Option<String>,
}

/// How a console connects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionMode {
    /// Its own connection (DataGrip's default).
    #[default]
    Own,
    /// The data source's shared session, used by every console that asks for it.
    Shared,
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
    /// The SSH host's key is unknown or changed: trust it and test again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_key: Option<forge_remote::session::HostKeyInfo>,
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
    /// MongoDB: the documents of the rows (the JSON view).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<serde_json::Value>,
}

pub use forge_dataedit::{ColumnFilter, FilterOp, SortKey};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataRequest {
    pub schema: String,
    pub table: String,
    /// Raw `WHERE` condition typed into the filter row (checked: one read-only condition).
    #[serde(default)]
    pub filter: Option<String>,
    /// "Filter by value" conditions and foreign-key navigation, combined with `AND`.
    #[serde(default)]
    pub column_filters: Vec<ColumnFilter>,
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

/// The DML the data editor's pending changes become, before Submit.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePreview {
    pub statements: Vec<String>,
    /// The key that identifies rows (`primary key`, `unique key users_email_unique`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<forge_dataedit::KeyChoice>,
    /// Refused: a read-only data source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    /// Ask first: a production data source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

/// The key of a table that identifies its rows for the data editor.
pub fn row_key(info: &introspect::TableInfo) -> Option<forge_dataedit::KeyChoice> {
    use forge_dataedit::api::{table_key, ColumnShape, IndexShape, TableShape};
    table_key(&TableShape {
        primary_key: info.primary_key.clone(),
        indexes: info
            .indexes
            .iter()
            .map(|i| IndexShape {
                name: i.name.clone(),
                columns: i.columns.clone(),
                unique: i.unique,
                primary: i.primary,
            })
            .collect(),
        columns: info
            .columns
            .iter()
            .map(|c| ColumnShape {
                name: c.name.clone(),
                nullable: c.nullable,
            })
            .collect(),
    })
}

/// A console session (an open connection) and its transaction, for the consoles and the
/// Sessions view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub data_source_id: String,
    /// `console`, or `shared` for the data source's shared session.
    pub name: String,
    pub shared: bool,
    /// Consoles using the session.
    pub consoles: u32,
    pub auto_commit: bool,
    pub tx_open: bool,
    /// The driver supports manual transactions.
    pub transactions: bool,
    pub running: bool,
    /// Unix milliseconds.
    pub opened_at: u64,
    /// When the open transaction started (Unix milliseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_started_at: Option<u64>,
    /// Statements run in the open transaction.
    pub tx_statements: u32,
    /// The statement running now, or the last one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sql: Option<String>,
}

/// One console session: a dedicated connection, so transactions span several executions.
pub struct Session {
    id: String,
    ds_id: String,
    dialect: Dialect,
    pool: DbPool,
    conn: tokio::sync::Mutex<Conn>,
    backend_id: Option<i64>,
    canceller: Option<Canceller>,
    reconnect_after_cancel: bool,
    shared: bool,
    transactions: bool,
    consoles: AtomicU32,
    opened_at: u64,
    auto_commit: AtomicBool,
    tx_open: AtomicBool,
    tx: Mutex<(Option<u64>, u32)>,
    last_sql: Mutex<Option<String>>,
    running: Mutex<Option<CancellationToken>>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        let tx_open = self.tx_open.load(Ordering::SeqCst);
        let (started, statements) = *self.tx.lock().expect("poisoned");
        SessionInfo {
            id: self.id.clone(),
            data_source_id: self.ds_id.clone(),
            name: if self.shared { "shared" } else { "console" }.into(),
            shared: self.shared,
            consoles: self.consoles.load(Ordering::SeqCst),
            auto_commit: self.auto_commit.load(Ordering::SeqCst),
            tx_open,
            transactions: self.transactions,
            running: self.running.lock().expect("poisoned").is_some(),
            opened_at: self.opened_at,
            tx_started_at: started.filter(|_| tx_open),
            tx_statements: if tx_open { statements } else { 0 },
            last_sql: self.last_sql.lock().expect("poisoned").clone(),
        }
    }

    fn set_tx(&self, open: bool) {
        self.tx_open.store(open, Ordering::SeqCst);
        let mut tx = self.tx.lock().expect("poisoned");
        *tx = if open {
            (tx.0.or(Some(now_ms())), tx.1)
        } else {
            (None, 0)
        };
    }

    fn count_tx_statement(&self) {
        if self.tx_open.load(Ordering::SeqCst) {
            self.tx.lock().expect("poisoned").1 += 1;
        }
    }
}

#[derive(Default)]
struct Sources {
    root: Option<PathBuf>,
    /// Project files read through the project backend (remote projects); `None` reads the
    /// files below `root` directly.
    files: Option<forge_core::SharedFiles>,
    tunnel: Option<Arc<dyn DbTunnel>>,
    list: Vec<DataSource>,
    /// Secrets of data sources with "Save: Never" (or without a keychain), kept for this
    /// run only, by keychain account.
    transient: HashMap<String, String>,
}

pub struct DbService {
    global_path: Option<PathBuf>,
    secrets: Arc<dyn SecretStore>,
    ssh: Arc<dyn SshConnector>,
    sources: Mutex<Sources>,
    pools: tokio::sync::Mutex<HashMap<String, DbPool>>,
    /// Tunnels of the open pools of a remote project.
    tunnels: Mutex<HashMap<String, TunnelEndpoint>>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_session: AtomicU64,
    /// Explorer models are cached here (user cache dir); `None` keeps them in memory.
    cache_dir: Option<PathBuf>,
    models: Mutex<HashMap<String, forge_dbmodel::DbModel>>,
    /// Running full-text searches.
    searches: Mutex<HashMap<String, CancellationToken>>,
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
            ssh: Arc::new(ForgeSsh::default()),
            sources: Mutex::default(),
            pools: tokio::sync::Mutex::default(),
            tunnels: Mutex::default(),
            sessions: Mutex::default(),
            next_session: AtomicU64::new(1),
            cache_dir: None,
            models: Mutex::default(),
            searches: Mutex::default(),
        };
        if let Err(e) = svc.reload() {
            tracing::warn!("{e}");
        }
        svc
    }

    /// Open data source SSH tunnels with `ssh` (tests use a temporary home directory).
    pub fn with_ssh(mut self, ssh: Arc<dyn SshConnector>) -> Self {
        self.ssh = ssh;
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Sources> {
        self.sources.lock().expect("data sources lock poisoned")
    }

    /// Switch to another project (or none) and load its data sources.
    pub async fn set_project(&self, root: Option<PathBuf>) -> DbResult<()> {
        {
            let mut s = self.lock();
            s.root = root;
            s.files = None;
            s.tunnel = None;
        }
        self.close_all().await;
        self.reload()
    }

    /// Switch to a project whose files (and databases) are reached through `ctx`, e.g. a
    /// project on a remote host.
    pub async fn set_project_context(&self, ctx: ProjectContext) -> DbResult<()> {
        {
            let mut s = self.lock();
            s.root = Some(ctx.root);
            s.files = Some(ctx.files);
            s.tunnel = ctx.tunnel;
        }
        self.close_all().await;
        self.reload()
    }

    fn project_files(&self) -> Option<forge_core::SharedFiles> {
        self.lock().files.clone()
    }

    fn tunnel(&self) -> Option<Arc<dyn DbTunnel>> {
        self.lock().tunnel.clone()
    }

    /// Where the pool of `ds` connects: through the data source's own SSH tunnel, or for a
    /// project data source of a remote project through a tunnel to its host and port as the
    /// remote host sees them.
    async fn endpoint(
        &self,
        ds: &DataSource,
        ssh_secret: Option<String>,
        accept_host_key: Option<String>,
    ) -> DbResult<(DataSource, Option<TunnelEndpoint>)> {
        if ds.options.ssh.enabled {
            if ds.driver.is_file() {
                return Err(DbError::Config(
                    "SQLite files cannot be opened through an SSH tunnel".into(),
                ));
            }
            let host = match ds.host.trim() {
                "" => "localhost".to_string(),
                h => h.to_string(),
            };
            let end = self
                .ssh
                .open(
                    &ds.options.ssh,
                    ssh_secret,
                    accept_host_key,
                    host,
                    ds.port_or_default(),
                )
                .await?;
            let mut local = ds.clone();
            local.host = "127.0.0.1".into();
            local.port = Some(end.port);
            return Ok((local, Some(end)));
        }
        let Some(tunnel) = self.tunnel().filter(|_| ds.scope == Scope::Project) else {
            return Ok((ds.clone(), None));
        };
        if ds.driver == config::Driver::Sqlite {
            return Err(DbError::Config(format!(
                "SQLite files on {} cannot be opened yet; use a global data source with a local copy",
                tunnel.label()
            )));
        }
        let host = if ds.host.trim().is_empty() {
            "localhost".to_string()
        } else {
            ds.host.trim().to_string()
        };
        let end = tunnel.open(host, ds.port_or_default()).await?;
        let mut local = ds.clone();
        local.host = "127.0.0.1".into();
        local.port = Some(end.port);
        Ok((local, Some(end)))
    }

    pub fn project_root(&self) -> Option<PathBuf> {
        self.lock().root.clone()
    }

    /// Re-read the project and global files.
    pub fn reload(&self) -> DbResult<()> {
        let root = self.project_root();
        let mut list = Vec::new();
        let mut first_err = None;
        if let Some(files) = self.project_files() {
            let loaded = files
                .read(config::PROJECT_FILE)
                .map_err(|e| DbError::Io(e.to_string()))
                .and_then(|text| match text {
                    Some(t) => config::parse_text(
                        &t,
                        Scope::Project,
                        &files.describe(config::PROJECT_FILE),
                    ),
                    None => Ok(Vec::new()),
                });
            match loaded {
                Ok(l) => list.extend(l),
                Err(e) => first_err = Some(e),
            }
        } else if let Some(root) = &root {
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

    fn has_secret(&self, account: &str) -> bool {
        self.lock().transient.contains_key(account)
            || matches!(self.secrets.get(account), Ok(Some(p)) if !p.is_empty())
    }

    fn info(&self, ds: DataSource) -> DataSourceInfo {
        let via = if ds.options.ssh.enabled {
            Some(self.ssh.label(&ds.options.ssh))
        } else {
            self.tunnel()
                .filter(|_| ds.scope == Scope::Project)
                .map(|t| t.label())
        };
        DataSourceInfo {
            scope: ds.scope,
            has_password: !ds.driver.is_file() && self.has_secret(&ds.id),
            has_ssh_secret: self.has_secret(&ssh_account(&ds.id)),
            has_ssl_passphrase: self.has_secret(&ssl_key_account(&ds.id)),
            summary: ds.summary(),
            via,
            model: ds.driver.model(),
            transactions: ds.driver.supports_transactions(),
            production: ds.is_production(),
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
        if let (Scope::Project, Some(files)) = (scope, self.project_files()) {
            let items: Vec<_> = self
                .lock()
                .list
                .iter()
                .filter(|d| d.scope == scope)
                .cloned()
                .collect();
            return files
                .write(config::PROJECT_FILE, &config::to_text(&items)?)
                .map_err(|e| DbError::Io(e.to_string()));
        }
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
    pub async fn save(&self, ds: DataSource, password: Option<String>) -> DbResult<DataSourceInfo> {
        self.save_with(ds, password, SecretUpdate::default()).await
    }

    /// Store (or with `""` clear) a secret under `account`; without a keychain, or with
    /// "Save: Never", it is kept for this run only.
    fn store_secret(&self, account: &str, secret: &str, keep: bool) {
        self.lock().transient.remove(account);
        if !keep {
            let _ = self.secrets.delete(account);
            if !secret.is_empty() {
                self.lock().transient.insert(account.into(), secret.into());
            }
            return;
        }
        let stored = if secret.is_empty() {
            self.secrets.delete(account)
        } else {
            self.secrets.set(account, secret)
        };
        if let Err(e) = stored {
            // No keychain (e.g. headless Linux): keep it for this run.
            tracing::warn!("{e}; secret kept in memory only");
            if !secret.is_empty() {
                self.lock().transient.insert(account.into(), secret.into());
            }
        }
    }

    /// [`save`](Self::save) with the secrets of the SSH and SSL tabs.
    pub async fn save_with(
        &self,
        mut ds: DataSource,
        password: Option<String>,
        extra: SecretUpdate,
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
        let (old_scope, retarget) = {
            let mut s = self.lock();
            let old = s.list.iter().position(|d| d.id == ds.id);
            let old_scope = old.map(|i| s.list[i].scope);
            let retarget = old.is_some_and(|i| {
                let o = &s.list[i];
                !o.same_target(&ds)
                    || o.user != ds.user
                    || o.options.ddl_dialect != ds.options.ddl_dialect
            });
            match old {
                Some(i) => s.list[i] = ds.clone(),
                None => s.list.push(ds.clone()),
            }
            (old_scope, retarget)
        };
        if retarget {
            self.forget_model(&ds.id);
        }
        if ds.driver == config::Driver::Ddl {
            // Files or dialect may have changed: parse again on the next request.
            self.models.lock().expect("poisoned").remove(&ds.id);
        }
        self.persist(ds.scope)?;
        if let Some(prev) = old_scope.filter(|p| *p != ds.scope) {
            self.persist(prev)?;
        }
        if let Some(p) = password {
            self.store_secret(&ds.id, &p, ds.options.save_password);
        } else if !ds.options.save_password {
            let _ = self.secrets.delete(&ds.id);
        }
        if let Some(p) = extra.ssh {
            self.store_secret(&ssh_account(&ds.id), &p, ds.options.save_password);
        }
        if let Some(p) = extra.ssl_key {
            self.store_secret(&ssl_key_account(&ds.id), &p, ds.options.save_password);
        }
        self.drop_pool(&ds.id).await;
        Ok(self.info(ds))
    }

    pub async fn delete(&self, id: &str) -> DbResult<()> {
        let ds = self.get(id)?;
        self.forget_model(id);
        self.lock().list.retain(|d| d.id != id);
        self.persist(ds.scope)?;
        for account in [id.to_string(), ssh_account(id), ssl_key_account(id)] {
            self.lock().transient.remove(&account);
            if let Err(e) = self.secrets.delete(&account) {
                tracing::warn!("{e}");
            }
        }
        self.drop_pool(id).await;
        Ok(())
    }

    fn secret(&self, account: &str) -> Option<String> {
        if let Some(p) = self.lock().transient.get(account) {
            return Some(p.clone());
        }
        match self.secrets.get(account) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("{e}");
                None
            }
        }
    }

    fn password(&self, ds: &DataSource) -> Option<String> {
        self.secret(&ds.id)
    }

    /// Open a pool for `ds` with its tunnel, certificates and secrets.
    async fn open_pool(
        &self,
        ds: &DataSource,
        password: Option<String>,
        extra: &SecretUpdate,
        accept_host_key: Option<String>,
    ) -> DbResult<(DbPool, Option<TunnelEndpoint>)> {
        let root = self.project_root();
        let ssh_secret = extra
            .ssh
            .clone()
            .or_else(|| self.secret(&ssh_account(&ds.id)));
        let ssl_pass = extra
            .ssl_key
            .clone()
            .or_else(|| self.secret(&ssl_key_account(&ds.id)));
        let tls = if ds.options.ssl_mode.enabled() && !ds.driver.is_file() {
            crate::tls::load(&ds.options.ssl, ssl_pass.as_deref(), root.as_deref())?
        } else {
            Default::default()
        };
        let (target, tunnel) = self.endpoint(ds, ssh_secret, accept_host_key).await?;
        let pool = DbPool::open(ConnectSpec {
            ds: &target,
            password: password.as_deref(),
            tls,
            tls_host: ds.host.trim().to_string(),
            root: root.as_deref(),
        })
        .await?;
        Ok((pool, tunnel))
    }

    /// Connect once with the given settings (the dialog's "Test Connection").
    /// `password: None` uses the stored password of `ds.id`.
    pub async fn test(&self, ds: &DataSource, password: Option<String>) -> TestResult {
        self.test_with(ds, password, SecretUpdate::default(), None)
            .await
    }

    /// [`test`](Self::test) with the dialog's SSH/SSL secrets and a host key the user trusts.
    pub async fn test_with(
        &self,
        ds: &DataSource,
        password: Option<String>,
        extra: SecretUpdate,
        accept_host_key: Option<String>,
    ) -> TestResult {
        let started = Instant::now();
        let password = password.or_else(|| self.password(ds));
        let res = async {
            let (pool, _tunnel) = self
                .open_pool(ds, password, &extra, accept_host_key)
                .await?;
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
            host_key: res.as_ref().err().and_then(host_key).cloned(),
            error: res.err().map(|e| e.to_string()),
        }
    }

    async fn drop_pool(&self, id: &str) {
        // Consoles give their connections back first: closing a pool waits for them.
        self.sessions
            .lock()
            .expect("poisoned")
            .retain(|_, s| s.ds_id != id);
        let pool = self.pools.lock().await.remove(id);
        if let Some(p) = pool {
            close_pool(p).await;
        }
        self.tunnels.lock().expect("poisoned").remove(id);
    }

    /// Close every pool and console (project switch, app exit).
    pub async fn close_all(&self) {
        self.sessions.lock().expect("poisoned").clear();
        let pools: Vec<_> = self.pools.lock().await.drain().map(|(_, p)| p).collect();
        for p in pools {
            close_pool(p).await;
        }
        self.tunnels.lock().expect("poisoned").clear();
    }

    /// The pool of a data source, connecting on first use.
    pub async fn pool(&self, id: &str) -> DbResult<DbPool> {
        let ds = self.get(id)?;
        if ds.driver == config::Driver::Ddl {
            return Err(DbError::Refused(format!(
                "{} is a DDL data source: it has a model but no database",
                ds.name
            )));
        }
        let mut pools = self.pools.lock().await;
        if let Some(p) = pools.get(id) {
            return Ok(p.clone());
        }
        let (pool, tunnel) = self
            .open_pool(&ds, self.password(&ds), &SecretUpdate::default(), None)
            .await?;
        if let Some(t) = tunnel {
            self.tunnels.lock().expect("poisoned").insert(id.into(), t);
        }
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

    pub async fn table_ddl(&self, id: &str, schema: &str, table: &str) -> DbResult<String> {
        let ds = self.get(id)?;
        if ds.driver == config::Driver::Ddl {
            let model = self.model(id, false).await?;
            let t = model
                .table(schema, table)
                .ok_or_else(|| DbError::Sql(format!("table {schema}.{table} not found")))?;
            return Ok(crate::ddl::generate(ds.dialect(), schema, t));
        }
        let mut conn = self.pool(id).await?.acquire().await?;
        crate::ddl::table_ddl(&mut conn, schema, table).await
    }

    fn page_size(&self, ds: &DataSource, limit: Option<u32>) -> usize {
        limit
            .or(ds.options.page_size)
            .map(|l| l.clamp(1, 100_000) as usize)
            .unwrap_or(DEFAULT_PAGE_SIZE)
    }

    /// A page of a table with the grid's filter and sort (SQL by `forge_dataedit::query`).
    pub async fn table_data(&self, id: &str, req: &TableDataRequest) -> DbResult<QueryPage> {
        let ds = self.get(id)?;
        let limit = self.page_size(&ds, req.limit);
        if !ds.dialect().is_sql() {
            return Err(DbError::Refused(format!(
                "{} has no tables; use a console",
                ds.driver.label()
            )));
        }
        let pool = self.pool(id).await?;
        let d = pool.dialect();
        let q = forge_dataedit::TableQuery {
            schema: req.schema.clone(),
            table: req.table.clone(),
            filter: req.filter.clone(),
            column_filters: req.column_filters.clone(),
            order: req.order.clone(),
        }
        .sql(d)
        .map_err(|e| DbError::Refused(e.0))?;
        let paged = d.page(&q.select, &q.order, limit + 1, req.offset);
        let started = Instant::now();
        let mut conn = pool.acquire().await?;
        let mut raw = conn.run(&paged, 0, limit).await?;
        let elapsed = elapsed_ms(started);
        if raw.columns.is_empty() {
            raw.columns = conn.describe(&q.display).await.unwrap_or_default();
        }
        let total = if req.offset == 0 && !raw.has_more {
            Some(raw.rows.len() as u64)
        } else {
            conn.text_rows(&q.count)
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
            sql: q.display,
            documents: Vec::new(),
        })
    }

    /// The table's description and the key that identifies its rows.
    async fn editable_table(
        &self,
        id: &str,
        schema: &str,
        table: &str,
    ) -> DbResult<(DataSource, Dialect, introspect::TableInfo)> {
        let ds = self.get(id)?;
        if !ds.driver.supports_transactions() {
            return Err(DbError::Refused(format!(
                "editing rows of {} data sources is not supported; use a console",
                ds.driver.label()
            )));
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
        Ok((ds, d, info))
    }

    /// The DML of the grid's pending changes and what the data source requires before
    /// they run (read-only: refused; production: confirmation).
    pub async fn preview_changes(
        &self,
        id: &str,
        schema: &str,
        table: &str,
        changes: &[RowChange],
    ) -> DbResult<ChangePreview> {
        let (ds, d, info) = self.editable_table(id, schema, table).await?;
        let key = row_key(&info);
        let statements = forge_dataedit::preview(d, schema, table, key.as_ref(), changes)
            .map_err(|e| DbError::Refused(e.0))?;
        Ok(ChangePreview {
            statements,
            key,
            blocked: ds
                .options
                .read_only
                .then(|| format!("{} is read-only", ds.name)),
            confirm: ds.is_production().then(|| {
                format!(
                    "{} is a production data source. Submit {} change{} to {table}?",
                    ds.name,
                    changes.len(),
                    if changes.len() == 1 { "" } else { "s" }
                )
            }),
        })
    }

    /// Submit the grid's pending changes in one transaction. Production data sources need
    /// `confirmed`; read-only ones refuse.
    pub async fn apply_changes(
        &self,
        id: &str,
        schema: &str,
        table: &str,
        changes: &[RowChange],
        confirmed: bool,
    ) -> DbResult<ApplyResult> {
        let ds = self.get(id)?;
        if ds.options.read_only {
            return Err(DbError::Refused(format!("{} is read-only", ds.name)));
        }
        if ds.is_production() && !confirmed {
            return Err(DbError::Refused(format!(
                "{} is a production data source; confirm the changes first",
                ds.name
            )));
        }
        let (_, d, info) = self.editable_table(id, schema, table).await?;
        let key = row_key(&info);
        let statements = forge_dataedit::preview(d, schema, table, key.as_ref(), changes)
            .map_err(|e| DbError::Refused(e.0))?;
        let mut conn = self.pool(id).await?.acquire().await?;
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
        self.open_session_with(id, SessionMode::Own).await
    }

    /// Open a console session: its own connection, or the data source's shared one (also
    /// when the data source is in single session mode).
    pub async fn open_session_with(&self, id: &str, mode: SessionMode) -> DbResult<SessionInfo> {
        let ds = self.get(id)?;
        let shared = mode == SessionMode::Shared || ds.options.single_session;
        let shared_id = format!("{id}#shared");
        if shared {
            if let Ok(s) = self.session(&shared_id) {
                s.consoles.fetch_add(1, Ordering::SeqCst);
                return Ok(s.info());
            }
        }
        let pool = self.pool(id).await?;
        let mut conn = pool.acquire().await?;
        let backend_id = conn.backend_id().await;
        let (canceller, reconnect_after_cancel) = match &conn {
            Conn::Engine(e) => (e.canceller(), e.reconnect_after_cancel()),
            _ => (None, false),
        };
        let n = self.next_session.fetch_add(1, Ordering::SeqCst);
        let transactions = ds.driver.supports_transactions();
        let session = Arc::new(Session {
            id: if shared {
                shared_id
            } else {
                format!("{id}#{n}")
            },
            ds_id: id.into(),
            dialect: pool.dialect(),
            pool,
            conn: tokio::sync::Mutex::new(conn),
            backend_id,
            canceller,
            reconnect_after_cancel,
            shared,
            transactions,
            consoles: AtomicU32::new(1),
            opened_at: now_ms(),
            auto_commit: AtomicBool::new(ds.options.auto_commit || !transactions),
            tx_open: AtomicBool::new(false),
            tx: Mutex::new((None, 0)),
            last_sql: Mutex::new(None),
            running: Mutex::new(None),
        });
        let info = session.info();
        self.sessions
            .lock()
            .expect("poisoned")
            .insert(session.id.clone(), session);
        Ok(info)
    }

    /// Open sessions, by data source and opening time (the Sessions view).
    pub fn sessions(&self) -> Vec<SessionInfo> {
        let mut list: Vec<_> = self
            .sessions
            .lock()
            .expect("poisoned")
            .values()
            .map(|s| s.info())
            .collect();
        list.sort_by(|a, b| {
            (a.data_source_id.as_str(), a.opened_at, a.id.as_str()).cmp(&(
                b.data_source_id.as_str(),
                b.opened_at,
                b.id.as_str(),
            ))
        });
        list
    }

    pub fn session(&self, id: &str) -> DbResult<Arc<Session>> {
        self.sessions
            .lock()
            .expect("poisoned")
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::UnknownSession(id.into()))
    }

    /// A console closes its session; the last console of a shared session (or the Sessions
    /// view's "Close") closes the connection, rolling back an open transaction.
    pub async fn close_session(&self, id: &str) -> DbResult<()> {
        if let Ok(s) = self.session(id) {
            if s.consoles.load(Ordering::SeqCst) > 1 {
                s.consoles.fetch_sub(1, Ordering::SeqCst);
                return Ok(());
            }
        }
        self.terminate_session(id).await
    }

    /// Close a session whatever uses it (the Sessions view).
    pub async fn terminate_session(&self, id: &str) -> DbResult<()> {
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
        self.execute_with(session_id, sql, page_size, false).await
    }

    /// What the console must do before running `sql` on data source `id`: refuse it
    /// (read-only) or ask (production).
    pub fn guard(&self, id: &str, sql: &str) -> DbResult<Guard> {
        let ds = self.get(id)?;
        Ok(guard::guard(sql, ds.dialect(), &policy(&ds)))
    }

    /// [`execute`](Self::execute); `confirmed` when the user agreed to the production
    /// warning of [`guard`](Self::guard).
    pub async fn execute_with(
        &self,
        session_id: &str,
        sql: &str,
        page_size: Option<u32>,
        confirmed: bool,
    ) -> DbResult<Vec<StatementResult>> {
        let s = self.session(session_id)?;
        let ds = self.get(&s.ds_id)?;
        let verdict = guard::guard(sql, s.dialect, &policy(&ds));
        if let Some(blocked) = verdict.blocked {
            return Err(DbError::Refused(blocked));
        }
        if let (Some(confirm), false) = (verdict.confirm, confirmed) {
            return Err(DbError::NeedsConfirmation(confirm));
        }
        let limit = self.page_size(&ds, page_size);
        let token = CancellationToken::new();
        *s.running.lock().expect("poisoned") = Some(token.clone());
        let mut conn = s.conn.lock().await;
        let mut results = Vec::new();
        for (index, st) in split_statements(sql, s.dialect).into_iter().enumerate() {
            let control = if s.transactions {
                transaction_control(&st.text)
            } else {
                None
            };
            *s.last_sql.lock().expect("poisoned") = Some(st.text.clone());
            let started = Instant::now();
            let mut res = Ok(());
            if control.is_none()
                && !s.auto_commit.load(Ordering::SeqCst)
                && !s.tx_open.load(Ordering::SeqCst)
            {
                res = conn
                    .execute(s.dialect.begin())
                    .await
                    .map(|_| s.set_tx(true));
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
                        Some("begin") => s.set_tx(true),
                        Some(_) => s.set_tx(false),
                        None => s.count_tx_statement(),
                    }
                    result.elapsed_ms = elapsed_ms(started);
                    result.returns_rows = !raw.columns.is_empty();
                    result.result = raw;
                    results.push(result);
                }
                Err(e) => {
                    if matches!(e, DbError::Cancelled) && s.reconnect_after_cancel {
                        // The interrupted connection cannot be reused (its transaction is gone).
                        match s.pool.acquire().await {
                            Ok(fresh) => *conn = fresh,
                            Err(err) => tracing::warn!("reconnect after cancel failed: {err}"),
                        }
                        s.set_tx(false);
                    }
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
        let raw = if s.dialect.is_sql() && is_query(body) {
            let wrapped = s.dialect.page(
                &format!("SELECT * FROM (\n{body}\n) forge_page"),
                "",
                limit + 1,
                offset,
            );
            match conn.run(&wrapped, 0, limit).await {
                Ok(r) => r,
                // Not wrappable (SHOW, PRAGMA, duplicate column names): skip rows instead.
                Err(_) => conn.run(body, offset as usize, limit).await?,
            }
        } else {
            conn.run(body, offset as usize, limit).await?
        };
        Ok(QueryPage {
            columns: raw.columns,
            rows: raw.rows,
            has_more: raw.has_more,
            offset,
            total: None,
            elapsed_ms: elapsed_ms(started),
            sql: body.into(),
            documents: raw.documents,
        })
    }

    /// Switch between auto-commit and manual transactions; turning auto-commit on commits
    /// an open transaction.
    pub async fn set_auto_commit(&self, session_id: &str, auto: bool) -> DbResult<SessionInfo> {
        let s = self.session(session_id)?;
        if !auto && !s.transactions {
            let ds = self.get(&s.ds_id)?;
            return Err(DbError::Refused(format!(
                "{} consoles run every statement on its own (no manual transactions)",
                ds.driver.label()
            )));
        }
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
            s.set_tx(false);
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
        if let Some(cancel) = &s.canceller {
            cancel().await;
        } else if let Some(sql) = s.backend_id.and_then(|id| cancel_statement(s.dialect, id)) {
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

    // ---- Redis -------------------------------------------------------------------------

    async fn redis(&self, id: &str, schema: &str) -> DbResult<crate::engine::redis::RedisConn> {
        let pool = self.pool(id).await?;
        let redis = pool
            .engine()
            .and_then(|e| e.as_any().downcast_ref::<RedisPool>())
            .ok_or_else(|| DbError::Refused("not a Redis data source".into()))?;
        let db = if schema.is_empty() {
            redis.default_db()
        } else {
            db_number(schema)?
        };
        redis.open(db).await
    }

    /// Keys of a Redis database (`db0`) matching `pattern`, with type and TTL.
    pub async fn redis_keys(
        &self,
        id: &str,
        schema: &str,
        pattern: &str,
        limit: Option<u32>,
    ) -> DbResult<RedisKeys> {
        let mut conn = self.redis(id, schema).await?;
        conn.keys(pattern, limit.unwrap_or(1000).clamp(1, 100_000) as usize)
            .await
    }

    /// A Redis key's value for the value viewer.
    pub async fn redis_value(&self, id: &str, schema: &str, key: &str) -> DbResult<RedisValue> {
        let mut conn = self.redis(id, schema).await?;
        conn.value(key).await
    }

    // ---- .env detection ----------------------------------------------------------------

    /// Data sources described in the project's `.env` that are not configured yet.
    pub fn detect_env(&self) -> Vec<DetectedDataSource> {
        let detected = match (self.project_files(), self.project_root()) {
            (Some(files), _) => env::detect_with(|name| files.read(name).ok().flatten()),
            (None, Some(root)) => env::detect(&root),
            (None, None) => return Vec::new(),
        };
        let list = self.lock().list.clone();
        detected
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

/// Close a pool; a statement still running on one of its connections (it ends with the
/// connection) does not hold up the caller.
async fn close_pool(pool: DbPool) {
    if tokio::time::timeout(std::time::Duration::from_secs(2), pool.close())
        .await
        .is_err()
    {
        tracing::warn!("a connection was still in use while closing its pool");
    }
}

/// The guard settings of a data source.
fn policy(ds: &DataSource) -> GuardPolicy {
    GuardPolicy {
        read_only: ds.options.read_only,
        production: ds.is_production(),
        name: ds.name.clone(),
    }
}
