//! Agent Tasks (`forge-agent-tasks`): AI agents that work on a task in a sandbox (a git
//! worktree in Forge's cache folder), run the tests and fix failures, and leave a diff to
//! review. Apply patches the working tree (all or nothing), Apply as Commit / Branch commit
//! the changes, Discard removes the sandbox.
//!
//! The sandbox lives where the project's files are: in this machine's cache folder for a
//! local project, in the host's cache folder for a remote one (created, diffed, applied and
//! removed through the backend's `AgentSandbox` calls; the Forge agent's file tools and the
//! task's commands run on the host). With "Run in container" the task's commands and tests
//! run in a Docker / Podman container that mounts only the sandbox, without network unless
//! the task allows it (then every command asks first).
//!
//! Refused in Safe Mode (agents run project code) and when AI is off for the project
//! (`ai.enabled`). Every change of a task is emitted as `agent-tasks://changed` with the
//! whole task.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use forge_agent_cli::{
    agent_by_id, AgentEvent, AgentSession, EventSink, PermissionDecision, PermissionMode,
    StartOptions,
};
use forge_agent_tasks::cli::{classify, CliVerdict, ToolClass};
use forge_agent_tasks::commands::{run_process, split_command};
use forge_agent_tasks::container::{self, ContainerOptions, ContainerSpec};
use forge_agent_tasks::remote::{SandboxInfo, ToolReply};
use forge_agent_tasks::sandbox;
use forge_agent_tasks::tools::ToolCall;
use forge_agent_tasks::{
    permission, run_builtin, run_cli, AgentBackend, AgentTask, ApplyAnswer, ApplyMode, Budget,
    CliTurns, FileChange, FileVersions, Host, Outcome, RunOutput, SandboxCall, SandboxRef,
    ShellCommand, StepKind, TaskContext, TaskRequest, TaskStatus, TaskStore, TestChoice,
    TestSpec, Workspace,
};
use forge_ai::{AiFeature, CancellationToken};
use forge_ai_editor::policy::IGNORE_FILES;
use forge_ai_editor::AiIgnore;
use forge_backend::agent_tasks::{run_to_end, sandbox_call};
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;

use crate::commands::ai::AiState;
use crate::commands::ai_editor::PROJECT_ENABLED;
use crate::commands::run::{configuration_command, RunState};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Settings read here (schema: `app/src-tauri/plugins/packs.toml`, Tools | AI Assistant).
const MAX_STEPS: &str = "ai.agentTasks.maxSteps";
const MAX_TEST_RUNS: &str = "ai.agentTasks.maxTestRuns";
const ALLOWED_COMMANDS: &str = "ai.agentTasks.allowedCommands";
const CONTAINER_ENGINE: &str = "ai.agentTasks.container.engine";
const CONTAINER_IMAGE: &str = "ai.agentTasks.container.image";
const CONTAINER_CPUS: &str = "ai.agentTasks.container.cpus";
const CONTAINER_MEMORY: &str = "ai.agentTasks.container.memory";
/// The Docker CLI of Settings | Build, Execution, Deployment | Docker.
const DOCKER_CLI: &str = "docker.cliPath";

/// Pulling or building the image and starting the container.
const CONTAINER_START_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const CONTAINER_STOP_TIMEOUT: Duration = Duration::from_secs(60);

const CHANGED: &str = "agent-tasks://changed";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

#[derive(Default)]
struct Inner {
    /// The key of the project the loaded tasks belong to ([`project_key`]).
    project: Option<String>,
    tasks: Vec<AgentTask>,
    /// Cancellation of running tasks.
    running: HashMap<String, CancellationToken>,
    /// Commands waiting for approval: (task id, step index).
    approvals: HashMap<(String, usize), oneshot::Sender<bool>>,
    /// A worker works through the queue.
    worker: bool,
}

/// The tasks of the open project.
#[derive(Default, Clone)]
pub struct AgentTasksState {
    inner: Arc<Mutex<Inner>>,
}

/// What identifies the open project's tasks: its root on this machine, or the trust key
/// (`ssh://user@host:port/path`, …) of a remote project.
fn project_key(state: &AppState) -> Result<String> {
    let info = state.project_info().ok_or_else(ForgeError::no_project)?;
    Ok(if info.is_local() {
        info.root.to_string_lossy().into_owned()
    } else {
        crate::state::trust_key(&info)
    })
}

/// `<cache>/forge/agent-tasks/<project key>`: the task list (and, for local projects, the
/// sandboxes), never inside the project.
fn store_for(key: &str) -> Result<TaskStore> {
    let base = forge_agent_tasks::remote::default_base().ok_or_else(|| err("no cache folder"))?;
    Ok(TaskStore::new(forge_agent_tasks::remote::project_dir(
        &base,
        Path::new(key),
    )))
}

impl AgentTasksState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Load the project's tasks when the project changed.
    fn ensure(&self, key: &str) -> Result<()> {
        let mut inner = self.lock();
        if inner.project.as_deref() != Some(key) {
            inner.tasks = store_for(key)?.load();
            inner.project = Some(key.to_string());
            // Tasks of the previous project stop (their sandboxes stay for review).
            for token in inner.running.values() {
                token.cancel();
            }
            inner.running.clear();
            inner.approvals.clear();
            inner.worker = false;
        }
        Ok(())
    }

    fn task(&self, id: &str) -> Result<AgentTask> {
        self.lock()
            .tasks
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| err(format!("no agent task {id}")))
    }

    /// Change a task, store the list and emit the task.
    fn update<R>(
        &self,
        app: &AppHandle,
        id: &str,
        f: impl FnOnce(&mut AgentTask) -> R,
    ) -> Option<R> {
        let (task, out, root, tasks) = {
            let mut inner = self.lock();
            let root = inner.project.clone();
            let t = inner.tasks.iter_mut().find(|t| t.id == id)?;
            let out = f(t);
            t.updated = now_ms();
            let task = t.clone();
            (task, out, root, inner.tasks.clone())
        };
        if let Some(root) = root {
            if let Ok(store) = store_for(&root) {
                if let Err(e) = store.save(&tasks) {
                    tracing::warn!("agent tasks not saved: {e}");
                }
            }
        }
        if let Err(e) = app.emit(CHANGED, &task) {
            tracing::warn!("could not emit {CHANGED}: {e}");
        }
        Some(out)
    }

    fn log(&self, app: &AppHandle, id: &str, kind: StepKind) -> usize {
        self.update(app, id, |t| t.push(kind, now_ms()))
            .unwrap_or(0)
    }
}

