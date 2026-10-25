//! Services tool window IPC (`forge_containers`): the Docker / Docker Compose / Kubernetes
//! tree, details, toolbar actions, logs, inspect, exec terminals, pulls and the gutter ▶ of
//! compose files and Dockerfiles. Everything streams on `services://event` as
//! `{ type: "tree" | "console" | "ended" | "status" | "error", … }` ([`HubEvent`]).
//!
//! The open project is handed to the hub as a [`ProjectHost`]: a local folder, or for a
//! remote project [`BackendProject`] (compose files read and `docker compose` run on the
//! host through the project backend; with `docker.remoteProjectHost` the host's Docker
//! daemon through the project's SSH connection; with `kubernetes.remoteProjectHost` the
//! host's kubeconfig and kubectl).

use std::path::Path;
use std::sync::{Arc, OnceLock};

use forge_backend::Backend;
use forge_containers::endpoint::{DockerContext, DockerSettings};
use forge_containers::exec::TerminalCommand;
use forge_containers::gutter::{self, GutterMarker};
use forge_containers::http::BoxIo;
use forge_containers::hub::{Hub, HubEvent, HubSettings};
use forge_containers::kubectl::ManifestAction;
use forge_containers::logs::LogStream;
use forge_containers::portforward::PortForward;
use forge_containers::project::{BoxFuture, LocalProject, OutputSink, ProjectHost};
use forge_containers::tree::{Action, Details, KubeSelection, Node, Target};
use forge_core::{FileKind, ForgeError, Result, Settings};
use forge_process::output::ConsoleLine;
use forge_process::{CommandLine, OutputStream, RunEvent, RunSink};
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
            ssh_url: str_of("docker.ssh.url"),
            ssh_socket: str_of("docker.ssh.socketPath"),
            context: str_of("docker.context"),
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
        project_docker: s.get_as::<bool>("docker.remoteProjectHost").unwrap_or(true),
        project_kube: s
            .get_as::<bool>("kubernetes.remoteProjectHost")
            .unwrap_or(true),
        host_kubectl: s
            .get_as::<Vec<String>>("kubernetes.hostKubectlPaths")
            .unwrap_or_default(),
    }
}

/// A remote project for the Services hub: files and processes through its backend (on the
/// host), the host's Unix sockets through its SSH tunnels.
pub struct BackendProject {
    backend: Arc<dyn Backend>,
    label: String,
}

enum RunMsg {
    Output(LogStream, String),
    Exit(Option<i32>),
}

/// Stops a process when its run is dropped before it ended (Stop in the Services window).
struct KillOnDrop(Option<Arc<dyn forge_backend::Process>>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            std::thread::spawn(move || p.kill(std::time::Duration::from_secs(2)));
        }
    }
}

impl ProjectHost for BackendProject {
    fn root(&self) -> &Path {
        &self.backend.info().root
    }

    fn remote(&self) -> Option<&str> {
        Some(&self.label)
    }

    fn read(&self, rel: &str) -> Option<String> {
        self.backend.read_optional(rel).ok().flatten()
    }

