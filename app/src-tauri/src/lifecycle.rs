//! The window's project lifecycle (`forge_core::lifecycle`): every open and close of the
//! project runs the Rust subscribers in order and is published to the UI on
//! [`LIFECYCLE_EVENT`] (`project://lifecycle`).
//!
//! - `closing` runs before the backend of the old project is replaced or dropped
//!   ([`closing`]), in every path that changes the project: `workspace_open`, a remote, dev
//!   container or WSL project, joining or leaving a Code With Me session, Close Project.
//! - `opened` runs in [`crate::commands::workspace::project_opened`] (and at startup): only in
//!   the window (= Forge process) that opens the project, after the trust decision the UI asks
//!   before `workspace_open` (remote projects are in Safe Mode until trusted).
//! - `ready` follows once the project's search index is built ([`wait_until_ready`]).
//! - `beforeClose` is the `project_before_close` command: the UI asks it, shows the
//!   objections and only then opens the other project.
//!
//! Each project window is a process of its own, so events reach only this window.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_core::lifecycle::{
    LifecycleEvent, Objection, PhaseReport, ProjectLifecycle, ProjectLifecycleBus, ProjectRef,
    LIFECYCLE_EVENT,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::workspace::WorkspaceInfo;
use crate::state::AppState;

/// The lifecycle bus of this window.
#[derive(Default)]
pub struct LifecycleState(pub ProjectLifecycleBus);

/// How often [`wait_until_ready`] looks at the index, and for how long at most.
const READY_POLL: Duration = Duration::from_millis(400);
const READY_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Subscribe the built-in services (file watcher, language servers).
pub fn install(app: &AppHandle) {
    let bus = &app.state::<LifecycleState>().0;
    bus.subscribe(Arc::new(Watcher(app.clone())));
    bus.subscribe(Arc::new(LanguageServers(app.clone())));
}

/// What the lifecycle says about the open project: its UI identity, VCS root and trust.
fn project_ref(app: &AppHandle) -> Option<ProjectRef> {
    let state = app.state::<AppState>();
    let info = state.project_info()?;
    let ws = WorkspaceInfo::from(&info);
    let mut project = ProjectRef::new(ws.name.clone(), ws.root.clone());
    project.trusted = !state.safe_mode.is_on();
    if info.is_local() {
        project.vcs_root = vcs_root(&info.root);
    }
    if let Ok(serde_json::Value::Object(mut details)) = serde_json::to_value(&ws) {
        details.remove("name");
        details.remove("root");
        project.details = details;
    }
    Some(project)
}

/// The closest folder at or above `root` with a `.git` entry.
fn vcs_root(root: &Path) -> Option<String> {
    root.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(|dir| dir.to_string_lossy().into_owned())
}

fn emit(app: &AppHandle, report: &PhaseReport) {
    if let Err(e) = app.emit(LIFECYCLE_EVENT, LifecycleEvent::from(report)) {
        tracing::warn!("cannot emit {LIFECYCLE_EVENT}: {e}");
    }
}

/// The open project closes: run `closing` and tell the UI. Call before the project's backend
/// is replaced or dropped; a no-op without an open project.
pub fn closing(app: &AppHandle) {
    if let Some(report) = app.state::<LifecycleState>().0.close() {
        emit(app, &report);
    }
}

/// A project opened in this window: `closing` of one still open, `opened`, then `ready` once
/// it is indexed.
pub fn opened(app: &AppHandle) {
    let Some(project) = project_ref(app) else {
        return;
    };
    let bus = &app.state::<LifecycleState>().0;
    for report in bus.open(project) {
        emit(app, &report);
    }
    wait_until_ready(app, bus.generation());
}

/// The trust of the open project changed (Trust Project in the Safe Mode banner).
pub fn trust_changed(app: &AppHandle) {
    let trusted = !app.state::<AppState>().safe_mode.is_on();
    app.state::<LifecycleState>()
        .0
        .update_current(|p| p.trusted = trusted);
}

/// Run `ready` for `generation` once the search index covers the project (at once without
/// an index), unless another project opened meanwhile.
fn wait_until_ready(app: &AppHandle, generation: u64) {
    let Ok(backend) = app.state::<AppState>().backend() else {
        return;
    };
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("project-ready".into())
        .spawn(move || {
            let start = Instant::now();
            loop {
                if app.state::<LifecycleState>().0.generation() != generation {
                    return;
                }
                match backend.search_status() {
                    Ok(s) if s.enabled && s.indexing => {}
                    _ => break,
                }
                if start.elapsed() > READY_TIMEOUT {
                    break;
                }
                std::thread::sleep(READY_POLL);
            }
            if let Some(report) = app.state::<LifecycleState>().0.ready(generation) {
                emit(&app, &report);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("cannot wait for the index: {e}");
    }
}

/// Ask the subscribers whether the open project may close (`beforeClose`). Closes nothing:
/// the UI shows the objections and, when the user agrees, opens the next project.
#[tauri::command]
pub async fn project_before_close(app: AppHandle) -> Vec<Objection> {
    tauri::async_runtime::spawn_blocking(move || app.state::<LifecycleState>().0.before_close())
        .await
        .unwrap_or_default()
}

/// File | Close Project: `closing`, then the project is closed in this window.
#[tauri::command]
pub fn workspace_close(app: AppHandle) {
    closing(&app);
    crate::commands::remote::close_project_connection(&app);
    app.state::<AppState>().close_project();
}

/// The file watcher stops with its project; `project_opened` starts the next one's.
struct Watcher(AppHandle);

impl ProjectLifecycle for Watcher {
    fn name(&self) -> &str {
        "file watcher"
    }
    fn order(&self) -> i32 {
        -10
    }
    fn closing(&self, _project: &ProjectRef) -> forge_core::Result<()> {
        crate::commands::workspace::stop_watching(&self.0);
        Ok(())
    }
}

/// The language servers of the old project stop; the next request starts the new one's.
struct LanguageServers(AppHandle);

impl ProjectLifecycle for LanguageServers {
    fn name(&self) -> &str {
        "language servers"
    }
    fn closing(&self, _project: &ProjectRef) -> forge_core::Result<()> {
        self.0.state::<crate::commands::lsp::LspState>().reset();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_repository_above_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("apps").join("shop");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert_eq!(
            vcs_root(&project).as_deref(),
            Some(dir.path().to_string_lossy().as_ref())
        );
    }
}
