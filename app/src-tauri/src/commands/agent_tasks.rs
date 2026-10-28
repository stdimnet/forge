//! Agent Tasks (`forge-agent-tasks`): AI agents that work on a task in a sandbox (a git
//! worktree in Forge's cache folder), run the tests and fix failures, and leave a diff to
//! review. Apply patches the working tree (all or nothing), Apply as Commit / Branch commit
//! the changes, Discard removes the sandbox.
//!
//! Refused in Safe Mode (agents run project code), when AI is off for the project
//! (`ai.enabled`) and for remote projects. Every change of a task is emitted as
//! `agent-tasks://changed` with the whole task.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use forge_agent_cli::{
    agent_by_id, AgentEvent, AgentSession, EventSink, PermissionDecision, PermissionMode,
    StartOptions,
};
use forge_agent_tasks::cli::CliVerdict;
use forge_agent_tasks::commands::{run_process, split_command};
use forge_agent_tasks::sandbox::{self, Sandbox};
use forge_agent_tasks::{
    permission, run_builtin, run_cli, AgentBackend, AgentTask, ApplyResult, Budget, CliTurns,
    FileChange, Host, Outcome, RunOutput, ShellCommand, StepKind, TaskContext, TaskRequest,
    TaskStatus, TaskStore, TestChoice, TestSpec, Workspace,
};
use forge_ai::{AiFeature, CancellationToken};
use forge_ai_editor::policy::IGNORE_FILES;
use forge_ai_editor::AiIgnore;
use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};
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
    /// The project root the loaded tasks belong to.
    project: Option<PathBuf>,
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

/// `<cache>/forge/agent-tasks/<project key>`: tasks and sandboxes, never inside the project.
fn store_for(root: &Path) -> Result<TaskStore> {
    let cache = dirs::cache_dir().ok_or_else(|| err("no cache folder"))?;
    let key = forge_core::project_state::project_key(&root.to_string_lossy());
    Ok(TaskStore::new(
        cache.join("forge").join("agent-tasks").join(key),
    ))
}

