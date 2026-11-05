//! Project trust and Safe Mode IPC (`forge-trust`): the "Trust and Open Project?" dialog,
//! the Safe Mode banner and Settings | Build, Execution, Deployment | Trusted Locations.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use forge_core::{ForgeError, Result};
use forge_trust::{TrustCheck, TrustStore};

use crate::commands::lsp::LspState;
use crate::state::AppState;

/// Emitted with [`TrustStatus`] (or `null` without a project) when the open project's trust
/// changes.
pub const TRUST_CHANGED_EVENT: &str = "trust://changed";

const GUEST_CANNOT_TRUST: &str =
    "The host's project of a Code With Me session cannot be trusted: nothing of it runs on this machine.";

/// The open project's trust state.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrustStatus {
    #[serde(flatten)]
    pub check: TrustCheck,
    /// Nothing of the project runs (it is not trusted).
    pub safe_mode: bool,
    pub remote: bool,
    /// The project is the host's in a Code With Me session: it is always in Safe Mode here
    /// and cannot be trusted from this window (no banner, no Trust Project…).
    pub guest: bool,
}

/// An entry of the Trusted Locations page.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrustedEntry {
    pub key: String,
    pub display: String,
    /// `location` (covers the projects below it) or `project` (one trusted project).
    pub kind: &'static str,
}

fn status(state: &AppState) -> Option<TrustStatus> {
    let info = state.project_info()?;
    let guest = info.collab.is_some();
    let mut check = state.trust_store().check(&crate::state::trust_key(&info));
    if guest {
        check.trusted = false;
        check.reason = None;
        check.parent = None;
        check.parent_display = None;
    }
    Some(TrustStatus {
        safe_mode: state.safe_mode.is_on(),
        remote: state.is_remote(),
        guest,
        check,
    })
}

/// Re-evaluate the open project after the store changed. When it leaves Safe Mode, the
/// language servers start again (project-local ones were refused) and the detection runs
/// again.
fn store_changed(app: &AppHandle) {
    let state = app.state::<AppState>();
    let was = state.safe_mode.is_on();
    let now = state.refresh_safe_mode();
    if was != now {
        // Servers refused in Safe Mode start now; project-local ones stop when it is back.
        app.state::<LspState>().reset();
        if !now {
            // Registers the language servers again.
            crate::commands::project::emit_detection(app);
        }
    }
    crate::lifecycle::trust_changed(app);
    if let Err(e) = app.emit(TRUST_CHANGED_EVENT, status(&state)) {
        tracing::warn!("cannot emit {TRUST_CHANGED_EVENT}: {e}");
    }
}

fn entries(store: &TrustStore) -> Vec<TrustedEntry> {
    let entry = |key: &String, kind| TrustedEntry {
        display: forge_trust::display(key),
        key: key.clone(),
        kind,
    };
    let mut list: Vec<TrustedEntry> = store
        .locations
        .iter()
        .map(|k| entry(k, "location"))
        .chain(store.projects.iter().map(|k| entry(k, "project")))
        .collect();
    list.sort_by(|a, b| a.display.cmp(&b.display));
    list
}

/// Whether the local folder `path` is trusted, before it is opened.
#[tauri::command]
pub fn trust_check(path: String, state: State<'_, AppState>) -> TrustCheck {
    let key = forge_trust::local_key(std::path::Path::new(&path));
    state.trust_store().check(&key)
}

/// The open project's trust state (`None` without a project).
#[tauri::command]
pub fn trust_status(state: State<'_, AppState>) -> Option<TrustStatus> {
    status(&state)
}

