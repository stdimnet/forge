//! The Services backend: one [`Hub`] per app holds the Docker connection (with a watcher that
//! follows the Engine's events and reconnects), the project's compose files, the kubeconfig
//! with lazily loaded namespaces and workloads, running log streams and the consoles (Log
//! tabs). The app only forwards [`HubEvent`]s to the UI and calls these methods from IPC
//! commands.

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
use crate::logs::{pod_log_prefix, LinePrefixer, LogStream};
use crate::portforward::PortForward;
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
    root: Option<PathBuf>,
    docker: tree::DockerInput,
    client: Option<DockerClient>,
    compose_files: Vec<ComposeFile>,
    kube: KubeInput,
    kube_clients: HashMap<String, KubeClient>,
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

/// Compose files of a project: the root and one directory below (not `vendor`,
/// `node_modules` or hidden ones), each directory's main file merged with its override file.
pub fn find_compose_files(root: &Path) -> Vec<ComposeFile> {
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut subs: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                !n.starts_with('.')
                    && !matches!(
                        n.as_str(),
                        "vendor" | "node_modules" | "target" | "storage" | "var"
                    )
            })
            .map(|e| e.path())
            .collect();
        subs.sort();
        dirs.extend(subs);
    }
    let process_env: BTreeMap<String, String> = std::env::vars().collect();
    let mut out = Vec::new();
    for dir in dirs {
        let Some(main) = compose::COMPOSE_FILE_NAMES
            .iter()
            .map(|n| dir.join(n))
            .find(|p| p.is_file())
        else {
            continue;
        };
        let mut env = process_env.clone();
        if let Ok(text) = std::fs::read_to_string(dir.join(".env")) {
            env.extend(compose::parse_env_file(&text));
        }
        let rel = |p: &Path| {
            p.strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        };
        let Ok(text) = std::fs::read_to_string(&main) else {
            continue;
        };
        let mut file = compose::parse(&rel(&main), &text, &env);
        // The directory name comes from the absolute path.
        if !text.lines().any(|l| l.starts_with("name:")) {
            let name = compose::normalize_project_name(
                &dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
            if !name.is_empty() {
                file.project = name;
            }
        }
        if let Some(over) = compose::OVERRIDE_FILE_NAMES
            .iter()
            .map(|n| dir.join(n))
            .find(|p| p.is_file())
        {
            if let Ok(t) = std::fs::read_to_string(&over) {
                let project = file.project.clone();
                file = compose::merge(file, compose::parse(&rel(&over), &t, &env));
                file.project = project;
            }
        }
        out.push(file);
    }
    out
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

    /// Apply settings and the project root; restarts the Docker watcher when the connection
    /// settings changed.
    pub fn configure(self: &Arc<Self>, settings: HubSettings, root: Option<PathBuf>) {
        let restart = {
            let mut s = self.lock();
            let restart = s.settings.docker != settings.docker || s.watcher.is_none();
            let kube_changed = s.settings.kubeconfig != settings.kubeconfig
                || s.settings.default_context != settings.default_context
                || s.settings.default_namespace != settings.default_namespace
                || s.kube.config.is_none();
            if kube_changed {
                s.kube = load_kube(&settings);
                s.kube_clients.clear();
            }
            s.compose_files = root.as_deref().map(find_compose_files).unwrap_or_default();
            s.settings = settings;
            s.root = root;
            restart
        };
        if restart {
            self.start_watcher();
        }
        self.emit_tree();
    }

    /// Re-read compose files and the kubeconfig and refresh Docker.
    pub async fn refresh(self: &Arc<Self>) {
        {
            let mut s = self.lock();
            s.compose_files = s
                .root
                .as_deref()
                .map(find_compose_files)
                .unwrap_or_default();
            let settings = s.settings.clone();
            let namespaces = std::mem::take(&mut s.kube.namespaces);
            let workloads = std::mem::take(&mut s.kube.workloads);
            s.kube = load_kube(&settings);
            s.kube.namespaces = namespaces;
            s.kube.workloads = workloads;
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

    pub fn compose_files_changed(self: &Arc<Self>) {
        {
            let mut s = self.lock();
            s.compose_files = s
                .root
                .as_deref()
                .map(find_compose_files)
                .unwrap_or_default();
        }
        self.emit_tree();
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
            let endpoint = endpoint::resolve(&s.settings.docker, &env, &|p| Path::new(p).exists());
            match endpoint {
                Ok(ep) => {
                    s.docker = tree::DockerInput {
                        state: ConnectionState::Connecting,
                        endpoint: ep.label(),
                        error: None,
                        snapshot: None,
                        contexts: env.contexts,
                    };
                    match DockerClient::new(&ep) {
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

    async fn kube_client(&self, context: &str) -> Result<KubeClient> {
        if let Some(c) = self.lock().kube_clients.get(context) {
            return Ok(c.clone());
        }
        let rc = {
            let s = self.lock();
            let Some(cfg) = &s.kube.config else {
                return err("No kubeconfig found");
            };
            cfg.resolve(context)?
        };
        let client = KubeClient::new(&rc).await?;
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
                let ns = {
                    let s = self.lock();
                    let current = s.kube.current_context() == Some(context.as_str());
                    s.kube
                        .default_namespace
                        .clone()
                        .filter(|_| current)
                        .or_else(|| s.kube.config.as_ref()?.context(&context)?.namespace.clone())
                        .unwrap_or_else(|| "default".into())
                };
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
            let root = s.root.as_ref().map(|r| r.to_string_lossy().into_owned());
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
                            c.pod_logs(
                                &namespace,
                                &name,
                                container.as_deref(),
                                tail,
                                true,
                                move |t| h.console_text(&kk, LogStream::Stdout, t),
                            )
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
                        .pod_logs(&ns, &pod, Some(&container), tail, true, move |t| {
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

    fn docker_cli(&self) -> (String, Vec<(String, String)>) {
        let s = self.lock();
        let cli = if s.settings.docker_cli.trim().is_empty() {
            "docker".to_string()
        } else {
            s.settings.docker_cli.clone()
        };
        let env = endpoint::resolve(&s.settings.docker, &process_environment(), &|p| {
            Path::new(p).exists()
        })
        .map(|e| e.cli_env())
        .unwrap_or_default();
        (cli, env)
    }

    /// Run a CLI command, streaming into `run:<node_id>`.
    async fn run_cli(
        &self,
        node_id: &str,
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
        // Compose and BuildKit write their progress to stderr; it is not an error there.
        let code = crate::cli::run_streaming(program, args, env, cwd, |_stream, text| {
            self.console_text(&key, LogStream::Stdout, text)
        })
        .await;
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
                s.root.clone(),
            )
        };
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
                if let Some(over) = compose::OVERRIDE_FILE_NAMES
                    .iter()
                    .find(|n| dir.join(n).is_file())
                {
                    files.push(over.to_string());
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
        let (cli, env) = self.docker_cli();
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
            .run_cli(&node, &cli, &compose::compose_args(&cmd), &env, &cwd)
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
                let root = self
                    .lock()
                    .root
                    .clone()
                    .ok_or_else(|| Error("No project is open".into()))?;
                let text = std::fs::read_to_string(root.join(path))?;
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

    /// Dockerfile ▶: `docker build` then `docker run -d` (replacing an old container of the
    /// same name). Output goes to the Docker node's Log tab.
    pub async fn dockerfile_run(&self, path: &str) -> Result<()> {
        let root = self
            .lock()
            .root
            .clone()
            .ok_or_else(|| Error("No project is open".into()))?;
        let content = std::fs::read_to_string(root.join(path))?;
        let project = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let plan = gutter::dockerfile_run(path, &project, &content);
        let (cli, env) = self.docker_cli();
        let node = Target::Docker.node_id();
        self.set_busy(&node, true);
        let mut r = self
            .run_cli(&node, &cli, &plan.build_args, &env, &root)
            .await;
        if r.is_ok() {
            let client = self.lock().client.clone();
            if let Some(client) = client {
                let _ = client.remove_container(&plan.container, true).await;
            }
            r = self.run_cli(&node, &cli, &plan.run_args, &env, &root).await;
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
                    Ok(c) => c.restart_deployment(namespace, name, &rfc3339_now()).await,
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
                let s = self.lock();
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
                drop(s);
                let container = self.pod_container(context, namespace, name, container);
                let make = if attach {
                    exec::kubectl_attach
                } else {
                    exec::kubectl_exec
                };
                return Ok(make(
                    &kubectl,
                    joined.as_deref(),
                    context,
                    namespace,
                    name,
                    container.as_deref(),
                ));
            }
            _ => None,
        };
        let (id, name) = found.ok_or_else(|| Error("No container to open a terminal in".into()))?;
        Ok(if attach {
            exec::docker_attach(&cli, &id, &name, env)
        } else {
            exec::docker_exec(&cli, &id, &name, env)
        })
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
        };
        let hub = self.clone();
        let task = self.runtime.spawn(async move {
            // Aborting the listener drops the set, which aborts its connections.
            let mut conns = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((tcp, _)) = accepted else { continue };
                        tcp.set_nodelay(true).ok();
                        hub.update_forward(id, |f| f.connections += 1);
                        let (hub, client, ns, pod) =
                            (hub.clone(), client.clone(), namespace.clone(), name.clone());
                        conns.spawn(async move {
                            let r = match client.port_forward(&ns, &pod, remote).await {
                                Ok(ws) => ws.port_forward(remote, tcp).await,
                                Err(e) => Err(e),
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
            KubeClient::with_transport(crate::http::Transport::Unix(path), None),
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
        assert_eq!(hub.input().port_forwards, [f.clone()]);
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
            KubeClient::with_transport(crate::http::Transport::Unix(path), None),
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
}
