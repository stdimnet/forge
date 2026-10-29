//! Library files: read-only sources outside the project that navigation opens, like
//! PHPStorm's "External Libraries".
//!
//! - `library://php-stubs/<path>`: PHP's own declarations from the PHP stubs (the "PHP
//!   Runtime"; see `forge_langservers::managed::stubs`), indexed once with
//!   `phpforge_lang_php::stubs::StubIndex`.
//! - `library://file/<absolute path>`: a file outside the project that a language server
//!   pointed to (e.g. php-lsp's own stubs). Only paths handed out by a navigation answer can
//!   be read.
//!
//! IPC: `library_read({ path }) -> LibraryFile`. Stub downloads report
//! `langservers://install-progress` `{ id: "php-stubs", label, done, total }`.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use forge_langservers::managed::stubs::{self, StubsRelease};
use forge_langservers::managed::{Event, Http};
use phpforge_lang_php::stubs::StubIndex;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const LIBRARY_PREFIX: &str = "library://";
pub const STUBS_PREFIX: &str = "library://php-stubs/";
pub const FILE_PREFIX: &str = "library://file/";

/// Where the PHP stubs come from (shown in the library banner).
#[derive(Debug, Clone, PartialEq, Eq)]
enum StubsSource {
    Setting(PathBuf),
    Downloaded(String),
}

impl StubsSource {
    fn describe(&self) -> String {
        match self {
            StubsSource::Setting(p) => format!("Stubs from {}", p.display()),
            StubsSource::Downloaded(v) => format!("JetBrains phpstorm-stubs {v} (Apache-2.0)"),
        }
    }
}

struct LoadedStubs {
    index: Arc<StubIndex>,
    source: StubsSource,
    /// `php.stubs.path` when it was loaded; a change reloads.
    setting: String,
}

#[derive(Default)]
pub struct LibraryState {
    stubs: tokio::sync::Mutex<Option<LoadedStubs>>,
    /// Absolute files outside the project that navigation answers pointed to.
    granted: Mutex<HashSet<PathBuf>>,
    /// The last error, so hovering does not retry a failed download.
    failed: Mutex<Option<String>>,
}

fn stubs_setting(ws: &AppState) -> String {
    ws.read_settings()
        .get("php.stubs.path")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Turn a workspace-relative-free path into its parts, refusing `..` and absolute parts.
fn safe_relative(rel: &str) -> Option<PathBuf> {
    let p = Path::new(rel);
    p.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then(|| p.to_path_buf())
}

impl LibraryState {
    /// The PHP stubs index, built on first use. With `download`, the pinned phpstorm-stubs
    /// release is downloaded when nothing else is available; without, only what is on
    /// disk is used (Ctrl+hover must not start a download).
    pub async fn stubs(
        &self,
        app: &AppHandle,
        ws: &AppState,
        download: bool,
    ) -> std::result::Result<Arc<StubIndex>, String> {
        let setting = stubs_setting(ws);
        let mut slot = self.stubs.lock().await;
        if let Some(loaded) = slot.as_ref().filter(|l| l.setting == setting) {
            return Ok(loaded.index.clone());
        }
        if !download {
            if let Some(err) = self.failed.lock().expect("failed lock").clone() {
                return Err(err);
            }
        }
        let release = stubs::pinned_release();
        let base = stubs::default_stubs_dir();
        let source = if !setting.is_empty() {
            StubsSource::Setting(PathBuf::from(&setting))
        } else if base
            .as_deref()
            .and_then(|b| stubs::installed(b, &release.version))
            .is_some()
        {
            StubsSource::Downloaded(release.version.clone())
        } else if download {
            let base = base.clone().ok_or("no cache folder for the PHP stubs")?;
            match self.download(app, &base, &release).await {
                Ok(_) => StubsSource::Downloaded(release.version.clone()),
                Err(e) => {
                    *self.failed.lock().expect("failed lock") = Some(e.clone());
                    return Err(e);
                }
            }
        } else {
            return Err("PHP stubs are not available yet".into());
        };
        let dir = match &source {
            StubsSource::Setting(p) => p.clone(),
            StubsSource::Downloaded(v) => base
                .map(|b| b.join(v))
                .ok_or("no cache folder for the PHP stubs")?,
        };
        if !dir.is_dir() {
            return Err(format!(
                "The PHP stubs folder {} does not exist (Settings | PHP | Default stubs path)",
                dir.display()
            ));
        }
        let index =
            blocking(move || StubIndex::build(&dir).map_err(|e| ForgeError::Other(e.to_string())))
                .await
                .map_err(|e| e.to_string())?;
        let index = Arc::new(index);
        *self.failed.lock().expect("failed lock") = None;
        *slot = Some(LoadedStubs {
            index: index.clone(),
            source,
            setting,
        });
        Ok(index)
    }

    /// The stubs index if it is loaded already.
    pub fn stubs_ready(&self) -> Option<Arc<StubIndex>> {
        self.stubs
            .try_lock()
            .ok()
            .and_then(|s| s.as_ref().map(|l| l.index.clone()))
    }

    async fn download(
        &self,
        app: &AppHandle,
        base: &Path,
        release: &StubsRelease,
    ) -> std::result::Result<PathBuf, String> {
        let http = Http::new()?;
        let app = app.clone();
        let mut last = 0u64;
        let mut on = move |e: Event| {
            if let Event::Progress { label, done, total } = e {
                let step = total.map_or(done >> 20, |t| done * 100 / t.max(1));
                if step == last && done != 0 {
                    return;
                }
                last = step;
                let _ = app.emit(
                    "langservers://install-progress",
                    serde_json::json!({ "id": "php-stubs", "label": label, "done": done, "total": total }),
                );
            }
        };
        stubs::ensure(&http, base, release, &mut on).await
    }

    /// A path of the PHP index as the UI opens it: workspace-relative paths stay, absolute
    /// ones (include paths) become granted `library://file/…` paths.
    pub fn ui_path(&self, path: &str) -> String {
        let p = Path::new(path);
        if path.starts_with('/') || p.is_absolute() {
            self.grant(p)
        } else {
            path.to_string()
        }
    }

    /// The UI path of a file outside the project a navigation answer points to; it may be
    /// read with `library_read` from now on.
    pub fn grant(&self, abs: &Path) -> String {
        self.granted
            .lock()
            .expect("granted lock")
            .insert(abs.to_path_buf());
        format!("{FILE_PREFIX}{}", abs.to_string_lossy().replace('\\', "/"))
    }
}

/// `library://php-stubs/<path>` for a file of the stubs index.
pub fn stub_path(rel: &str) -> String {
    format!("{STUBS_PREFIX}{rel}")
}

/// A read-only library file and what the editor banner says about it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    pub text: String,
    /// "PHP Runtime", "External Libraries".
    pub library: String,
    /// "JetBrains phpstorm-stubs 2025.3 (Apache-2.0)", the absolute path, ...
    pub detail: String,
}

