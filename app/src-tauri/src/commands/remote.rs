//! Remote development IPC (Welcome screen > Remote Development, Settings | Tools | SSH
//! Configurations, the connection status bar item). The SSH and agent work is in
//! `forge-remote`; a project on a remote host is opened as a [`RemoteBackend`] and every
//! other command works on it unchanged.
//!
//! Events: `remote://progress` `{ message }` while connecting, `remote://status`
//! [`RemoteStatusView`] whenever the project's connection changes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::protocol::BrowseResult;
use forge_backend::remote::Connection;
use forge_backend::{Backend, RemoteBackend, RemoteHost};
use forge_core::{ForgeError, Result};
use forge_remote::agent::AgentBinaries;
use forge_remote::connection::{ConnectionStatus, RemoteConnection};
use forge_remote::session::{ConnectError, ConnectParams, SshSession};
use forge_remote::ssh_config::HostEntry;
use forge_remote::{store, SshConfiguration};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::lsp::LspState;
use crate::commands::workspace::{blocking, WorkspaceInfo};
use crate::state::AppState;

pub const PROGRESS_EVENT: &str = "remote://progress";
pub const STATUS_EVENT: &str = "remote://status";

/// The project's connection and connections opened for browsing.
#[derive(Default)]
pub struct RemoteState {
    connections: Mutex<HashMap<String, Arc<RemoteConnection>>>,
    project: Mutex<Option<ProjectConnection>>,
    next_id: AtomicU64,
}

struct ProjectConnection {
    id: String,
    connection: Arc<RemoteConnection>,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// A saved configuration for the settings page.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigView {
    #[serde(flatten)]
    pub config: SshConfiguration,
    pub label: String,
    /// A password / passphrase is stored in the keychain.
    pub has_secret: bool,
}

fn view(config: SshConfiguration) -> SshConfigView {
    SshConfigView {
        label: config.label(),
        has_secret: store::secret(&config.id).is_some(),
        config,
    }
}

#[tauri::command]
pub async fn remote_ssh_configs() -> Result<Vec<SshConfigView>> {
    blocking(|| Ok(store::load().configurations.into_iter().map(view).collect())).await
}

/// Save (insert or update by id; a new id is made for an empty one). `secret` replaces the
/// stored password / passphrase; an empty string deletes it.
#[tauri::command]
pub async fn remote_ssh_config_save(
    config: SshConfiguration,
    secret: Option<String>,
) -> Result<SshConfigView> {
    blocking(move || {
        let mut config = config;
        let problems = config.validate();
        if !problems.is_empty() {
            return Err(ForgeError::Other(problems.join("\n")));
        }
        let mut all = store::load();
        if config.id.trim().is_empty() {
            // The same account again: update its configuration instead of adding another.
            config.id = all
                .configurations
                .iter()
                .find(|c| c.same_target(&config))
                .map(|c| c.id.clone())
                .unwrap_or_else(|| new_config_id(&config));
        }
        all.upsert(config.clone());
        store::save(&all)?;
        if config.dont_save_secret {
            store::set_secret(&config.id, None)?;
        } else if let Some(s) = secret {
            store::set_secret(&config.id, Some(&s))?;
        }
        Ok(view(config))
    })
    .await
}

fn new_config_id(config: &SshConfiguration) -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let slug: String = format!("{}-{}", config.user, config.host)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("{slug}-{millis:x}")
}

/// Problems with a configuration, worded for the dialog (empty when it can be used).
#[tauri::command]
pub fn remote_validate_config(config: SshConfiguration) -> Vec<String> {
    config.validate()
}

#[tauri::command]
pub async fn remote_ssh_config_delete(id: String) -> Result<()> {
    blocking(move || {
        let mut all = store::load();
        all.remove(&id);
        store::save(&all)?;
        store::set_secret(&id, None)
    })
    .await
}

/// Concrete hosts of `~/.ssh/config` for the Host field's completion.
#[tauri::command]
pub async fn remote_ssh_hosts() -> Result<Vec<HostEntry>> {
    blocking(|| {
        let home = home();
        Ok(store::read_ssh_config(&home).hosts(&home.to_string_lossy(), &store::local_user()))
    })
    .await
}

fn params(
    config: &SshConfiguration,
    secret: Option<String>,
    accept_fingerprint: Option<String>,
) -> ConnectParams {
    let home = home();
    let ssh = store::read_ssh_config(&home);
    let mut p = ConnectParams::for_configuration(config, &ssh, &home, &store::local_user());
    p.secret = secret.filter(|s| !s.is_empty()).or_else(|| {
        (!config.id.is_empty())
            .then(|| store::secret(&config.id))
            .flatten()
    });
    p.accept_fingerprint = accept_fingerprint;
    p
}

fn remote_host(config: &SshConfiguration, params: &ConnectParams) -> RemoteHost {
    RemoteHost {
        host: config.host.trim().to_string(),
        port: params.port,
        user: params.user.clone(),
        config_id: (!config.id.is_empty()).then(|| config.id.clone()),
    }
}