/// Checks before anything an agent does.
fn require_allowed(state: &AppState) -> Result<PathBuf> {
    state.require_trusted("Agent tasks")?;
    if state.project_info().is_some_and(|i| i.collab.is_some()) {
        return Err(err(
            "Agent tasks are not available in a Code With Me session.",
        ));
    }
    if !state
        .read_settings()
        .get_as::<bool>(PROJECT_ENABLED)
        .unwrap_or(true)
    {
        return Err(err("AI features are turned off for this project"));
    }
    state.project_root()
}

#[tauri::command]
pub async fn agent_tasks_list(
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<Vec<AgentTask>> {
    tasks.ensure(&project_key(&state)?)?;
    let mut list = tasks.lock().tasks.clone();
    list.sort_by_key(|a| std::cmp::Reverse(a.created));
    Ok(list)
}

/// Queue a new task; it starts when no other task of the project runs.
#[tauri::command]
pub async fn agent_task_create(
    request: TaskRequest,
    app: AppHandle,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<AgentTask> {
    let root = require_allowed(&state)?;
    if request.prompt.trim().is_empty() {
        return Err(err("Describe the task first"));
    }
    if let TestChoice::Command { command } = &request.tests {
        split_command(command).map_err(err)?;
    }
    let remote = state.is_remote();
    if remote {
        if let AgentBackend::Cli { .. } = &request.backend {
            return Err(err(
                "Agent CLIs run on this machine; use the Forge agent for tasks of a remote project.",
            ));
        }
        if let TestChoice::Configuration { .. } = &request.tests {
            return Err(err(
                "Run configurations cannot verify tasks of a remote project yet; use a test command.",
            ));
        }
    } else {
        let r = root.clone();
        blocking(move || sandbox::repository_of(&r).map(|_| ()).map_err(err)).await?;
    }
    if request.container.is_some() {
        let s = state.read_settings();
        container::parse_cpus(&s.get_as::<String>(CONTAINER_CPUS).unwrap_or_default())
            .map_err(err)?;
        container::parse_memory(&s.get_as::<String>(CONTAINER_MEMORY).unwrap_or_default())
            .map_err(err)?;
    }
    tasks.ensure(&project_key(&state)?)?;
    let now = now_ms();
    let id = format!("task-{now:x}");
    let mut task = AgentTask::new(&id, request, now);
    task.remote = remote;
    tasks.lock().tasks.push(task.clone());
    tasks.update(&app, &id, |_| ());
    start_worker(&app, &tasks);
    Ok(task)
}

fn start_worker(app: &AppHandle, tasks: &AgentTasksState) {
    {
        let mut inner = tasks.lock();
        if inner.worker {
            return;
        }
        inner.worker = true;
    }
    let (app, tasks) = (app.clone(), tasks.clone());
    let project = tasks.lock().project.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let next = {
                let mut inner = tasks.lock();
                if inner.project != project {
                    // Another project was opened; its own worker takes over.
                    break;
                }
                let next = inner
                    .tasks
                    .iter()
                    .find(|t| t.status == TaskStatus::Queued)
                    .map(|t| t.id.clone());
                if next.is_none() {
                    inner.worker = false;
                }
                next
            };
            let Some(id) = next else { break };
            run_task(&app, &tasks, &id).await;
        }
    });
}

/// Run one task from sandbox creation to its outcome.
async fn run_task(app: &AppHandle, tasks: &AgentTasksState, id: &str) {
    let cancel = CancellationToken::new();
    tasks.lock().running.insert(id.to_string(), cancel.clone());
    tasks.update(app, id, |t| {
        t.status = TaskStatus::Running;
        t.error = None;
    });
    let result = prepare_and_run(app, tasks, id, &cancel).await;
    tasks.lock().running.remove(id);
    tasks.update(app, id, |t| match result {
        Ok(outcome) => {
            t.status = TaskStatus::NeedsReview;
            t.outcome = Some(outcome.clone());
            t.push(StepKind::Finished { outcome }, now_ms());
        }
        Err(e) if cancel.is_cancelled() || e == "cancelled" => {
            t.status = TaskStatus::Cancelled;
            t.push(
                StepKind::Info {
                    text: "Cancelled".into(),
                },
                now_ms(),
            );
        }
        Err(e) => {
            t.status = TaskStatus::Failed;
            t.push(StepKind::Error { message: e.clone() }, now_ms());
            t.error = Some(e);
        }
    });
}

fn setting<T: serde::de::DeserializeOwned>(state: &AppState, key: &str, default: T) -> T {
    state.read_settings().get_as(key).unwrap_or(default)
}

/// Map paths of the project in `text` to the same paths in the sandbox (only whole path
/// prefixes: `/p/app` matches `/p/app/x` but not `/p/apps`).
fn to_sandbox(text: &str, project: &str, sandbox: &str) -> String {
    container::map_paths(text, project, sandbox)
}