/// Trust a local folder before opening it (`path`), or the open project (no `path`).
/// `location` also trusts every project in that folder ("Trust projects in …").
#[tauri::command]
pub fn trust_project(
    path: Option<String>,
    location: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<TrustCheck> {
    let key = match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => forge_trust::local_key(std::path::Path::new(&p)),
        None => {
            if state.project_info().is_some_and(|i| i.collab.is_some()) {
                return Err(ForgeError::Other(GUEST_CANNOT_TRUST.into()));
            }
            state
                .project_trust_key()
                .ok_or_else(ForgeError::no_project)?
        }
    };
    let mut store = state.trust_store();
    match location.filter(|l| !l.trim().is_empty()) {
        Some(location) => {
            if !forge_trust::contains(&location, &key) {
                return Err(ForgeError::Other(format!(
                    "{} is not in {}",
                    forge_trust::display(&key),
                    forge_trust::display(&location)
                )));
            }
            store.add_location(&location)?;
        }
        None => {
            store.trust(&key)?;
        }
    }
    state.save_trust_store(&store)?;
    store_changed(&app);
    Ok(store.check(&key))
}

/// Mark a project Forge created as trusted (New Project).
pub fn trust_created(state: &AppState, root: &std::path::Path) {
    let mut store = state.trust_store();
    let saved = store
        .trust(&forge_trust::local_key(root))
        .and_then(|_| state.save_trust_store(&store));
    if let Err(e) = saved {
        tracing::warn!("cannot trust the new project {}: {e}", root.display());
    }
}

/// Settings | Build, Execution, Deployment | Trusted Locations.
#[tauri::command]
pub fn trust_locations(state: State<'_, AppState>) -> Vec<TrustedEntry> {
    entries(&state.trust_store())
}

/// Add a trusted location (`~` is the home folder).
#[tauri::command]
pub fn trust_location_add(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<TrustedEntry>> {
    let mut store = state.trust_store();
    store.add_location(&path)?;
    state.save_trust_store(&store)?;
    store_changed(&app);
    Ok(entries(&store))
}

/// Remove a trusted location or trusted project. The open project returns to Safe Mode when
/// nothing trusts it any more.
#[tauri::command]
pub fn trust_location_remove(
    key: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<TrustedEntry>> {
    let mut store = state.trust_store();
    let removed = store.remove_location(&key) | store.distrust(&key);
    if !removed {
        return Err(ForgeError::Other(format!(
            "{} is not trusted",
            forge_trust::display(&key)
        )));
    }
    state.save_trust_store(&store)?;
    store_changed(&app);
    Ok(entries(&store))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_list_locations_and_projects() {
        let mut store = TrustStore::default();
        store.add_location("/home/me/work").unwrap();
        store.trust("/opt/shop").unwrap();
        store
            .trust(&forge_trust::remote_key("me", "box", 22, "/srv/shop"))
            .unwrap();
        let list = entries(&store);
        assert_eq!(
            list.iter()
                .map(|e| (e.display.as_str(), e.kind))
                .collect::<Vec<_>>(),
            vec![
                ("/home/me/work", "location"),
                ("/opt/shop", "project"),
                ("me@box:/srv/shop", "project"),
            ]
        );
    }

    #[test]
    fn safe_mode_follows_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("shop");
        std::fs::create_dir(&project).unwrap();
        let mut state = AppState::default();
        state.trust_path = Some(dir.path().join("trusted-projects.toml"));
        state.set_workspace(forge_core::Workspace::open(&project).unwrap());
        assert!(state.safe_mode.is_on());
        let err = state.require_trusted("Running tests").unwrap_err();
        assert!(err.to_string().contains("Safe Mode"));

        let mut store = state.trust_store();
        store.add_location(&dir.path().to_string_lossy()).unwrap();
        state.save_trust_store(&store).unwrap();
        assert!(!state.refresh_safe_mode());
        assert!(state.require_trusted("Running tests").is_ok());

        trust_created(&state, &project);
        let mut store = state.trust_store();
        store.remove_location(&dir.path().to_string_lossy());
        state.save_trust_store(&store).unwrap();
        assert!(!state.refresh_safe_mode(), "created projects stay trusted");
    }
}