/// Result of "Test Connection".
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestOutcome {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// `uname -srm` of the host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ConnectError>,
}

/// Connect and authenticate without installing anything.
#[tauri::command]
pub async fn remote_test_connection(
    config: SshConfiguration,
    secret: Option<String>,
    accept_fingerprint: Option<String>,
) -> Result<TestOutcome> {
    let p = blocking({
        let config = config.clone();
        move || Ok(params(&config, secret, accept_fingerprint))
    })
    .await?;
    let start = Instant::now();
    let session = match SshSession::connect(&p).await {
        Ok(s) => s,
        Err(error) => {
            return Ok(TestOutcome {
                ok: false,
                latency_ms: None,
                system: None,
                error: Some(error),
            })
        }
    };
    let latency = start.elapsed().as_millis() as u64;
    let system = session
        .exec("uname -srm", None)
        .await
        .ok()
        .map(|o| o.stdout_text().trim().to_string())
        .filter(|s| !s.is_empty());
    session.disconnect().await;
    Ok(TestOutcome {
        ok: true,
        latency_ms: Some(latency),
        system,
        error: None,
    })
}

/// Result of [`remote_connect`].
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ConnectOutcome {
    #[serde(rename_all = "camelCase")]
    Connected {
        connection_id: String,
        label: String,
        home: String,
        hostname: String,
        system: String,
    },
    Failed {
        error: ConnectError,
    },
}

#[derive(Serialize, Clone)]
struct Progress {
    message: String,
}

fn agent_binaries(app: &AppHandle) -> AgentBinaries {
    let resources = app.path().resource_dir().ok().map(|d| d.join("agents"));
    AgentBinaries::discover(resources)
}

/// Connect to a host and start the agent there (uploading it when needed), for browsing
/// its folders and opening a project.
#[tauri::command]
pub async fn remote_connect(
    config: SshConfiguration,
    secret: Option<String>,
    accept_fingerprint: Option<String>,
    app: AppHandle,
    remote: State<'_, RemoteState>,
) -> Result<ConnectOutcome> {
    let p = blocking({
        let config = config.clone();
        move || Ok(params(&config, secret, accept_fingerprint))
    })
    .await?;
    let host = remote_host(&config, &p);
    let emitter = app.clone();
    let progress = move |m: &str| {
        let _ = emitter.emit(
            PROGRESS_EVENT,
            Progress {
                message: m.to_string(),
            },
        );
    };
    let conn = match RemoteConnection::open(host.clone(), p, agent_binaries(&app), &progress).await
    {
        Ok(c) => c,
        Err(error) => return Ok(ConnectOutcome::Failed { error }),
    };
    let hello = conn
        .hello()
        .unwrap_or_else(|| forge_backend::protocol::Hello {
            version: String::new(),
            protocol: 0,
            os: String::new(),
            arch: String::new(),
            home: String::new(),
            hostname: String::new(),
        });
    let id = format!("conn-{}", remote.next_id.fetch_add(1, Ordering::SeqCst) + 1);
    remote
        .connections
        .lock()
        .expect("connections lock")
        .insert(id.clone(), conn);
    Ok(ConnectOutcome::Connected {
        connection_id: id,
        label: host.label(),
        home: hello.home,
        hostname: hello.hostname,
        system: format!("{} {}", hello.os, hello.arch).trim().to_string(),
    })
}

fn connection(remote: &RemoteState, id: &str) -> Result<Arc<RemoteConnection>> {
    if let Some(c) = remote.connections.lock().expect("connections lock").get(id) {
        return Ok(c.clone());
    }
    match remote.project.lock().expect("project lock").as_ref() {
        Some(p) if p.id == id => Ok(p.connection.clone()),
        _ => Err(ForgeError::Other(format!("no connection {id}"))),
    }
}

/// List an absolute folder on the host (`~` = home) for the remote folder chooser.
#[tauri::command]
pub async fn remote_browse(
    connection_id: String,
    path: String,
    remote: State<'_, RemoteState>,
) -> Result<BrowseResult> {
    let conn = connection(&remote, &connection_id)?;
    conn.browse(&path).await
}

/// The project connection as the status bar shows it.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatusView {
    pub connection_id: String,
    pub label: String,
    pub host: String,
    #[serde(flatten)]
    pub status: ConnectionStatus,
}

fn status_view(id: &str, conn: &RemoteConnection) -> RemoteStatusView {
    RemoteStatusView {
        connection_id: id.to_string(),
        label: conn.host().label(),
        host: conn.host().host.clone(),
        status: conn.status(),
    }
}