/// The test command of a task, pointed at the sandbox.
fn test_spec(
    app: &AppHandle,
    choice: &TestChoice,
    project: &Path,
    sb: &Path,
) -> std::result::Result<Option<TestSpec>, String> {
    match choice {
        TestChoice::None => Ok(None),
        TestChoice::Command { command } => {
            let words = split_command(command)?;
            Ok(Some(TestSpec {
                label: "Tests".into(),
                command: ShellCommand {
                    program: words[0].clone(),
                    args: words[1..].to_vec(),
                    cwd: sb.to_path_buf(),
                    env: Default::default(),
                },
                report: None,
            }))
        }
        TestChoice::Configuration { name } => {
            let state = app.state::<AppState>();
            let runs = app.state::<RunState>();
            let configs =
                forge_process::RunConfigurations::load(project).map_err(|e| e.to_string())?;
            let cfg = configs
                .configurations
                .into_iter()
                .find(|c| &c.name == name)
                .ok_or_else(|| format!("run configuration {name:?} not found"))?;
            let (cmd, report) =
                configuration_command(&state, &runs, cfg).map_err(|e| e.to_string())?;
            let (p, s) = (project.to_string_lossy(), sb.to_string_lossy());
            let map = |t: &str| to_sandbox(t, &p, &s);
            let cwd = if cmd.cwd.as_os_str().is_empty() {
                sb.to_path_buf()
            } else {
                PathBuf::from(map(&cmd.cwd.to_string_lossy()))
            };
            Ok(Some(TestSpec {
                label: name.clone(),
                command: ShellCommand {
                    program: map(&cmd.program),
                    args: cmd.args.iter().map(|a| map(a)).collect(),
                    cwd,
                    env: cmd.env.iter().map(|(k, v)| (k.clone(), map(v))).collect(),
                },
                report,
            }))
        }
    }
}

/// Where a task's processes run: on this machine, or on a remote project's host through
/// its backend.
#[derive(Clone)]
struct Runner {
    remote: Option<Arc<dyn Backend>>,
}

impl Runner {
    fn of(state: &AppState) -> Result<Self> {
        Ok(Self {
            remote: if state.is_remote() {
                Some(state.backend()?)
            } else {
                None
            },
        })
    }

    async fn run(
        &self,
        cmd: &ShellCommand,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> RunOutput {
        match &self.remote {
            None => run_process(cmd, timeout, cancel).await,
            Some(backend) => {
                let mut env = cmd.env.clone();
                env.entry("CI".into()).or_insert_with(|| "1".into());
                env.entry("GIT_TERMINAL_PROMPT".into())
                    .or_insert_with(|| "0".into());
                let line = forge_process::CommandLine {
                    program: cmd.program.clone(),
                    args: cmd.args.clone(),
                    cwd: cmd.cwd.clone(),
                    env,
                };
                run_to_end(backend.clone(), line, timeout, cancel.cancelled()).await
            }
        }
    }
}

/// A sandbox call through the project's backend, off the async runtime.
async fn call<T: serde::de::DeserializeOwned + Send + 'static>(
    backend: &Arc<dyn Backend>,
    c: SandboxCall,
) -> Result<T> {
    let b = backend.clone();
    blocking(move || sandbox_call::<T>(b.as_ref(), c)).await
}

/// The Docker / Podman CLI for container tasks: the Agent Tasks setting, else the Docker
/// setting (on this machine) or `docker` (on a remote host).
fn container_engine(state: &AppState) -> String {
    let s = state.read_settings();
    let own = s
        .get_as::<String>(CONTAINER_ENGINE)
        .filter(|e| !e.trim().is_empty());
    let docker = if state.is_remote() {
        None
    } else {
        s.get_as::<String>(DOCKER_CLI)
            .filter(|e| !e.trim().is_empty())
    };
    own.or(docker).unwrap_or_else(|| "docker".into())
}

/// Where a container task's image comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ImagePlan {
    Use(String),
    /// `docker build` arguments (from `devcontainer.json`) and the tag they produce.
    Build {
        args: Vec<String>,
        tag: String,
    },
}

/// The image of a container task: the project's `devcontainer.json` (`config`: its
/// absolute path and text) when it names an image or a Dockerfile, else `fallback`. Also
/// returns its `containerEnv` and a note for the log. Its mounts, ports, run arguments and
/// lifecycle commands are not used: the container mounts only the sandbox.
fn container_image(
    config: Option<(&str, &str)>,
    folder: &str,
    fallback: &str,
) -> (
    ImagePlan,
    std::collections::BTreeMap<String, String>,
    Option<String>,
) {
    use forge_devenv::devcontainer::{load, ContainerSource, LocalContext};
    let fallback = || ImagePlan::Use(fallback.trim().to_string());
    let Some((path, text)) = config else {
        return (fallback(), Default::default(), None);
    };
    let no_env = |_: &str| None;
    let ctx = LocalContext {
        folder,
        config_path: path,
        env: &no_env,
    };
    match load(text, &ctx) {
        Ok(cfg) => match &cfg.source {
            ContainerSource::Image { image } => (
                ImagePlan::Use(image.clone()),
                cfg.container_env.clone(),
                Some(format!("Image {image} from devcontainer.json")),
            ),
            ContainerSource::Dockerfile { .. } => match forge_devenv::docker::build_args(&cfg, false)
            {
                Some(args) => (
                    ImagePlan::Build {
                        args,
                        tag: forge_devenv::docker::image_tag(&cfg),
                    },
                    cfg.container_env.clone(),
                    Some("Building the image of devcontainer.json".into()),
                ),
                None => (fallback(), Default::default(), None),
            },
            ContainerSource::Compose { .. } => (
                fallback(),
                Default::default(),
                Some(
                    "devcontainer.json uses Docker Compose, which agent tasks do not start; using the image of the settings"
                        .into(),
                ),
            ),
        },
        Err(e) => (
            fallback(),
            Default::default(),
            Some(format!(
                "devcontainer.json is not used ({e}); using the image of the settings"
            )),
        ),
    }
}

