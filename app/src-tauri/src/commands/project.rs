//! Project IPC: project type detection and the New Project wizard (`forge_project`).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use forge_project::{CreateEvent, CreateRequest, Generator, Plan, ProjectInfo};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

/// Emitted with [`Detected`] after a project was opened and analysed.
pub const DETECTED_EVENT: &str = "project://detected";
/// Emitted with [`CreateEvent`]s while a project is created.
pub const CREATE_EVENT: &str = "project://create";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub root: String,
    pub info: ProjectInfo,
}

/// Detect in the background and emit `project://detected`, so opening stays instant.
pub fn emit_detection(app: &AppHandle, root: &Path) {
    let app = app.clone();
    let root = root.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let payload = Detected {
            root: root.display().to_string(),
            info: forge_project::detect(&root),
        };
        if let Err(e) = app.emit(DETECTED_EVENT, payload) {
            tracing::warn!("could not emit {DETECTED_EVENT}: {e}");
        }
    });
}

/// Languages, frameworks and tools of the open project.
#[tauri::command]
pub async fn project_detect(state: State<'_, AppState>) -> Result<ProjectInfo> {
    let root = state.with_workspace(|ws| Ok(ws.root().to_path_buf()))?;
    tauri::async_runtime::spawn_blocking(move || forge_project::detect(&root))
        .await
        .map_err(|e| ForgeError::Other(e.to_string()))
}

#[tauri::command]
pub fn project_generators_list() -> Vec<Generator> {
    forge_project::generators()
}

fn request(
    generator: String,
    location: String,
    name: String,
    options: Option<BTreeMap<String, Value>>,
) -> CreateRequest {
    CreateRequest {
        generator,
        location,
        name,
        options: options.unwrap_or_default(),
    }
}

/// The steps a creation would run (command preview), missing tools and form errors.
#[tauri::command]
pub async fn project_plan(
    generator: String,
    location: String,
    name: String,
    options: Option<BTreeMap<String, Value>>,
) -> Result<Plan> {
    let req = request(generator, location, name, options);
    tauri::async_runtime::spawn_blocking(move || forge_project::plan_for_system(&req))
        .await
        .map_err(|e| ForgeError::Other(e.to_string()))?
}

/// The running creation's cancel flag.
#[derive(Default)]
pub struct CreateState {
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

/// Create a project, streaming progress as `project://create`. Returns the project root.
#[tauri::command]
pub async fn project_create(
    generator: String,
    location: String,
    name: String,
    options: Option<BTreeMap<String, Value>>,
    app: AppHandle,
    creating: State<'_, CreateState>,
) -> Result<String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = creating.cancel.lock().expect("create lock poisoned");
        if slot.is_some() {
            return Err(ForgeError::Other(
                "A project is already being created".into(),
            ));
        }
        *slot = Some(cancel.clone());
    }
    let req = request(generator, location, name, options);
    let result = tauri::async_runtime::spawn_blocking(move || {
        forge_project::create(&req, &cancel, &mut |event: CreateEvent| {
            if let Err(e) = app.emit(CREATE_EVENT, event) {
                tracing::warn!("could not emit {CREATE_EVENT}: {e}");
            }
        })
    })
    .await
    .map_err(|e| ForgeError::Other(e.to_string()));
    *creating.cancel.lock().expect("create lock poisoned") = None;
    Ok(result??.display().to_string())
}

#[tauri::command]
pub fn project_create_cancel(creating: State<'_, CreateState>) {
    if let Some(flag) = creating
        .cancel
        .lock()
        .expect("create lock poisoned")
        .as_ref()
    {
        flag.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// Browser mode lists the same generators from a JSON copy. Run with
    /// `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app` after changing a generator.
    #[test]
    fn browser_mock_generators_snapshot_is_current() {
        let json = serde_json::to_string_pretty(&forge_project::generators()).unwrap() + "\n";
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/src/mock/projectGenerators.json");
        if std::env::var_os("FORGE_UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, &json).unwrap();
        }
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            current.replace("\r\n", "\n") == json,
            "{} is out of date; rerun with FORGE_UPDATE_SNAPSHOTS=1",
            path.display()
        );
    }
}
