//! The Services backend: one [`Hub`] per app holds the Docker connection (with a watcher that
//! follows the Engine's events and reconnects), the project's compose files, the kubeconfig
//! with lazily loaded namespaces and workloads, running log streams and the consoles (Log
//! tabs). The app only forwards [`HubEvent`]s to the UI and calls these methods from IPC
//! commands. The open project is a [`ProjectHost`]: for a remote project its compose files
//! are read on the host and compose / build commands run there, and with
//! [`HubSettings::project_docker`] the Docker daemon of the host is used through the
//! project's SSH connection. With [`HubSettings::project_kube`] Kubernetes comes from the
//! host too: its kubeconfig, and `kubectl` running there ([`crate::kubectl`]) for the
//! tree, logs, exec, port forwards (tunnelled back through the SSH connection) and
//! applying manifests. Local projects use this machine's kubeconfig and the API server.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forge_output::{ConsoleDelta, ConsoleLine, OutputEvent, RunInput, RunOutput, RunOutputOptions};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::compose::{self, ComposeAction, ComposeCommand, ComposeFile};
use crate::docker::RawSnapshot;
use crate::endpoint::{self, DockerSettings, Endpoint};
use crate::engine::{registry_auth, ContainerOp, DockerClient, ObjectKind};
use crate::exec::{self, TerminalCommand};
use crate::gutter;
use crate::kube::{self, KubeConfig};
use crate::kubeclient::KubeClient;
use crate::kubectl::{self, KubectlClient, ManifestAction};
use crate::logs::{pod_log_prefix, LinePrefixer, LogStream};
use crate::portforward::PortForward;
use crate::project::{LocalProject, ProjectHost};
use crate::tree::{
    self, Action, ConnectionState, Details, KubeInput, Loaded, Node, ServicesInput, Target,
};
use crate::{err, Error, Result};

/// Settings | Build, Execution, Deployment | Docker and | Kubernetes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubSettings {
    pub docker: DockerSettings,
    /// Docker CLI for compose, build and exec (`docker`).
    pub docker_cli: String,
    /// Kubeconfig path; empty: `$KUBECONFIG` or `~/.kube/config`.
    pub kubeconfig: String,
    pub kubectl: String,
    pub default_context: String,
    pub default_namespace: String,
    /// Registry user names by server (`docker.io` → `acme`); passwords are in the keychain.
    pub registries: BTreeMap<String, String>,
    /// In a remote project: connect to Docker on the project's host
    /// (`docker.remoteProjectHost`).
    #[serde(default)]
    pub project_docker: bool,
    /// In a remote project: Kubernetes of the project's host, with its kubeconfig and
    /// `kubectl` (`kubernetes.remoteProjectHost`).
    #[serde(default)]
    pub project_kube: bool,
    /// kubectl per host, `host<TAB>path` lines (`kubernetes.hostKubectlPaths`).
    #[serde(default)]
    pub host_kubectl: Vec<String>,
}

/// Where Kubernetes runs for a remote project: the host, its `user@host` label and the
/// kubectl there.
type KubePlace = (Arc<dyn ProjectHost>, String, String);

/// The project host whose Kubernetes is used, when the project is remote and
/// [`HubSettings::project_kube`] is on.
fn kube_place(settings: &HubSettings, project: &Option<Arc<dyn ProjectHost>>) -> Option<KubePlace> {
    if !settings.project_kube {
        return None;
    }
    let p = project.as_ref()?;
    let label = p.remote()?.to_string();
    let kubectl = kubectl::kubectl_for_host(&settings.host_kubectl, &label);
    Some((p.clone(), label, kubectl))
}

/// A connection to one kubeconfig context: the API server from this machine, or kubectl on
/// a remote project's host.
#[derive(Clone)]
enum KubeConn {
    Api(KubeClient),
    Cli(KubectlClient),
}

impl KubeConn {
    async fn namespaces(&self) -> Result<Vec<String>> {
        match self {
            KubeConn::Api(c) => c.namespaces().await,
            KubeConn::Cli(c) => c.namespaces().await,
        }
    }

    async fn workloads(&self, ns: &str) -> Result<kube::RawWorkloads> {
        match self {
            KubeConn::Api(c) => c.workloads(ns).await,
            KubeConn::Cli(c) => c.workloads(ns).await,
        }
    }

    async fn pod(&self, ns: &str, name: &str) -> Result<serde_json::Value> {
        match self {
            KubeConn::Api(c) => c.pod(ns, name).await,
            KubeConn::Cli(c) => c.get(ns, "pod", name).await,
        }
    }

    async fn deployment(&self, ns: &str, name: &str) -> Result<serde_json::Value> {
        match self {
            KubeConn::Api(c) => c.deployment(ns, name).await,
            KubeConn::Cli(c) => c.get(ns, "deployment", name).await,
        }
    }

    async fn service(&self, ns: &str, name: &str) -> Result<serde_json::Value> {
        match self {
            KubeConn::Api(c) => c.service(ns, name).await,
            KubeConn::Cli(c) => c.get(ns, "service", name).await,
        }
    }

    async fn delete_pod(&self, ns: &str, name: &str) -> Result<()> {
        match self {
            KubeConn::Api(c) => c.delete_pod(ns, name).await,
            KubeConn::Cli(c) => c.delete_pod(ns, name).await,
        }
    }

    async fn restart_deployment(&self, ns: &str, name: &str) -> Result<()> {
        match self {
            KubeConn::Api(c) => c.restart_deployment(ns, name, &rfc3339_now()).await,
            KubeConn::Cli(c) => c.restart_deployment(ns, name).await,
        }
    }

    async fn pods_matching(&self, ns: &str, selector: &str) -> Result<Vec<kube::Pod>> {
        match self {
            KubeConn::Api(c) => c.pods_matching(ns, selector).await,
            KubeConn::Cli(c) => c.pods_matching(ns, selector).await,
        }
    }

    async fn pod_logs(
        &self,
        ns: &str,
        pod: &str,
        container: Option<&str>,
        tail: u32,
        sink: impl FnMut(String) + Send,
    ) -> Result<()> {
        match self {
            KubeConn::Api(c) => c.pod_logs(ns, pod, container, tail, true, sink).await,
            KubeConn::Cli(c) => c.pod_logs(ns, pod, container, tail, sink).await,
        }
    }
}

/// Aborts a task when dropped (the host's `kubectl port-forward` of a stopped forward).
struct AbortOnDrop(tokio::task::AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Wait until a host's `kubectl port-forward` ends; never without one.
async fn kubectl_ended(task: &mut Option<JoinHandle<Result<()>>>) -> String {
    match task {
        Some(t) => match t.await {
            Ok(Err(e)) => e.0,
            Ok(Ok(())) => "kubectl port-forward ended".into(),
            Err(_) => "kubectl port-forward was stopped".into(),
        },
        None => std::future::pending().await,
    }
}

/// What the UI is told.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HubEvent {
    /// The tree changed (`services://changed`).
    Tree { nodes: Vec<Node> },
    /// Console output for a Log tab (`services://console`). `key` is `log:<nodeId>` for log
    /// streams and `run:<nodeId>` for compose / build output.
    Console {
        key: String,
        delta: ConsoleDelta,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        reset: bool,
    },
    /// A log stream or run ended.
    Ended { key: String },
    /// Docker connection changes (`services://status`) for the status bar.
    Status {
        state: ConnectionState,
        endpoint: String,
        error: Option<String>,
        version: Option<String>,
    },
    /// An action failed (balloon).
    Error { message: String },
}

pub type Emit = Arc<dyn Fn(HubEvent) + Send + Sync>;

const MAX_LINES: usize = 5000;

struct Console {
    output: RunOutput,
    lines: Vec<ConsoleLine>,
}

#[derive(Default)]
struct State {
    settings: HubSettings,
    project: Option<Arc<dyn ProjectHost>>,
    docker: tree::DockerInput,
    client: Option<DockerClient>,
    compose_files: Vec<ComposeFile>,
    kube: KubeInput,
    kube_clients: HashMap<String, KubeConn>,
    /// Context and namespace picked in the Services tool window (this session).
    kube_select: Option<(String, Option<String>)>,
    busy: Vec<String>,
    consoles: HashMap<String, Console>,
    streams: HashMap<String, JoinHandle<()>>,
    watcher: Option<JoinHandle<()>>,
    generation: u64,
    /// Active port forwards with their listener tasks.
    forwards: Vec<(PortForward, JoinHandle<()>)>,
    next_forward: u64,
}

pub struct Hub {
    state: Mutex<State>,
    emit: Emit,
    runtime: tokio::runtime::Handle,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn rfc3339_now() -> String {
    let s = crate::time::format_utc(now());
    format!("{}Z", s.replace(' ', "T"))
}

/// The process environment for [`endpoint::resolve`].
pub fn process_environment() -> endpoint::Environment {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    endpoint::Environment {
        docker_host: var("DOCKER_HOST"),
        docker_tls_verify: var("DOCKER_TLS_VERIFY").is_some_and(|v| v != "0"),
        docker_cert_path: var("DOCKER_CERT_PATH"),
        home: dirs::home_dir().map(|h| h.to_string_lossy().into_owned()),
        xdg_runtime_dir: var("XDG_RUNTIME_DIR"),
        #[cfg(unix)]
        uid: Some(current_uid()),
        #[cfg(not(unix))]
        uid: None,
        windows: cfg!(windows),
        contexts: docker_config_dir()
            .map(|d| load_contexts(&d, var("DOCKER_CONTEXT").as_deref()))
            .unwrap_or_default(),
    }
}

/// The Docker CLI's configuration directory: `$DOCKER_CONFIG` or `~/.docker`.
pub fn docker_config_dir() -> Option<PathBuf> {
    std::env::var_os("DOCKER_CONFIG")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".docker")))
}

