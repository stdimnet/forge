//! Inspections (`forge-inspections`): the registry of the enabled plugins, profiles
//! (Settings | Editor | Inspections; user settings and `.forge/inspectionProfiles/*.xml`),
//! Inspect Code / Run Inspection by Name with progress and cancel, quick fixes from the
//! results, Code Cleanup and the exports. The profile tree, suppressions and the results
//! tree are pure Rust the UI runs through WebAssembly.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result, SettingsScope};
use forge_inspections::cleanup::{self, CleanupPlan};
use forge_inspections::io::{self as profile_io, ProfileFormat};
use forge_inspections::problem::stamp;
use forge_inspections::report as reports;
use forge_inspections::store::{self, WriteScope};
use forge_inspections::{Fix, Profile, ProfilesState, Registry, Report, RunOptions, SeverityDef};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::commands::workspace::blocking;
use crate::inspections::{self as engine, InspectScope};
use crate::state::AppState;

/// Running inspections, by run id, for Cancel.
#[derive(Default)]
pub struct InspectionsState {
    runs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

/// The registry of the enabled plugins.
pub(crate) fn registry(state: &AppState) -> Registry {
    state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .inspection_registry()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryView {
    pub registry: Registry,
    /// The built-in severities.
    pub severities: Vec<SeverityDef>,
}

/// The inspections of the enabled plugins, their tool mappings and the built-in severities.
#[tauri::command]
pub fn inspections_registry(state: State<'_, AppState>) -> RegistryView {
    RegistryView {
        registry: registry(&state),
        severities: forge_inspections::severity::builtin(),
    }
}

fn load(state: &AppState) -> ProfilesState {
    let backend = state.backend().ok();
    let settings = state.read_settings();
    engine::load_profiles(&settings, backend.as_deref(), &registry(state))
}

/// The profiles and the selected one.
#[tauri::command]
pub async fn inspections_profiles(state: State<'_, AppState>) -> Result<ProfilesState> {
    let state = state.inner();
    Ok(load(state))
}

/// Store the profiles edited on the Inspections page (Apply / OK).
#[tauri::command]
pub async fn inspections_save(
    profiles: ProfilesState,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ProfilesState> {
    let registry = registry(&state);
    let old = load(&state);
    let plan = store::plan_save(&old, &profiles, &registry);
    {
        let mut settings = state.write_settings();
        let has_project = settings.has_project();
        settings.set_many(
            plan.settings
                .into_iter()
                .filter(|w| w.scope == WriteScope::User || has_project)
                .map(|w| {
                    let scope = match w.scope {
                        WriteScope::User => SettingsScope::User,
                        WriteScope::Project => SettingsScope::Project,
                    };
                    (w.key, w.value.unwrap_or(Value::Null), scope)
                })
                .collect(),
        )?;
    }
    if !plan.files.is_empty() {
        let backend = state.backend()?;
        let files = plan.files;
        blocking(move || {
            for f in files {
                match f.text {
                    Some(t) => backend.write_file(&f.path, &t)?,
                    None => {
                        if backend.read_optional(&f.path)?.is_some() {
                            backend.delete(&f.path)?;
                        }
                    }
                }
            }
            Ok(())
        })
        .await?;
    }
    let saved = load(&state);
    // Every window (the main window, a Settings dialog window) reloads its profiles.
    let _ = app.emit("inspections://changed", &saved);
    Ok(saved)
}

/// A profile as PHPStorm XML or JSON; with `path` it is also written to that file (picked in
/// a save dialog).
#[tauri::command]
pub async fn inspections_export_profile(
    profile: Profile,
    format: ProfileFormat,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<String> {
    let text = profile_io::export(&profile, &registry(&state), format);
    if let Some(p) = path {
        tokio::fs::write(&p, &text)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?;
    }
    Ok(text)
}

/// Read a profile from `text`, or from the file at `path` (picked in an open dialog).
#[tauri::command]
pub async fn inspections_import_profile(
    text: Option<String>,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Profile> {
    let text = match (text, path) {
        (Some(t), _) => t,
        (None, Some(p)) => tokio::fs::read_to_string(&p)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?,
        (None, None) => return Err(ForgeError::Other("nothing to import".into())),
    };
    profile_io::import(&text, &registry(&state)).map_err(ForgeError::Other)
}

/// What Inspect Code, Run Inspection by Name and Code Cleanup run.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InspectRequest {
    /// Matches progress events and Cancel.
    pub run_id: u64,
    pub scope: InspectScope,
    /// The profile's name (the selected one when missing).
    pub profile: Option<String>,
    /// Only these inspections (Run Inspection by Name).
    pub only: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    run_id: u64,
    done: usize,
    total: usize,
    text: String,
}

/// Run inspections over a scope in the backend; progress goes out as
/// `inspections://progress`.
async fn run_request(
    request: InspectRequest,
    state: &AppState,
    runs: &InspectionsState,
    app: AppHandle,
    cleanup_only: bool,
) -> Result<(Report, Vec<forge_inspections::FileInput>)> {
    let backend = state.backend()?;
    let mut registry = registry(state);
    let profiles = load(state);
    let scopes = engine::scope_set(&state.read_settings());
    let only = if cleanup_only {
        Some(
            registry
                .inspections
                .iter()
                .filter(|s| s.cleanup)
                .map(|s| s.id.clone())
                .collect::<Vec<_>>(),
        )
    } else {
        request.only.clone()
    };
    // Editor-only inspections (a running language server or tool) do not run in a batch.
    registry
        .inspections
        .retain(|s| s.batch || only.as_ref().is_some_and(|o| o.contains(&s.id)));
    let cancel = Arc::new(AtomicBool::new(false));
    runs.runs
        .lock()
        .expect("runs lock")
        .insert(request.run_id, cancel.clone());
    let run_id = request.run_id;
    let result = blocking(move || {
        let emit = |p: forge_inspections::RunProgress| {
            let _ = app.emit(
                "inspections://progress",
                ProgressEvent {
                    run_id,
                    done: p.done,
                    total: p.total,
                    text: p.text,
                },
            );
        };
        let cancelled = || cancel.load(Ordering::Relaxed);
        let test_roots = backend
            .read_optional("composer.json")?
            .map(|t| forge_scopes::composer_test_roots(&t))
            .unwrap_or_default();
        let paths = engine::scope_files(backend.as_ref(), &request.scope, &test_roots)?;
        let mut last = std::time::Instant::now();
        let mut throttled = |p: forge_inspections::RunProgress| {
            if p.done == 0 || p.done >= p.total || last.elapsed().as_millis() > 80 {
                last = std::time::Instant::now();
                emit(p);
            }
        };
        let files = engine::read_files(backend.as_ref(), &paths, &mut throttled, &cancelled);
        let options = RunOptions {
            only,
            max_problems: 20_000,
        };
        let mut report = engine::inspect(
            backend.clone(),
            registry,
            &profiles,
            request.profile.as_deref(),
            &scopes,
            &test_roots,
            &files,
            &options,
            &mut throttled,
            &cancelled,
        );
        report.scope = request.scope.label.clone();
        report.cancelled |= cancelled();
        Ok((report, files))
    })
    .await;
    runs.runs.lock().expect("runs lock").remove(&run_id);
    result
}

/// Inspect Code / Run Inspection by Name.
#[tauri::command]
pub async fn inspections_run(
    request: InspectRequest,
    state: State<'_, AppState>,
    runs: State<'_, InspectionsState>,
    app: AppHandle,
) -> Result<Report> {
    Ok(run_request(request, &state, &runs, app, false).await?.0)
}

/// Stop a run (its result comes back marked `cancelled`).
#[tauri::command]
pub fn inspections_cancel(run_id: u64, runs: State<'_, InspectionsState>) {
    if let Some(flag) = runs.runs.lock().expect("runs lock").get(&run_id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Code Cleanup: the safe fixes of the cleanup inspections over a scope, as a preview.
#[tauri::command]
pub async fn inspections_cleanup_plan(
    request: InspectRequest,
    state: State<'_, AppState>,
    runs: State<'_, InspectionsState>,
    app: AppHandle,
) -> Result<CleanupPlan> {
    let (report, files) = run_request(request, &state, &runs, app, true).await?;
    Ok(cleanup::plan(
        &report.problems,
        &engine::texts(&files),
        &registry(&state),
    ))
}

/// A file to write (Code Cleanup): written only if it still has the text the change was
/// computed for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub stamp: u64,
    pub after: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub written: Vec<String>,
    /// Changed since they were inspected.
    pub skipped: Vec<String>,
}

/// Write the files of a Code Cleanup preview.
#[tauri::command]
pub async fn inspections_cleanup_apply(
    files: Vec<FileChange>,
    state: State<'_, AppState>,
) -> Result<Applied> {
    let backend = state.backend()?;
    blocking(move || {
        let mut out = Applied::default();
        for f in files {
            let now = backend.read_file(&f.path)?;
            if stamp(&now) != f.stamp {
                out.skipped.push(f.path);
                continue;
            }
            backend.write_file(&f.path, &f.after)?;
            out.written.push(f.path);
        }
        Ok(out)
    })
    .await
}

/// Apply one quick fix from Inspection Results; `stamp` is the inspected file's.
#[tauri::command]
pub async fn inspections_apply_fix(
    path: String,
    stamp: u64,
    fix: Fix,
    state: State<'_, AppState>,
) -> Result<Applied> {
    let backend = state.backend()?;
    blocking(move || {
        let mut by_path: HashMap<String, Vec<forge_inspections::FileEdit>> = HashMap::new();
        for e in fix.edits {
            by_path.entry(e.path.clone()).or_default().push(e);
        }
        let mut out = Applied::default();
        for (p, edits) in by_path {
            let now = backend.read_file(&p)?;
            if p == path && forge_inspections::problem::stamp(&now) != stamp {
                out.skipped.push(p);
                continue;
            }
            let after = cleanup::apply_edits(&now, &edits).map_err(ForgeError::Other)?;
            backend.write_file(&p, &after)?;
            out.written.push(p);
        }
        Ok(out)
    })
    .await
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReportFormat {
    Html,
    Xml,
    Json,
}

/// Export to HTML / XML / JSON: the text, also written to `path` when given.
#[tauri::command]
pub async fn inspections_export_report(
    report: Report,
    format: ReportFormat,
    path: Option<String>,
) -> Result<String> {
    let text = match format {
        ReportFormat::Html => reports::to_html(&report),
        ReportFormat::Xml => reports::to_xml(&report),
        ReportFormat::Json => reports::to_json(&report),
    };
    if let Some(p) = path {
        tokio::fs::write(&p, &text)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?;
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    #[test]
    fn builtin_manifests_register_the_inspections() {
        let plugins = crate::plugins::builtin_registry().unwrap();
        let r = plugins.inspection_registry();
        for id in [
            "DuplicatedCode",
            "PhpUnusedAssignment",
            "PhpConstantCondition",
            "PhpDeprecation",
            "RsClippy",
            "SpellCheckingInspection",
            "LanguageServerDiagnostics",
        ] {
            assert!(r.get(id).is_some(), "{id}");
        }
        assert_eq!(
            r.inspection_for("clippy", Some("needless_return")),
            Some("RsClippy")
        );
        assert_eq!(
            r.inspection_for("pyforge", Some("python.undeclaredImport")),
            Some("PyUndeclaredRequirement")
        );
        assert_eq!(
            r.inspection_for("php-inspections", Some("PhpUnusedAssignment")),
            Some("PhpUnusedAssignment")
        );
        // Every PHP problem code the index produces is registered.
        for id in crate::inspections::PhpProvider::ids() {
            assert!(r.get(&id).is_some(), "{id}");
        }
    }
}
