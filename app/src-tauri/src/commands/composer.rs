//! Composer (`phpforge-composer`): outdated markers, completion and hover in composer.json,
//! the Manage Composer Dependencies dialog (Packagist search, versions, installed
//! packages) and the composer.phar download.
//!
//! Project files are read through the backend and Composer runs on the project's machine,
//! so remote projects work. Packagist is reached from here only (while
//! `php.composer.packagist` is on), cached in `<cache dir>/forge/packagist`; without
//! network the cached data or the installed packages answer. Composer commands that change
//! the project (install, update, require, remove) run as `composer-command` configurations
//! in the Run tool window.
//!
//! IPC: `composer_project`, `composer_markers({ path, text })`,
//! `composer_complete({ path, text, offset })`, `composer_hover({ path, text, offset })`,
//! `composer_search({ query })`, `composer_package({ name })`, `composer_download_phar`.
//! `composer audit` runs with the vulnerable dependency check (`commands/vulnerabilities.rs`).

use std::collections::HashMap;
use std::sync::Arc;

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_jsonschema::{CompletionOut, HoverOut};
use phpforge_composer::complete::{self, CompletionData, Request};
use phpforge_composer::lock::{self, InstalledPackage};
use phpforge_composer::manifest::{self, Requirement};
use phpforge_composer::net::{self, Packagist};
use phpforge_composer::outdated::{self, OutdatedMarker};
use phpforge_composer::packagist::{PackageInfo, SearchHit};
use phpforge_composer::{installed_json_path, lock_path, EXECUTABLE_KEY, JSON_KEY, PACKAGIST_KEY};
use serde::Serialize;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

pub struct ComposerState {
    packagist: Option<Packagist>,
}

impl Default for ComposerState {
    fn default() -> Self {
        let packagist = Packagist::default_cache_dir().and_then(|dir| {
            Packagist::new(dir)
                .map_err(|e| tracing::warn!("no Packagist client: {e}"))
                .ok()
        });
        ComposerState { packagist }
    }
}

pub(crate) struct Options {
    pub json: String,
    pub executable: String,
    pub php: String,
    packagist: bool,
}

/// Composer settings: composer.json, the executable and the PHP interpreter that runs a
/// `.phar` (also used by the vulnerable dependency check).
pub(crate) fn options(state: &AppState) -> Options {
    let php = crate::commands::php_interpreters::local_php(state);
    let s = state.read_settings();
    Options {
        json: s
            .get_as::<String>(JSON_KEY)
            .map(|p| p.trim().trim_start_matches("./").to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "composer.json".into()),
        executable: s.get_as::<String>(EXECUTABLE_KEY).unwrap_or_default(),
        php,
        packagist: s.get_as::<bool>(PACKAGIST_KEY).unwrap_or(true),
    }
}

impl ComposerState {
    fn packagist(&self, opts: &Options) -> Option<&Packagist> {
        self.packagist.as_ref().filter(|_| opts.packagist)
    }
}

/// Installed packages of the composer.json at `json` (its lock file, else installed.json).
fn installed(backend: &Arc<dyn Backend>, json: &str) -> Vec<InstalledPackage> {
    let lock = backend.read_optional(&lock_path(json)).ok().flatten();
    let installed_json = if lock.is_none() {
        backend
            .read_optional(&installed_json_path(json))
            .ok()
            .flatten()
    } else {
        None
    };
    lock::installed(lock.as_deref(), installed_json.as_deref())
}

async fn installed_async(state: &AppState, json: String) -> Vec<InstalledPackage> {
    let Ok(backend) = state.backend() else {
        return Vec::new();
    };
    blocking(move || Ok(installed(&backend, &json)))
        .await
        .unwrap_or_default()
}

/// The project's Composer files for the dependencies dialog.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerProject {
    /// Path of composer.json (Settings | PHP | Composer); `exists` false when there is none.
    pub composer_json: String,
    pub exists: bool,
    pub has_lock: bool,
    pub requirements: Vec<Requirement>,
    pub installed: Vec<InstalledPackage>,
}

#[tauri::command]
pub async fn composer_project(state: State<'_, AppState>) -> Result<ComposerProject> {
    let opts = options(&state);
    let backend = state.backend()?;
    blocking(move || {
        let text = backend.read_optional(&opts.json)?;
        let has_lock = backend.read_optional(&lock_path(&opts.json))?.is_some();
        Ok(ComposerProject {
            requirements: text
                .as_deref()
                .map(manifest::requirements)
                .unwrap_or_default(),
            exists: text.is_some(),
            has_lock,
            installed: installed(&backend, &opts.json),
            composer_json: opts.json,
        })
    })
    .await
}

