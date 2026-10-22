//! Terminal tool window: shell sessions on a PTY, on the project's host (a remote project's
//! terminal runs on the remote machine). Output and exit arrive on `terminal://<id>` as
//! `{ type: "output", data } | { type: "exited", code }`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use forge_backend::{Backend, LocalBackend, Terminal, TerminalSpec};
use forge_core::{ForgeError, Result};
use forge_process::{PtyEvent, PtySink};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::workspace::blocking;
use crate::state::AppState;

#[derive(Default)]
pub struct TerminalState {
    sessions: Mutex<HashMap<String, Arc<dyn Terminal>>>,
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
    pub cols: u16,
    pub rows: u16,
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
pub async fn terminal_open(
    options: TerminalOpenOptions,
    app: AppHandle,
    state: State<'_, AppState>,
    terminals: State<'_, TerminalState>,
) -> Result<TerminalOpened> {
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
        (
            s.get_as::<String>("terminal.startDirectory")
                .unwrap_or_default(),
            s.get_as::<String>("terminal.shellPath").unwrap_or_default(),
            s.get_as::<Vec<String>>("terminal.environment")
                .unwrap_or_default(),
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
    let event = format!("terminal://{id}");
    let sink: PtySink = Arc::new(move |e: PtyEvent| {
        if let Err(err) = app.emit(&event, &e) {
            tracing::warn!("cannot emit {event}: {err}");
        }
    });
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
    terminals: State<'_, TerminalState>,
) -> Result<()> {
    let term = terminals.get(&id)?;
    blocking(move || term.resize(cols, rows)).await
}

/// Kill the shell and forget the session (closing its tab).
#[tauri::command]
pub fn terminal_close(id: String, terminals: State<'_, TerminalState>) -> Result<()> {
    let session = terminals
        .sessions
        .lock()
        .expect("terminal sessions poisoned")
        .remove(&id);
    if let Some(s) = session {
        s.kill()?;
    }
    Ok(())
}
