//! Dev Containers and WSL projects (Welcome screen > Remote Development > Dev Containers /
//! WSL, "Create Dev Container and Mount Sources…", the devcontainer.json gutter icon and
//! Terminal › WSL). The logic is in `forge-devenv`: devcontainer.json, docker command lines,
//! `wsl.exe` parsing and the Forge agent over the stdio of `docker exec` / `wsl.exe`. A
//! project in a container or a distribution opens as a [`RemoteBackend`], so every other
//! command works on it unchanged.
//!
//! Events: `devcontainer://log` [`LogLine`] while a container is built or started,
//! `remote://progress` `{ message }` while the agent starts, and `remote://status` for the
//! project's connection (shared with SSH projects).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use forge_backend::protocol::BrowseResult;
use forge_backend::remote::Connection;
use forge_backend::{Backend, RemoteBackend, RemoteHost, RemoteKind};
use forge_core::{ForgeError, Result};
use forge_devenv::agent::{start_agent, DockerTarget, WslTarget};
use forge_devenv::connection::{Starter, StdioConnection};
use forge_devenv::devcontainer::{self, LocalContext, ResolvedConfig};
use forge_devenv::docker::{self, DevContainerInstance};
use forge_devenv::runner::{Runner, SystemRunner};
use forge_devenv::up::{self, LogLine, UpOptions, UpOutcome};
use forge_devenv::wsl::{self, Distro, LinuxPath};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::lsp::LspState;
use crate::commands::remote::{RemoteStatusView, PROGRESS_EVENT, STATUS_EVENT};
use crate::commands::workspace::{blocking, WorkspaceInfo};
use crate::state::AppState;

pub const LOG_EVENT: &str = "devcontainer://log";

/// Agent connections of dev containers and WSL distributions.
#[derive(Default)]
pub struct DevEnvState {
    /// Opened for browsing (WSL folder chooser), not a project yet.
    connections: Mutex<HashMap<String, Arc<StdioConnection>>>,
    project: Mutex<Option<(String, Arc<StdioConnection>)>>,
    next_id: AtomicU64,
    /// The running build, so it can be cancelled.
    build: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn docker_program(state: &AppState) -> String {
    state
        .read_settings()
        .get_as::<String>("docker.cliPath")
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "docker".into())
}

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("forge")
        .join("devcontainers")
}

#[derive(Serialize, Clone)]
struct Progress {
    message: String,
}

fn progress(app: &AppHandle) -> impl Fn(&str) + Send + Sync + 'static {
    let app = app.clone();
    move |m: &str| {
        let _ = app.emit(
            PROGRESS_EVENT,
            Progress {
                message: m.to_string(),
            },
        );
    }
}

// ---- dev containers -------------------------------------------------------------------------

/// A `devcontainer.json` of a folder, resolved (or why it cannot be).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevContainerConfigView {
    /// Absolute path of the file.
    pub path: String,
    /// Relative to the folder (`.devcontainer/devcontainer.json`).
    pub rel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ResolvedConfig>,
    /// "Image php:8.3", "Dockerfile …", "Docker Compose service app".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The `devcontainer.json` files of a local folder: `.devcontainer/devcontainer.json`,
/// `.devcontainer.json` and `.devcontainer/<name>/devcontainer.json`.
pub fn find_configs(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let primary = folder.join(devcontainer::PRIMARY_CONFIG);
    if primary.is_file() {
        out.push(primary);
    }
    let root = folder.join(devcontainer::ROOT_CONFIG);
    if root.is_file() {
        out.push(root);
    }
    if let Ok(entries) = std::fs::read_dir(folder.join(".devcontainer")) {
        let mut named: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("devcontainer.json"))
            .filter(|p| p.is_file())
            .collect();
        named.sort();
        out.extend(named);
    }
    out
}

fn resolve(folder: &str, config_path: &str) -> std::result::Result<ResolvedConfig, String> {
    let text = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Cannot read {config_path}: {e}"))?;
    let env = |name: &str| std::env::var(name).ok();
    devcontainer::load(
        &text,
        &LocalContext {
            folder,
            config_path,
            env: &env,
        },
    )
}