/// Start the container of a container task; it mounts only the sandbox `sandbox_dir`.
#[allow(clippy::too_many_arguments)]
async fn start_container(
    app: &AppHandle,
    tasks: &AgentTasksState,
    id: &str,
    opts: &ContainerOptions,
    sandbox_dir: &str,
    root: &Path,
    runner: &Runner,
    cancel: &CancellationToken,
) -> std::result::Result<ContainerSpec, String> {
    let state = app.state::<AppState>();
    let engine = container_engine(&state);
    let (cpus, memory, fallback) = {
        let s = state.read_settings();
        (
            container::parse_cpus(&s.get_as::<String>(CONTAINER_CPUS).unwrap_or_default())?,
            container::parse_memory(&s.get_as::<String>(CONTAINER_MEMORY).unwrap_or_default())?,
            s.get_as::<String>(CONTAINER_IMAGE).unwrap_or_default(),
        )
    };
    let (plan, env, note) = match opts.image.as_deref().map(str::trim) {
        Some(image) if !image.is_empty() => {
            (ImagePlan::Use(image.into()), Default::default(), None)
        }
        _ => {
            let backend = state.backend().map_err(|e| e.to_string())?;
            let folder = root.to_string_lossy().into_owned();
            let found = blocking(move || {
                for rel in [
                    forge_devenv::devcontainer::PRIMARY_CONFIG,
                    forge_devenv::devcontainer::ROOT_CONFIG,
                ] {
                    if let Some(text) = backend.read_optional(rel)? {
                        return Ok(Some((format!("{folder}/{rel}"), text)));
                    }
                }
                Ok(None)
            })
            .await
            .map_err(|e| e.to_string())?;
            container_image(
                found.as_ref().map(|(p, t)| (p.as_str(), t.as_str())),
                &root.to_string_lossy(),
                &fallback,
            )
        }
    };
    if let Some(note) = note {
        tasks.log(app, id, StepKind::Info { text: note });
    }
    let image = match plan {
        ImagePlan::Use(image) if image.is_empty() => {
            return Err("No container image: set one in Settings | Tools | AI Assistant | Agent Tasks or add a devcontainer.json.".into())
        }
        ImagePlan::Use(image) => image,
        ImagePlan::Build { args, tag } => {
            let build = ShellCommand {
                program: engine.clone(),
                args,
                cwd: root.to_path_buf(),
                env: Default::default(),
            };
            tasks.log(
                app,
                id,
                StepKind::Info {
                    text: build.display(),
                },
            );
            let out = runner.run(&build, CONTAINER_START_TIMEOUT, cancel).await;
            if out.exit_code != Some(0) {
                return Err(format!(
                    "The dev container image could not be built:\n{}",
                    forge_agent_tasks::commands::tail(&out.output, 4000)
                ));
            }
            tag
        }
    };
    // The container runs as the sandbox's owner so its files stay removable (Podman maps
    // the user itself; Docker Desktop on Windows maps file ownership).
    let user = if container::is_podman(&engine) || (runner.remote.is_none() && cfg!(windows)) {
        None
    } else {
        let id_of = |flag: &str| ShellCommand {
            program: "id".into(),
            args: vec![flag.into()],
            cwd: PathBuf::from(sandbox_dir),
            env: Default::default(),
        };
        let u = runner.run(&id_of("-u"), CONTAINER_STOP_TIMEOUT, cancel).await;
        let g = runner.run(&id_of("-g"), CONTAINER_STOP_TIMEOUT, cancel).await;
        match (u.exit_code, g.exit_code) {
            (Some(0), Some(0)) => Some(format!("{}:{}", u.output.trim(), g.output.trim())),
            _ => None,
        }
    };
    let spec = ContainerSpec {
        engine,
        name: container::container_name(id),
        task_id: id.to_string(),
        image,
        sandbox: sandbox_dir.to_string(),
        network: opts.network,
        cpus,
        memory,
        user,
        env,
    };
    let start = spec.start()?;
    tasks.update(app, id, |t| t.container = Some(spec.name.clone()));
    tasks.log(
        app,
        id,
        StepKind::Info {
            text: format!(
                "Starting container {} from {} ({}; only the sandbox is mounted)",
                spec.name,
                spec.image,
                if spec.network {
                    "network allowed, every command asks first"
                } else {
                    "no network"
                }
            ),
        },
    );
    let out = runner.run(&start, CONTAINER_START_TIMEOUT, cancel).await;
    if out.exit_code != Some(0) {
        tasks.update(app, id, |t| t.container = None);
        return Err(format!(
            "The container could not be started ({}):\n{}",
            start.display(),
            forge_agent_tasks::commands::tail(&out.output, 4000)
        ));
    }
    Ok(spec)
}

/// Remove a task's container (best effort; not cancellable).
async fn remove_container(runner: &Runner, engine: &str, name: &str, cwd: &str) -> Option<String> {
    let cmd = container::remove_command(engine, name, cwd);
    let out = runner
        .run(&cmd, CONTAINER_STOP_TIMEOUT, &CancellationToken::new())
        .await;
    if out.exit_code == Some(0) || container::already_gone(&out.output) {
        None
    } else {
        Some(out.output)
    }
}

