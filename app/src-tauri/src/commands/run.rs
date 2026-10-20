//! Run/Debug configurations and the processes of the Run tool window.
//!
//! Configurations are stored in `<project>/.forge/runConfigurations.toml` (shared) and
//! `.forge/runConfigurations.local.toml`. A started run streams `RunEvent`s on
//! `run://<runId>`: `started`, `output` (stdout/stderr) and `exited`.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_core::{ForgeError, Result};
use forge_process::{
    BuildContext, CommandLine, KindRegistry, RunConfiguration, RunConfigurations, RunEvent,
    RunSink, RunningProcess, TemplateKind,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

pub struct RunState {
    runs: Mutex<HashMap<String, Arc<RunningProcess>>>,
    next_id: AtomicU64,
    /// Configuration kinds known to the backend; plugins register more at startup.
    pub kinds: KindRegistry,
}

impl RunState {
    /// Built-in kinds plus the manifest-declared ones (types with a `command` template).
    pub fn new(app: &AppState) -> Self {
        let mut kinds = KindRegistry::builtin();
        let plugins = app.plugins.read().expect("plugins lock poisoned");
        for t in plugins.run_configuration_types() {
            if t.command.is_empty() {
                continue;
            }
            kinds.register(Box::new(TemplateKind {
                id: t.id.clone(),
                command: t.command.clone(),
                default_interpreter: t.default_interpreter.clone(),
                required: t
                    .fields
                    .iter()
                    .filter(|f| f.required)
                    .map(|f| (f.key.clone(), f.label.clone()))
                    .collect(),
            }));
        }
        Self {
            runs: Mutex::default(),
            next_id: AtomicU64::new(0),
            kinds,
        }
    }

    fn get(&self, id: &str) -> Result<Arc<RunningProcess>> {
        self.runs
            .lock()
            .expect("runs poisoned")
            .get(id)
            .cloned()
            .ok_or_else(|| ForgeError::Other(format!("no run {id}")))
    }
}

fn root(state: &AppState) -> Result<std::path::PathBuf> {
    state.with_workspace(|ws| Ok(ws.root().to_path_buf()))
}

#[tauri::command]
pub fn run_configurations_load(state: State<'_, AppState>) -> Result<RunConfigurations> {
    RunConfigurations::load(&root(&state)?)
}

#[tauri::command]
pub fn run_configurations_save(
    configurations: RunConfigurations,
    state: State<'_, AppState>,
) -> Result<()> {
    configurations.save(&root(&state)?)
}

/// What to start: a configuration the backend builds, or a command line built by a
/// UI-side configuration type.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    /// Id chosen by the UI so it can subscribe to `run://<runId>` before the start;
    /// generated when missing.
    pub run_id: Option<String>,
    pub configuration: Option<RunConfiguration>,
    pub command: Option<CommandLine>,
    /// Extra environment, e.g. `XDEBUG_SESSION` from a debug launcher.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Extra interpreter arguments placed right after the program (e.g. `-dxdebug.mode=debug`).
    #[serde(default)]
    pub interpreter_args: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStarted {
    pub run_id: String,
    pub command_line: String,
}

fn build(state: &AppState, runs: &RunState, req: RunRequest) -> Result<CommandLine> {
    let root = root(state)?;
    let mut cmd = match (req.configuration, req.command) {
        (_, Some(mut cmd)) => {
            if cmd.cwd.as_os_str().is_empty() {
                cmd.cwd = root;
            } else if cmd.cwd.is_relative() {
                cmd.cwd = root.join(&cmd.cwd);
            }
            cmd
        }
        (Some(mut cfg), None) => {
            let settings = state.read_settings();
            if cfg.type_id == "composer" && cfg.option("composerPath").trim().is_empty() {
                if let Some(composer) = settings
                    .get_as::<String>("php.composer.executable")
                    .filter(|c| !c.trim().is_empty() && c != "composer")
                {
                    cfg.options.insert("composerPath".into(), composer);
                }
            }
            let ctx = BuildContext {
                php_interpreter: settings
                    .get_as::<String>("php.interpreter")
                    .unwrap_or_else(|| "php".into()),
                root,
            };
            drop(settings);
            runs.kinds.build(&cfg, &ctx)?
        }
        (None, None) => return Err(ForgeError::Other("nothing to run".into())),
    };
    cmd.env.extend(req.env);
    if !req.interpreter_args.is_empty() {
        cmd.args.splice(0..0, req.interpreter_args);
    }
    Ok(cmd)
}

/// The command line a request would start, for previews in the configuration dialog.
#[tauri::command]
pub fn run_command_line(
    request: RunRequest,
    state: State<'_, AppState>,
    runs: State<'_, RunState>,
) -> Result<CommandLine> {
    build(&state, &runs, request)
}

#[tauri::command]
pub fn run_start(
    request: RunRequest,
    app: AppHandle,
    state: State<'_, AppState>,
    runs: State<'_, RunState>,
) -> Result<RunStarted> {
    let run_id = request.run_id.clone().unwrap_or_else(|| {
        let n = runs.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        format!("run-{n}")
    });
    let cmd = build(&state, &runs, request)?;
    let event = format!("run://{run_id}");
    let sink: RunSink = Arc::new(move |e: RunEvent| {
        if let Err(err) = app.emit(&event, &e) {
            tracing::warn!("cannot emit {event}: {err}");
        }
    });
    let process = RunningProcess::start(&cmd, sink)?;
    runs.runs
        .lock()
        .expect("runs poisoned")
        .insert(run_id.clone(), Arc::new(process));
    Ok(RunStarted {
        run_id,
        command_line: cmd.display(),
    })
}

/// Stop a run and its child processes (SIGTERM, then SIGKILL after two seconds).
#[tauri::command]
pub fn run_stop(run_id: String, runs: State<'_, RunState>) -> Result<()> {
    runs.get(&run_id)?.kill(Duration::from_secs(2));
    Ok(())
}

/// Send console input to a run's stdin.
#[tauri::command]
pub fn run_write(run_id: String, data: String, runs: State<'_, RunState>) -> Result<()> {
    runs.get(&run_id)?.write_stdin(data.as_bytes())
}

/// Forget a finished run (its tab was closed); a running one is stopped first.
#[tauri::command]
pub fn run_dispose(run_id: String, runs: State<'_, RunState>) -> Result<()> {
    let removed = runs.runs.lock().expect("runs poisoned").remove(&run_id);
    if let Some(p) = removed {
        p.kill(Duration::from_secs(2));
    }
    Ok(())
}