/// The local folder to look in: `folder`, else the open project (a local one).
fn local_folder(folder: Option<String>, state: &AppState) -> Result<String> {
    if let Some(f) = folder.filter(|f| !f.trim().is_empty()) {
        return Ok(f);
    }
    match state.project_info() {
        Some(info) if info.remote.is_none() => Ok(info.root.to_string_lossy().into_owned()),
        Some(_) => Err(other(
            "Dev containers are created from local projects; this project is not on this machine",
        )),
        None => Err(other("no project is open")),
    }
}

#[tauri::command]
pub async fn devcontainer_configs(
    folder: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DevContainerConfigView>> {
    let folder = local_folder(folder, &state)?;
    blocking(move || {
        let base = Path::new(&folder);
        Ok(find_configs(base)
            .into_iter()
            .map(|p| {
                let path = p.to_string_lossy().into_owned();
                let rel = devcontainer::relative_to(&folder, &path);
                match resolve(&folder, &path) {
                    Ok(c) => DevContainerConfigView {
                        source: Some(c.source_label()),
                        config: Some(c),
                        error: None,
                        path,
                        rel,
                    },
                    Err(e) => DevContainerConfigView {
                        path,
                        rel,
                        config: None,
                        source: None,
                        error: Some(e),
                    },
                }
            })
            .collect())
    })
    .await
}

/// The line of the "Create Dev Container" gutter icon (`None`: no icon).
#[tauri::command]
pub fn devcontainer_gutter(path: String, content: String) -> Option<u32> {
    devcontainer::is_config_file(&path)
        .then(|| devcontainer::gutter_line(&content))
        .flatten()
}

/// Forge's dev containers known to Docker (Welcome screen list).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevContainerList {
    /// The Docker CLI answered.
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub containers: Vec<DevContainerInstance>,
}

#[tauri::command]
pub async fn devcontainer_list(state: State<'_, AppState>) -> Result<DevContainerList> {
    let docker = docker_program(&state);
    let out = SystemRunner
        .output(&docker, &docker::list_args(), None)
        .await;
    Ok(match out {
        Ok(o) if o.success() => DevContainerList {
            available: true,
            error: None,
            containers: docker::parse_list(&o.stdout_text()),
        },
        Ok(o) => DevContainerList {
            available: false,
            error: Some(o.message()),
            containers: Vec::new(),
        },
        Err(e) => DevContainerList {
            available: false,
            error: Some(e.to_string()),
            containers: Vec::new(),
        },
    })
}

/// Refuse to build or start a dev container of an untrusted folder: the image build and
/// the lifecycle commands run the project's code.
fn require_trusted_folder(state: &AppState, folder: &str) -> Result<()> {
    let key = forge_trust::local_key(Path::new(folder));
    if state.trust_store().check(&key).trusted {
        Ok(())
    } else {
        Err(ForgeError::Other(forge_trust::refusal(
            "Building and starting a dev container",
        )))
    }
}

/// Build (or start) the dev container of `configPath`; the log streams as
/// `devcontainer://log`. Cancelled by `devcontainer_cancel`.
#[tauri::command]
pub async fn devcontainer_up(
    folder: String,
    config_path: String,
    rebuild: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
    devenv: State<'_, DevEnvState>,
) -> Result<UpOutcome> {
    require_trusted_folder(&state, &folder)?;
    let cfg = {
        let (folder, config_path) = (folder.clone(), config_path.clone());
        blocking(move || resolve(&folder, &config_path).map_err(ForgeError::Other)).await?
    };
    let opts = UpOptions {
        docker: docker_program(&state),
        rebuild: rebuild.unwrap_or(false),
        cache_dir: cache_dir(),
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let emitter = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        let log = move |line: LogLine| {
            let _ = emitter.emit(LOG_EVENT, &line);
        };
        let result = up::up(&SystemRunner, &cfg, &opts, &log).await;
        let _ = tx.send(result);
    });
    if let Some(previous) = devenv.build.lock().expect("build lock").replace(task) {
        previous.abort();
    }
    let result = rx.await;
    devenv.build.lock().expect("build lock").take();
    match result {
        Ok(r) => r.map_err(ForgeError::Other),
        Err(_) => Err(other("The build was cancelled")),
    }
}

/// Stop the running build (the docker process ends with it).
#[tauri::command]
pub fn devcontainer_cancel(devenv: State<'_, DevEnvState>) {
    if let Some(task) = devenv.build.lock().expect("build lock").take() {
        task.abort();
    }
}