    fn dirs(&self) -> Vec<String> {
        self.backend
            .list_dir("")
            .map(|entries| {
                entries
                    .into_iter()
                    .filter(|e| e.kind == FileKind::Dir)
                    .map(|e| e.name)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn run<'a>(
        &'a self,
        program: &'a str,
        args: &'a [String],
        env: &'a [(String, String)],
        cwd: &'a Path,
        mut sink: OutputSink<'a>,
    ) -> BoxFuture<'a, forge_containers::Result<Option<i32>>> {
        Box::pin(async move {
            let mut cmd = CommandLine::new(program).args(args.iter().cloned());
            cmd.cwd = cwd.to_path_buf();
            cmd.env.extend(env.iter().cloned());
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<RunMsg>();
            let run_sink: RunSink = Arc::new(move |event| {
                let _ = match event {
                    RunEvent::Output { stream, text } => tx.send(RunMsg::Output(
                        if stream == OutputStream::Stdout {
                            LogStream::Stdout
                        } else {
                            LogStream::Stderr
                        },
                        text,
                    )),
                    RunEvent::Exited { code, .. } => tx.send(RunMsg::Exit(code)),
                    RunEvent::Started { .. } => Ok(()),
                };
            });
            let backend = self.backend.clone();
            let process =
                tauri::async_runtime::spawn_blocking(move || backend.start_process(&cmd, run_sink))
                    .await
                    .map_err(|e| forge_containers::Error(e.to_string()))?
                    .map_err(|e| forge_containers::Error(e.to_string()))?;
            let mut guard = KillOnDrop(Some(process));
            while let Some(msg) = rx.recv().await {
                match msg {
                    RunMsg::Output(stream, text) => sink(stream, text),
                    RunMsg::Exit(code) => {
                        guard.0 = None;
                        return Ok(code);
                    }
                }
            }
            guard.0 = None;
            Ok(None)
        })
    }

    fn open_socket(&self, path: &str) -> BoxFuture<'static, forge_containers::Result<BoxIo>> {
        let tunnels = self.backend.tunnels();
        let path = path.to_string();
        Box::pin(async move {
            let tunnels = tunnels.ok_or_else(|| {
                forge_containers::Error("the project has no SSH connection".into())
            })?;
            let stream = tunnels
                .open_socket(path)
                .await
                .map_err(|e| forge_containers::Error(e.to_string()))?;
            Ok(Box::new(stream) as BoxIo)
        })
    }

    fn open_tcp(&self, port: u16) -> BoxFuture<'static, forge_containers::Result<BoxIo>> {
        let tunnels = self.backend.tunnels();
        Box::pin(async move {
            let tunnels = tunnels.ok_or_else(|| {
                forge_containers::Error("the project has no SSH connection".into())
            })?;
            let stream = tunnels
                .open_tcp("127.0.0.1".into(), port)
                .await
                .map_err(|e| forge_containers::Error(e.to_string()))?;
            Ok(Box::new(stream) as BoxIo)
        })
    }
}

