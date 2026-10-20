//! Settings | Tools | Browser Companion: server status, pairing, framework integrations.

use serde::Serialize;
use tauri::{AppHandle, State};

use forge_core::{ForgeError, Result};
use forge_open::pairing::{now_secs, ClientInfo, PairingCode};
use forge_open::project::{self, ProjectProfile};
use forge_open::{LinkAction, OpenSource};

use crate::companion::{self, CompanionState, PendingEvent, ServerStatus};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionStatus {
    pub server: ServerStatus,
    pub pairing: Option<PairingCode>,
    pub clients: Vec<ClientInfo>,
}

#[tauri::command]
pub fn companion_status(state: State<'_, CompanionState>) -> CompanionStatus {
    let server = state.status.lock().expect("status lock poisoned").clone();
    let pairing = state.pairing();
    CompanionStatus {
        server,
        pairing: pairing.pending(now_secs()).cloned(),
        clients: pairing.clients(),
    }
}

/// Show a new pairing code ("Pair New Browser…").
#[tauri::command]
pub fn companion_start_pairing(state: State<'_, CompanionState>) -> PairingCode {
    state.pairing().start(now_secs())
}

#[tauri::command]
pub fn companion_cancel_pairing(state: State<'_, CompanionState>) {
    state.pairing().cancel();
}

/// Forget a paired browser; its token stops working immediately.
#[tauri::command]
pub fn companion_revoke_client(id: String, state: State<'_, CompanionState>) -> bool {
    state.pairing().revoke(&id)
}

/// Called once by the UI after it subscribed to `companion://open`: returns requests that
/// arrived earlier (e.g. `forge src/App.tsx:12` at startup).
#[tauri::command]
pub fn companion_ui_ready(state: State<'_, CompanionState>) -> Vec<PendingEvent> {
    state.take_pending()
}

/// Open a `forge://` link as if the OS had handed it over (the "Test" button).
#[tauri::command]
pub fn companion_open_link(url: String, app: AppHandle) -> Result<()> {
    match forge_open::request::parse_forge_url(&url) {
        Ok(LinkAction::Open(r)) => companion::open_request(&app, &r, OpenSource::Link, None)
            .map(|_| ())
            .map_err(|e| ForgeError::Other(e.to_string())),
        Ok(LinkAction::OpenSymbol(r)) => companion::open_symbol(&app, &r, OpenSource::Link, None)
            .map(|_| ())
            .map_err(|e| ForgeError::Other(e.message)),
        Err(e) => Err(ForgeError::Other(e.to_string())),
    }
}

/// Packages, dotenv values and config files of the open project, to rank snippets.
#[tauri::command]
pub fn companion_project_profile(
    env_keys: Vec<String>,
    files: Vec<String>,
    state: State<'_, AppState>,
) -> Result<ProjectProfile> {
    state.with_workspace(|ws| Ok(project::profile(ws.root(), &env_keys, &files)))
}

/// Set `key=value` in `.env` or `.env.local` of the open project ("Apply" buttons, after
/// the user confirmed).
#[tauri::command]
pub fn companion_set_env_var(
    file: String,
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<()> {
    if !project::WRITABLE_ENV_FILES.contains(&file.as_str()) {
        return Err(ForgeError::Other(format!("{file} is not a dotenv file")));
    }
    if !project::is_valid_env_key(&key) || value.contains(['\n', '\r', '\0']) {
        return Err(ForgeError::Other("invalid environment variable".into()));
    }
    state.with_workspace(|ws| {
        let current = if ws.resolve(&file)?.exists() {
            ws.read_file(&file)?
        } else {
            String::new()
        };
        ws.write_file(&file, &project::upsert_env_var(&current, &key, &value))
    })
}