/// Docker contexts of `<dir>/contexts/meta/*/meta.json` sorted by name, with their TLS
/// folders; the current one (`DOCKER_CONTEXT`, else `currentContext` of `config.json`) is
/// marked.
pub fn load_contexts(dir: &Path, docker_context: Option<&str>) -> Vec<endpoint::DockerContext> {
    let current = docker_context.map(str::to_string).or_else(|| {
        std::fs::read_to_string(dir.join("config.json"))
            .ok()
            .and_then(|t| endpoint::current_context_of_config(&t))
    });
    let mut out: Vec<endpoint::DockerContext> = std::fs::read_dir(dir.join("contexts/meta"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path().join("meta.json")).ok()?;
            let mut ctx = endpoint::parse_context_meta(&text)?;
            let tls = dir.join("contexts/tls").join(e.file_name()).join("docker");
            if tls.is_dir() {
                ctx.tls_dir = Some(tls.to_string_lossy().into_owned());
            }
            ctx.current = current.as_deref() == Some(ctx.name.as_str());
            Some(ctx)
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // `id -u` without libc: the owner of /proc/self.
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(1000)
}

/// Where a CLI command runs (`None`: this machine), the program and its environment.
type CliPlace = (Option<Arc<dyn ProjectHost>>, String, Vec<(String, String)>);

/// Compose files of a project folder on this machine (see [`find_compose_files_in`]).
pub fn find_compose_files(root: &Path) -> Vec<ComposeFile> {
    find_compose_files_in(&LocalProject::new(root))
}

/// Compose files of a project: the root and one directory below (not `vendor`,
/// `node_modules` or hidden ones), each directory's main file merged with its override file.
/// Reads through `project`, so a remote project's files are read on its host. Blocks.
pub fn find_compose_files_in(project: &dyn ProjectHost) -> Vec<ComposeFile> {
    let root = project.root();
    let mut dirs = vec![String::new()];
    let mut subs: Vec<String> = project
        .dirs()
        .into_iter()
        .filter(|n| {
            !n.starts_with('.')
                && !matches!(
                    n.as_str(),
                    "vendor" | "node_modules" | "target" | "storage" | "var"
                )
        })
        .collect();
    subs.sort();
    dirs.extend(subs);
    // Variables of this machine only apply to a project on this machine.
    let process_env: BTreeMap<String, String> = if project.remote().is_none() {
        std::env::vars().collect()
    } else {
        BTreeMap::new()
    };
    let join = |dir: &str, name: &str| {
        if dir.is_empty() {
            name.to_string()
        } else {
            format!("{dir}/{name}")
        }
    };
    let mut out = Vec::new();
    for dir in dirs {
        let Some((main, text)) = compose::COMPOSE_FILE_NAMES
            .iter()
            .map(|n| join(&dir, n))
            .find_map(|p| project.read(&p).map(|t| (p, t)))
        else {
            continue;
        };
        let mut env = process_env.clone();
        if let Some(text) = project.read(&join(&dir, ".env")) {
            env.extend(compose::parse_env_file(&text));
        }
        let mut file = compose::parse(&main, &text, &env);
        // The directory name comes from the absolute path.
        if !text.lines().any(|l| l.starts_with("name:")) {
            let dir_path = if dir.is_empty() {
                root.to_path_buf()
            } else {
                root.join(&dir)
            };
            let name = compose::normalize_project_name(
                &dir_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
            if !name.is_empty() {
                file.project = name;
            }
        }
        if let Some((over, t)) = compose::OVERRIDE_FILE_NAMES
            .iter()
            .map(|n| join(&dir, n))
            .find_map(|p| project.read(&p).map(|t| (p, t)))
        {
            let project = file.project.clone();
            file = compose::merge(file, compose::parse(&over, &t, &env));
            file.project = project;
            file.override_path = Some(over);
        }
        out.push(file);
    }
    out
}

/// Compose files of `project` read off the async runtime (a remote project's files come
/// over the network).
async fn load_compose_files(project: Option<Arc<dyn ProjectHost>>) -> Vec<ComposeFile> {
    match project {
        Some(p) => tokio::task::spawn_blocking(move || find_compose_files_in(p.as_ref()))
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| p.into()),
        None => p.into(),
    }
}

/// Kubeconfig files: the setting, `$KUBECONFIG` (path list) or `~/.kube/config`.
pub fn kubeconfig_paths(setting: &str) -> Vec<PathBuf> {
    if !setting.trim().is_empty() {
        return vec![expand_home(setting.trim())];
    }
    if let Some(v) = std::env::var_os("KUBECONFIG").filter(|v| !v.is_empty()) {
        return std::env::split_paths(&v).collect();
    }
    dirs::home_dir()
        .map(|h| vec![h.join(".kube").join("config")])
        .unwrap_or_default()
}

/// The kubeconfig for the tree: this machine's files, or for a remote project's host the
/// place it comes from (read later with [`Hub::load_host_kube`]).
fn fresh_kube(settings: &HubSettings, place: Option<&KubePlace>) -> KubeInput {
    match place {
        None => load_kube(settings),
        Some((_, label, kubectl)) => KubeInput {
            path: "$KUBECONFIG or ~/.kube/config".into(),
            host: Some(label.clone()),
            kubectl: Some(kubectl.clone()),
            loading: true,
            default_context: Some(settings.default_context.clone()).filter(|s| !s.is_empty()),
            default_namespace: Some(settings.default_namespace.clone()).filter(|s| !s.is_empty()),
            ..Default::default()
        },
    }
}

fn load_kube(settings: &HubSettings) -> KubeInput {
    let paths = kubeconfig_paths(&settings.kubeconfig);
    let shown = paths
        .iter()
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            match dirs::home_dir().map(|h| h.to_string_lossy().into_owned()) {
                Some(h) if s.starts_with(&h) => format!("~{}", &s[h.len()..]),
                _ => s,
            }
        })
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { ";" } else { ":" });
    let mut input = KubeInput {
        path: shown,
        default_context: Some(settings.default_context.clone()).filter(|s| !s.is_empty()),
        default_namespace: Some(settings.default_namespace.clone()).filter(|s| !s.is_empty()),
        ..Default::default()
    };
    let mut config: Option<KubeConfig> = None;
    for p in &paths {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        let base = p
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        match kube::parse_kubeconfig(&text, &base) {
            Ok(c) => match &mut config {
                Some(existing) => existing.merge(c),
                None => config = Some(c),
            },
            Err(e) => input.error = Some(format!("{}: {e}", p.display())),
        }
    }
    input.config = config;
    input
}

