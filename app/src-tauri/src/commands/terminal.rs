//! Terminal tool window: shell sessions on a PTY, on the project's host (a remote project's
//! terminal runs on the remote machine), and SSH sessions to other hosts (Tools | Start SSH
//! Session…). Output and exit arrive on `terminal://<id>` as
//! `{ type: "output", data } | { type: "exited", code }`.
//!
//! Each session keeps its recent output ([`Scrollback`]) and size: Code With Me shares a tab
//! with guests from what it showed so far, and the host's window shows a terminal a guest
//! opened on this machine by attaching to it (`terminal_attach`).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use forge_backend::{Backend, LocalBackend, Terminal, TerminalSpec};
use forge_collab::share::Scrollback;
use forge_core::lifecycle::{Objection, ProjectLifecycle, ProjectRef};
use forge_core::{ForgeError, Result};
use forge_process::terminal_layout::{self, TerminalLayout};
use forge_process::{PtyEvent, PtySink};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// What a session showed recently and its size.
#[derive(Default)]
struct Screen {
    scrollback: Scrollback,
    cols: u16,
    rows: u16,
}

#[derive(Default)]
pub struct TerminalState {
    sessions: Mutex<HashMap<String, Arc<dyn Terminal>>>,
    screens: Mutex<HashMap<String, Arc<Mutex<Screen>>>>,
    /// Sessions ended with their project: their exit is not reported (the UI keeps the tabs
    /// for the layout it saves).
    quiet: Mutex<HashSet<String>>,
    next_id: AtomicU64,
}

impl TerminalState {
    fn get(&self, id: &str) -> Result<Arc<dyn Terminal>> {
        self.sessions
            .lock()
            .expect("terminal sessions poisoned")
            .get(id)
            .cloned()
            .ok_or_else(|| ForgeError::Other(format!("no terminal session {id}")))
    }

    /// A session started elsewhere (a Code With Me guest's view of a shared terminal).
    pub(crate) fn insert(&self, id: String, terminal: Arc<dyn Terminal>) {
        self.sessions
            .lock()
            .expect("terminal sessions poisoned")
            .insert(id, terminal);
    }

    fn screen(&self, id: &str, cols: u16, rows: u16) -> Arc<Mutex<Screen>> {
        let screen = Arc::new(Mutex::new(Screen {
            scrollback: Scrollback::new(),
            cols,
            rows,
        }));
        self.screens
            .lock()
            .expect("terminal screens poisoned")
            .insert(id.to_string(), screen.clone());
        screen
    }

    /// The recent output of a session and its size.
    pub(crate) fn snapshot(&self, id: &str) -> Option<(String, u16, u16)> {
        let screen = self
            .screens
            .lock()
            .expect("terminal screens poisoned")
            .get(id)
            .cloned()?;
        let s = screen.lock().expect("terminal screen poisoned");
        Some((s.scrollback.text().to_string(), s.cols, s.rows))
    }

    fn resized(&self, id: &str, cols: u16, rows: u16) {
        if let Some(screen) = self
            .screens
            .lock()
            .expect("terminal screens poisoned")
            .get(id)
        {
            let mut s = screen.lock().expect("terminal screen poisoned");
            s.cols = cols;
            s.rows = rows;
        }
    }

    /// `(id, program)` of every session running a program in the foreground, by id.
    pub(crate) fn running(&self) -> Vec<(String, String)> {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .expect("terminal sessions poisoned")
            .iter()
            .map(|(id, s)| (id.clone(), s.clone()))
            .collect();
        let mut running: Vec<_> = sessions
            .into_iter()
            .filter_map(|(id, s)| s.foreground_process().map(|p| (id, p)))
            .collect();
        running.sort();
        running
    }

    /// The recent output of every session, by id.
    fn scrollbacks(&self) -> Vec<(String, String)> {
        let screens = self.screens.lock().expect("terminal screens poisoned");
        let mut all: Vec<_> = screens
            .iter()
            .map(|(id, screen)| {
                let text = screen
                    .lock()
                    .expect("terminal screen poisoned")
                    .scrollback
                    .text()
                    .to_string();
                (id.clone(), text)
            })
            .collect();
        all.sort();
        all
    }

    /// End every session without reporting their exit (their project closes).
    fn end_all(&self) {
        let ids: Vec<String> = self
            .sessions
            .lock()
            .expect("terminal sessions poisoned")
            .keys()
            .cloned()
            .collect();
        for id in ids {
            self.quiet
                .lock()
                .expect("terminal state poisoned")
                .insert(id.clone());
            if let Some(s) = self.remove(&id) {
                if let Err(e) = s.kill() {
                    tracing::warn!("cannot end terminal {id}: {e}");
                }
            }
        }
    }