async fn find_instance(docker: &str, container_id: &str) -> Result<DevContainerInstance> {
    let out = SystemRunner
        .output(docker, &docker::list_args(), None)
        .await
        .map_err(other)?;
    if !out.success() {
        return Err(other(out.message()));
    }
    docker::parse_list(&out.stdout_text())
        .into_iter()
        .find(|c| {
            c.container_id.starts_with(container_id)
                || container_id.starts_with(&c.container_id)
                || c.container_name == container_id
        })
        .ok_or_else(|| other(format!("There is no dev container {container_id}")))
}

/// Start, stop or delete a dev container (Welcome screen list).
#[tauri::command]
pub async fn devcontainer_action(
    container_id: String,
    action: String,
    state: State<'_, AppState>,
) -> Result<()> {
    let docker = docker_program(&state);
    let instance = find_instance(&docker, &container_id).await?;
    let args = match action.as_str() {
        "start" => return start_container(&state, &docker, &instance).await,
        "stop" => docker::stop_args(&instance.container_id),
        "remove" => docker::remove_args(&instance.container_id),
        other => return Err(ForgeError::Other(format!("unknown action {other}"))),
    };
    let out = SystemRunner
        .output(&docker, &args, None)
        .await
        .map_err(other)?;
    if !out.success() {
        return Err(ForgeError::Other(out.message()));
    }
    Ok(())
}

/// Start a stopped dev container and run its `postStartCommand` (through [`up::up`], which
/// finds the container by its configuration's label); a container whose configuration is
/// gone or changed is only started.
async fn start_container(
    state: &AppState,
    docker: &str,
    instance: &DevContainerInstance,
) -> Result<()> {
    require_trusted_folder(state, &instance.local_folder)?;
    match resolve(&instance.local_folder, &instance.config_file) {
        Ok(cfg) if cfg.id == instance.id => {
            let opts = UpOptions {
                docker: docker.to_string(),
                rebuild: false,
                cache_dir: cache_dir(),
            };
            up::up(&SystemRunner, &cfg, &opts, &|_| {})
                .await
                .map(|_| ())
                .map_err(ForgeError::Other)
        }
        _ => {
            let out = SystemRunner
                .output(docker, &docker::start_args(&instance.container_id), None)
                .await
                .map_err(other)?;
            if out.success() {
                Ok(())
            } else {
                Err(other(out.message()))
            }
        }
    }
}

/// Start the Forge agent in a running dev container and open its workspace folder as the
/// project.
#[tauri::command]
pub async fn devcontainer_open(
    container_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WorkspaceInfo> {
    let docker = docker_program(&state);
    let instance = find_instance(&docker, &container_id).await?;
    if !instance.running() {
        start_container(&state, &docker, &instance).await?;
    }
    let env: BTreeMap<String, String> = resolve(&instance.local_folder, &instance.config_file)
        .map(|c| c.remote_env)
        .unwrap_or_default();
    let target = DockerTarget {
        docker,
        container: instance.container_id.clone(),
        user: instance.remote_user.clone(),
        workdir: Some(instance.workspace_folder.clone()).filter(|w| !w.is_empty()),
        env,
        name: instance.name.clone(),
    };
    let host = RemoteHost {
        host: instance.name.clone(),
        port: 0,
        user: instance
            .remote_user
            .clone()
            .unwrap_or_else(|| "root".into()),
        config_id: None,
        kind: RemoteKind::DevContainer {
            container: instance.container_id.clone(),
            source: instance.local_folder.clone(),
            config: instance.config_file.clone(),
        },
    };
    let path = if instance.workspace_folder.is_empty() {
        "/".to_string()
    } else {
        instance.workspace_folder.clone()
    };
    let conn = connect(&app, target_starter(&app, Arc::new(target)), &host).await?;
    open_project(&app, conn, host, path).await
}

fn target_starter<T: forge_devenv::agent::AgentTarget + 'static>(
    app: &AppHandle,
    target: Arc<T>,
) -> Starter {
    let binaries = crate::commands::remote::agent_binaries(app);
    let report = Arc::new(progress(app));
    Arc::new(move || {
        let (target, binaries, report) = (target.clone(), binaries.clone(), report.clone());
        Box::pin(async move {
            start_agent(&SystemRunner, target.as_ref(), &binaries, report.as_ref()).await
        })
    })
}