impl Hub {
    pub fn new(emit: Emit, runtime: tokio::runtime::Handle) -> Arc<Self> {
        Arc::new(Hub {
            state: Mutex::new(State::default()),
            emit,
            runtime,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn input(&self) -> ServicesInput {
        let s = self.lock();
        ServicesInput {
            docker: s.docker.clone(),
            compose_files: s.compose_files.clone(),
            kube: s.kube.clone(),
            now: now(),
            busy: s.busy.clone(),
            port_forwards: s.forwards.iter().map(|(f, _)| f.clone()).collect(),
        }
    }

    pub fn tree(&self) -> Vec<Node> {
        tree::build_tree(&self.input())
    }

    pub fn details(&self, node_id: &str) -> Option<Details> {
        let mut d = tree::details(&self.input(), node_id)?;
        // Build output of Dockerfile runs and pulls lands on the Docker node.
        if d.target == Target::Docker && !d.tabs.contains(&tree::Tab::Log) {
            d.tabs.push(tree::Tab::Log);
        }
        Some(d)
    }

    fn emit_tree(&self) {
        (self.emit)(HubEvent::Tree { nodes: self.tree() });
    }

    fn emit_status(&self) {
        let s = self.lock();
        let ev = HubEvent::Status {
            state: s.docker.state,
            endpoint: s.docker.endpoint.clone(),
            error: s.docker.error.clone(),
            version: s
                .docker
                .snapshot
                .as_ref()
                .and_then(|x| x.version.as_ref())
                .map(|v| v.version.clone()),
        };
        drop(s);
        (self.emit)(ev);
    }

    pub fn status(&self) -> HubEvent {
        let s = self.lock();
        HubEvent::Status {
            state: s.docker.state,
            endpoint: s.docker.endpoint.clone(),
            error: s.docker.error.clone(),
            version: s
                .docker
                .snapshot
                .as_ref()
                .and_then(|x| x.version.as_ref())
                .map(|v| v.version.clone()),
        }
    }

    /// Apply settings and the open project; restarts the Docker watcher when the connection
    /// settings (or the remote host whose Docker is used) changed. Reads the project's
    /// compose files, so call it off the async runtime.
    pub fn configure(
        self: &Arc<Self>,
        settings: HubSettings,
        project: Option<Arc<dyn ProjectHost>>,
    ) {
        let compose_files = project
            .as_deref()
            .map(find_compose_files_in)
            .unwrap_or_default();
        let restart = {
            let mut s = self.lock();
            let host = |p: &Option<Arc<dyn ProjectHost>>| {
                p.as_ref().and_then(|p| p.remote().map(str::to_string))
            };
            let restart = s.settings.docker != settings.docker
                || s.settings.project_docker != settings.project_docker
                || (settings.project_docker && host(&s.project) != host(&project))
                || s.watcher.is_none();
            let place = kube_place(&settings, &project);
            let label = |p: &Option<KubePlace>| p.as_ref().map(|(_, h, k)| (h.clone(), k.clone()));
            let kube_changed = s.settings.kubeconfig != settings.kubeconfig
                || s.settings.default_context != settings.default_context
                || s.settings.default_namespace != settings.default_namespace
                || label(&kube_place(&s.settings, &s.project)) != label(&place)
                || (s.kube.config.is_none() && !s.kube.loading);
            let mut load_host = None;
            if kube_changed {
                s.kube = fresh_kube(&settings, place.as_ref());
                s.kube_clients.clear();
                s.kube_select = None;
                load_host = place;
            }
            s.compose_files = compose_files;
            s.settings = settings;
            s.project = project;
            (restart, load_host)
        };
        let (restart, load_host) = restart;
        if restart {
            self.start_watcher();
        }
        self.emit_tree();
        if let Some((host, _, kubectl)) = load_host {
            let hub = self.clone();
            self.runtime
                .spawn(async move { hub.load_host_kube(host, kubectl).await });
        }
    }

    /// Read the kubeconfig of a remote project's host (`kubectl config view`) into the
    /// tree. Ignored when the project or kubectl changed meanwhile.
    async fn load_host_kube(self: &Arc<Self>, host: Arc<dyn ProjectHost>, program: String) {
        let r = kubectl::config_view(host.as_ref(), &program).await;
        {
            let mut s = self.lock();
            if s.kube.host.as_deref() != host.remote()
                || s.kube.kubectl.as_deref() != Some(&program)
            {
                return;
            }
            s.kube.loading = false;
            match r {
                Ok(c) => {
                    s.kube.config = Some(c);
                    s.kube.error = None;
                }
                Err(e) => {
                    s.kube.config = None;
                    s.kube.error = Some(e.0);
                }
            }
        }
        self.emit_tree();
    }

    /// The selected (or current) context and its namespace, after a reload.
    fn apply_selection(s: &mut State) {
        if let Some((c, ns)) = s.kube_select.clone() {
            s.kube.default_context = Some(c);
            s.kube.default_namespace = ns;
        }
    }

    /// Re-read compose files and the kubeconfig and refresh Docker.
    pub async fn refresh(self: &Arc<Self>) {
        let project = self.lock().project.clone();
        let compose_files = load_compose_files(project).await;
        let place = {
            let mut s = self.lock();
            s.compose_files = compose_files;
            let settings = s.settings.clone();
            let place = kube_place(&settings, &s.project);
            let namespaces = std::mem::take(&mut s.kube.namespaces);
            let workloads = std::mem::take(&mut s.kube.workloads);
            s.kube = fresh_kube(&settings, place.as_ref());
            s.kube.namespaces = namespaces;
            s.kube.workloads = workloads;
            Self::apply_selection(&mut s);
            place
        };
        if let Some((host, _, kubectl)) = place {
            self.emit_tree();
            self.load_host_kube(host, kubectl).await;
        }
        if self.lock().client.is_some() {
            let _ = self.refresh_docker().await;
        } else {
            self.start_watcher();
        }
        self.reload_loaded_kube().await;
        self.emit_tree();
    }

    /// Reload the namespaces and workloads that are shown.
    async fn reload_loaded_kube(self: &Arc<Self>) {
        let (contexts, keys): (Vec<String>, Vec<String>) = {
            let s = self.lock();
            (
                s.kube.namespaces.keys().cloned().collect(),
                s.kube.workloads.keys().cloned().collect(),
            )
        };
        for c in contexts {
            self.load_namespaces(&c).await;
        }
        for k in keys {
            if let Some((c, ns)) = k.split_once('\u{1f}') {
                self.load_workloads(c, ns).await;
            }
        }
    }

    /// Compose files changed: read them again (in the background) and update the tree.
    pub fn compose_files_changed(self: &Arc<Self>) {
        let hub = self.clone();
        self.runtime.spawn(async move {
            let project = hub.lock().project.clone();
            let compose_files = load_compose_files(project).await;
            hub.lock().compose_files = compose_files;
            hub.emit_tree();
        });
    }

    /// The daemon to use: the project host's in a remote project with
    /// [`HubSettings::project_docker`], else the one of the settings / environment.
    fn endpoint(s: &State, env: &endpoint::Environment) -> Result<Endpoint> {
        if let (true, Some(via)) = (
            s.settings.project_docker,
            s.project.as_ref().and_then(|p| p.remote()),
        ) {
            let socket = s.settings.docker.ssh_socket.trim();
            return Ok(Endpoint::ProjectHost {
                via: via.to_string(),
                socket: if socket.is_empty() {
                    endpoint::DEFAULT_REMOTE_SOCKET.to_string()
                } else {
                    socket.to_string()
                },
            });
        }
        endpoint::resolve(&s.settings.docker, env, &|p| Path::new(p).exists())
    }

    fn client_for(s: &State, ep: &Endpoint) -> Result<DockerClient> {
        match (ep, &s.project) {
            (Endpoint::ProjectHost { socket, .. }, Some(project)) => {
                Ok(DockerClient::with_transport(
                    crate::http::Transport::Project(project.clone(), socket.clone()),
                    ep.clone(),
                ))
            }
            _ => DockerClient::new(ep),
        }
    }

    fn start_watcher(self: &Arc<Self>) {
        let generation = {
            let mut s = self.lock();
            if let Some(w) = s.watcher.take() {
                w.abort();
            }
            s.generation += 1;
            s.client = None;
            let env = process_environment();
            let endpoint = Self::endpoint(&s, &env);
            match endpoint {
                Ok(ep) => {
                    s.docker = tree::DockerInput {
                        state: ConnectionState::Connecting,
                        endpoint: ep.label(),
                        error: None,
                        snapshot: None,
                        contexts: env.contexts,
                    };
                    match Self::client_for(&s, &ep) {
                        Ok(c) => s.client = Some(c),
                        Err(e) => {
                            s.docker.state = ConnectionState::Failed;
                            s.docker.error = Some(e.0);
                        }
                    }
                }
                Err(e) => {
                    s.docker = tree::DockerInput {
                        state: ConnectionState::Failed,
                        endpoint: String::new(),
                        error: Some(e.0),
                        snapshot: None,
                        contexts: env.contexts,
                    };
                }
            }
            s.generation
        };
        self.emit_status();
        if self.lock().client.is_none() {
            return;
        }
        let hub = self.clone();
        let task = self.runtime.spawn(async move {
            loop {
                let ok = hub.refresh_docker().await.is_ok();
                hub.emit_tree();
                if !ok {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
                let Some(client) = hub.lock().client.clone() else {
                    return;
                };
                // Follow events; refresh (debounced) after changes.
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
                let follower = tokio::spawn(async move {
                    let _ = client
                        .events(|_| {
                            let _ = tx.send(());
                        })
                        .await;
                });
                while rx.recv().await.is_some() {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    while rx.try_recv().is_ok() {}
                    if hub.lock().generation != generation {
                        return;
                    }
                    let _ = hub.refresh_docker().await;
                    hub.emit_tree();
                }
                follower.abort();
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
        self.lock().watcher = Some(task);
    }

    /// Fetch a fresh snapshot; updates the connection state.
    pub async fn refresh_docker(&self) -> Result<()> {
        let Some(client) = self.lock().client.clone() else {
            return err("Docker is not configured");
        };
        let result = client.snapshot().await;
        let changed_state;
        {
            let mut s = self.lock();
            let before = s.docker.state;
            match &result {
                Ok(raw) => {
                    s.docker.state = ConnectionState::Connected;
                    s.docker.error = None;
                    s.docker.snapshot = Some(raw.parse());
                }
                Err(e) => {
                    s.docker.state = ConnectionState::Failed;
                    s.docker.error = Some(e.0.clone());
                    s.docker.snapshot = None;
                }
            }
            changed_state = before != s.docker.state;
        }
        if changed_state {
            self.emit_status();
        }
        result.map(|_| ())
    }

    /// The raw Engine responses (for tests and debugging).
    pub async fn raw_snapshot(&self) -> Result<RawSnapshot> {
        let Some(client) = self.lock().client.clone() else {
            return err("Docker is not configured");
        };
        client.snapshot().await
    }

    // ---- Kubernetes ---------------------------------------------------------------------

    async fn kube_client(&self, context: &str) -> Result<KubeConn> {
        if let Some(c) = self.lock().kube_clients.get(context) {
            return Ok(c.clone());
        }
        let (rc, place) = {
            let s = self.lock();
            let place = kube_place(&s.settings, &s.project);
            let Some(cfg) = &s.kube.config else {
                return err(match &place {
                    Some((_, h, _)) if s.kube.loading => {
                        format!("The kubeconfig of {h} is still being read")
                    }
                    Some((_, h, _)) => format!("No kubeconfig found on {h}"),
                    None => "No kubeconfig found".into(),
                });
            };
            match place {
                // kubectl on the host resolves the context itself.
                Some(p) => {
                    if cfg.context(context).is_none() {
                        return err(format!(
                            "Context `{context}` is not in the kubeconfig of {}",
                            p.1
                        ));
                    }
                    (None, Some(p))
                }
                None => (Some(cfg.resolve(context)?), None),
            }
        };
        let client = match (rc, place) {
            (_, Some((host, _, program))) => KubeConn::Cli(KubectlClient {
                host,
                program,
                context: context.to_string(),
            }),
            (Some(rc), None) => KubeConn::Api(KubeClient::new(&rc).await?),
            (None, None) => return err("No kubeconfig found"),
        };
        self.lock()
            .kube_clients
            .insert(context.to_string(), client.clone());
        Ok(client)
    }

    async fn load_namespaces(&self, context: &str) {
        self.lock()
            .kube
            .namespaces
            .insert(context.into(), Loaded::Loading);
        self.emit_tree();
        let result = match self.kube_client(context).await {
            Ok(c) => c.namespaces().await,
            Err(e) => Err(e),
        };
        let loaded = match result {
            Ok(value) => Loaded::Ready { value },
            Err(e) => Loaded::Failed { error: e.0 },
        };
        self.lock().kube.namespaces.insert(context.into(), loaded);
    }

    async fn load_workloads(&self, context: &str, ns: &str) {
        let key = tree::workloads_key(context, ns);
        self.lock()
            .kube
            .workloads
            .insert(key.clone(), Loaded::Loading);
        self.emit_tree();
        let result = match self.kube_client(context).await {
            Ok(c) => c.workloads(ns).await,
            Err(e) => Err(e),
        };
        let loaded = match result {
            Ok(raw) => Loaded::Ready { value: raw.parse() },
            Err(e) => Loaded::Failed { error: e.0 },
        };
        self.lock().kube.workloads.insert(key, loaded);
    }

    /// Load the children of a lazy node (kube contexts and namespaces).
    pub async fn expand(self: &Arc<Self>, node_id: &str) -> Vec<Node> {
        match Target::parse(node_id) {
            Some(Target::KubeContext { context }) => {
                self.load_namespaces(&context).await;
                // Load the default namespace right away, like kubectl would show it.
                let ns = self.lock().kube.namespace_of(&context);
                self.load_workloads(&context, &ns).await;
            }
            Some(Target::KubeNamespace { context, namespace }) => {
                self.load_workloads(&context, &namespace).await
            }
            _ => {}
        }
        let nodes = self.tree();
        (self.emit)(HubEvent::Tree {
            nodes: nodes.clone(),
        });
        nodes
    }

    // ---- consoles -----------------------------------------------------------------------

    /// Lines of a console (`log:<nodeId>` / `run:<nodeId>`).
    pub fn console(&self, key: &str) -> Vec<ConsoleLine> {
        self.lock()
            .consoles
            .get(key)
            .map(|c| c.lines.clone())
            .unwrap_or_default()
    }

    fn console_input(&self, key: &str, input: RunInput, reset: bool) {
        let delta = {
            let mut s = self.lock();
            let root = s
                .project
                .as_ref()
                .map(|p| p.root().to_string_lossy().into_owned());
            if reset {
                s.consoles.remove(key);
            }
            let console = s
                .consoles
                .entry(key.to_string())
                .or_insert_with(|| Console {
                    output: RunOutput::new(&RunOutputOptions {
                        test_runner: false,
                        root,
                        cwd: None,
                        cycle_buffer_kb: None,
                        ..Default::default()
                    }),
                    lines: Vec::new(),
                });
            let mut merged = ConsoleDelta::default();
            for e in console.output.handle(&input) {
                if let OutputEvent::Console(d) = e {
                    d.apply_to(&mut console.lines);
                    if d.replace_last && merged.lines.is_empty() {
                        merged.replace_last = true;
                    } else if d.replace_last {
                        merged.lines.pop();
                    }
                    merged.lines.extend(d.lines);
                }
            }
            if console.lines.len() > MAX_LINES {
                let extra = console.lines.len() - MAX_LINES;
                console.lines.drain(..extra);
            }
            merged
        };
        if !delta.is_empty() || reset {
            (self.emit)(HubEvent::Console {
                key: key.to_string(),
                delta,
                reset,
            });
        }
    }

    fn console_text(&self, key: &str, stream: LogStream, text: String) {
        self.console_input(
            key,
            RunInput::Output {
                stream: stream.name().into(),
                text,
            },
            false,
        );
    }

    /// A loaded pod of the tree.
    fn loaded_pod(&self, context: &str, ns: &str, name: &str) -> Option<kube::Pod> {
        match self
            .lock()
            .kube
            .workloads
            .get(&tree::workloads_key(context, ns))
        {
            Some(Loaded::Ready { value }) => value.pods.iter().find(|p| p.name == name).cloned(),
            _ => None,
        }
    }

    /// The container of a pod for logs / exec / attach: the chosen one, else the default
    /// container (annotation or the first one) when the pod is loaded.
    fn pod_container(
        &self,
        context: &str,
        ns: &str,
        pod: &str,
        container: Option<&str>,
    ) -> Option<String> {
        match self.loaded_pod(context, ns, pod) {
            Some(p) => p.pick_container(container).map(str::to_string),
            None => container.map(str::to_string),
        }
    }

    /// Start (or restart) streaming the logs of a container, compose service, pod (one of
    /// its containers) or deployment (all containers of all its pods, prefixed) into
    /// `log:<nodeId>`.
    pub fn start_logs(
        self: &Arc<Self>,
        node_id: &str,
        tail: u32,
        container: Option<&str>,
    ) -> Result<String> {
        let target =
            Target::parse(node_id).ok_or_else(|| Error(format!("Unknown node {node_id}")))?;
        let key = format!("log:{node_id}");
        if let Some(h) = self.lock().streams.remove(&key) {
            h.abort();
        }
        self.console_input(
            &key,
            RunInput::Output {
                stream: "stdout".into(),
                text: String::new(),
            },
            true,
        );
        let hub = self.clone();
        let k = key.clone();
        let task: JoinHandle<()> = match target {
            Target::Container { id } => {
                let client = self
                    .lock()
                    .client
                    .clone()
                    .ok_or_else(|| Error("Docker is not connected".into()))?;
                self.runtime.spawn(async move {
                    let h = hub.clone();
                    let kk = k.clone();
                    let r = client
                        .logs(&id, tail, true, false, move |chunks| {
                            for c in chunks {
                                h.console_text(&kk, c.stream, c.text);
                            }
                        })
                        .await;
                    hub.stream_ended(&k, r);
                })
            }
            Target::ComposeService { project, service } => {
                let input = self.input();
                let id = tree::compose_projects(&input)
                    .iter()
                    .find(|p| p.name == project)
                    .and_then(|p| {
                        p.containers
                            .get(&service)
                            .and_then(|c| c.first().map(|c| c.id.clone()))
                    })
                    .ok_or_else(|| Error(format!("Service {service} has no container yet")))?;
                let client = self
                    .lock()
                    .client
                    .clone()
                    .ok_or_else(|| Error("Docker is not connected".into()))?;
                self.runtime.spawn(async move {
                    let h = hub.clone();
                    let kk = k.clone();
                    let r = client
                        .logs(&id, tail, true, false, move |chunks| {
                            for c in chunks {
                                h.console_text(&kk, c.stream, c.text);
                            }
                        })
                        .await;
                    hub.stream_ended(&k, r);
                })
            }
            Target::Pod {
                context,
                namespace,
                name,
            } => {
                let container = self.pod_container(&context, &namespace, &name, container);
                self.runtime.spawn(async move {
                    let r = match hub.kube_client(&context).await {
                        Ok(c) => {
                            let h = hub.clone();
                            let kk = k.clone();
                            c.pod_logs(&namespace, &name, container.as_deref(), tail, move |t| {
                                h.console_text(&kk, LogStream::Stdout, t)
                            })
                            .await
                        }
                        Err(e) => Err(e),
                    };
                    hub.stream_ended(&k, r);
                })
            }
            Target::Deployment {
                context,
                namespace,
                name,
            } => {
                let selector = match self
                    .lock()
                    .kube
                    .workloads
                    .get(&tree::workloads_key(&context, &namespace))
                {
                    Some(Loaded::Ready { value }) => value
                        .deployments
                        .iter()
                        .find(|d| d.name == name)
                        .map(|d| d.label_selector()),
                    _ => None,
                };
                self.runtime.spawn(async move {
                    let r = hub
                        .deployment_logs(&k, &context, &namespace, &name, selector, tail)
                        .await;
                    hub.stream_ended(&k, r);
                })
            }
            _ => return err("This node has no logs"),
        };
        self.lock().streams.insert(key.clone(), task);
        Ok(key)
    }

    /// `kubectl logs deploy/<name> --all-containers --prefix -f`: every container of every
    /// pod of the deployment, each line prefixed with `[pod/<pod>/<container>]`.
    async fn deployment_logs(
        self: &Arc<Self>,
        key: &str,
        context: &str,
        ns: &str,
        name: &str,
        selector: Option<String>,
        tail: u32,
    ) -> Result<()> {
        let client = self.kube_client(context).await?;
        let selector = match selector {
            Some(s) => s,
            None => {
                let d = client.deployment(ns, name).await?;
                let list = serde_json::json!({ "items": [d] });
                kube::parse_deployments(&list)
                    .first()
                    .map(|d| d.label_selector())
                    .unwrap_or_default()
            }
        };
        if selector.is_empty() {
            return err(format!("Deployment {name} has no label selector"));
        }
        let pods = client.pods_matching(ns, &selector).await?;
        if pods.is_empty() {
            self.console_text(
                key,
                LogStream::Stdout,
                format!("No pods of deployment {name} found ({selector})\n"),
            );
            return Ok(());
        }
        let mut streams = tokio::task::JoinSet::new();
        for pod in &pods {
            for container in &pod.containers {
                let (hub, c, key) = (self.clone(), client.clone(), key.to_string());
                let (ns, pod, container) = (ns.to_string(), pod.name.clone(), container.clone());
                streams.spawn(async move {
                    let prefixer = Arc::new(Mutex::new(LinePrefixer::new(pod_log_prefix(
                        &pod, &container,
                    ))));
                    let (h, kk, p) = (hub.clone(), key.clone(), prefixer.clone());
                    let r = c
                        .pod_logs(&ns, &pod, Some(&container), tail, move |t| {
                            let lines = p.lock().unwrap_or_else(|e| e.into_inner()).push(&t);
                            if !lines.is_empty() {
                                h.console_text(&kk, LogStream::Stdout, lines);
                            }
                        })
                        .await;
                    let rest = prefixer.lock().unwrap_or_else(|e| e.into_inner()).finish();
                    if !rest.is_empty() {
                        hub.console_text(&key, LogStream::Stdout, rest);
                    }
                    if let Err(e) = r {
                        hub.console_text(
                            &key,
                            LogStream::Stderr,
                            format!("{}{e}\n", pod_log_prefix(&pod, &container)),
                        );
                    }
                });
            }
        }
        while streams.join_next().await.is_some() {}
        Ok(())
    }

    fn stream_ended(&self, key: &str, r: Result<()>) {
        if let Err(e) = r {
            self.console_text(key, LogStream::Stderr, format!("{e}\n"));
        }
        self.lock().streams.remove(key);
        (self.emit)(HubEvent::Ended {
            key: key.to_string(),
        });
    }

    pub fn stop_logs(&self, node_id: &str) {
        let key = format!("log:{node_id}");
        if let Some(h) = self.lock().streams.remove(&key) {
            h.abort();
        }
    }

    // ---- actions ------------------------------------------------------------------------

    fn set_busy(&self, node_id: &str, busy: bool) {
        {
            let mut s = self.lock();
            s.busy.retain(|b| b != node_id);
            if busy {
                s.busy.push(node_id.to_string());
            }
        }
        self.emit_tree();
    }

    /// The docker CLI and its environment for commands run on this machine (or, with the
    /// project host's daemon, on that host, where `docker` is on its `PATH`).
    fn docker_cli(&self) -> (String, Vec<(String, String)>) {
        let s = self.lock();
        let endpoint = Self::endpoint(&s, &process_environment());
        let on_host = matches!(endpoint, Ok(Endpoint::ProjectHost { .. }));
        let cli = if s.settings.docker_cli.trim().is_empty() || on_host {
            "docker".to_string()
        } else {
            s.settings.docker_cli.clone()
        };
        let env = endpoint.map(|e| e.cli_env()).unwrap_or_default();
        (cli, env)
    }

    /// Where compose and build commands run: on a remote project's host when the files
    /// are there (`project_files`) or Docker of the host is used; else on this machine.
    /// The command line (`docker`, its environment) to use there.
    fn cli_for(&self, project_files: bool) -> CliPlace {
        let (cli, env) = self.docker_cli();
        let s = self.lock();
        let remote = s.project.clone().filter(|p| p.remote().is_some());
        let host_docker = s.settings.project_docker && remote.is_some();
        match remote {
            Some(p) if host_docker => (Some(p), cli, env),
            // The host's own Docker: no DOCKER_HOST of this machine.
            Some(p) if project_files => (Some(p), "docker".into(), Vec::new()),
            _ => (None, cli, env),
        }
    }

    /// Run a CLI command, streaming into `run:<node_id>`.
    async fn run_cli(
        &self,
        node_id: &str,
        on: Option<&dyn ProjectHost>,
        program: &str,
        args: &[String],
        env: &[(String, String)],
        cwd: &Path,
    ) -> Result<()> {
        let key = format!("run:{node_id}");
        self.console_input(
            &key,
            RunInput::Started {
                command_line: compose::display_command(program, args),
            },
            false,
        );
        if let Some(via) = on.and_then(|h| h.remote()) {
            self.console_text(
                &key,
                LogStream::Stdout,
                format!("Running on {via} in {}\n", cwd.display()),
            );
        }
        // Compose and BuildKit write their progress to stderr; it is not an error there.
        let sink = |_stream, text| self.console_text(&key, LogStream::Stdout, text);
        let code = match on {
            Some(host) => {
                let mut env = env.to_vec();
                env.push(("COMPOSE_PROGRESS".into(), "plain".into()));
                env.push(("BUILDKIT_PROGRESS".into(), "plain".into()));
                host.run(program, args, &env, cwd, Box::new(sink)).await
            }
            None => crate::cli::run_streaming(program, args, env, cwd, sink).await,
        };
        match code {
            Ok(code) => {
                self.console_input(&key, RunInput::Exited { code, signal: None }, false);
                (self.emit)(HubEvent::Ended { key });
                if code == Some(0) {
                    Ok(())
                } else {
                    err(format!(
                        "{} failed (exit code {})",
                        compose::display_command(program, &args[..args.len().min(4)]),
                        code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())
                    ))
                }
            }
            Err(e) => {
                self.console_text(&key, LogStream::Stderr, format!("{e}\n"));
                (self.emit)(HubEvent::Ended { key });
                Err(e)
            }
        }
    }

    /// Run a compose command for a project (all services or some).
    pub async fn compose(
        &self,
        project: &str,
        action: ComposeAction,
        services: Vec<String>,
    ) -> Result<()> {
        let (file, root) = {
            let s = self.lock();
            (
                s.compose_files
                    .iter()
                    .find(|f| f.project == project)
                    .cloned(),
                s.project.as_ref().map(|p| p.root().to_path_buf()),
            )
        };
        let (host, cli, env) = self.cli_for(file.is_some());
        let (files, cwd) = match (&file, &root) {
            (Some(f), Some(root)) => {
                let path = root.join(&f.path);
                let dir = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| root.clone());
                let mut files = vec![path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()];
                // The override file was merged when the files were read.
                if let Some(over) = f.override_path.as_deref() {
                    files.push(over.rsplit('/').next().unwrap_or(over).to_string());
                }
                (files, dir)
            }
            _ => {
                // A project known only from container labels.
                let input = self.input();
                let c = input
                    .docker
                    .snapshot
                    .as_ref()
                    .and_then(|s| {
                        s.containers
                            .iter()
                            .find_map(|c| c.compose.clone().filter(|r| r.project == project))
                    })
                    .ok_or_else(|| Error(format!("Compose project {project} not found")))?;
                let cwd = c
                    .working_dir
                    .clone()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                (c.config_files.clone(), cwd)
            }
        };
        let cmd = ComposeCommand {
            files,
            project: Some(project.to_string()),
            profiles: file
                .as_ref()
                .map(|f| {
                    // Selected services with a profile need it enabled.
                    let mut p: Vec<String> = f
                        .services
                        .iter()
                        .filter(|s| services.contains(&s.name))
                        .flat_map(|s| s.profiles.clone())
                        .collect();
                    p.sort();
                    p.dedup();
                    p
                })
                .unwrap_or_default(),
            env_file: None,
            action,
            services: services.clone(),
            build: false,
        };
        let node = Target::ComposeProject {
            project: project.to_string(),
        }
        .node_id();
        let busy: Vec<String> = if services.is_empty() {
            vec![node.clone()]
        } else {
            services
                .iter()
                .map(|s| {
                    Target::ComposeService {
                        project: project.into(),
                        service: s.clone(),
                    }
                    .node_id()
                })
                .collect()
        };
        for b in &busy {
            self.set_busy(b, true);
        }
        let r = self
            .run_cli(
                &node,
                host.as_deref(),
                &cli,
                &compose::compose_args(&cmd),
                &env,
                &cwd,
            )
            .await;
        for b in &busy {
            self.set_busy(b, false);
        }
        let _ = self.refresh_docker().await;
        self.emit_tree();
        r
    }

    /// Compose file ▶: up one service or all (`service` = None) of the file at `path`
    /// (workspace relative).
    pub async fn compose_up_file(&self, path: &str, service: Option<String>) -> Result<()> {
        let project = {
            let s = self.lock();
            s.compose_files
                .iter()
                .find(|f| f.path == path)
                .map(|f| f.project.clone())
        };
        let project = match project {
            Some(p) => p,
            None => {
                // A compose file outside the scanned directories: parse it now.
                let (root, text) = self.read_project_file(path).await?;
                let mut f = compose::parse(path, &text, &Default::default());
                if !text.lines().any(|l| l.starts_with("name:")) {
                    let dir = root
                        .join(path)
                        .parent()
                        .and_then(|d| d.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    f.project = compose::normalize_project_name(&dir);
                }
                let p = f.project.clone();
                self.lock().compose_files.push(f);
                p
            }
        };
        self.compose(&project, ComposeAction::Up, service.into_iter().collect())
            .await
    }

    /// The project root and a project file's text, read off the async runtime.
    async fn read_project_file(&self, path: &str) -> Result<(PathBuf, String)> {
        let project = self
            .lock()
            .project
            .clone()
            .ok_or_else(|| Error("No project is open".into()))?;
        let root = project.root().to_path_buf();
        let rel = path.to_string();
        let text = tokio::task::spawn_blocking(move || project.read(&rel))
            .await
            .map_err(|e| Error(e.to_string()))?
            .ok_or_else(|| Error(format!("{path} was not found")))?;
        Ok((root, text))
    }

    /// Dockerfile ▶: `docker build` then `docker run -d` (replacing an old container of the
    /// same name). Output goes to the Docker node's Log tab.
    pub async fn dockerfile_run(&self, path: &str) -> Result<()> {
        let (root, content) = self.read_project_file(path).await?;
        let project = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let plan = gutter::dockerfile_run(path, &project, &content);
        let (host, cli, env) = self.cli_for(true);
        let node = Target::Docker.node_id();
        self.set_busy(&node, true);
        let mut r = self
            .run_cli(&node, host.as_deref(), &cli, &plan.build_args, &env, &root)
            .await;
        if r.is_ok() {
            let client = self.lock().client.clone();
            if let Some(client) = client {
                let _ = client.remove_container(&plan.container, true).await;
            }
            r = self
                .run_cli(&node, host.as_deref(), &cli, &plan.run_args, &env, &root)
                .await;
        }
        self.set_busy(&node, false);
        let _ = self.refresh_docker().await;
        self.emit_tree();
        r
    }

    /// Pull an image with progress in the Docker node's Log tab.
    pub async fn pull(&self, image: &str) -> Result<()> {
        let client = self
            .lock()
            .client
            .clone()
            .ok_or_else(|| Error("Docker is not connected".into()))?;
        let registry = crate::secrets::registry_of(image);
        let auth = {
            let s = self.lock();
            s.settings.registries.get(&registry).cloned()
        }
        .and_then(|user| {
            crate::secrets::password(&registry).map(|pw| registry_auth(&registry, &user, &pw))
        });
        let key = format!("run:{}", Target::Docker.node_id());
        self.console_input(
            &key,
            RunInput::Started {
                command_line: format!("Pulling image {image}"),
            },
            false,
        );
        let r = client
            .pull(image, auth, |lines, _| {
                for l in lines {
                    self.console_text(&key, LogStream::Stdout, format!("{l}\n"));
                }
            })
            .await;
        match &r {
            Ok(_) => self.console_text(&key, LogStream::Stdout, format!("Image {image} pulled\n")),
            Err(e) => self.console_text(&key, LogStream::Stderr, format!("Pull failed: {e}\n")),
        }
        (self.emit)(HubEvent::Ended { key });
        let _ = self.refresh_docker().await;
        self.emit_tree();
        r.map(|_| ())
    }

    /// Run a toolbar action on a node.
    pub async fn action(self: &Arc<Self>, node_id: &str, action: Action) -> Result<()> {
        let target =
            Target::parse(node_id).ok_or_else(|| Error(format!("Unknown node {node_id}")))?;
        let client = || {
            self.lock()
                .client
                .clone()
                .ok_or_else(|| Error("Docker is not connected".into()))
        };
        let r = match (&target, action) {
            (Target::Docker, Action::Connect | Action::Refresh) => {
                self.start_watcher();
                Ok(())
            }
            (
                Target::Container { id },
                Action::Start | Action::Stop | Action::Restart | Action::Remove,
            ) => {
                let c = client()?;
                self.set_busy(node_id, true);
                let r = match action {
                    Action::Start => c.container(id, ContainerOp::Start).await,
                    Action::Stop => c.container(id, ContainerOp::Stop).await,
                    Action::Restart => c.container(id, ContainerOp::Restart).await,
                    _ => c.remove_container(id, true).await,
                };
                self.set_busy(node_id, false);
                r
            }
            (Target::Container { id }, Action::Pull) => {
                let image = self
                    .input()
                    .docker
                    .snapshot
                    .and_then(|s| {
                        s.containers
                            .into_iter()
                            .find(|c| &c.id == id)
                            .map(|c| c.image)
                    })
                    .ok_or_else(|| Error("Unknown container".into()))?;
                self.pull(&image).await
            }
            (Target::Image { id }, Action::Pull) => {
                let tag = self
                    .input()
                    .docker
                    .snapshot
                    .and_then(|s| {
                        s.images
                            .into_iter()
                            .find(|i| &i.id == id)
                            .map(|i| i.label())
                    })
                    .filter(|t| t != "<none>:<none>")
                    .ok_or_else(|| Error("This image has no tag to pull".into()))?;
                self.pull(&tag).await
            }
            (Target::Image { id }, Action::Remove) => client()?.remove_image(id, false).await,
            (Target::Volume { name }, Action::Remove) => client()?.remove_volume(name).await,
            (Target::Network { id }, Action::Remove) => client()?.remove_network(id).await,
            (Target::ComposeProject { project }, a) => {
                let a = compose_action(a)?;
                self.compose(project, a, Vec::new()).await
            }
            (Target::ComposeService { project, service }, a) => {
                let a = compose_action(a)?;
                self.compose(project, a, vec![service.clone()]).await
            }
            (
                Target::Pod {
                    context,
                    namespace,
                    name,
                },
                Action::Remove,
            ) => {
                self.set_busy(node_id, true);
                let r = match self.kube_client(context).await {
                    Ok(c) => c.delete_pod(namespace, name).await,
                    Err(e) => Err(e),
                };
                self.set_busy(node_id, false);
                self.load_workloads(context, namespace).await;
                r
            }
            (
                Target::Deployment {
                    context,
                    namespace,
                    name,
                },
                Action::Restart,
            ) => {
                let r = match self.kube_client(context).await {
                    Ok(c) => c.restart_deployment(namespace, name).await,
                    Err(e) => Err(e),
                };
                self.load_workloads(context, namespace).await;
                r
            }
            (Target::KubeRoot, Action::Refresh) => {
                self.refresh().await;
                Ok(())
            }
            (Target::KubeContext { context }, Action::Refresh) => {
                self.lock().kube_clients.remove(context);
                self.load_namespaces(context).await;
                Ok(())
            }
            (
                Target::KubeNamespace { context, namespace }
                | Target::KubeGroup {
                    context, namespace, ..
                },
                Action::Refresh,
            ) => {
                self.load_workloads(context, namespace).await;
                Ok(())
            }
            (_, Action::Refresh) => self.refresh_docker().await,
            (_, Action::ForwardPorts) => err("Forward Ports needs the ports to forward"),
            _ => err(format!("{action:?} is not available here")),
        };
        if matches!(
            target,
            Target::Container { .. }
                | Target::Image { .. }
                | Target::Volume { .. }
                | Target::Network { .. }
        ) {
            let _ = self.refresh_docker().await;
        }
        self.emit_tree();
        r
    }

    /// Pretty JSON of `docker inspect` / the Kubernetes object.
    pub async fn inspect(&self, node_id: &str) -> Result<String> {
        let target =
            Target::parse(node_id).ok_or_else(|| Error(format!("Unknown node {node_id}")))?;
        let client = || {
            self.lock()
                .client
                .clone()
                .ok_or_else(|| Error("Docker is not connected".into()))
        };
        let value = match &target {
            Target::Container { id } => client()?.inspect(ObjectKind::Container, id).await?,
            Target::Image { id } => client()?.inspect(ObjectKind::Image, id).await?,
            Target::Volume { name } => client()?.inspect(ObjectKind::Volume, name).await?,
            Target::Network { id } => client()?.inspect(ObjectKind::Network, id).await?,
            Target::ComposeService { project, service } => {
                let input = self.input();
                let id = tree::compose_projects(&input)
                    .iter()
                    .find(|p| &p.name == project)
                    .and_then(|p| {
                        p.containers
                            .get(service)
                            .and_then(|c| c.first().map(|c| c.id.clone()))
                    })
                    .ok_or_else(|| Error("The service has no container".into()))?;
                client()?.inspect(ObjectKind::Container, &id).await?
            }
            Target::Pod {
                context,
                namespace,
                name,
            } => {
                self.kube_client(context)
                    .await?
                    .pod(namespace, name)
                    .await?
            }
            Target::Deployment {
                context,
                namespace,
                name,
            } => {
                self.kube_client(context)
                    .await?
                    .deployment(namespace, name)
                    .await?
            }
            Target::KubeService {
                context,
                namespace,
                name,
            } => {
                self.kube_client(context)
                    .await?
                    .service(namespace, name)
                    .await?
            }
            _ => return err("Nothing to inspect"),
        };
        Ok(tree::inspection_text(&value))
    }

    /// Command line for Exec (`attach` = false) or the Attached Console (`attach` = true);
    /// `container` picks the container of a pod (default: the pod's default container).
    pub fn terminal_command(
        &self,
        node_id: &str,
        attach: bool,
        container: Option<&str>,
    ) -> Result<TerminalCommand> {
        let target =
            Target::parse(node_id).ok_or_else(|| Error(format!("Unknown node {node_id}")))?;
        let input = self.input();
        let (cli, env) = self.docker_cli();
        let docker_container = |id: &str| {
            input
                .docker
                .snapshot
                .as_ref()
                .and_then(|s| s.containers.iter().find(|c| c.id == id))
                .map(|c| (c.id.clone(), c.name.clone()))
        };
        let found = match &target {
            Target::Container { id } => docker_container(id),
            Target::ComposeService { project, service } => tree::compose_projects(&input)
                .iter()
                .find(|p| &p.name == project)
                .and_then(|p| {
                    p.containers
                        .get(service)
                        .and_then(|c| c.first().map(|c| (c.id.clone(), c.name.clone())))
                }),
            Target::Pod {
                context,
                namespace,
                name,
            } => {
                let (kubectl, kubeconfig, on_host) = {
                    let s = self.lock();
                    match kube_place(&s.settings, &s.project) {
                        // The host's kubectl with the host's kubeconfig, in a terminal there.
                        Some((_, _, kubectl)) => (kubectl, None, true),
                        None => {
                            let (kubectl, kubeconfig) = Self::local_kubectl(&s);
                            (kubectl, kubeconfig, false)
                        }
                    }
                };
                let container = self.pod_container(context, namespace, name, container);
                let make = if attach {
                    exec::kubectl_attach
                } else {
                    exec::kubectl_exec
                };
                let mut cmd = make(
                    &kubectl,
                    kubeconfig.as_deref(),
                    context,
                    namespace,
                    name,
                    container.as_deref(),
                );
                cmd.on_host = on_host;
                return Ok(cmd);
            }
            _ => None,
        };
        let (id, name) = found.ok_or_else(|| Error("No container to open a terminal in".into()))?;
        let mut cmd = if attach {
            exec::docker_attach(&cli, &id, &name, env)
        } else {
            exec::docker_exec(&cli, &id, &name, env)
        };
        // With the Docker daemon of a remote project's host, `docker` runs on that host.
        cmd.on_host = matches!(
            Self::endpoint(&self.lock(), &process_environment()),
            Ok(Endpoint::ProjectHost { .. })
        );
        Ok(cmd)
    }

    /// kubectl on this machine and the `KUBECONFIG` it gets (the setting, `$KUBECONFIG` or
    /// `~/.kube/config`).
    fn local_kubectl(s: &State) -> (String, Option<String>) {
        let kubectl = if s.settings.kubectl.trim().is_empty() {
            "kubectl".to_string()
        } else {
            s.settings.kubectl.clone()
        };
        let cfg = kubeconfig_paths(&s.settings.kubeconfig)
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let joined = std::env::join_paths(&cfg)
            .ok()
            .map(|j| j.to_string_lossy().into_owned());
        (kubectl, joined)
    }

    /// The context and namespace selector of the Services tool window.
    pub fn kube_selection(&self) -> Option<tree::KubeSelection> {
        tree::kube_selection(&self.lock().kube)
    }

    /// Pick the context (and its namespace; `None`: the context's own) to work with: it
    /// becomes the current one in the tree and its namespace is loaded. Manifests are
    /// applied there.
    pub async fn select_kube(
        self: &Arc<Self>,
        context: &str,
        namespace: Option<String>,
    ) -> Result<Option<tree::KubeSelection>> {
        {
            let mut s = self.lock();
            let known = s
                .kube
                .config
                .as_ref()
                .is_some_and(|c| c.context(context).is_some());
            if !known {
                return err(format!("Context `{context}` is not in the kubeconfig"));
            }
            let namespace = namespace.filter(|n| !n.trim().is_empty());
            s.kube_select = Some((context.to_string(), namespace));
            Self::apply_selection(&mut s);
        }
        self.expand(
            &Target::KubeContext {
                context: context.to_string(),
            }
            .node_id(),
        )
        .await;
        Ok(self.kube_selection())
    }

    /// `kubectl apply -f` / `kubectl delete -f` of a project manifest in the selected
    /// context (on a remote project's host with its kubectl); output in the Kubernetes
    /// node's Log tab. `-n <selected namespace>` is passed for manifests that name no
    /// namespace.
    pub async fn apply_manifest(&self, path: &str, action: ManifestAction) -> Result<()> {
        let (root, content) = self.read_project_file(path).await?;
        let objects = kubectl::manifest_objects(&content);
        if objects.is_empty() {
            return err(format!(
                "{path} has no Kubernetes objects (apiVersion and kind)"
            ));
        }
        let (place, local, context, namespace) = {
            let s = self.lock();
            let context = tree::kube_selection(&s.kube).and_then(|k| k.context);
            let namespace = context.as_deref().map(|c| s.kube.namespace_of(c));
            (
                kube_place(&s.settings, &s.project),
                Self::local_kubectl(&s),
                context,
                namespace,
            )
        };
        let Some(context) = context else {
            return err("No Kubernetes context to apply the manifest to: check the kubeconfig");
        };
        let ns = namespace
            .clone()
            .filter(|_| kubectl::manifest_needs_namespace(&objects));
        let args = kubectl::manifest_args(&context, ns.as_deref(), action, path);
        let node = Target::KubeRoot.node_id();
        self.set_busy(&node, true);
        let r = match &place {
            Some((host, _, program)) => {
                self.run_cli(&node, Some(host.as_ref()), program, &args, &[], &root)
                    .await
            }
            None => {
                let env = exec::kubeconfig_env(local.1.as_deref());
                self.run_cli(&node, None, &local.0, &args, &env, &root)
                    .await
            }
        };
        self.set_busy(&node, false);
        if let Some(ns) = namespace {
            let loaded = self
                .lock()
                .kube
                .workloads
                .contains_key(&tree::workloads_key(&context, &ns));
            if loaded {
                self.load_workloads(&context, &ns).await;
                self.emit_tree();
            }
        }
        r
    }

    /// The resolved endpoint (Settings page "Test connection").
    pub async fn test_connection(settings: &DockerSettings) -> Result<String> {
        let ep: Endpoint =
            endpoint::resolve(settings, &process_environment(), &|p| Path::new(p).exists())?;
        let client = DockerClient::new(&ep)?;
        let v = client.version().await?;
        Ok(format!(
            "Connection successful: {} {} (API {}) at {}",
            v.platform.unwrap_or_else(|| "Docker Engine".into()),
            v.version,
            v.api_version,
            ep.label()
        ))
    }

    // ---- port forwarding ----------------------------------------------------------------

    /// Forward `127.0.0.1:<local>` (0: any free port) to `remote` of a pod. Every local
    /// connection opens its own `portforward` WebSocket.
    pub async fn start_port_forward(
        self: &Arc<Self>,
        node_id: &str,
        local: u16,
        remote: u16,
    ) -> Result<PortForward> {
        let Some(Target::Pod {
            context,
            namespace,
            name,
        }) = Target::parse(node_id)
        else {
            return err("Ports can only be forwarded to pods");
        };
        if remote == 0 {
            return err("Enter the pod port to forward");
        }
        let client = self.kube_client(&context).await?;
        // On a remote project's host: kubectl port-forward runs there, and every local
        // connection is tunnelled to its port through the project's SSH connection.
        let host_forward = match &client {
            KubeConn::Cli(c) => {
                let (port, task) = c.port_forward(&namespace, &name, remote).await?;
                Some((c.host.clone(), port, task))
            }
            KubeConn::Api(_) => None,
        };
        let stop_kubectl = host_forward
            .as_ref()
            .map(|(_, _, t)| AbortOnDrop(t.abort_handle()));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", local))
            .await
            .map_err(|e| {
                Error(if e.kind() == std::io::ErrorKind::AddrInUse {
                    format!("Local port {local} is already in use")
                } else {
                    format!("Cannot listen on 127.0.0.1:{local}: {e}")
                })
            })?;
        let local_port = listener.local_addr()?.port();
        let id = {
            let mut s = self.lock();
            s.next_forward += 1;
            s.next_forward
        };
        let forward = PortForward {
            id,
            node_id: node_id.to_string(),
            context,
            namespace: namespace.clone(),
            pod: name.clone(),
            address: "127.0.0.1".into(),
            local_port,
            remote_port: remote,
            connections: 0,
            error: None,
            via: host_forward
                .as_ref()
                .and_then(|(h, _, _)| h.remote().map(str::to_string)),
        };
        let hub = self.clone();
        let task = self.runtime.spawn(async move {
            // Aborting the listener drops the set, which aborts its connections, and stops
            // kubectl on the host.
            let _stop_kubectl = stop_kubectl;
            let (tunnel, mut kubectl) = match host_forward {
                Some((host, port, task)) => (Some((host, port)), Some(task)),
                None => (None, None),
            };
            let mut conns = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((mut tcp, _)) = accepted else { continue };
                        tcp.set_nodelay(true).ok();
                        hub.update_forward(id, |f| f.connections += 1);
                        let (hub, client, ns, pod) =
                            (hub.clone(), client.clone(), namespace.clone(), name.clone());
                        let tunnel = tunnel.clone();
                        conns.spawn(async move {
                            let r = match (&client, tunnel) {
                                (_, Some((host, port))) => match host.open_tcp(port).await {
                                    Ok(mut io) => tokio::io::copy_bidirectional(&mut tcp, &mut io)
                                        .await
                                        .map(|_| ())
                                        .map_err(Error::from),
                                    Err(e) => Err(e),
                                },
                                (KubeConn::Api(c), None) => match c.port_forward(&ns, &pod, remote).await {
                                    Ok(ws) => ws.port_forward(remote, tcp).await,
                                    Err(e) => Err(e),
                                },
                                (KubeConn::Cli(_), None) => err("kubectl port-forward is not running"),
                            };
                            hub.update_forward(id, |f| {
                                f.connections = f.connections.saturating_sub(1);
                                if let Err(e) = &r {
                                    f.error = Some(e.0.clone());
                                }
                            });
                        });
                    }
                    Some(_) = conns.join_next(), if !conns.is_empty() => {}
                    ended = kubectl_ended(&mut kubectl) => {
                        kubectl = None;
                        hub.update_forward(id, |f| f.error = Some(ended));
                    }
                }
            }
        });
        self.lock().forwards.push((forward.clone(), task));
        self.emit_tree();
        Ok(forward)
    }

    /// Start the forwards of a spec (`8080:80, 9000`); stops the ones already started when
    /// one fails.
    pub async fn start_port_forwards(
        self: &Arc<Self>,
        node_id: &str,
        spec: &str,
    ) -> Result<Vec<PortForward>> {
        let mut started = Vec::new();
        for (local, remote) in crate::portforward::parse_specs(spec)? {
            match self.start_port_forward(node_id, local, remote).await {
                Ok(f) => started.push(f),
                Err(e) => {
                    for f in &started {
                        let _ = self.stop_port_forward(f.id);
                    }
                    return Err(e);
                }
            }
        }
        Ok(started)
    }

    fn update_forward(&self, id: u64, f: impl FnOnce(&mut PortForward)) {
        {
            let mut s = self.lock();
            let Some((fw, _)) = s.forwards.iter_mut().find(|(x, _)| x.id == id) else {
                return;
            };
            f(fw);
        }
        self.emit_tree();
    }

    pub fn stop_port_forward(&self, id: u64) -> Result<()> {
        let removed = {
            let mut s = self.lock();
            let i = s
                .forwards
                .iter()
                .position(|(f, _)| f.id == id)
                .ok_or_else(|| Error("The port forward has already stopped".into()))?;
            s.forwards.remove(i)
        };
        removed.1.abort();
        self.emit_tree();
        Ok(())
    }

    pub fn port_forwards(&self) -> Vec<PortForward> {
        self.lock()
            .forwards
            .iter()
            .map(|(f, _)| f.clone())
            .collect()
    }

    /// Stop the watcher, every stream and every port forward (app exit / tests).
    pub fn shutdown(&self) {
        let mut s = self.lock();
        if let Some(w) = s.watcher.take() {
            w.abort();
        }
        for (_, h) in s.streams.drain() {
            h.abort();
        }
        for (_, h) in s.forwards.drain(..) {
            h.abort();
        }
    }
}

fn compose_action(a: Action) -> Result<ComposeAction> {
    Ok(match a {
        Action::Up | Action::Start => ComposeAction::Up,
        Action::Stop => ComposeAction::Stop,
        Action::Restart => ComposeAction::Restart,
        Action::Down | Action::Remove => ComposeAction::Down,
        Action::Pull => ComposeAction::Pull,
        other => return err(format!("{other:?} is not available for compose")),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::http::tests::{chunked, fake_server, ok_json};

    fn collect() -> (Emit, Arc<Mutex<Vec<HubEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let e = events.clone();
        (Arc::new(move |ev| e.lock().unwrap().push(ev)), events)
    }

    #[test]
    fn finds_compose_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Acme Shop");
        std::fs::create_dir_all(root.join("worker")).unwrap();
        std::fs::create_dir_all(root.join("vendor/pkg")).unwrap();
        std::fs::write(
            root.join("docker-compose.yml"),
            "services:\n  app:\n    image: ${IMG}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("docker-compose.override.yml"),
            "services:\n  mailpit:\n    image: axllent/mailpit\n",
        )
        .unwrap();
        std::fs::write(root.join(".env"), "IMG=php:8.3\n").unwrap();
        std::fs::write(
            root.join("worker/compose.yaml"),
            "name: jobs\nservices:\n  q:\n    image: redis\n",
        )
        .unwrap();
        std::fs::write(
            root.join("vendor/pkg/compose.yaml"),
            "services:\n  x:\n    image: y\n",
        )
        .unwrap();
        let files = find_compose_files(&root);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "docker-compose.yml");
        assert_eq!(files[0].project, "acmeshop");
        assert_eq!(
            files[0].service("app").unwrap().image.as_deref(),
            Some("php:8.3")
        );
        assert!(files[0].service("mailpit").is_some());
        assert_eq!(files[1].path, "worker/compose.yaml");
        assert_eq!(files[1].project, "jobs");
    }

    #[test]
    fn loads_docker_contexts() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for (id, meta) in [
            (
                "aa11",
                r#"{"Name":"remote","Endpoints":{"docker":{"Host":"tcp://10.0.0.9:2376"}}}"#,
            ),
            (
                "bb22",
                r#"{"Name":"buildbox","Endpoints":{"docker":{"Host":"ssh://ci@buildbox"}}}"#,
            ),
            ("cc33", "not json"),
        ] {
            std::fs::create_dir_all(d.join("contexts/meta").join(id)).unwrap();
            std::fs::write(d.join("contexts/meta").join(id).join("meta.json"), meta).unwrap();
        }
        std::fs::create_dir_all(d.join("contexts/tls/aa11/docker")).unwrap();
        std::fs::write(d.join("config.json"), r#"{"currentContext":"remote"}"#).unwrap();
        let contexts = load_contexts(d, None);
        let names: Vec<_> = contexts.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["buildbox", "remote"]);
        assert!(contexts[1].current);
        assert!(contexts[1]
            .tls_dir
            .as_deref()
            .unwrap()
            .ends_with("contexts/tls/aa11/docker"));
        assert!(contexts[0].tls_dir.is_none());
        let contexts = load_contexts(d, Some("buildbox"));
        assert!(contexts[0].current && !contexts[1].current);
        assert!(load_contexts(&d.join("missing"), None).is_empty());
    }

    #[test]
    fn loads_kubeconfig_from_setting() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config");
        std::fs::write(&p, "current-context: a\ncontexts:\n- name: a\n  context: { cluster: c, user: u }\nclusters:\n- name: c\n  cluster: { server: 'https://127.0.0.1:1' }\n").unwrap();
        let k = load_kube(&HubSettings {
            kubeconfig: p.to_string_lossy().into_owned(),
            ..Default::default()
        });
        assert_eq!(k.config.unwrap().contexts[0].name, "a");
        let k = load_kube(&HubSettings {
            kubeconfig: "/nonexistent/kubeconfig".into(),
            ..Default::default()
        });
        assert!(k.config.is_none());
    }

    #[tokio::test]
    async fn connects_streams_logs_and_runs_actions() {
        let (_d, sock) = fake_server(|head, _| {
            let target = head.split_whitespace().nth(1).unwrap().to_string();
            match target.as_str() {
                "/containers/json?all=1" => ok_json(r#"[{"Id":"c1","Names":["/web"],"Image":"nginx","State":"running","Status":"Up","Labels":{}}]"#),
                "/images/json" => ok_json("[]"),
                "/volumes" => ok_json(r#"{"Volumes":[]}"#),
                "/networks" => ok_json("[]"),
                "/version" => ok_json(r#"{"Version":"27.3.1","ApiVersion":"1.47"}"#),
                "/containers/c1/json" => ok_json(r#"{"Id":"c1","Config":{"Tty":true}}"#),
                "/containers/c1/stop" => b"HTTP/1.1 204 No Content\r\n\r\n".to_vec(),
                t if t.starts_with("/containers/c1/logs") => chunked("200 OK", &[b"\x1b[32mready\x1b[0m\n"]),
                // Keep the event stream open for a moment, then end it.
                t if t.starts_with("/events") => chunked("200 OK", &[]),
                _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
            }
        })
        .await;
        let (emit, events) = collect();
        let hub = Hub::new(emit, tokio::runtime::Handle::current());
        hub.configure(
            HubSettings {
                docker: DockerSettings {
                    connection: "unix".into(),
                    socket_path: sock.clone(),
                    ..Default::default()
                },
                kubeconfig: "/nonexistent/kubeconfig".into(),
                ..Default::default()
            },
            None,
        );
        for _ in 0..100 {
            if hub.input().docker.state == ConnectionState::Connected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(hub.input().docker.state, ConnectionState::Connected);
        assert_eq!(hub.tree()[0].children[0].children[0].label, "web");

        let key = hub.start_logs("docker/container/c1", 100, None).unwrap();
        for _ in 0..100 {
            if !hub.console(&key).is_empty() && !hub.lock().streams.contains_key(&key) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let lines = hub.console(&key);
        assert_eq!(
            lines.iter().map(|l| l.text()).collect::<Vec<_>>().concat(),
            "ready"
        );
        assert!(
            lines[0].segments[0].style.fg.is_some(),
            "ANSI colors are kept"
        );

        hub.action("docker/container/c1", Action::Stop)
            .await
            .unwrap();
        let err = hub
            .action("docker/container/c1", Action::Up)
            .await
            .err()
            .unwrap();
        assert!(err.0.contains("not available"));
        let cmd = hub
            .terminal_command("docker/container/c1", false, None)
            .unwrap();
        assert_eq!(cmd.args[..3], ["exec", "-it", "c1"]);
        assert!(cmd
            .env
            .iter()
            .any(|(k, v)| k == "DOCKER_HOST" && v == &format!("unix://{sock}")));
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, HubEvent::Console { .. })));
        assert!(events.lock().unwrap().iter().any(|e| matches!(
            e,
            HubEvent::Status {
                state: ConnectionState::Connected,
                ..
            }
        )));
        hub.shutdown();
    }

    async fn wait_until(mut f: impl FnMut() -> bool) {
        for _ in 0..200 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("timed out");
    }

    #[tokio::test]
    async fn forwards_ports_to_pods() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (_d, path) = crate::forward::tests::fake_portforward_server(80).await;
        let (emit, _events) = collect();
        let hub = Hub::new(emit, tokio::runtime::Handle::current());
        hub.lock().kube_clients.insert(
            "kind".into(),
            KubeConn::Api(KubeClient::with_transport(
                crate::http::Transport::Unix(path),
                None,
            )),
        );
        let node = Target::Pod {
            context: "kind".into(),
            namespace: "shop".into(),
            name: "web-1".into(),
        }
        .node_id();
        let f = hub
            .start_port_forwards(&node, ":80")
            .await
            .unwrap()
            .remove(0);
        assert!(f.local_port > 0);
        assert_eq!(hub.input().port_forwards, std::slice::from_ref(&f));
        let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", f.local_port))
            .await
            .unwrap();
        tcp.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        tcp.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"PING");
        assert_eq!(hub.port_forwards()[0].connections, 1);
        // The pod refuses: the error is kept on the forward.
        let mut bad = tokio::net::TcpStream::connect(("127.0.0.1", f.local_port))
            .await
            .unwrap();
        bad.write_all(b"fail").await.unwrap();
        wait_until(|| hub.port_forwards()[0].error.is_some()).await;
        assert_eq!(
            hub.port_forwards()[0].error.as_deref(),
            Some("error forwarding port 80: connection refused")
        );
        let e = hub
            .start_port_forward(&node, f.local_port, 80)
            .await
            .unwrap_err();
        assert_eq!(
            e.0,
            format!("Local port {} is already in use", f.local_port)
        );
        assert!(hub
            .start_port_forward("kube/kind/shop/deployments/web", 0, 80)
            .await
            .is_err());
        hub.stop_port_forward(f.id).unwrap();
        assert!(hub.port_forwards().is_empty());
        assert!(hub.stop_port_forward(f.id).is_err());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            tokio::net::TcpStream::connect(("127.0.0.1", f.local_port))
                .await
                .is_err(),
            "the listener is closed"
        );
        hub.shutdown();
    }

    #[tokio::test]
    async fn aggregates_deployment_logs_and_picks_containers() {
        let (_d, path) = fake_server(|head, _| {
            let target = head.split_whitespace().nth(1).unwrap().to_string();
            match target.as_str() {
                "/apis/apps/v1/namespaces/shop/deployments/web" => ok_json(
                    r#"{"metadata":{"name":"web","namespace":"shop"},"spec":{"selector":{"matchLabels":{"app":"web"}}}}"#,
                ),
                "/api/v1/namespaces/shop/pods?labelSelector=app%3Dweb" => ok_json(
                    r#"{"items":[
                        {"metadata":{"name":"web-1","namespace":"shop","labels":{"app":"web"}},"spec":{"containers":[{"name":"php"},{"name":"nginx"}]},"status":{"phase":"Running"}},
                        {"metadata":{"name":"web-2","namespace":"shop","labels":{"app":"web"}},"spec":{"containers":[{"name":"php"}]},"status":{"phase":"Running"}}
                    ]}"#,
                ),
                t if t.starts_with("/api/v1/namespaces/shop/pods/web-1/log") && t.ends_with("container=php") => {
                    chunked("200 OK", &[b"booting\nre", b"ady\nno newline"])
                }
                t if t.starts_with("/api/v1/namespaces/shop/pods/web-1/log") && t.ends_with("container=nginx") => {
                    chunked("200 OK", &[b"GET / 200\n"])
                }
                t if t.starts_with("/api/v1/namespaces/shop/pods/web-2/log") => {
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 47\r\n\r\n{\"message\":\"container php is waiting to start\"}".to_vec()
                }
                _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
            }
        })
        .await;
        let (emit, _events) = collect();
        let hub = Hub::new(emit, tokio::runtime::Handle::current());
        hub.lock().kube_clients.insert(
            "kind".into(),
            KubeConn::Api(KubeClient::with_transport(
                crate::http::Transport::Unix(path),
                None,
            )),
        );
        let node = Target::Deployment {
            context: "kind".into(),
            namespace: "shop".into(),
            name: "web".into(),
        }
        .node_id();
        let key = hub.start_logs(&node, 50, None).unwrap();
        wait_until(|| !hub.lock().streams.contains_key(&key)).await;
        let mut lines: Vec<String> = hub
            .console(&key)
            .iter()
            .map(|l| l.text())
            .filter(|t| !t.is_empty())
            .collect();
        lines.sort();
        assert_eq!(
            lines,
            [
                "[pod/web-1/nginx] GET / 200",
                "[pod/web-1/php] booting",
                "[pod/web-1/php] no newline",
                "[pod/web-1/php] ready",
                "[pod/web-2/php] container php is waiting to start",
            ]
        );

        // Pods: the picked container, else the default one from the loaded workloads.
        let raw = kube::RawWorkloads {
            pods: serde_json::json!({ "items": [{
                "metadata": { "name": "api-1", "namespace": "shop",
                              "annotations": { "kubectl.kubernetes.io/default-container": "app" } },
                "spec": { "containers": [{ "name": "istio-proxy" }, { "name": "app" }] },
                "status": { "phase": "Running" }
            }] }),
            ..Default::default()
        };
        hub.lock().kube.workloads.insert(
            tree::workloads_key("kind", "shop"),
            Loaded::Ready { value: raw.parse() },
        );
        let pod = Target::Pod {
            context: "kind".into(),
            namespace: "shop".into(),
            name: "api-1".into(),
        }
        .node_id();
        let exec = hub.terminal_command(&pod, false, None).unwrap();
        assert_eq!(exec.title, "api-1 [app]");
        let attach = hub
            .terminal_command(&pod, true, Some("istio-proxy"))
            .unwrap();
        assert!(attach.args.contains(&"attach".to_string()));
        assert_eq!(attach.args.last().map(String::as_str), Some("istio-proxy"));
        hub.shutdown();
    }

