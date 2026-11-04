//! Settings Sync (`forge_maintenance::sync`): the chosen settings groups kept in step
//! through a folder or a git repository the user provides (setting `settingsSync.*`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use forge_core::{ForgeError, Result, Settings, SettingsScope};
use forge_maintenance::archive::ImportPlan;
use forge_maintenance::groups::{settings_groups, INSTALLED_PLUGINS_FILE};
use forge_maintenance::sync::{self, GitRemote, SyncInput, SyncReport, SyncState};

use crate::commands::workspace::blocking;
use crate::state::AppState;

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn modified_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The installed third-party plugins as the `installedPlugins` group's file.
pub fn installed_plugins_file(state: &AppState) -> Option<Vec<u8>> {
    let refs = state.plugin_store.as_ref()?.refs().ok()?;
    if refs.is_empty() {
        return None;
    }
    serde_json::to_vec_pretty(&refs).ok()
}

/// Write imported or synced config files; the installed plugin list is not a file: it marks
/// missing plugins for the Marketplace tab. Returns whether a restart is needed.
pub fn apply_files(state: &AppState, files: Vec<(String, Vec<u8>)>) -> Result<bool> {
    let mut restart = false;
    let config = crate::commands::maintenance::forge_dirs().forge_config();
    for (name, bytes) in files {
        if name == INSTALLED_PLUGINS_FILE {
            let refs: Vec<forge_plugins::PluginRef> =
                serde_json::from_slice(&bytes).map_err(err)?;
            if let Some(store) = &state.plugin_store {
                store.import_refs(&refs).map_err(err)?;
            }
            continue;
        }
        let Some(config) = &config else { continue };
        std::fs::create_dir_all(config).map_err(|e| ForgeError::io(config, e))?;
        let target = config.join(&name);
        std::fs::write(&target, bytes).map_err(|e| ForgeError::io(&target, e))?;
        restart = true;
    }
    Ok(restart)
}

fn apply(state: &AppState, plan: ImportPlan) -> Result<bool> {
    if !plan.changes.is_empty() {
        state.write_settings().set_many(
            plan.changes
                .into_iter()
                .map(|(k, v)| (k, v, SettingsScope::User))
                .collect(),
        )?;
    }
    apply_files(state, plan.files)
}

fn config_dir() -> Result<PathBuf> {
    crate::commands::maintenance::forge_dirs()
        .forge_config()
        .ok_or_else(|| err("no user config directory"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub enabled: bool,
    pub method: String,
    /// The folder or repository, empty when not configured.
    pub target: String,
    pub last_report: Option<SyncReport>,
    /// Groups that can be synced (id, label).
    pub groups: Vec<(String, String)>,
}

fn machine_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|h| !h.is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|h| h.trim().to_string())
        })
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "this computer".into())
}

#[tauri::command]
pub fn settings_sync_status(state: State<'_, AppState>) -> Result<SyncStatus> {
    let s = state.read_settings();
    let method: String = s.get_as("settingsSync.method").unwrap_or_else(|| "folder".into());
    let target = if method == "git" {
        s.get_as("settingsSync.gitUrl")
    } else {
        s.get_as("settingsSync.folder")
    }
    .unwrap_or_default();
    Ok(SyncStatus {
        enabled: s.get_as("settingsSync.enabled").unwrap_or(false),
        method,
        target,
        last_report: SyncState::load(&config_dir()?.join(sync::STATE_FILE)).last_report,
        groups: settings_groups()
            .iter()
            .map(|g| (g.id.to_string(), g.label.to_string()))
            .collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    #[serde(flatten)]
    pub report: SyncReport,
    /// Config files changed: Forge loads them after a restart.
    pub restart_needed: bool,
}

/// Sync now: push local changes, apply remote ones, resolve conflicts (newest wins, the
/// other side is backed up).
#[tauri::command]
pub async fn settings_sync_now(app: AppHandle) -> Result<SyncResult> {
    blocking(move || {
        let state = app.state::<AppState>();
        let (values, method, folder, git_url, branch, groups) = {
            let s = state.read_settings();
            (
                s.layer_values(SettingsScope::User).clone(),
                s.get_as::<String>("settingsSync.method")
                    .unwrap_or_else(|| "folder".into()),
                s.get_as::<String>("settingsSync.folder").unwrap_or_default(),
                s.get_as::<String>("settingsSync.gitUrl").unwrap_or_default(),
                s.get_as::<String>("settingsSync.gitBranch")
                    .unwrap_or_else(|| "main".into()),
                s.get_as::<Vec<String>>("settingsSync.groups")
                    .unwrap_or_default(),
            )
        };
        let config = config_dir()?;
        let mut files: BTreeMap<String, (Vec<u8>, u64)> = BTreeMap::new();
        for name in settings_groups().iter().filter_map(|g| g.file) {
            let path = config.join(name);
            if let Ok(bytes) = std::fs::read(&path) {
                files.insert(name.to_string(), (bytes, modified_ms(&path)));
            }
        }
        if let Some(bytes) = installed_plugins_file(&state) {
            let stamp = state
                .plugin_store
                .as_ref()
                .map_or(0, |s| modified_ms(&s.root().join("installed.json")));
            files.insert(INSTALLED_PLUGINS_FILE.to_string(), (bytes, stamp));
        }
        let settings_ms = Settings::default_user_path().map_or(0, |p| modified_ms(&p));
        let local = sync::local_components(&values, settings_ms, &files, &groups).map_err(err)?;
        let state_path = config.join(sync::STATE_FILE);
        let mut sync_state = SyncState::load(&state_path);
        let machine = machine_name();
        let backups = config.join(sync::LOCAL_BACKUPS);
        let input = SyncInput {
            local: &local,
            groups: &groups,
            machine: &machine,
            now_ms: now_ms(),
            local_backups: &backups,
        };
        let (report, pulled) = if method == "git" {
            let remote = GitRemote {
                url: git_url,
                branch,
            };
            let clone_dir = sync::default_clone_dir().ok_or_else(|| err("no cache directory"))?;
            sync::sync_git(&remote, &clone_dir, &input, &mut sync_state).map_err(err)?
        } else {
            if folder.trim().is_empty() {
                return Err(err("choose a sync folder in Settings | Settings Sync"));
            }
            sync::sync_folder(Path::new(&folder), &input, &mut sync_state).map_err(err)?
        };
        let plan = sync::apply_plan(&pulled, &values).map_err(err)?;
        let restart_needed = apply(&state, plan)?;
        sync_state.save(&state_path).map_err(err)?;
        Ok(SyncResult {
            report,
            restart_needed,
        })
    })
    .await
}
