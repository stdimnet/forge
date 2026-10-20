//! Terminal tool window: shell sessions on a PTY. Output and exit arrive on
//! `terminal://<id>` as `{ type: "output", data } | { type: "exited", code }`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use forge_process::{PtyEvent, PtyOptions, PtySession, PtySink};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

#[derive(Default)]
pub struct TerminalState {
    sessions: Mutex<HashMap<String, Arc<PtySession>>>,
    next_id: AtomicU64,
}

impl TerminalState {
    fn get(&self, id: &str) -> Result<Arc<PtySession>> {
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
    /// Start directory, relative to the project root or absolute.
    pub cwd: Option<String>,
    pub shell: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
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
pub fn terminal_open(
    options: TerminalOpenOptions,
    app: AppHandle,
    state: State<'_, AppState>,
    terminals: State<'_, TerminalState>,
) -> Result<TerminalOpened> {
    let root = state
        .with_workspace(|ws| Ok(ws.root().to_path_buf()))
        .ok()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let (setting_dir, setting_shell, setting_env) = {
        let s = state.read_settings();
        (
            s.get_as::<String>("terminal.startDirectory")
                .unwrap_or_default(),
            s.get_as::<String>("terminal.shellPath").unwrap_or_default(),
            s.get_as::<Vec<String>>("terminal.environment")
                .unwrap_or_default(),
        )
    };
    let dir = options
        .cwd
        .filter(|d| !d.trim().is_empty())
        .unwrap_or(setting_dir);
    let cwd = match dir.trim() {
        "" => root.clone(),
        d if PathBuf::from(d).is_absolute() => PathBuf::from(d),
        d => root.join(d),
    };
    let cwd = if cwd.is_dir() { cwd } else { root };
    let mut env: Vec<(String, String)> = setting_env
        .iter()
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .collect();
    env.extend(options.env);

    let n = terminals.next_id.fetch_add(1, Ordering::SeqCst) + 1;
    let id = format!("term-{n}");
    let event = format!("terminal://{id}");
    let sink: PtySink = Arc::new(move |e: PtyEvent| {
        if let Err(err) = app.emit(&event, &e) {
            tracing::warn!("cannot emit {event}: {err}");
        }
    });
    let session = PtySession::spawn(
        PtyOptions {
            shell: options
                .shell
                .filter(|s| !s.trim().is_empty())
                .or(Some(setting_shell).filter(|s| !s.trim().is_empty())),
            args: None,
            cwd: cwd.clone(),
            env,
            cols: options.cols,
            rows: options.rows,
        },
        sink,
    )?;
    let opened = TerminalOpened {
        id: id.clone(),
        shell: session.shell().to_string(),
        cwd: cwd.to_string_lossy().into_owned(),
    };
    terminals
        .sessions
        .lock()
        .expect("terminal sessions poisoned")
        .insert(id, Arc::new(session));
    Ok(opened)
}

/// Send input (keys, paste) to a terminal.
#[tauri::command]
pub fn terminal_write(id: String, data: String, terminals: State<'_, TerminalState>) -> Result<()> {
    terminals.get(&id)?.write(data.as_bytes())
}

#[tauri::command]
pub fn terminal_resize(
    id: String,
    cols: u16,
    rows: u16,
    terminals: State<'_, TerminalState>,
) -> Result<()> {
    terminals.get(&id)?.resize(cols, rows)
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