async fn prepare_and_run(
    app: &AppHandle,
    tasks: &AgentTasksState,
    id: &str,
    cancel: &CancellationToken,
) -> std::result::Result<Outcome, String> {
    let state = app.state::<AppState>();
    let root = require_allowed(&state).map_err(|e| e.to_string())?;
    let task = tasks.task(id).map_err(|e| e.to_string())?;
    let backend = state.backend().map_err(|e| e.to_string())?;
    let runner = Runner::of(&state).map_err(|e| e.to_string())?;
    let remote = runner.remote.is_some();

    tasks.log(
        app,
        id,
        StepKind::Info {
            text: if remote {
                "Creating the sandbox (a git worktree in the cache folder of the remote host)"
                    .into()
            } else {
                "Creating the sandbox (a git worktree in Forge's cache folder)".into()
            },
        },
    );
    let info: SandboxInfo = call(&backend, SandboxCall::Create { id: id.to_string() })
        .await
        .map_err(|e| e.to_string())?;
    tasks.update(app, id, |t| {
        t.sandbox = Some(PathBuf::from(&info.dir));
        t.base_commit = Some(info.base.clone());
    });
    tasks.log(
        app,
        id,
        StepKind::Info {
            text: format!("Sandbox ready at {}", info.dir),
        },
    );
    let project_dir = PathBuf::from(&info.project_dir);

    // Locally the file tools read the sandbox directly; for a remote project they run on
    // the host, which applies .aiignore there.
    let ignore = if remote {
        AiIgnore::default()
    } else {
        AiIgnore::merged(
            IGNORE_FILES
                .iter()
                .filter_map(|f| std::fs::read_to_string(project_dir.join(f)).ok())
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str),
        )
    };
    let tests = test_spec(app, &task.request.tests, &root, &project_dir)?;
    let budget = Budget {
        max_steps: task
            .request
            .max_steps
            .unwrap_or_else(|| setting(&state, MAX_STEPS, 40))
            .clamp(1, 500),
        max_test_runs: setting(&state, MAX_TEST_RUNS, 6u32).clamp(1, 100),
        ..Budget::default()
    };
    let ctx = TaskContext {
        prompt: task.request.prompt.clone(),
        workspace: Workspace::new(&project_dir, ignore, task.request.scope.clone()),
        allowlist: setting(&state, ALLOWED_COMMANDS, Vec::<String>::new()),
        tests,
        budget,
        approve_all: task.request.container.as_ref().is_some_and(|c| c.network),
    };

    let container = match &task.request.container {
        Some(opts) => Some(
            start_container(app, tasks, id, opts, &info.dir, &root, &runner, cancel).await?,
        ),
        None => None,
    };
    let host = AppHost {
        app: app.clone(),
        tasks: tasks.clone(),
        id: id.to_string(),
        cancel: cancel.clone(),
        runner: runner.clone(),
        container: container.clone(),
        remote_tools: remote.then(|| RemoteTools {
            backend: backend.clone(),
            sandbox: SandboxRef {
                dir: info.dir.clone(),
                base: info.base.clone(),
            },
            scope: task.request.scope.clone(),
        }),
    };
    let result = match &task.request.backend {
        AgentBackend::Builtin { provider, model } => {
            let ai = app.state::<AiState>();
            match ai
                .service
                .selection(provider.as_deref(), model.as_deref(), Some(AiFeature::Chat))
            {
                Err(e) => Err(e.to_string()),
                Ok((cfg, model)) => {
                    let provider = ai.service.build(&cfg);
                    tasks.log(
                        app,
                        id,
                        StepKind::Info {
                            text: format!(
                                "Forge agent with {} ({})",
                                cfg.name,
                                model
                                    .clone()
                                    .or(cfg.default_model.clone())
                                    .unwrap_or_default()
                            ),
                        },
                    );
                    run_builtin(provider.as_ref(), model, &ctx, &host, cancel).await
                }
            }
        }
        AgentBackend::Cli { agent_id, model } => {
            run_with_cli(
                app,
                &state,
                agent_id,
                model.clone(),
                &PathBuf::from(&info.dir),
                &ctx,
                &host,
                cancel,
            )
            .await
        }
    };
    if let Some(spec) = container {
        // The container goes with the run; the sandbox stays for review.
        match remove_container(&runner, &spec.engine, &spec.name, &info.dir).await {
            None => {
                tasks.update(app, id, |t| t.container = None);
                tasks.log(
                    app,
                    id,
                    StepKind::Info {
                        text: format!("Removed container {}", spec.name),
                    },
                );
            }
            Some(e) => {
                tracing::warn!("container {} not removed: {e}", spec.name);
            }
        }
    }
    result
}

/// The file tools of a remote project's sandbox, run on its host.
struct RemoteTools {
    backend: Arc<dyn Backend>,
    sandbox: SandboxRef,
    scope: Vec<String>,
}

/// Processes, approvals and the log of a running task.
struct AppHost {
    app: AppHandle,
    tasks: AgentTasksState,
    id: String,
    cancel: CancellationToken,
    runner: Runner,
    /// The container a container task's processes run in.
    container: Option<ContainerSpec>,
    remote_tools: Option<RemoteTools>,
}

impl AppHost {
    /// A host that only logs and asks for approvals.
    fn logger(app: &AppHandle, tasks: &AgentTasksState, id: &str, cancel: &CancellationToken) -> Self {
        Self {
            app: app.clone(),
            tasks: tasks.clone(),
            id: id.to_string(),
            cancel: cancel.clone(),
            runner: Runner { remote: None },
            container: None,
            remote_tools: None,
        }
    }
}

