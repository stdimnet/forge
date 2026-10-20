//! Services tool window IPC (`forge_containers`): the Docker / Docker Compose / Kubernetes
//! tree, details, toolbar actions, logs, inspect, exec terminals, pulls and the gutter ▶ of
//! compose files and Dockerfiles. Everything streams on `services://event` as
//! `{ type: "tree" | "console" | "ended" | "status" | "error", … }` ([`HubEvent`]).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use forge_containers::endpoint::DockerSettings;
use forge_containers::exec::TerminalCommand;
use forge_containers::gutter::{self, GutterMarker};
use forge_containers::hub::{Hub, HubEvent, HubSettings};
use forge_containers::tree::{Action, Details, Node};
use forge_core::{ForgeError, Result, Settings};
use forge_process::output::ConsoleLine;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

pub const EVENT: &str = "services://event";

#[derive(Default)]
pub struct ServicesState {
    hub: OnceLock<Arc<Hub>>,
}

impl ServicesState {
    fn hub(&self) -> Result<&Arc<Hub>> {
        self.hub
            .get()
            .ok_or_else(|| ForgeError::Other("Services are starting".into()))
    }
}

fn e(err: forge_containers::Error) -> ForgeError {
    ForgeError::Other(err.0)
}

/// Settings keys that reconfigure the hub.
pub fn affects_services(key: &str) -> bool {
    key.starts_with("docker.") || key.starts_with("kubernetes.")
}

pub fn hub_settings(s: &Settings) -> HubSettings {
    let str_of = |k: &str| s.get_as::<String>(k).unwrap_or_default();
    HubSettings {
        docker: DockerSettings {
            connection: str_of("docker.connection"),
            socket_path: str_of("docker.socketPath"),
            tcp_url: str_of("docker.tcp.url"),
            tls: s.get_as::<bool>("docker.tcp.tls").unwrap_or(false),
            cert_path: str_of("docker.tcp.certPath"),
        },
        docker_cli: str_of("docker.cliPath"),
        kubeconfig: str_of("kubernetes.kubeconfig"),
        kubectl: str_of("kubernetes.kubectlPath"),
        default_context: str_of("kubernetes.defaultContext"),
        default_namespace: str_of("kubernetes.defaultNamespace"),
        registries: s
            .get_as::<Vec<String>>("docker.registries")
            .unwrap_or_default()
            .iter()
            .filter_map(|line| {
                let (server, user) = line.split_once('\t')?;
                Some((
                    forge_containers::secrets::normalize_server(server),
                    user.trim().to_string(),
                ))
            })
            .collect(),
    }
}

fn project_root(app: &AppHandle) -> Option<PathBuf> {
    app.state::<AppState>()
        .with_workspace(|ws| Ok(ws.root().to_path_buf()))
        .ok()
}

/// Create the hub (app setup) and connect.
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    let emit = Arc::new(move |ev: HubEvent| {
        if let Err(err) = handle.emit(EVENT, &ev) {
            tracing::warn!("cannot emit {EVENT}: {err}");
        }
    });
    let tauri::async_runtime::RuntimeHandle::Tokio(runtime) = tauri::async_runtime::handle();
    let hub = Hub::new(emit, runtime);
    let state = app.state::<ServicesState>();
    if state.hub.set(hub.clone()).is_err() {
        return;
    }
    reconfigure(app);
}

/// Apply the current settings and project (settings changed, project opened).
pub fn reconfigure(app: &AppHandle) {
    let Some(hub) = app.state::<ServicesState>().hub.get().cloned() else {
        return;
    };
    let settings = hub_settings(&app.state::<AppState>().read_settings());
    let root = project_root(app);
    // Scanning compose files and reading the kubeconfig touch the disk.
    tauri::async_runtime::spawn_blocking(move || hub.configure(settings, root));
}

#[tauri::command]
pub fn services_tree(state: State<'_, ServicesState>) -> Result<Vec<Node>> {
    Ok(state.hub()?.tree())
}

#[tauri::command]
pub async fn services_refresh(state: State<'_, ServicesState>) -> Result<Vec<Node>> {
    let hub = state.hub()?.clone();
    hub.refresh().await;
    Ok(hub.tree())
}

/// Compose files changed on disk: re-read them (cheap, no Docker round trip).
#[tauri::command]
pub fn services_compose_changed(state: State<'_, ServicesState>) -> Result<()> {
    state.hub()?.compose_files_changed();
    Ok(())
}