impl AgentTasksState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Load the project's tasks when the project changed.
    fn ensure(&self, root: &Path) -> Result<()> {
        let mut inner = self.lock();
        if inner.project.as_deref() != Some(root) {
            inner.tasks = store_for(root)?.load();
            inner.project = Some(root.to_path_buf());
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
    if state.is_remote() {
        return Err(err(
            "Agent tasks run on this machine and are not available for remote projects yet.",
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
    let root = state.project_root()?;
    tasks.ensure(&root)?;
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
    {
        let r = root.clone();
        blocking(move || sandbox::repository_of(&r).map(|_| ()).map_err(err)).await?;
    }
    tasks.ensure(&root)?;
    let now = now_ms();
    let id = format!("task-{now:x}");
    let task = AgentTask::new(&id, request, now);
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
    let project = project.trim_end_matches(['/', '\\']);
    if project.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(project) {
        let after = &rest[at + project.len()..];
        let whole = after
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.')));
        out.push_str(&rest[..at]);
        out.push_str(if whole { sandbox } else { project });
        rest = after;
    }
    out.push_str(rest);
    out
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

async fn prepare_and_run(
    app: &AppHandle,
    tasks: &AgentTasksState,
    id: &str,
    cancel: &CancellationToken,
) -> std::result::Result<Outcome, String> {
    let state = app.state::<AppState>();
    let root = require_allowed(&state).map_err(|e| e.to_string())?;
    let task = tasks.task(id).map_err(|e| e.to_string())?;
    let dir = store_for(&root).map_err(|e| e.to_string())?.sandbox_dir(id);

    tasks.log(
        app,
        id,
        StepKind::Info {
            text: "Creating the sandbox (a git worktree in Forge's cache folder)".into(),
        },
    );
    let sb = {
        let (r, d) = (root.clone(), dir.clone());
        tauri::async_runtime::spawn_blocking(move || sandbox::create(&r, &d))
            .await
            .map_err(|e| e.to_string())??
    };
    tasks.update(app, id, |t| {
        t.sandbox = Some(sb.dir.clone());
        t.base_commit = Some(sb.base.clone());
    });
    tasks.log(
        app,
        id,
        StepKind::Info {
            text: format!("Sandbox ready at {}", sb.dir.display()),
        },
    );

    let ignore = AiIgnore::merged(
        IGNORE_FILES
            .iter()
            .filter_map(|f| std::fs::read_to_string(sb.project_dir.join(f)).ok())
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str),
    );
    let tests = test_spec(app, &task.request.tests, &root, &sb.project_dir)?;
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
        workspace: Workspace::new(&sb.project_dir, ignore, task.request.scope.clone()),
        allowlist: setting(&state, ALLOWED_COMMANDS, Vec::<String>::new()),
        tests,
        budget,
    };
    let host = AppHost {
        app: app.clone(),
        tasks: tasks.clone(),
        id: id.to_string(),
        cancel: cancel.clone(),
    };
    match &task.request.backend {
        AgentBackend::Builtin { provider, model } => {
            let ai = app.state::<AiState>();
            let (cfg, model) = ai
                .service
                .selection(provider.as_deref(), model.as_deref(), Some(AiFeature::Chat))
                .map_err(|e| e.to_string())?;
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
        AgentBackend::Cli { agent_id, model } => {
            run_with_cli(
                app,
                &state,
                agent_id,
                model.clone(),
                &sb,
                &ctx,
                &host,
                cancel,
            )
            .await
        }
    }
}

/// Processes, approvals and the log of a running task.
struct AppHost {
    app: AppHandle,
    tasks: AgentTasksState,
    id: String,
    cancel: CancellationToken,
}

#[async_trait]
impl Host for AppHost {
    async fn run(&self, cmd: &ShellCommand, timeout: Duration) -> RunOutput {
        run_process(cmd, timeout, &self.cancel).await
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
    sb: &Sandbox,
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
                let verdict = permission(&tool_name, &input, &ws, &allowlist);
                let Some(session) = cell.get().cloned() else {
                    return;
                };
                let host = AppHost {
                    app: app.clone(),
                    tasks: tasks.clone(),
                    id: id.clone(),
                    cancel: cancel.clone(),
                };
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
        AgentSession::start(adapter, sb.dir.clone(), opts, sink)
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

fn sandbox_of(task: &AgentTask, root: &Path) -> Result<Sandbox> {
    let (Some(dir), Some(base)) = (&task.sandbox, &task.base_commit) else {
        return Err(err("This task has no sandbox"));
    };
    if !dir.exists() {
        return Err(err("The sandbox of this task is gone"));
    }
    let prefix = sandbox::repository_of(root)
        .map(|(_, p)| p)
        .unwrap_or_default();
    Ok(Sandbox {
        dir: dir.clone(),
        project_dir: if prefix.is_empty() {
            dir.clone()
        } else {
            dir.join(prefix)
        },
        base: base.clone(),
    })
}

/// The files the agent changed.
#[tauri::command]
pub async fn agent_task_changes(
    id: String,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<Vec<FileChange>> {
    let root = state.project_root()?;
    let sb = sandbox_of(&tasks.task(&id)?, &root)?;
    blocking(move || sandbox::changes(&sb).map_err(err)).await
}

/// Both sides of a changed file for the diff viewer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub before: Option<String>,
    pub after: Option<String>,
}

#[tauri::command]
pub async fn agent_task_file_diff(
    id: String,
    path: String,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<FileDiff> {
    let root = state.project_root()?;
    let sb = sandbox_of(&tasks.task(&id)?, &root)?;
    blocking(move || {
        let (before, after) = sandbox::file_versions(&sb, &path).map_err(err)?;
        Ok(FileDiff { before, after })
    })
    .await
}

/// How to apply a task.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum ApplyMode {
    /// Patch the working tree.
    Apply,
    /// Patch the working tree and commit the changed files.
    Commit { message: String },
    /// Commit in the sandbox and create a branch; the working tree is not touched.
    Branch { branch: String, message: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyAnswer {
    #[serde(flatten)]
    pub result: ApplyResult,
    /// The commit or branch created.
    pub created: Option<String>,
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
    let root = state.project_root()?;
    let task = tasks.task(&id)?;
    if !task.status.reviewable() {
        return Err(err("This task has nothing to apply"));
    }
    let sb = sandbox_of(&task, &root)?;
    let paths = paths.unwrap_or_default();
    let partial = !paths.is_empty();
    let answer = {
        let (root, sb, mode) = (root.clone(), sb.clone(), mode.clone());
        blocking(move || {
            let r = match mode {
                ApplyMode::Apply => (sandbox::apply(&root, &sb, &paths).map_err(err)?, None),
                ApplyMode::Commit { message } => {
                    let (r, hash) =
                        sandbox::apply_as_commit(&root, &sb, &paths, &message).map_err(err)?;
                    (r, hash)
                }
                ApplyMode::Branch { branch, message } => {
                    let hash = sandbox::apply_as_branch(&sb, &branch, &message).map_err(err)?;
                    let files = sandbox::changes(&sb)
                        .map(|c| c.into_iter().map(|f| f.path).collect())
                        .unwrap_or_default();
                    (
                        ApplyResult {
                            applied: true,
                            files,
                            conflicts: Vec::new(),
                            message: None,
                        },
                        Some(format!("{branch} ({hash})")),
                    )
                }
            };
            Ok(ApplyAnswer {
                result: r.0,
                created: r.1,
            })
        })
        .await?
    };
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
        let dir = sb.dir.clone();
        let r = root.clone();
        let _ = blocking(move || sandbox::remove(&r, &dir).map_err(err)).await;
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

/// Remove the sandbox without applying anything.
#[tauri::command]
pub async fn agent_task_discard(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    tasks: State<'_, AgentTasksState>,
) -> Result<()> {
    let root = state.project_root()?;
    let task = tasks.task(&id)?;
    if task.status.is_active() {
        return Err(err("Stop the task first"));
    }
    if let Some(dir) = task.sandbox.clone() {
        blocking(move || sandbox::remove(&root, &dir).map_err(err)).await?;
    }
    tasks.update(&app, &id, |t| {
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
    let root = state.project_root()?;
    let list = {
        let mut inner = tasks.lock();
        inner.tasks.retain(|t| t.id != id);
        inner.tasks.clone()
    };
    store_for(&root)?.save(&list).map_err(err)?;
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
}