#[async_trait]
impl Host for AppHost {
    async fn run(&self, cmd: &ShellCommand, timeout: Duration) -> RunOutput {
        let cmd = match &self.container {
            None => cmd.clone(),
            Some(c) => match c.exec(cmd) {
                Ok(cmd) => cmd,
                Err(e) => {
                    return RunOutput {
                        exit_code: None,
                        output: e,
                        timed_out: false,
                    }
                }
            },
        };
        self.runner.run(&cmd, timeout, &self.cancel).await
    }

    async fn file_tool(
        &self,
        ws: &Workspace,
        tool: &ToolCall,
    ) -> std::result::Result<String, String> {
        let Some(r) = &self.remote_tools else {
            return forge_agent_tasks::tools::execute_file_tool(ws, tool)
                .unwrap_or_else(|| Err(format!("{} is not a file tool", tool.name())));
        };
        let c = SandboxCall::Tool {
            sandbox: r.sandbox.clone(),
            scope: r.scope.clone(),
            call: tool.clone(),
        };
        match call::<ToolReply>(&r.backend, c).await {
            Ok(reply) => reply.into(),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn approve(&self, step: usize, _command: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        self.tasks
            .lock()
            .approvals
            .insert((self.id.clone(), step), tx);
        tokio::select! {
            r = rx => r.unwrap_or(false),
            _ = self.cancel.cancelled() => false,
        }
    }

    fn log(&self, kind: StepKind) -> usize {
        self.tasks.log(&self.app, &self.id, kind)
    }

    fn update(&self, index: usize, kind: StepKind) {
        self.tasks
            .update(&self.app, &self.id, |t| t.replace(index, kind, now_ms()));
    }
}

/// Where the end of the running CLI turn is reported.
type TurnDone = Arc<Mutex<Option<oneshot::Sender<std::result::Result<Option<String>, String>>>>>;

/// An agent CLI session in the sandbox, one turn at a time.
struct CliSession {
    session: Arc<AgentSession>,
    turn_done: TurnDone,
}

#[async_trait]
impl CliTurns for CliSession {
    async fn turn(&self, message: &str) -> std::result::Result<Option<String>, String> {
        let (tx, rx) = oneshot::channel();
        *self.turn_done.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        self.session
            .send(message)
            .await
            .map_err(|e| e.to_string())?;
        rx.await.unwrap_or_else(|_| Err("the agent stopped".into()))
    }
}

fn short_input(input: &serde_json::Value) -> String {
    for key in [
        "command",
        "file_path",
        "path",
        "pattern",
        "absolute_path",
        "description",
    ] {
        if let Some(v) = input.get(key).and_then(|v| v.as_str()) {
            return v
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(160)
                .collect();
        }
    }
    String::new()
}

#[allow(clippy::too_many_arguments)]
async fn run_with_cli(
    app: &AppHandle,
    state: &AppState,
    agent_id: &str,
    model: Option<String>,
    sandbox_dir: &Path,
    ctx: &TaskContext,
    host: &AppHost,
    cancel: &CancellationToken,
) -> std::result::Result<Outcome, String> {
    let adapter = agent_by_id(agent_id).ok_or_else(|| format!("unknown agent {agent_id}"))?;
    // Claude Code asks Forge for every tool call; Gemini CLI cannot ask in headless mode, so
    // it may edit (in the sandbox) but gets no shell; Forge runs the tests between turns.
    let permission_mode = if agent_id == "gemini-cli" {
        PermissionMode::AcceptEdits
    } else {
        PermissionMode::Default
    };
    let opts = StartOptions {
        model,
        permission_mode,
        partial_messages: Some(false),
        ..Default::default()
    }
    .with_settings(agent_id, |k| {
        state.read_settings().get_str(k).map(String::from)
    });

    let session_cell: Arc<OnceLock<Arc<AgentSession>>> = Arc::new(OnceLock::new());
    let turn_done: TurnDone = Arc::new(Mutex::new(None));
    let tool_steps: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
    let sink: EventSink = {
        let (app, tasks, id) = (app.clone(), host.tasks.clone(), host.id.clone());
        let (cell, turn_done, tool_steps) =
            (session_cell.clone(), turn_done.clone(), tool_steps.clone());
        let ws = ctx.workspace.clone();
        let allowlist = ctx.allowlist.clone();
        let cancel = cancel.clone();
        // The CLI's own shell would run on this machine, outside the container.
        let in_container = host.container.is_some();
        Arc::new(move |event: AgentEvent| match event {
            AgentEvent::AssistantText {
                text, delta: false, ..
            } if !text.trim().is_empty() => {
                tasks.log(&app, &id, StepKind::Thought { text });
            }
            AgentEvent::ToolUse {
                id: use_id,
                name,
                input,
                ..
            } => {
                let summary = short_input(&input);
                let step = tasks.log(
                    &app,
                    &id,
                    StepKind::Tool {
                        tool: name,
                        summary,
                        input,
                        output: String::new(),
                        is_error: false,
                    },
                );
                tool_steps
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(use_id, step);
            }
            AgentEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let step = tool_steps
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&tool_use_id)
                    .copied();
                if let Some(step) = step {
                    tasks.update(&app, &id, |t| {
                        if let Some(StepKind::Tool {
                            output,
                            is_error: e,
                            ..
                        }) = t.steps.get_mut(step).map(|s| &mut s.kind)
                        {
                            *output = forge_agent_tasks::commands::tail(&content, 4000);
                            *e = is_error;
                        }
                    });
                }
            }
            AgentEvent::PermissionRequest {
                request_id,
                tool_name,
                input,
                ..
            } => {
                let verdict = if in_container && classify(&tool_name) == ToolClass::Shell {
                    CliVerdict::Deny(
                        "This task runs its commands in a container, which your shell cannot reach. Do not run commands; Forge runs the tests in the container after your turn."
                            .into(),
                    )
                } else {
                    permission(&tool_name, &input, &ws, &allowlist)
                };
                let Some(session) = cell.get().cloned() else {
                    return;
                };
                let host = AppHost::logger(&app, &tasks, &id, &cancel);
                tauri::async_runtime::spawn(async move {
                    let allowed = match verdict {
                        CliVerdict::Allow => Ok(()),
                        CliVerdict::Deny(m) => Err(m),
                        CliVerdict::Ask(command) => {
                            let step = host.log(StepKind::Approval {
                                command: command.clone(),
                                state: forge_agent_tasks::ApprovalState::Pending,
                            });
                            let ok = host.approve(step, &command).await;
                            host.update(
                                step,
                                StepKind::Approval {
                                    command,
                                    state: if ok {
                                        forge_agent_tasks::ApprovalState::Allowed
                                    } else {
                                        forge_agent_tasks::ApprovalState::Denied
                                    },
                                },
                            );
                            if ok {
                                Ok(())
                            } else {
                                Err("The user denied this command.".into())
                            }
                        }
                    };
                    let decision = match allowed {
                        Ok(()) => PermissionDecision::Allow {
                            updated_input: None,
                            scope: Default::default(),
                        },
                        Err(message) => PermissionDecision::Deny {
                            message: Some(message),
                        },
                    };
                    if let Err(e) = session.respond_permission(&request_id, decision).await {
                        tracing::warn!("permission answer failed: {e}");
                    }
                });
            }
            AgentEvent::TurnFinished {
                result, is_error, ..
            } => {
                if let Some(tx) = turn_done.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = tx.send(if is_error {
                        Err(result.unwrap_or_else(|| "the agent reported an error".into()))
                    } else {
                        Ok(result)
                    });
                }
            }
            AgentEvent::Error { message } => {
                tasks.log(&app, &id, StepKind::Error { message });
            }
            AgentEvent::Exited { code } => {
                if let Some(tx) = turn_done.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = tx.send(Err(format!(
                        "the agent exited (code {})",
                        code.map_or("none".into(), |c| c.to_string())
                    )));
                }
            }
            _ => {}
        })
    };
    host.log(StepKind::Info {
        text: format!("Starting {} in the sandbox", adapter.name()),
    });
    let session = Arc::new(
        AgentSession::start(adapter, sandbox_dir.to_path_buf(), opts, sink)
            .await
            .map_err(|e| e.to_string())?,
    );
    let _ = session_cell.set(session.clone());
    let turns = CliSession {
        session: session.clone(),
        turn_done,
    };
    let result = run_cli(&turns, ctx, host, cancel).await;
    session.stop().await;
    result
}