/// The latest releases of `names` on Packagist (cached; unknown or unreachable ones are
/// left out).
async fn latest_versions(packagist: &Packagist, names: Vec<String>) -> HashMap<String, String> {
    let mut set = tokio::task::JoinSet::new();
    for name in names {
        let p = packagist.clone();
        set.spawn(async move {
            let latest = p.package(&name).await.ok()?.latest()?;
            Some((name, latest.original().to_string()))
        });
    }
    let mut out = HashMap::new();
    while let Some(r) = set.join_next().await {
        if let Ok(Some((name, latest))) = r {
            out.insert(name, latest);
        }
    }
    out
}

/// Outdated packages of the composer.json in the editor (`text`, saved or not).
#[tauri::command]
pub async fn composer_markers(
    path: String,
    text: String,
    state: State<'_, AppState>,
    composer: State<'_, ComposerState>,
) -> Result<Vec<OutdatedMarker>> {
    let opts = options(&state);
    let Some(packagist) = composer.packagist(&opts).cloned() else {
        return Ok(Vec::new());
    };
    let installed = installed_async(&state, path).await;
    let names: Vec<String> = manifest::requirements(&text)
        .into_iter()
        .filter(|r| {
            !r.is_platform()
                && manifest::is_package_name(&r.name)
                && installed.iter().any(|p| p.name == r.name)
        })
        .map(|r| r.name)
        .collect();
    let latest = latest_versions(&packagist, names).await;
    Ok(outdated::markers(&text, &installed, &latest))
}

/// Package names and versions in `require` / `require-dev`.
#[tauri::command]
pub async fn composer_complete(
    path: String,
    text: String,
    offset: usize,
    state: State<'_, AppState>,
    composer: State<'_, ComposerState>,
) -> Result<Option<CompletionOut>> {
    let Some(req) = complete::request(&text, offset) else {
        return Ok(None);
    };
    let opts = options(&state);
    let mut data = CompletionData {
        installed: installed_async(&state, path).await,
        ..CompletionData::default()
    };
    if let Some(p) = composer.packagist(&opts) {
        match &req {
            Request::Packages { prefix } if prefix.trim().len() >= 2 => {
                data.search = p.search(prefix, 30).await.unwrap_or_default();
            }
            Request::Versions { package, .. } if manifest::is_package_name(package) => {
                data.package = p.package(package).await.ok();
            }
            _ => {}
        }
    }
    Ok(complete::items(&text, offset, &data))
}

/// Description, versions and links of the package under the cursor.
#[tauri::command]
pub async fn composer_hover(
    path: String,
    text: String,
    offset: usize,
    state: State<'_, AppState>,
    composer: State<'_, ComposerState>,
) -> Result<Option<HoverOut>> {
    let Some((name, _)) = complete::hover_target(&text, offset) else {
        return Ok(None);
    };
    let opts = options(&state);
    let installed = installed_async(&state, path).await;
    let info = match composer.packagist(&opts) {
        Some(p) => p.package(&name).await.ok(),
        None => None,
    };
    Ok(complete::hover(&text, offset, &installed, info.as_ref()))
}

fn packagist_off() -> ForgeError {
    ForgeError::Other(
        "Packagist access is turned off (Settings | Languages & Frameworks | PHP | Composer)"
            .into(),
    )
}

#[tauri::command]
pub async fn composer_search(
    query: String,
    state: State<'_, AppState>,
    composer: State<'_, ComposerState>,
) -> Result<Vec<SearchHit>> {
    let opts = options(&state);
    let p = composer.packagist(&opts).ok_or_else(packagist_off)?;
    p.search(&query, 30).await.map_err(ForgeError::Other)
}

/// A package with its versions and the constraint `composer require` would write.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDetails {
    #[serde(flatten)]
    pub info: PackageInfo,
    pub suggested: Option<String>,
    pub url: String,
}

#[tauri::command]
pub async fn composer_package(
    name: String,
    state: State<'_, AppState>,
    composer: State<'_, ComposerState>,
) -> Result<PackageDetails> {
    let opts = options(&state);
    let p = composer.packagist(&opts).ok_or_else(packagist_off)?;
    let info = p.package(&name).await.map_err(ForgeError::Other)?;
    Ok(PackageDetails {
        suggested: info.suggested_constraint(),
        url: info.page_url(),
        info,
    })
}

/// "Download composer.phar from getcomposer.org" (Settings | PHP | Composer, after the
/// user agreed): into `<data dir>/forge/composer`, checked against the published SHA-256.
#[tauri::command]
pub async fn composer_download_phar() -> Result<String> {
    let dir = net::default_phar_dir()
        .ok_or_else(|| ForgeError::Other("no data directory on this machine".into()))?;
    let path = net::download_phar(net::GETCOMPOSER_URL, &dir, false)
        .await
        .map_err(ForgeError::Other)?;
    Ok(path.display().to_string())
}