/// Open `path` on the connected host as the project.
#[tauri::command]
pub async fn remote_open_project(
    connection_id: String,
    path: String,
    app: AppHandle,
    remote: State<'_, RemoteState>,
) -> Result<WorkspaceInfo> {
    let conn = connection(&remote, &connection_id)?;
    let host = conn.host().clone();
    let source: Arc<dyn Connection> = conn.clone();
    let backend = Arc::new(blocking(move || RemoteBackend::open(source, host, &path)).await?);

    // The previous project's connection goes away unless it is this one.
    let previous = remote.project.lock().expect("project lock").take();
    if let Some(p) = previous.filter(|p| p.id != connection_id) {
        tauri::async_runtime::spawn(async move { p.connection.close().await });
    }
    remote
        .connections
        .lock()
        .expect("connections lock")
        .remove(&connection_id);
    *remote.project.lock().expect("project lock") = Some(ProjectConnection {
        id: connection_id.clone(),
        connection: conn.clone(),
    });

    {
        let b = backend.clone();
        conn.on_lost(move || b.connection_lost());
        let b = backend.clone();
        let handle = app.clone();
        conn.on_reconnect(move || {
            if let Err(e) = b.reattach() {
                tracing::warn!("cannot reopen the remote project: {e}");
                return;
            }
            // Language servers died with the old agent; they start again on demand.
            handle.state::<LspState>().reset();
            crate::commands::php::sync_lsp_server(&handle, None);
        });
    }
    forward_status(&app, connection_id.clone(), conn.clone());

    app.state::<LspState>().reset();
    {
        // Reads the project settings on the host.
        let (app, backend) = (app.clone(), backend.clone());
        blocking(move || {
            app.state::<AppState>().set_remote_project(backend);
            Ok(())
        })
        .await?;
    }
    crate::commands::workspace::project_opened(&app);
    let _ = app.emit(STATUS_EVENT, status_view(&connection_id, &conn));
    Ok(WorkspaceInfo::from(backend.info()))
}

/// Emit `remote://status` whenever the connection's state changes.
fn forward_status(app: &AppHandle, id: String, conn: Arc<RemoteConnection>) {
    let app = app.clone();
    let mut rx = conn.subscribe();
    let weak = Arc::downgrade(&conn);
    drop(conn);
    tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let Some(conn) = weak.upgrade() else { break };
            let _ = app.emit(STATUS_EVENT, status_view(&id, &conn));
        }
    });
}

/// Close the open project's remote connection (another project is opened).
pub fn close_project_connection(app: &AppHandle) {
    let remote = app.state::<RemoteState>();
    let previous = remote.project.lock().expect("project lock").take();
    if let Some(p) = previous {
        app.state::<LspState>().reset();
        tauri::async_runtime::spawn(async move { p.connection.close().await });
    }
}

/// Close a connection opened for browsing (dialog cancelled) or the project's connection.
#[tauri::command]
pub async fn remote_disconnect(
    connection_id: String,
    remote: State<'_, RemoteState>,
) -> Result<()> {
    let conn = remote
        .connections
        .lock()
        .expect("connections lock")
        .remove(&connection_id);
    if let Some(c) = conn {
        c.close().await;
    }
    Ok(())
}

/// The open project's connection, `null` for local projects.
#[tauri::command]
pub fn remote_status(remote: State<'_, RemoteState>) -> Option<RemoteStatusView> {
    remote
        .project
        .lock()
        .expect("project lock")
        .as_ref()
        .map(|p| status_view(&p.id, &p.connection))
}

/// Drop and re-establish the project's connection ("Reconnect" in the status popup).
#[tauri::command]
pub async fn remote_reconnect(remote: State<'_, RemoteState>) -> Result<()> {
    let conn = remote
        .project
        .lock()
        .expect("project lock")
        .as_ref()
        .map(|p| p.connection.clone())
        .ok_or_else(|| other("the project is not on a remote host"))?;
    conn.drop_connection();
    // Give the supervisor a moment to notice, so the UI sees "Reconnecting".
    tokio::time::sleep(Duration::from_millis(50)).await;
    Ok(())
}

/// Where Get from VCS clones on a host: a backend that runs git there and the host's home
/// directory. The open project's connection reuses the project's backend; a connection
/// opened for browsing gets a backend rooted at the home directory (the project opened
/// after the clone replaces it).
pub(crate) async fn clone_target(
    remote: &RemoteState,
    state: &AppState,
    connection_id: &str,
) -> Result<(Arc<dyn Backend>, String)> {
    let conn = connection(remote, connection_id)?;
    let home = conn.browse("~").await?.path;
    let is_project = remote
        .project
        .lock()
        .expect("project lock")
        .as_ref()
        .is_some_and(|p| p.id == connection_id);
    if is_project {
        return Ok((state.backend()?, home));
    }
    let host = conn.host().clone();
    let source: Arc<dyn Connection> = conn.clone();
    let root = home.clone();
    let backend: Arc<dyn Backend> =
        Arc::new(blocking(move || RemoteBackend::open(source, host, &root)).await?);
    Ok((backend, home))
}

/// The open project's connection id, for actions that open another folder on its host
/// (a cloned repository, a worktree).
#[tauri::command]
pub fn remote_project_connection(remote: State<'_, RemoteState>) -> Option<String> {
    remote
        .project
        .lock()
        .expect("project lock")
        .as_ref()
        .map(|p| p.id.clone())
}