/// Stop a queued or running task (its sandbox stays for review).
#[tauri::command]
pub async fn agent_task_cancel(
    id: String,
    app: AppHandle,
    tasks: State<'_, AgentTasksState>,
) -> Result<()> {
    let token = tasks.lock().running.get(&id).cloned();
    match token {
        Some(t) => t.cancel(),
        None => {
            tasks.update(&app, &id, |t| {
                if t.status == TaskStatus::Queued {
                    t.status = TaskStatus::Discarded;
                }
            });
        }
    }
    Ok(())
}

/// Answer a command approval (the step index of the approval in the task's log).
#[tauri::command]
pub async fn agent_task_approve(
    id: String,
    step: usize,
    allow: bool,
    tasks: State<'_, AgentTasksState>,
) -> Result<()> {
    let tx = tasks.lock().approvals.remove(&(id, step));
    match tx {
        Some(tx) => {
            let _ = tx.send(allow);
            Ok(())
        }
        None => Err(err("This approval is no longer pending")),
    }
}

/// The sandbox of a task as the backend's calls name it.
fn sandbox_ref(task: &AgentTask) -> Result<SandboxRef> {
    let (Some(dir), Some(base)) = (&task.sandbox, &task.base_commit) else {
        return Err(err("This task has no sandbox"));
    };
    Ok(SandboxRef {
        dir: dir.to_string_lossy().into_owned(),
        base: base.clone(),
    })
}

/// The task, when it belongs to the open project's machine (a task of a remote project
/// cannot be reviewed while a local one is open with the same key, and vice versa).
fn task_here(state: &AppState, tasks: &AgentTasksState, id: &str) -> Result<AgentTask> {
    let task = tasks.task(id)?;
    if task.remote != state.is_remote() {
        return Err(err("This task belongs to another machine"));
    }
    Ok(task)
}

