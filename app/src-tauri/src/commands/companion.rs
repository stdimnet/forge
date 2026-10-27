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

/// Packages, dotenv values and config files of the open project, to rank snippets (read
/// through the project backend, on the host for remote projects).
#[tauri::command]
pub async fn companion_project_profile(
    env_keys: Vec<String>,
    files: Vec<String>,
    state: State<'_, AppState>,
) -> Result<ProjectProfile> {
    let backend = state.backend()?;
    crate::commands::workspace::blocking(move || {
        let read = |name: &str| backend.read_optional(name).ok().flatten();
        Ok(project::profile_with(
            read,
            |name| read(name).is_some(),
            &env_keys,
            &files,
        ))
    })
    .await
}

/// Set `key=value` in `.env` or `.env.local` of the open project ("Apply" buttons, after
/// the user confirmed).
#[tauri::command]
pub async fn companion_set_env_var(
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
    let backend = state.backend()?;
    crate::commands::workspace::blocking(move || {
        let current = backend.read_optional(&file)?.unwrap_or_default();
        backend.write_file(&file, &project::upsert_env_var(&current, &key, &value))
    })
    .await
}

/// Live Edit: tell the paired browsers that a project file changed (saved, or typed into
/// for style sheets with `text`). Style sheets are swapped in place, other files reload
/// the pages that use them. `None` when Live Edit is off or ignores the file.
#[tauri::command]
pub fn companion_live_edit(
    path: String,
    text: Option<String>,
    app: AppHandle,
    state: State<'_, CompanionState>,
) -> Option<forge_open::live_edit::LiveEditEvent> {
    use tauri::Manager;
    let on = app
        .state::<AppState>()
        .read_settings()
        .get_bool(companion::KEY_LIVE_EDIT)
        .unwrap_or(true);
    if !on {
        return None;
    }
    state.live_edit.push(&path, text)
}