    /// Forget a session; returns it so it can be killed.
    pub(crate) fn remove(&self, id: &str) -> Option<Arc<dyn Terminal>> {
        self.screens
            .lock()
            .expect("terminal screens poisoned")
            .remove(id);
        self.sessions
            .lock()
            .expect("terminal sessions poisoned")
            .remove(id)
    }
}

/// Emit a session's output and exit on `terminal://<id>`, keep its recent output, and pass
/// both on to the Code With Me session (which shares the terminal only if it is shared).
fn pty_sink(app: &AppHandle, id: &str, cols: u16, rows: u16) -> PtySink {
    let screen = app.state::<TerminalState>().screen(id, cols, rows);
    let event = format!("terminal://{id}");
    let (app, id) = (app.clone(), id.to_string());
    Arc::new(move |e: PtyEvent| {
        match &e {
            PtyEvent::Output { data } => {
                screen
                    .lock()
                    .expect("terminal screen poisoned")
                    .scrollback
                    .push(data);
                crate::commands::collab::on_terminal_output(&app, &id, data);
            }
            PtyEvent::Exited { code } => {
                crate::commands::collab::on_terminal_exited(&app, &id, *code);
                let ended_with_project = app
                    .state::<TerminalState>()
                    .quiet
                    .lock()
                    .expect("terminal state poisoned")
                    .remove(&id);
                if ended_with_project {
                    return;
                }
            }
        }
        if let Err(err) = app.emit(&event, &e) {
            tracing::warn!("cannot emit {event}: {err}");
        }
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpenOptions {
    /// Id chosen by the UI so it can subscribe to `terminal://<id>` before the shell starts;
    /// generated when missing.
    pub id: Option<String>,
    /// Start directory, relative to the project root or absolute.
    pub cwd: Option<String>,
    pub shell: Option<String>,
    /// Arguments for `shell` (e.g. `docker exec -it <id> sh` from the Services tool window).
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Start on this machine even when the project is remote (Services: `docker` or
    /// `kubectl` of this machine).
    #[serde(default)]
    pub local: bool,
    /// An SSH session to another host instead of a shell (Tools | Start SSH Session…).
    #[serde(default)]
    pub ssh: Option<SshTarget>,
    pub cols: u16,
    pub rows: u16,
}

/// What an SSH terminal session connects to: a saved SSH configuration, or the SSH
/// configuration of an SFTP deployment server.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTarget {
    pub config_id: Option<String>,
    pub server_id: Option<String>,
    /// A password or passphrase typed for this session (configurations that do not save it).
    pub secret: Option<String>,
    /// The user confirmed a session to a server of an untrusted project (Safe Mode).
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpened {
    pub id: String,
    pub shell: String,
    pub cwd: String,
}

/// Start a shell. Without `cwd` it starts in the setting `terminal.startDirectory` or the
/// project root; without `shell` in `terminal.shellPath` or the system default shell.
#[tauri::command]
pub async fn terminal_open(options: TerminalOpenOptions, app: AppHandle) -> Result<TerminalOpened> {
    open(options, app).await
}

/// [`terminal_open`] for callers inside the app (a Code With Me guest's terminal on this
/// machine).
pub(crate) async fn open(options: TerminalOpenOptions, app: AppHandle) -> Result<TerminalOpened> {
    let state = app.state::<AppState>();
    let terminals = app.state::<TerminalState>();
    // Without a project (or for a program of this machine in a remote project) the shell
    // starts in the home directory of this machine.
    let backend: Arc<dyn Backend> = match state.backend() {
        Ok(b) if !(options.local && b.info().remote.is_some()) => b,
        _ => Arc::new(LocalBackend::open(
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
        )?),
    };
    let root = backend.info().root.clone();
    let remote = !backend.info().is_local();
    let (setting_dir, setting_shell, setting_env, integration) = {
        let s = state.read_settings();
        // In Safe Mode the project's own settings cannot pick the shell or its environment
        // (`BASH_ENV`, `LD_PRELOAD`, ...): the terminal is the user's, not the project's.
        let safe = state.safe_mode.is_on();
        let shell = if safe {
            s.get_without_project_as::<String>("terminal.shellPath")
        } else {
            s.get_as::<String>("terminal.shellPath")
        };
        let env = if safe {
            s.get_without_project_as::<Vec<String>>("terminal.environment")
        } else {
            s.get_as::<Vec<String>>("terminal.environment")
        };
        (
            s.get_as::<String>("terminal.startDirectory")
                .unwrap_or_default(),
            shell.unwrap_or_default(),
            env.unwrap_or_default(),
            s.get_bool("terminal.shellIntegration").unwrap_or(true),
        )
    };
    let dir = options
        .cwd
        .filter(|d| !d.trim().is_empty())
        .unwrap_or(setting_dir);
    let cwd = match dir.trim() {
        "" => root.clone(),
        // A local start directory setting means nothing on a remote host.
        d if remote && PathBuf::from(d).is_absolute() => root.clone(),
        d if PathBuf::from(d).is_absolute() => PathBuf::from(d),
        d => root.join(d),
    };
    let mut env: Vec<(String, String)> = setting_env
        .iter()
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .collect();
    env.extend(options.env);

    let id = options.id.clone().unwrap_or_else(|| {
        let n = terminals.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        format!("term-{n}")
    });
    let sink = pty_sink(&app, &id, options.cols, options.rows);
    if let Some(target) = options.ssh {
        let session = open_ssh(&state, target, options.cols, options.rows, sink).await?;
        let opened = TerminalOpened {
            id: id.clone(),
            shell: session.shell(),
            cwd: session.cwd().to_string_lossy().into_owned(),
        };
        terminals
            .sessions
            .lock()
            .expect("terminal sessions poisoned")
            .insert(id, session);
        return Ok(opened);
    }
    let shell = options
        .shell
        .filter(|s| !s.trim().is_empty())
        // The local shell setting does not apply on a remote host.
        .or(Some(setting_shell).filter(|s| !remote && !s.trim().is_empty()));
    let spec = TerminalSpec {
        shell,
        args: options.args,
        cwd,
        env,
        cols: options.cols,
        rows: options.rows,
        shell_integration: integration,
    };
    let session = blocking(move || backend.open_terminal(spec, sink)).await?;
    let opened = TerminalOpened {
        id: id.clone(),
        shell: session.shell(),
        cwd: session.cwd().to_string_lossy().into_owned(),
    };
    terminals
        .sessions
        .lock()
        .expect("terminal sessions poisoned")
        .insert(id, session);
    Ok(opened)
}

/// An SSH terminal session (a pseudo terminal on an SSH channel, `forge_remote::shell`).
///
/// Safe Mode: the user's own SSH configurations connect right away. A deployment server is
/// defined in the project's files, so in an untrusted project its session needs the user's
/// confirmation (`confirmed`, asked by the UI) - the project must not pick a host the user
/// is logged in to with their credentials (`forge_trust::ssh_session_needs_confirmation`).
async fn open_ssh(
    state: &AppState,
    target: SshTarget,
    cols: u16,
    rows: u16,
    sink: PtySink,
) -> Result<Arc<dyn Terminal>> {
    let config_id = match (&target.server_id, &target.config_id) {
        (Some(server_id), _) => {
            let server = crate::commands::deployment::load(state)?
                .get(server_id)
                .cloned()
                .ok_or_else(|| {
                    ForgeError::Other(format!("The deployment server {server_id} does not exist"))
                })?;
            let from = forge_trust::SshTarget::ProjectServer;
            if forge_trust::ssh_session_needs_confirmation(state.safe_mode.is_on(), from)
                && !target.confirmed
            {
                return Err(ForgeError::Other(forge_trust::ssh_confirmation_required(
                    &server.label(),
                )));
            }
            if server.kind != forge_deploy::ServerKind::Sftp || server.ssh_config.is_empty() {
                return Err(ForgeError::Other(format!(
                    "{} is not an SFTP server: SSH sessions need an SSH configuration",
                    server.label()
                )));
            }
            server.ssh_config
        }
        (None, Some(id)) => id.clone(),
        (None, None) => {
            return Err(ForgeError::Other(
                "Choose an SSH configuration or a deployment server".into(),
            ))
        }
    };
    let secret = target.secret;
    blocking(move || {
        let params = forge_deploy::connect::ssh_params(&config_id, secret.as_deref())
            .map_err(crate::commands::deployment::err)?;
        let label = format!("{}@{}", params.user, params.host);
        let shell = forge_remote::shell::SshShell::open(&params, cols, rows, sink)
            .map_err(|e| forge_deploy::sftp::connect_error(e, &label))
            .map_err(crate::commands::deployment::err)?;
        Ok(Arc::new(shell) as Arc<dyn Terminal>)
    })
    .await
}

/// Send input (keys, paste) to a terminal.
#[tauri::command]
pub fn terminal_write(id: String, data: String, terminals: State<'_, TerminalState>) -> Result<()> {
    terminals.get(&id)?.write(data.as_bytes())
}

#[tauri::command]
pub async fn terminal_resize(
    id: String,
    cols: u16,
    rows: u16,
    app: AppHandle,
    terminals: State<'_, TerminalState>,
) -> Result<()> {
    let term = terminals.get(&id)?;
    terminals.resized(&id, cols, rows);
    crate::commands::collab::on_terminal_resized(&app, &id, cols, rows);
    blocking(move || term.resize(cols, rows)).await
}

/// Kill the shell and forget the session (closing its tab).
#[tauri::command]
pub fn terminal_close(
    id: String,
    app: AppHandle,
    terminals: State<'_, TerminalState>,
) -> Result<()> {
    crate::commands::collab::on_terminal_closed(&app, &id);
    if let Some(s) = terminals.remove(&id) {
        s.kill()?;
    }
    Ok(())
}

/// A session that is already running (a Code With Me guest opened it on this machine): what
/// it showed so far; its output follows on `terminal://<id>`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttached {
    pub scrollback: String,
    pub cols: u16,
    pub rows: u16,
}

#[tauri::command]
pub fn terminal_attach(
    id: String,
    terminals: State<'_, TerminalState>,
) -> Result<TerminalAttached> {
    terminals.get(&id)?;
    let (scrollback, cols, rows) = terminals.snapshot(&id).unwrap_or_default();
    Ok(TerminalAttached {
        scrollback,
        cols,
        rows,
    })
}

/// The setting that keeps each tab's output for the next time the project opens.
const RESTORE_SCROLLBACK_KEY: &str = "terminal.restoreScrollback";

/// Terminals belong to their project: `beforeClose` objects while commands run, `closing`
/// keeps the output (setting `terminal.restoreScrollback`) and ends the sessions. The tabs'
/// layout is the UI's (`terminal_layout_save`).
pub struct TerminalLifecycle(pub AppHandle);

impl ProjectLifecycle for TerminalLifecycle {
    fn name(&self) -> &str {
        "terminals"
    }

    fn before_close(&self, _project: &ProjectRef) -> Option<Objection> {
        let running = self.0.state::<TerminalState>().running();
        if running.is_empty() {
            return None;
        }
        let (targets, items): (Vec<_>, Vec<_>) = running.into_iter().unzip();
        Some(Objection {
            source: self.name().to_string(),
            kind: "terminal.running".into(),
            message: forge_i18n::t_args(
                "errors.terminal.running",
                &forge_i18n::args! { "count" => items.len() as i64 },
            ),
            items,
            targets,
        })
    }

    fn closing(&self, project: &ProjectRef) -> Result<()> {
        let terminals = self.0.state::<TerminalState>();
        let keep = self
            .0
            .state::<AppState>()
            .read_settings()
            .get_bool(RESTORE_SCROLLBACK_KEY)
            .unwrap_or(true);
        if let Some(dir) = terminal_layout::default_scrollback_dir() {
            let saved = if keep {
                terminal_layout::save_scrollback(&dir, &project.root, &terminals.scrollbacks())
            } else {
                terminal_layout::clear_scrollback(&dir, &project.root)
            };
            if let Err(e) = saved {
                tracing::warn!("terminal history not saved: {e}");
            }
        }
        terminals.end_all();
        Ok(())
    }
}

/// A terminal running a program in the foreground (`sleep 100`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningCommand {
    pub id: String,
    pub process: String,
}

/// The sessions that run a program instead of waiting at the prompt.
#[tauri::command]
pub async fn terminal_running(app: AppHandle) -> Vec<RunningCommand> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<TerminalState>()
            .running()
            .into_iter()
            .map(|(id, process)| RunningCommand { id, process })
            .collect()
    })
    .await
    .unwrap_or_default()
}

fn layout_dir() -> Result<PathBuf> {
    terminal_layout::default_layout_dir()
        .ok_or_else(|| ForgeError::Other("no state directory".into()))
}

/// The terminal layout of the project at `root` (the root as the UI names it), with the
/// saved output of each tab when `terminal.restoreScrollback` is on.
#[tauri::command]
pub async fn terminal_layout_load(
    root: String,
    state: State<'_, AppState>,
) -> Result<Option<TerminalLayout>> {
    let keep = state
        .read_settings()
        .get_bool(RESTORE_SCROLLBACK_KEY)
        .unwrap_or(true);
    blocking(move || {
        let scrollback = keep.then(terminal_layout::default_scrollback_dir).flatten();
        Ok(terminal_layout::load(
            &layout_dir()?,
            scrollback.as_deref(),
            &root,
        ))
    })
    .await
}

/// Store the terminal layout of the project at `root`. Takes the root explicitly: the UI
/// saves the closing project's layout while the next one may already be open in Rust.
#[tauri::command]
pub async fn terminal_layout_save(root: String, layout: TerminalLayout) -> Result<()> {
    blocking(move || terminal_layout::save(&layout_dir()?, &root, &layout)).await
}