#[tauri::command]
pub async fn library_read(
    path: String,
    app: AppHandle,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<LibraryFile> {
    read_library(&path, &app, &ws, &lib).await
}

/// A library file's text (the PHP stubs, or a file outside the project that navigation
/// granted).
pub async fn read_library(
    path: &str,
    app: &AppHandle,
    ws: &AppState,
    lib: &LibraryState,
) -> Result<LibraryFile> {
    if let Some(rel) = path.strip_prefix(STUBS_PREFIX) {
        let rel = safe_relative(rel).ok_or_else(|| ForgeError::Other("invalid path".into()))?;
        let index = lib.stubs(app, ws, false).await.map_err(ForgeError::Other)?;
        let detail = lib
            .stubs
            .lock()
            .await
            .as_ref()
            .map(|l| l.source.describe())
            .unwrap_or_default();
        let file = index.root().join(rel);
        let text = blocking(move || {
            std::fs::read_to_string(&file).map_err(|e| ForgeError::Other(e.to_string()))
        })
        .await?;
        return Ok(LibraryFile {
            text,
            library: "PHP Runtime".into(),
            detail,
        });
    }
    if let Some(abs) = path.strip_prefix(FILE_PREFIX) {
        let abs = PathBuf::from(abs);
        if !lib.granted.lock().expect("granted lock").contains(&abs) {
            return Err(ForgeError::Other(
                "only files opened through navigation can be read".into(),
            ));
        }
        let detail = abs.display().to_string();
        // A remote project's include paths live on its host.
        let remote = ws.is_remote().then(|| ws.backend()).transpose()?;
        let text = blocking(move || match remote {
            Some(backend) => backend.php().library_read(&abs.to_string_lossy()),
            None => std::fs::read_to_string(&abs).map_err(|e| ForgeError::Other(e.to_string())),
        })
        .await?;
        return Ok(LibraryFile {
            text,
            library: "External Libraries".into(),
            detail,
        });
    }
    Err(ForgeError::Other(format!("not a library file: {path}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_paths_stay_inside_the_stubs() {
        assert_eq!(
            safe_relative("Core/Core_c.php"),
            Some(PathBuf::from("Core/Core_c.php"))
        );
        assert_eq!(safe_relative("../etc/passwd"), None);
        assert_eq!(safe_relative("/etc/passwd"), None);
        assert_eq!(
            stub_path("Core/Core_c.php"),
            "library://php-stubs/Core/Core_c.php"
        );
    }

    #[test]
    fn granted_files_get_library_paths() {
        let lib = LibraryState::default();
        let p = lib.grant(Path::new("/opt/stubs/Core/Core_c.php"));
        assert_eq!(p, "library://file//opt/stubs/Core/Core_c.php");
        assert!(lib
            .granted
            .lock()
            .unwrap()
            .contains(Path::new("/opt/stubs/Core/Core_c.php")));
    }
}