async fn connect(
    app: &AppHandle,
    starter: Starter,
    host: &RemoteHost,
) -> Result<Arc<StdioConnection>> {
    (progress(app))(&format!("Connecting to {}…", host.label()));
    StdioConnection::open(host.label(), starter)
        .await
        .map_err(ForgeError::Other)
}

/// Open `path` through `conn` as the project (the previous project's connection closes).
async fn open_project(
    app: &AppHandle,
    conn: Arc<StdioConnection>,
    host: RemoteHost,
    path: String,
) -> Result<WorkspaceInfo> {
    let devenv = app.state::<DevEnvState>();
    let source: Arc<dyn Connection> = conn.clone();
    let backend = Arc::new(blocking(move || RemoteBackend::open(source, host, &path)).await?);
    crate::commands::remote::close_project_connection(app);
    close_project(app);
    let id = format!(
        "local-{}",
        devenv.next_id.fetch_add(1, Ordering::SeqCst) + 1
    );
    *devenv.project.lock().expect("project lock") = Some((id.clone(), conn.clone()));
    {
        let b = backend.clone();
        conn.on_lost(move || b.connection_lost());
        let b = backend.clone();
        let handle = app.clone();
        conn.on_reconnect(move || {
            if let Err(e) = b.reattach() {
                tracing::warn!("cannot reopen the project: {e}");
                return;
            }
            handle.state::<LspState>().reset();
            crate::commands::php::sync_lsp_server(&handle, None);
        });
    }
    forward_status(app, id.clone(), conn.clone());
    app.state::<LspState>().reset();
    {
        let (app, backend) = (app.clone(), backend.clone());
        blocking(move || {
            app.state::<AppState>().set_remote_project(backend);
            Ok(())
        })
        .await?;
    }
    crate::commands::workspace::project_opened(app);
    let _ = app.emit(STATUS_EVENT, view(&id, &conn));
    Ok(WorkspaceInfo::from(backend.info()))
}

fn view(id: &str, conn: &StdioConnection) -> RemoteStatusView {
    RemoteStatusView {
        connection_id: id.to_string(),
        label: conn.label().to_string(),
        host: conn.label().to_string(),
        status: conn.status(),
    }
}

fn forward_status(app: &AppHandle, id: String, conn: Arc<StdioConnection>) {
    let app = app.clone();
    let mut rx = conn.subscribe();
    let weak = Arc::downgrade(&conn);
    drop(conn);
    tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let Some(conn) = weak.upgrade() else { break };
            let _ = app.emit(STATUS_EVENT, view(&id, &conn));
        }
    });
}

/// Close the open project's container / distribution connection (another project opens).
pub fn close_project(app: &AppHandle) {
    let previous = app
        .state::<DevEnvState>()
        .project
        .lock()
        .expect("project lock")
        .take();
    if let Some((_, conn)) = previous {
        app.state::<LspState>().reset();
        tauri::async_runtime::spawn(async move { conn.close().await });
    }
}

/// The project's connection for the status bar (`None`: not a container / WSL project).
pub fn project_status(app: &AppHandle) -> Option<RemoteStatusView> {
    app.state::<DevEnvState>()
        .project
        .lock()
        .expect("project lock")
        .as_ref()
        .map(|(id, c)| view(id, c))
}

/// Restart the project's agent ("Reconnect"); `false` when there is none.
pub fn reconnect_project(app: &AppHandle) -> bool {
    let conn = app
        .state::<DevEnvState>()
        .project
        .lock()
        .expect("project lock")
        .as_ref()
        .map(|(_, c)| c.clone());
    match conn {
        Some(c) => {
            c.drop_connection();
            true
        }
        None => false,
    }
}

// ---- WSL -----------------------------------------------------------------------------------

/// The WSL distributions of this machine.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WslInfo {
    /// WSL is installed (Windows only).
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub distros: Vec<Distro>,
}

#[tauri::command]
pub async fn wsl_distributions() -> WslInfo {
    if !cfg!(windows) {
        return WslInfo {
            available: false,
            error: Some("WSL is only available on Windows".into()),
            distros: Vec::new(),
        };
    }
    match SystemRunner
        .output(wsl::PROGRAM, &wsl::list_args(), None)
        .await
    {
        Ok(out) => {
            let distros: Vec<Distro> = wsl::parse_list_bytes(&out.stdout)
                .into_iter()
                .filter(|d| !d.is_internal())
                .collect();
            WslInfo {
                available: true,
                error: (!out.success() && distros.is_empty())
                    .then(|| wsl::decode_output(&out.stdout).trim().to_string())
                    .filter(|m| !m.is_empty()),
                distros,
            }
        }
        Err(e) => WslInfo {
            available: false,
            error: Some(e.to_string()),
            distros: Vec::new(),
        },
    }
}

