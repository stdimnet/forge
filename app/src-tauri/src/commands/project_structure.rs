//! Project structure (Settings | Directories, the Project view's "Mark Directory as" ›
//! Excluded / Resource Root / Test Resources Root) and the Project view's rows (File
//! Nesting, Compact Middle Packages, Show Excluded Files, Folders Always on Top, Sort by
//! Type). The rules are computed by `forge_scopes::{structure, view}`; the excluded paths
//! reach the backend (`Backend::set_structure`), so the search index, walks and the watcher
//! of local and remote projects skip them.

use forge_backend::Backend;
use forge_core::excludes::{ExcludeSet, StructureConfig};
use forge_core::{FileEntry, FileKind, Result};
use forge_scopes::structure::{FOLDERS_KEY, PATTERNS_KEY};
use forge_scopes::view::{arrange, TreeRow, ViewEntry, ViewOptions};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::workspace::{blocking, IGNORED_FILES_KEY};
use crate::state::AppState;

/// Emitted when the excluded paths changed: the UI re-reads the tree and reindexes.
pub const CHANGED_EVENT: &str = "project-structure://changed";

/// The project structure from the settings.
pub fn config(state: &AppState) -> StructureConfig {
    let settings = state.read_settings();
    StructureConfig::from_settings(|k| settings.get(k).cloned())
}

/// The excluded paths now in effect.
pub fn excludes(state: &AppState) -> ExcludeSet {
    ExcludeSet::new(&config(state))
}

/// Settings keys that change the excluded paths.
pub fn affects(key: &str) -> bool {
    key == FOLDERS_KEY || key == PATTERNS_KEY
}

/// Hand the excluded paths to the open project's backend (blocks for a remote project);
/// announces a change to the UI.
pub fn apply(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(backend) = state.backend() else {
        return;
    };
    match backend.set_structure(config(&state)) {
        Ok(true) => {
            if let Err(e) = app.emit(CHANGED_EVENT, ()) {
                tracing::warn!("cannot emit {CHANGED_EVENT}: {e}");
            }
        }
        Ok(false) => {}
        Err(e) => tracing::warn!("cannot apply the excluded folders: {e}"),
    }
}

fn view_entry(e: FileEntry) -> ViewEntry {
    ViewEntry {
        name: e.name,
        path: e.path,
        kind: if e.kind == FileKind::Dir {
            "dir"
        } else {
            "file"
        }
        .into(),
        ignored: e.ignored,
        excluded: e.excluded,
    }
}

/// The rows of one folder with the view options applied; names matching `ignored`
/// (Settings | Editor | File Types | Ignored Files and Folders) are left out.
pub fn tree_rows(
    backend: &dyn Backend,
    path: &str,
    options: &ViewOptions,
    ignored: &str,
) -> Result<Vec<TreeRow>> {
    let shown = |v: Vec<FileEntry>| -> Vec<ViewEntry> {
        v.into_iter()
            .filter(|e| !forge_filetypes::matches_glob_list(&e.name, ignored))
            .map(view_entry)
            .collect()
    };
    let entries = backend.list_dir(path)?;
    let mut list = |dir: &str| backend.list_dir(dir).map(shown).unwrap_or_default();
    Ok(arrange(shown(entries), &mut list, options))
}

/// The Project view's rows of a folder ("" is the project root).
#[tauri::command]
pub async fn project_tree_list(
    path: String,
    options: Option<ViewOptions>,
    state: State<'_, AppState>,
) -> Result<Vec<TreeRow>> {
    let backend = state.backend()?;
    let options = options.unwrap_or_default();
    let ignored: String = state
        .read_settings()
        .get_as(IGNORED_FILES_KEY)
        .unwrap_or_else(|| forge_filetypes::DEFAULT_IGNORED_FILES.to_string());
    blocking(move || tree_rows(backend.as_ref(), &path, &options, &ignored)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_backend::LocalBackend;

    #[test]
    fn rows_come_from_the_backend_with_excluded_folders_flagged() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["src/Acme/Shop/A.php", "node_modules/x.js", "a.ts", "a.js"] {
            let p = dir.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "").unwrap();
        }
        let backend = LocalBackend::open(dir.path()).unwrap();
        let options = ViewOptions {
            compact_middle_packages: true,
            ..ViewOptions::default()
        };
        let rows = tree_rows(&backend, "", &options, "").unwrap();
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["node_modules", "src/Acme/Shop", "a.ts"]);
        assert!(rows[0].excluded);
        assert_eq!(rows[2].nested[0].name, "a.js");
        let hidden = ViewOptions {
            show_excluded: false,
            ..ViewOptions::default()
        };
        // Without excluded entries; ignored names (`*.ts`) leave the tree and free `a.js`.
        let names: Vec<String> = tree_rows(&backend, "", &hidden, "*.ts")
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["src", "a.js"]);
        assert!(affects(FOLDERS_KEY) && affects(PATTERNS_KEY) && !affects("x"));
    }
}