/// The files the agent changed.
#[tauri::command]
pub async fn agent_task_changes(
    id: String,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<Vec<FileChange>> {
    let sb = sandbox_ref(&task_here(&state, &tasks, &id)?)?;
    call(&state.backend()?, SandboxCall::Changes { sandbox: sb }).await
}

/// Both sides of a changed file for the diff viewer.
#[tauri::command]
pub async fn agent_task_file_diff(
    id: String,
    path: String,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<FileVersions> {
    let sb = sandbox_ref(&task_here(&state, &tasks, &id)?)?;
    call(
        &state.backend()?,
        SandboxCall::FileVersions { sandbox: sb, path },
    )
    .await
}

/// Apply the agent's changes (all of them, or `paths`). The sandbox is removed once the
/// changes are applied completely.
#[tauri::command]
pub async fn agent_task_apply(
    id: String,
    mode: ApplyMode,
    paths: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<ApplyAnswer> {
    state.require_trusted("Applying agent tasks")?;
    let task = task_here(&state, &tasks, &id)?;
    if !task.status.reviewable() {
        return Err(err("This task has nothing to apply"));
    }
    let sb = sandbox_ref(&task)?;
    let backend = state.backend()?;
    let paths = paths.unwrap_or_default();
    let partial = !paths.is_empty();
    let answer: ApplyAnswer = call(
        &backend,
        SandboxCall::Apply {
            sandbox: sb.clone(),
            mode: mode.clone(),
            paths,
        },
    )
    .await?;
    if answer.result.applied && !partial {
        let created = answer.created.clone();
        let applied_text = match &mode {
            ApplyMode::Apply => "Applied to the working tree".to_string(),
            ApplyMode::Commit { .. } => {
                format!("Committed as {}", created.clone().unwrap_or_default())
            }
            ApplyMode::Branch { .. } => {
                format!("Created branch {}", created.clone().unwrap_or_default())
            }
        };
        let _ = call::<()>(&backend, SandboxCall::Remove { dir: sb.dir }).await;
        tasks.update(&app, &id, |t| {
            t.status = TaskStatus::Applied;
            t.applied = Some(applied_text.clone());
            t.push(StepKind::Info { text: applied_text }, now_ms());
        });
    } else if answer.result.applied {
        let text = format!("Applied {}", answer.result.files.join(", "));
        tasks.log(&app, &id, StepKind::Info { text });
    }
    Ok(answer)
}

/// Remove the sandbox (and a container left behind) without applying anything.
#[tauri::command]
pub async fn agent_task_discard(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<()> {
    let task = task_here(&state, &tasks, &id)?;
    if task.status.is_active() {
        return Err(err("Stop the task first"));
    }
    if let Some(name) = &task.container {
        let root = state.project_root()?.to_string_lossy().into_owned();
        let engine = container_engine(&state);
        if let Some(e) = remove_container(&Runner::of(&state)?, &engine, name, &root).await {
            tracing::warn!("container {name} not removed: {e}");
        }
    }
    if let Some(dir) = &task.sandbox {
        let dir = dir.to_string_lossy().into_owned();
        call::<()>(&state.backend()?, SandboxCall::Remove { dir }).await?;
    }
    tasks.update(&app, &id, |t| {
        t.container = None;
        if t.status != TaskStatus::Applied {
            t.status = TaskStatus::Discarded;
        }
    });
    Ok(())
}

/// Remove a finished task from the list (discarding its sandbox).
#[tauri::command]
pub async fn agent_task_remove(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<()> {
    agent_task_discard(id.clone(), app, state.clone(), tasks.clone()).await?;
    let key = project_key(&state)?;
    let list = {
        let mut inner = tasks.lock();
        inner.tasks.retain(|t| t.id != id);
        inner.tasks.clone()
    };
    store_for(&key)?.save(&list).map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_project_paths_into_the_sandbox() {
        assert_eq!(
            to_sandbox("--configuration=/p/app/phpunit.xml", "/p/app", "/c/t1"),
            "--configuration=/c/t1/phpunit.xml"
        );
        assert_eq!(to_sandbox("x", "", "/c"), "x");
        assert_eq!(to_sandbox("/p/apps:/p/app", "/p/app", "/c"), "/p/apps:/c");
        assert_eq!(
            to_sandbox("-v/p/app/:/var/www", "/p/app/", "/c"),
            "-v/c/:/var/www"
        );
        assert_eq!(
            short_input(&serde_json::json!({ "command": "ls\n-la" })),
            "ls"
        );
        let m: ApplyMode =
            serde_json::from_str(r#"{"mode":"branch","branch":"b","message":"m"}"#).unwrap();
        assert!(matches!(m, ApplyMode::Branch { .. }));
    }

    #[test]
    fn container_images_come_from_devcontainer_json_or_the_settings() {
        let (plan, env, note) = container_image(None, "/p", " php:8.3-cli ");
        assert_eq!(plan, ImagePlan::Use("php:8.3-cli".into()));
        assert!(env.is_empty() && note.is_none());

        let image = r#"{ // comment
            "image": "mcr.microsoft.com/devcontainers/php:8.3",
            "containerEnv": { "APP_ENV": "testing" },
            "mounts": ["source=/,target=/host,type=bind"],
        }"#;
        let (plan, env, note) = container_image(
            Some(("/p/.devcontainer/devcontainer.json", image)),
            "/p",
            "php:8.3-cli",
        );
        assert_eq!(
            plan,
            ImagePlan::Use("mcr.microsoft.com/devcontainers/php:8.3".into())
        );
        assert_eq!(env.get("APP_ENV").map(String::as_str), Some("testing"));
        assert!(note.unwrap().contains("devcontainer.json"));

        let build = r#"{ "build": { "dockerfile": "Dockerfile", "args": { "V": "1" } } }"#;
        let (plan, _, _) = container_image(
            Some(("/p/.devcontainer/devcontainer.json", build)),
            "/p",
            "php:8.3-cli",
        );
        let ImagePlan::Build { args, tag } = plan else {
            panic!("expected a build");
        };
        assert_eq!(args[0], "build");
        assert!(args.contains(&"/p/.devcontainer/Dockerfile".to_string()));
        assert!(args.contains(&tag));

        let compose = r#"{ "dockerComposeFile": "compose.yaml", "service": "app" }"#;
        let (plan, _, note) = container_image(
            Some(("/p/.devcontainer/devcontainer.json", compose)),
            "/p",
            "node:22",
        );
        assert_eq!(plan, ImagePlan::Use("node:22".into()));
        assert!(note.unwrap().contains("Compose"));
        let (plan, _, note) =
            container_image(Some(("/p/.devcontainer.json", "{ oops")), "/p", "node:22");
        assert_eq!(plan, ImagePlan::Use("node:22".into()));
        assert!(note.is_some());
    }
}