/// The open project for the hub: a local folder, or a remote project through its backend.
fn project_host(app: &AppHandle) -> Option<Arc<dyn ProjectHost>> {
    let state = app.state::<AppState>();
    let info = state.project_info()?;
    match info.remote {
        None => Some(Arc::new(LocalProject::new(info.root))),
        Some(host) => Some(Arc::new(BackendProject {
            backend: state.backend().ok()?,
            label: host.label(),
        })),
    }
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
    let project = project_host(app);
    // Scanning compose files (on the host for a remote project) and reading the
    // kubeconfig touch the disk.
    tauri::async_runtime::spawn_blocking(move || hub.configure(settings, project));
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
    app: AppHandle,
    state: State<'_, ServicesState>,
) -> Result<()> {
    // `docker compose up` builds the project's images.
    if action == Action::Up {
        app.state::<AppState>()
            .require_trusted("Running Docker Compose")?;
    }
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
/// `container` picks a pod's container (default: the pod's default container).
#[tauri::command]
pub fn services_logs_start(
    node_id: String,
    container: Option<String>,
    app: AppHandle,
    state: State<'_, ServicesState>,
) -> Result<String> {
    let tail = app
        .state::<AppState>()
        .read_settings()
        .get_as::<u32>("docker.logs.tail")
        .unwrap_or(500);
    state
        .hub()?
        .start_logs(&node_id, tail, container.as_deref())
        .map_err(e)
}

#[tauri::command]
pub fn services_logs_stop(node_id: String, state: State<'_, ServicesState>) -> Result<()> {
    state.hub()?.stop_logs(&node_id);
    Ok(())
}

/// Command line for an Exec terminal (`attach: false`) or the Attached Console, in a pod's
/// `container` (default: the pod's default container).
#[tauri::command]
pub fn services_terminal(
    node_id: String,
    attach: bool,
    container: Option<String>,
    app: AppHandle,
    state: State<'_, ServicesState>,
) -> Result<TerminalCommand> {
    // A shell in a pod runs commands against the cluster of the project's kubeconfig.
    if matches!(Target::parse(&node_id), Some(Target::Pod { .. })) {
        app.state::<AppState>()
            .require_trusted("Opening a terminal in a pod")?;
    }
    state
        .hub()?
        .terminal_command(&node_id, attach, container.as_deref())
        .map_err(e)
}

/// Forward Ports: `spec` lists `local:pod` ports (`8080:80, 9000`, `:80` for any free local
/// port); each listens on `127.0.0.1` and forwards to the pod.
#[tauri::command]
pub async fn services_port_forward_start(
    node_id: String,
    spec: String,
    state: State<'_, ServicesState>,
) -> Result<Vec<PortForward>> {
    let hub = state.hub()?.clone();
    hub.start_port_forwards(&node_id, &spec).await.map_err(e)
}

#[tauri::command]
pub fn services_port_forward_stop(id: u64, state: State<'_, ServicesState>) -> Result<()> {
    state.hub()?.stop_port_forward(id).map_err(e)
}

#[tauri::command]
pub fn services_port_forwards(state: State<'_, ServicesState>) -> Result<Vec<PortForward>> {
    Ok(state.hub()?.port_forwards())
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
    app: AppHandle,
    state: State<'_, ServicesState>,
) -> Result<()> {
    app.state::<AppState>()
        .require_trusted("Running Docker Compose")?;
    let hub = state.hub()?.clone();
    hub.compose_up_file(&path, service).await.map_err(e)
}

/// Gutter ▶ in a Dockerfile: build the image and run a container.
#[tauri::command]
pub async fn services_dockerfile_run(
    path: String,
    app: AppHandle,
    state: State<'_, ServicesState>,
) -> Result<()> {
    app.state::<AppState>()
        .require_trusted("Building Dockerfiles")?;
    let hub = state.hub()?.clone();
    hub.dockerfile_run(&path).await.map_err(e)
}

/// The context and namespace selector of the Services tool window (`None`: no kubeconfig).
#[tauri::command]
pub fn services_kube_selection(state: State<'_, ServicesState>) -> Result<Option<KubeSelection>> {
    Ok(state.hub()?.kube_selection())
}

/// Pick the Kubernetes context and namespace (`None`: the context's own namespace).
#[tauri::command]
pub async fn services_kube_select(
    context: String,
    namespace: Option<String>,
    state: State<'_, ServicesState>,
) -> Result<Option<KubeSelection>> {
    let hub = state.hub()?.clone();
    hub.select_kube(&context, namespace).await.map_err(e)
}

/// Gutter ▶ of a Kubernetes manifest: `kubectl apply -f` / `kubectl delete -f` in the
/// selected context (on a remote project's host). Refused in Safe Mode.
#[tauri::command]
pub async fn services_kube_apply(
    path: String,
    action: ManifestAction,
    app: AppHandle,
    state: State<'_, ServicesState>,
) -> Result<()> {
    app.state::<AppState>()
        .require_trusted("Applying Kubernetes manifests")?;
    let hub = state.hub()?.clone();
    hub.apply_manifest(&path, action).await.map_err(e)
}

#[tauri::command]
pub fn services_gutter(path: String, content: String) -> Vec<GutterMarker> {
    gutter::markers(&path, &content)
}

#[tauri::command]
pub fn services_status(state: State<'_, ServicesState>) -> Result<HubEvent> {
    Ok(state.hub()?.status())
}

/// Settings page "Test Connection": the entered (possibly unsaved) connection settings, or
/// the applied ones.
#[tauri::command]
pub async fn docker_test_connection(
    settings: Option<DockerSettings>,
    app: AppHandle,
) -> Result<String> {
    let docker = match settings {
        Some(s) => s,
        None => hub_settings(&app.state::<AppState>().read_settings()).docker,
    };
    Hub::test_connection(&docker).await.map_err(e)
}

/// Docker CLI contexts (`~/.docker/contexts`) for the Docker settings page.
#[tauri::command]
pub fn docker_contexts() -> Vec<DockerContext> {
    forge_containers::hub::process_environment().contexts
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