/// A Windows path in the Linux view (`\\wsl$\Ubuntu\home\me` → Ubuntu `/home/me`,
/// `C:\src` → `/mnt/c/src`); `null` when it has none.
#[tauri::command]
pub fn wsl_map_path(path: String) -> Option<LinuxPath> {
    wsl::to_linux(&path)
}

/// A Linux path of a distribution in the Windows view.
#[tauri::command]
pub fn wsl_windows_path(distro: String, path: String) -> String {
    wsl::to_windows(&distro, &path)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WslConnected {
    pub connection_id: String,
    pub label: String,
    pub home: String,
}

fn wsl_host(distro: &str, user: &str) -> RemoteHost {
    RemoteHost {
        host: distro.to_string(),
        port: 0,
        user: user.to_string(),
        config_id: None,
        kind: RemoteKind::Wsl {
            distro: distro.to_string(),
        },
    }
}

/// Start the Forge agent in a distribution, for its folder chooser.
#[tauri::command]
pub async fn wsl_connect(
    distro: String,
    app: AppHandle,
    devenv: State<'_, DevEnvState>,
) -> Result<WslConnected> {
    let target = Arc::new(WslTarget {
        wsl: wsl::PROGRAM.into(),
        distro: distro.clone(),
        user: None,
    });
    let host = wsl_host(&distro, "");
    let conn = connect(&app, target_starter(&app, target), &host).await?;
    let home = conn.hello().map(|h| h.home).unwrap_or_default();
    let id = format!("wsl-{}", devenv.next_id.fetch_add(1, Ordering::SeqCst) + 1);
    devenv
        .connections
        .lock()
        .expect("connections lock")
        .insert(id.clone(), conn);
    Ok(WslConnected {
        connection_id: id,
        label: host.label(),
        home,
    })
}

fn browsing(devenv: &DevEnvState, id: &str) -> Result<Arc<StdioConnection>> {
    devenv
        .connections
        .lock()
        .expect("connections lock")
        .get(id)
        .cloned()
        .ok_or_else(|| other(format!("no connection {id}")))
}

#[tauri::command]
pub async fn wsl_browse(
    connection_id: String,
    path: String,
    devenv: State<'_, DevEnvState>,
) -> Result<BrowseResult> {
    let conn = browsing(&devenv, &connection_id)?;
    conn.browse(&path).await
}

/// Open `path` of the connected distribution as the project.
#[tauri::command]
pub async fn wsl_open_project(
    connection_id: String,
    distro: String,
    path: String,
    app: AppHandle,
    devenv: State<'_, DevEnvState>,
) -> Result<WorkspaceInfo> {
    let conn = devenv
        .connections
        .lock()
        .expect("connections lock")
        .remove(&connection_id)
        .ok_or_else(|| other(format!("no connection {connection_id}")))?;
    let user = conn
        .hello()
        .map(|h| h.home.rsplit('/').next().unwrap_or_default().to_string())
        .unwrap_or_default();
    open_project(&app, conn, wsl_host(&distro, &user), path).await
}

/// Close a connection opened for browsing (chooser cancelled).
#[tauri::command]
pub async fn wsl_disconnect(connection_id: String, devenv: State<'_, DevEnvState>) -> Result<()> {
    let conn = devenv
        .connections
        .lock()
        .expect("connections lock")
        .remove(&connection_id);
    if let Some(c) = conn {
        c.close().await;
    }
    Ok(())
}

/// The trust key of a project in a dev container (its local sources, trusted before the
/// container was built) or in WSL (the folder's `\\wsl.localhost` path).
pub fn trust_key(host: &RemoteHost, path: &str) -> Option<String> {
    match &host.kind {
        RemoteKind::Ssh => None,
        RemoteKind::DevContainer { source, .. } => Some(forge_trust::local_key(Path::new(source))),
        RemoteKind::Wsl { distro } => Some(forge_trust::normalize(&wsl::unc(distro, path))),
    }
}