#[tauri::command]
pub async fn services_expand(
    node_id: String,
    state: State<'_, ServicesState>,
) -> Result<Vec<Node>> {
    let hub = state.hub()?.clone();
    Ok(hub.expand(&node_id).await)
}

#[tauri::command]
pub fn services_details(
    node_id: String,
    state: State<'_, ServicesState>,
) -> Result<Option<Details>> {
    Ok(state.hub()?.details(&node_id))
}

#[tauri::command]
pub async fn services_action(
    node_id: String,
    action: Action,
    state: State<'_, ServicesState>,
) -> Result<()> {
    let hub = state.hub()?.clone();
    hub.action(&node_id, action).await.map_err(e)
}

#[tauri::command]
pub async fn services_inspect(node_id: String, state: State<'_, ServicesState>) -> Result<String> {
    let hub = state.hub()?.clone();
    hub.inspect(&node_id).await.map_err(e)
}

#[tauri::command]
pub fn services_console(key: String, state: State<'_, ServicesState>) -> Result<Vec<ConsoleLine>> {
    Ok(state.hub()?.console(&key))
}

/// Start following the logs of a node; returns the console key (`log:<nodeId>`).
#[tauri::command]
pub fn services_logs_start(
    node_id: String,
    app: AppHandle,
    state: State<'_, ServicesState>,
) -> Result<String> {
    let tail = app
        .state::<AppState>()
        .read_settings()
        .get_as::<u32>("docker.logs.tail")
        .unwrap_or(500);
    state.hub()?.start_logs(&node_id, tail).map_err(e)
}

#[tauri::command]
pub fn services_logs_stop(node_id: String, state: State<'_, ServicesState>) -> Result<()> {
    state.hub()?.stop_logs(&node_id);
    Ok(())
}

/// Command line for an Exec terminal (`attach: false`) or the Attached Console.
#[tauri::command]
pub fn services_terminal(
    node_id: String,
    attach: bool,
    state: State<'_, ServicesState>,
) -> Result<TerminalCommand> {
    state.hub()?.terminal_command(&node_id, attach).map_err(e)
}

#[tauri::command]
pub async fn services_pull(image: String, state: State<'_, ServicesState>) -> Result<()> {
    let hub = state.hub()?.clone();
    hub.pull(image.trim()).await.map_err(e)
}

/// Gutter ▶ in a compose file: up one service or all of them.
#[tauri::command]
pub async fn services_compose_up(
    path: String,
    service: Option<String>,
    state: State<'_, ServicesState>,
) -> Result<()> {
    let hub = state.hub()?.clone();
    hub.compose_up_file(&path, service).await.map_err(e)
}

/// Gutter ▶ in a Dockerfile: build the image and run a container.
#[tauri::command]
pub async fn services_dockerfile_run(path: String, state: State<'_, ServicesState>) -> Result<()> {
    let hub = state.hub()?.clone();
    hub.dockerfile_run(&path).await.map_err(e)
}

#[tauri::command]
pub fn services_gutter(path: String, content: String) -> Vec<GutterMarker> {
    gutter::markers(&path, &content)
}

#[tauri::command]
pub fn services_status(state: State<'_, ServicesState>) -> Result<HubEvent> {
    Ok(state.hub()?.status())
}

/// Settings page "Test Connection" with the current (applied) settings.
#[tauri::command]
pub async fn docker_test_connection(app: AppHandle) -> Result<String> {
    let settings = hub_settings(&app.state::<AppState>().read_settings());
    Hub::test_connection(&settings.docker).await.map_err(e)
}

/// Store a registry password in the keychain (the server and user name are a setting).
#[tauri::command]
pub fn docker_registry_login(server: String, password: String) -> Result<()> {
    forge_containers::secrets::store_password(&server, &password).map_err(e)
}

#[tauri::command]
pub fn docker_registry_logout(server: String) -> Result<()> {
    forge_containers::secrets::delete_password(&server).map_err(e)
}

/// Whether a password is stored for each registry server.
#[tauri::command]
pub fn docker_registry_status(servers: Vec<String>) -> Vec<bool> {
    servers
        .iter()
        .map(|s| forge_containers::secrets::password(s).is_some())
        .collect()
}