    #[tokio::test]
    async fn reports_failed_connections() {
        let (emit, _events) = collect();
        let hub = Hub::new(emit, tokio::runtime::Handle::current());
        hub.configure(
            HubSettings {
                docker: DockerSettings {
                    connection: "unix".into(),
                    socket_path: "/nonexistent/docker.sock".into(),
                    ..Default::default()
                },
                kubeconfig: "/nonexistent/kubeconfig".into(),
                ..Default::default()
            },
            None,
        );
        for _ in 0..100 {
            if hub.input().docker.state == ConnectionState::Failed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let tree = hub.tree();
        assert!(tree[0].children[0]
            .label
            .starts_with("Cannot connect: Cannot connect to /nonexistent/docker.sock"));
        hub.shutdown();
    }

    /// Program, arguments, working directory and environment of a command run on the host.
    type FakeRun = (String, Vec<String>, PathBuf, Vec<(String, String)>);

    /// A remote project as the app's backend serves it: files in memory, commands recorded,
    /// the host's Docker socket forwarded to a fake daemon.
    struct FakeHost {
        root: PathBuf,
        files: BTreeMap<String, String>,
        daemon: String,
        runs: Mutex<Vec<FakeRun>>,
        sockets: Mutex<Vec<String>>,
    }

    impl ProjectHost for FakeHost {
        fn root(&self) -> &Path {
            &self.root
        }
        fn remote(&self) -> Option<&str> {
            Some("deploy@shop.example.com")
        }
        fn read(&self, rel: &str) -> Option<String> {
            self.files.get(rel).cloned()
        }
        fn dirs(&self) -> Vec<String> {
            vec!["vendor".into(), "worker".into()]
        }
        fn run<'a>(
            &'a self,
            program: &'a str,
            args: &'a [String],
            env: &'a [(String, String)],
            cwd: &'a Path,
            mut sink: crate::project::OutputSink<'a>,
        ) -> crate::project::BoxFuture<'a, Result<Option<i32>>> {
            self.runs.lock().unwrap().push((
                program.to_string(),
                args.to_vec(),
                cwd.to_path_buf(),
                env.to_vec(),
            ));
            sink(LogStream::Stderr, "Container shop-app-1  Started\n".into());
            Box::pin(async { Ok(Some(0)) })
        }
        fn open_socket(
            &self,
            path: &str,
        ) -> crate::project::BoxFuture<'static, Result<crate::http::BoxIo>> {
            self.sockets.lock().unwrap().push(path.to_string());
            let daemon = self.daemon.clone();
            Box::pin(async move {
                let s = tokio::net::UnixStream::connect(daemon).await?;
                Ok(Box::new(s) as crate::http::BoxIo)
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn remote_projects_use_the_host_for_compose_and_docker() {
        let (_d, sock) = fake_server(|head, _| {
            let target = head.split_whitespace().nth(1).unwrap().to_string();
            match target.as_str() {
                "/containers/json?all=1" => ok_json(r#"[{"Id":"c1","Names":["/shop-app-1"],"Image":"php:8.3","State":"running","Status":"Up","Labels":{"com.docker.compose.project":"shop","com.docker.compose.service":"app"}}]"#),
                "/images/json" => ok_json("[]"),
                "/volumes" => ok_json(r#"{"Volumes":[]}"#),
                "/networks" => ok_json("[]"),
                "/version" => ok_json(r#"{"Version":"27.3.1","ApiVersion":"1.47"}"#),
                t if t.starts_with("/events") => chunked("200 OK", &[]),
                _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
            }
        })
        .await;
        let mut files = BTreeMap::new();
        files.insert(
            "compose.yaml".to_string(),
            "services:\n  app:\n    image: ${IMG}\n".to_string(),
        );
        files.insert(
            "compose.override.yaml".to_string(),
            "services:\n  mailpit:\n    image: axllent/mailpit\n".to_string(),
        );
        files.insert(".env".to_string(), "IMG=php:8.3\n".to_string());
        files.insert(
            "worker/compose.yaml".to_string(),
            "name: jobs\nservices:\n  q:\n    image: redis\n".to_string(),
        );
        files.insert(
            "docker/Dockerfile".to_string(),
            "FROM php:8.3\n".to_string(),
        );
        let host = Arc::new(FakeHost {
            root: PathBuf::from("/srv/shop"),
            files,
            daemon: sock,
            runs: Mutex::new(Vec::new()),
            sockets: Mutex::new(Vec::new()),
        });
        let project: Arc<dyn ProjectHost> = host.clone();

        let (emit, _events) = collect();
        let hub = Hub::new(emit, tokio::runtime::Handle::current());
        let settings = HubSettings {
            kubeconfig: "/nonexistent/kubeconfig".into(),
            project_docker: true,
            ..Default::default()
        };
        let (h, p, s) = (hub.clone(), project.clone(), settings.clone());
        tokio::task::spawn_blocking(move || h.configure(s, Some(p)))
            .await
            .unwrap();
        wait_until(|| hub.input().docker.state == ConnectionState::Connected).await;
        assert_eq!(
            hub.input().docker.endpoint,
            "ssh://deploy@shop.example.com/var/run/docker.sock"
        );
        assert!(host
            .sockets
            .lock()
            .unwrap()
            .iter()
            .all(|s| s == "/var/run/docker.sock"));

        // Compose files were read on the host, override and `.env` included.
        let compose = hub.input().compose_files;
        assert_eq!(compose.len(), 2);
        assert_eq!(compose[0].project, "shop");
        assert_eq!(
            compose[0].service("app").unwrap().image.as_deref(),
            Some("php:8.3")
        );
        assert!(compose[0].service("mailpit").is_some());
        assert_eq!(compose[1].path, "worker/compose.yaml");

        hub.compose("shop", ComposeAction::Up, Vec::new())
            .await
            .unwrap();
        hub.dockerfile_run("docker/Dockerfile").await.unwrap();
        {
            let runs = host.runs.lock().unwrap();
            let (program, args, cwd, env) = &runs[0];
            assert_eq!(program, "docker");
            assert_eq!(cwd, Path::new("/srv/shop"));
            assert!(
                args.contains(&"compose.override.yaml".to_string()),
                "{args:?}"
            );
            assert!(env
                .iter()
                .any(|(k, v)| k == "DOCKER_HOST" && v == "unix:///var/run/docker.sock"));
            assert!(env
                .iter()
                .any(|(k, v)| k == "COMPOSE_PROGRESS" && v == "plain"));
            // Dockerfile ▶: build and run on the host too.
            assert_eq!(runs.len(), 3);
            assert_eq!(runs[1].1[0], "build");
        }
        let cmd = hub
            .terminal_command("docker/container/c1", false, None)
            .unwrap();
        assert_eq!(cmd.program, "docker");
        assert!(cmd.on_host);
        assert!(cmd
            .env
            .iter()
            .any(|(k, v)| k == "DOCKER_HOST" && v == "unix:///var/run/docker.sock"));

        // Docker of this machine: compose still runs on the host, against its own daemon.
        let local = HubSettings {
            project_docker: false,
            docker: DockerSettings {
                connection: "unix".into(),
                socket_path: "/nonexistent/docker.sock".into(),
                ..Default::default()
            },
            ..settings
        };
        let (h, p) = (hub.clone(), project.clone());
        tokio::task::spawn_blocking(move || h.configure(local, Some(p)))
            .await
            .unwrap();
        hub.compose("jobs", ComposeAction::Up, Vec::new())
            .await
            .unwrap();
        let runs = host.runs.lock().unwrap();
        let (program, _, cwd, env) = runs.last().unwrap();
        assert_eq!(program, "docker");
        assert_eq!(cwd, Path::new("/srv/shop/worker"));
        assert!(!env.iter().any(|(k, _)| k == "DOCKER_HOST"));
        drop(runs);
        hub.shutdown();
    }

    /// A remote project whose host has `kubectl` (faked: canned JSON per command line) and
    /// a port reachable through the SSH connection (an echo server).
    struct KubeHost {
        files: BTreeMap<String, String>,
        echo_port: u16,
        runs: Mutex<Vec<(String, Vec<String>)>>,
        tcp: Mutex<Vec<u16>>,
    }

    const HOST_KUBECONFIG: &str = r#"{"kind":"Config","current-context":"k3s-shop",
        "clusters":[{"name":"k3s","cluster":{"server":"https://127.0.0.1:6443","certificate-authority-data":"DATA+OMITTED"}},
                    {"name":"prod","cluster":{"server":"https://k8s.acme.example:6443"}}],
        "contexts":[{"name":"k3s-shop","context":{"cluster":"k3s","user":"k3s","namespace":"shop"}},
                    {"name":"prod-eu","context":{"cluster":"prod","user":"deploy"}}],
        "users":[{"name":"k3s","user":{"client-certificate-data":"DATA+OMITTED"}}]}"#;

    impl ProjectHost for KubeHost {
        fn root(&self) -> &Path {
            Path::new("/srv/shop")
        }
        fn remote(&self) -> Option<&str> {
            Some("deploy@shop.example.com")
        }
        fn read(&self, rel: &str) -> Option<String> {
            self.files.get(rel).cloned()
        }
        fn dirs(&self) -> Vec<String> {
            Vec::new()
        }
        fn run<'a>(
            &'a self,
            program: &'a str,
            args: &'a [String],
            _env: &'a [(String, String)],
            _cwd: &'a Path,
            mut sink: crate::project::OutputSink<'a>,
        ) -> crate::project::BoxFuture<'a, Result<Option<i32>>> {
            self.runs
                .lock()
                .unwrap()
                .push((program.to_string(), args.to_vec()));
            let line = args.join(" ");
            let out = |t: &str| (LogStream::Stdout, t.to_string());
            let (stream, text, code) = if program != "/opt/k3s/kubectl" {
                (
                    LogStream::Stderr,
                    format!("sh: {program}: not found\n"),
                    127,
                )
            } else if line == "config view -o json" {
                let (s, t) = out(HOST_KUBECONFIG);
                (s, t, 0)
            } else if line.ends_with("get namespaces -o json") {
                let (s, t) = out(
                    r#"{"items":[{"metadata":{"name":"shop"}},{"metadata":{"name":"kube-system"}}]}"#,
                );
                (s, t, 0)
            } else if line.contains("get pods -o json") {
                let (s, t) = out(
                    r#"{"kind":"List","items":[{"metadata":{"name":"web-1","namespace":"shop"},"spec":{"containers":[{"name":"php"}]},"status":{"phase":"Running"}}]}"#,
                );
                (s, t, 0)
            } else if line.contains(" get ") {
                let (s, t) = out(r#"{"items":[]}"#);
                (s, t, 0)
            } else if line.contains(" logs -f ") {
                let (s, t) = out("[12:00:01] GET /orders 200\n");
                (s, t, 0)
            } else if line.contains(" port-forward ") {
                // kubectl keeps running while it forwards.
                sink(
                    LogStream::Stdout,
                    format!("Forwarding from 127.0.0.1:{} -> 80\n", self.echo_port),
                );
                return Box::pin(std::future::pending());
            } else if line.contains(" apply -f ") {
                let (s, t) = out("deployment.apps/web configured\n");
                (s, t, 0)
            } else {
                (LogStream::Stderr, "error: unknown command\n".into(), 1)
            };
            sink(stream, text);
            Box::pin(async move { Ok(Some(code)) })
        }
        fn open_tcp(
            &self,
            port: u16,
        ) -> crate::project::BoxFuture<'static, Result<crate::http::BoxIo>> {
            self.tcp.lock().unwrap().push(port);
            Box::pin(async move {
                let s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
                Ok(Box::new(s) as crate::http::BoxIo)
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn remote_projects_use_kubernetes_of_the_host() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let echo = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = echo.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = s.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        });
        let mut files = BTreeMap::new();
        files.insert(
            "k8s/web.yaml".to_string(),
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n".to_string(),
        );
        let host = Arc::new(KubeHost {
            files,
            echo_port,
            runs: Mutex::new(Vec::new()),
            tcp: Mutex::new(Vec::new()),
        });
        let project: Arc<dyn ProjectHost> = host.clone();
        let (emit, _events) = collect();
        let hub = Hub::new(emit, tokio::runtime::Handle::current());
        let settings = HubSettings {
            kubeconfig: "/nonexistent/kubeconfig".into(),
            project_kube: true,
            host_kubectl: vec!["deploy@shop.example.com\t/opt/k3s/kubectl".into()],
            docker: DockerSettings {
                connection: "unix".into(),
                socket_path: "/nonexistent/docker.sock".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let (h, p, s) = (hub.clone(), project.clone(), settings.clone());
        tokio::task::spawn_blocking(move || h.configure(s, Some(p)))
            .await
            .unwrap();
        wait_until(|| hub.input().kube.config.is_some()).await;

        // Contexts come from the host's kubeconfig, and the tree says so.
        let tree = hub.tree();
        let kube = tree.iter().find(|n| n.id == "kube").unwrap();
        assert_eq!(
            kube.detail.as_deref(),
            Some("from host deploy@shop.example.com")
        );
        assert_eq!(kube.children.len(), 2);
        let sel = hub.kube_selection().unwrap();
        assert_eq!(sel.contexts, ["k3s-shop", "prod-eu"]);
        assert_eq!(
            (sel.context.as_deref(), sel.namespace.as_deref()),
            (Some("k3s-shop"), Some("shop"))
        );

        // Namespaces and workloads through the host's kubectl.
        let ctx = Target::KubeContext {
            context: "k3s-shop".into(),
        }
        .node_id();
        hub.expand(&ctx).await;
        let sel = hub.kube_selection().unwrap();
        assert_eq!(sel.namespaces, ["kube-system", "shop"]);
        let pod = Target::Pod {
            context: "k3s-shop".into(),
            namespace: "shop".into(),
            name: "web-1".into(),
        }
        .node_id();
        assert!(hub.loaded_pod("k3s-shop", "shop", "web-1").is_some());
        assert!(host
            .runs
            .lock()
            .unwrap()
            .iter()
            .any(|(_, a)| a.join(" ") == "--context k3s-shop -n shop get pods -o json"));

        // Logs: `kubectl logs -f` on the host.
        let key = hub.start_logs(&pod, 100, None).unwrap();
        wait_until(|| {
            hub.console(&key)
                .iter()
                .any(|l| l.text().contains("GET /orders"))
        })
        .await;

        // Exec: a terminal on the host with the host's kubectl and kubeconfig.
        let cmd = hub.terminal_command(&pod, false, None).unwrap();
        assert_eq!(cmd.program, "/opt/k3s/kubectl");
        assert!(cmd.on_host);
        assert!(cmd.env.is_empty(), "{:?}", cmd.env);
        assert_eq!(cmd.args[..2], ["--context", "k3s-shop"]);

        // Port forward: kubectl port-forward on the host, tunnelled back here.
        let f = hub.start_port_forward(&pod, 0, 80).await.unwrap();
        assert_eq!(f.via.as_deref(), Some("deploy@shop.example.com"));
        let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", f.local_port))
            .await
            .unwrap();
        tcp.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        tokio::time::timeout(Duration::from_secs(10), tcp.read_exact(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&buf, b"ping");
        assert_eq!(host.tcp.lock().unwrap().as_slice(), [echo_port]);
        hub.stop_port_forward(f.id).unwrap();

        // Apply a project manifest in the selected context and namespace, on the host.
        hub.apply_manifest("k8s/web.yaml", ManifestAction::Apply)
            .await
            .unwrap();
        assert!(host
            .runs
            .lock()
            .unwrap()
            .iter()
            .any(|(p, a)| p == "/opt/k3s/kubectl"
                && a.join(" ") == "--context k3s-shop -n shop apply -f k8s/web.yaml"));
        assert!(hub
            .console("run:kube")
            .iter()
            .any(|l| l.text().contains("deployment.apps/web configured")));
        assert!(hub
            .apply_manifest("missing.yaml", ManifestAction::Delete)
            .await
            .is_err());

        // Another context from the selector.
        let sel = hub.select_kube("prod-eu", None).await.unwrap().unwrap();
        assert_eq!(sel.context.as_deref(), Some("prod-eu"));
        assert_eq!(sel.namespace.as_deref(), Some("default"));
        assert!(hub.select_kube("nope", None).await.is_err());

        // A missing kubectl on the host is reported with the setting to change.
        let other = HubSettings {
            host_kubectl: Vec::new(),
            ..settings.clone()
        };
        let (h, p) = (hub.clone(), project.clone());
        tokio::task::spawn_blocking(move || h.configure(other, Some(p)))
            .await
            .unwrap();
        wait_until(|| hub.input().kube.error.is_some()).await;
        assert!(hub
            .input()
            .kube
            .error
            .unwrap()
            .contains("Settings | Build, Execution, Deployment | Kubernetes"));

        // Kubernetes of this machine: the local kubeconfig, not the host.
        let local = HubSettings {
            project_kube: false,
            ..settings
        };
        let (h, p) = (hub.clone(), project.clone());
        tokio::task::spawn_blocking(move || h.configure(local, Some(p)))
            .await
            .unwrap();
        assert_eq!(hub.input().kube.host, None);
        hub.shutdown();
    }
}
