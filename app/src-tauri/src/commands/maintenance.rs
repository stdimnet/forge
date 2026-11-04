//! Repairing and resetting the IDE (`forge_maintenance`): File | Invalidate Caches…,
//! Repair IDE's "Rescan project indexes", and File | Manage IDE Settings (Export / Import
//! Settings, Restore Default Settings). Work that must not race with open files (deleting
//! caches, removing the settings) is scheduled for the next start and runs in [`startup`]
//! before anything loads.

use std::collections::BTreeMap;
use std::path::PathBuf;

use forge_core::{ForgeError, Result, SettingsScope};
use forge_maintenance::archive::{self, SettingsArchive};
use forge_maintenance::caches::CacheOption;
use forge_maintenance::dirs::ForgeDirs;
use forge_maintenance::groups::{count_groups, settings_groups, GroupCount, INSTALLED_PLUGINS_FILE};
use forge_maintenance::{pending, restore, InvalidateModel};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// The bundle identifier, which names the webview's cache folders.
const IDENTIFIER: &str = "dev.forge.ide";

pub(crate) fn forge_dirs() -> ForgeDirs {
    ForgeDirs::system(IDENTIFIER)
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// Run the work scheduled by the previous session (call before the settings load).
pub fn startup() {
    if let Some(report) = pending::run_pending(&forge_dirs()) {
        if let Some(caches) = &report.caches {
            for item in &caches.deleted {
                tracing::info!("invalidated {}: {}", item.label, item.path.display());
            }
            for (item, e) in &caches.failed {
                tracing::warn!("cannot delete {}: {e}", item.path.display());
            }
        }
        for file in &report.reset_files {
            tracing::info!("restored default settings: removed {}", file.display());
        }
    }
}

/// The Invalidate Caches dialog: options and texts.
#[tauri::command]
pub fn maintenance_invalidate_model() -> InvalidateModel {
    forge_maintenance::invalidate_model()
}

/// "Invalidate and Restart" (`invalidate`) or "Just Restart".
#[tauri::command]
pub fn maintenance_invalidate_caches(
    options: Vec<CacheOption>,
    invalidate: bool,
    app: AppHandle,
) -> Result<()> {
    if invalidate {
        pending::schedule(&forge_dirs(), |w| {
            let mut all = w.invalidate.take().unwrap_or_default();
            all.extend(options);
            w.invalidate = Some(all);
        })
        .map_err(err)?;
    }
    app.restart();
}

/// Restart Forge (after Import Settings or Restore Default Settings).
#[tauri::command]
pub fn maintenance_restart(app: AppHandle) {
    app.restart();
}

/// Repair IDE's "Rescan project indexes": bring the search index in line with the disk.
/// Returns how many files changed. (The PHP index rescans through `php_index_workspace`.)
#[tauri::command]
pub async fn maintenance_rescan_indexes(state: State<'_, AppState>) -> Result<usize> {
    let backend = state.backend()?;
    blocking(move || backend.rescan_search_index()).await
}

/// The configuration files of `<config dir>/forge` that settings groups export.
fn config_files(dirs: &ForgeDirs) -> BTreeMap<String, Vec<u8>> {
    let Some(config) = dirs.forge_config() else {
        return BTreeMap::new();
    };
    settings_groups()
        .iter()
        .filter_map(|g| g.file)
        .filter_map(|f| Some((f.to_string(), std::fs::read(config.join(f)).ok()?)))
        .collect()
}

/// What Export Settings offers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportInfo {
    pub groups: Vec<GroupCount>,
    /// `<home>/settings.zip`.
    pub default_path: String,
}

#[tauri::command]
pub fn settings_export_info(state: State<'_, AppState>) -> ExportInfo {
    let user = state
        .read_settings()
        .layer_values(SettingsScope::User)
        .clone();
    let mut files = config_files(&forge_dirs());
    if let Some(bytes) = crate::commands::settings_sync::installed_plugins_file(&state) {
        files.insert(INSTALLED_PLUGINS_FILE.to_string(), bytes);
    }
    let names: Vec<&str> = files.keys().map(String::as_str).collect();
    ExportInfo {
        groups: count_groups(user.keys().map(String::as_str), &names),
        default_path: dirs::home_dir()
            .unwrap_or_default()
            .join("settings.zip")
            .display()
            .to_string(),
    }
}

/// Write the chosen groups to a settings archive at `path` (a local file).
#[tauri::command]
pub async fn settings_export(
    path: String,
    groups: Vec<String>,
    state: State<'_, AppState>,
) -> Result<String> {
    let user = state
        .read_settings()
        .layer_values(SettingsScope::User)
        .clone();
    let plugins = crate::commands::settings_sync::installed_plugins_file(&state);
    blocking(move || {
        let mut files = config_files(&forge_dirs());
        if let Some(bytes) = plugins {
            files.insert(INSTALLED_PLUGINS_FILE.to_string(), bytes);
        }
        let bytes = archive::export_settings(
            &user,
            &files,
            &groups,
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(err)?;
        let path = PathBuf::from(path);
        std::fs::write(&path, bytes).map_err(|e| ForgeError::io(&path, e))?;
        Ok(path.display().to_string())
    })
    .await
}

fn read(path: &str) -> Result<SettingsArchive> {
    let bytes = std::fs::read(path).map_err(|e| ForgeError::io(path, e))?;
    archive::read_archive(&bytes).map_err(err)
}

/// The groups a settings archive holds (Import Settings' "Select Components to Import").
#[tauri::command]
pub async fn settings_import_preview(path: String) -> Result<Vec<GroupCount>> {
    blocking(move || Ok(read(&path)?.groups())).await
}

/// Import the chosen groups: they replace the user's settings of those groups; files are
/// written next to the settings. Forge restarts afterwards to load everything.
#[tauri::command]
pub async fn settings_import(path: String, groups: Vec<String>, app: AppHandle) -> Result<usize> {
    blocking(move || {
        let archive = read(&path)?;
        let state = app.state::<AppState>();
        let current = state
            .read_settings()
            .layer_values(SettingsScope::User)
            .clone();
        let plan = archive.plan(&current, &groups);
        let count = plan.changes.len() + plan.files.len();
        state.write_settings().set_many(
            plan.changes
                .into_iter()
                .map(|(k, v)| (k, v, SettingsScope::User))
                .collect(),
        )?;
        crate::commands::settings_sync::apply_files(&state, plan.files)?;
        Ok(count)
    })
    .await
}

/// Restore Default Settings: back up the configuration and reset it on the next start.
/// Returns the backup folder; the UI restarts Forge afterwards.
#[tauri::command]
pub async fn settings_restore_defaults() -> Result<String> {
    blocking(|| {
        let dirs = forge_dirs();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let backup = restore::backup_settings(&dirs, now).map_err(err)?;
        pending::schedule(&dirs, |w| w.restore_defaults = true).map_err(err)?;
        Ok(backup.display().to_string())
    })
    .await
}
