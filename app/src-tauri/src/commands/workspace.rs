use serde::Serialize;
use tauri::{AppHandle, State};

use forge_core::{FileEntry, FileMatch, Result, Workspace};
use forge_vcs::Repo;

use crate::state::AppState;

#[derive(Serialize)]
pub struct WorkspaceInfo {
    pub name: String,
    pub root: String,
}

#[tauri::command]
pub fn workspace_open(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WorkspaceInfo> {
    let ws = Workspace::open(&path)?;
    crate::commands::project::emit_detection(&app, ws.root());
    let info = WorkspaceInfo {
        name: ws.name(),
        root: ws.root().display().to_string(),
    };
    state.set_workspace(ws);
    crate::recent::remember(crate::recent::RecentProject {
        name: info.name.clone(),
        root: info.root.clone(),
    });
    Ok(info)
}

/// The project opened at startup (command line argument), if any.
#[tauri::command]
pub fn workspace_current(state: State<'_, AppState>) -> Option<WorkspaceInfo> {
    state
        .with_workspace(|ws| {
            Ok(WorkspaceInfo {
                name: ws.name(),
                root: ws.root().display().to_string(),
            })
        })
        .ok()
}

/// A recent project with its detected framework or language for the Welcome screen badge.
#[derive(Serialize)]
pub struct RecentProjectInfo {
    pub name: String,
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<forge_project::Framework>,
}

/// Recently opened projects that still exist on disk.
#[tauri::command]
pub async fn workspace_recent() -> Vec<RecentProjectInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        crate::recent::load()
            .into_iter()
            .filter(|p| std::path::Path::new(&p.root).is_dir())
            .map(|p| RecentProjectInfo {
                primary: forge_project::detect(std::path::Path::new(&p.root)).primary,
                name: p.name,
                root: p.root,
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn fs_list_dir(path: String, state: State<'_, AppState>) -> Result<Vec<FileEntry>> {
    let (mut entries, root) =
        state.with_workspace(|ws| Ok((ws.list_dir(&path)?, ws.root().to_path_buf())))?;
    let candidates: Vec<String> = entries
        .iter()
        .filter(|e| e.ignored)
        .map(|e| e.path.clone())
        .collect();
    if !candidates.is_empty() {
        // Files git tracks keep their VCS status even when a pattern matches them, so
        // they must not be grayed out (`git add -f`, files ignored after being committed).
        let tracked = tauri::async_runtime::spawn_blocking(move || {
            Repo::open(&root).and_then(|r| r.tracked_among(&candidates))
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
        clear_tracked(&mut entries, &tracked);
    }
    Ok(entries)
}

/// Unmark entries that are tracked: a file listed in `tracked`, or a folder with tracked
/// files below it. Ignoring only applies to untracked content.
fn clear_tracked(entries: &mut [FileEntry], tracked: &[String]) {
    for e in entries.iter_mut().filter(|e| e.ignored) {
        let prefix = format!("{}/", e.path);
        e.ignored = !tracked
            .iter()
            .any(|t| t == &e.path || t.starts_with(&prefix));
    }
}

#[tauri::command]
pub fn fs_read_file(path: String, state: State<'_, AppState>) -> Result<String> {
    state.with_workspace(|ws| ws.read_file(&path))
}

#[tauri::command]
pub fn fs_write_file(path: String, contents: String, state: State<'_, AppState>) -> Result<()> {
    state.with_workspace(|ws| ws.write_file(&path, &contents))
}

/// "Go to File" backend: fuzzy file-name search over the project, honoring `.gitignore`.
#[tauri::command]
pub fn fs_find_files(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<FileMatch>> {
    state.with_workspace(|ws| ws.find_files(&query, limit.unwrap_or(100)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::FileKind;

    fn entry(path: &str, kind: FileKind, ignored: bool) -> FileEntry {
        FileEntry {
            name: path.rsplit('/').next().unwrap().to_string(),
            path: path.to_string(),
            kind,
            ignored,
        }
    }

    #[test]
    fn tracked_files_and_folders_with_tracked_files_are_not_ignored() {
        let mut entries = vec![
            entry("vendor", FileKind::Dir, true),
            entry("cache", FileKind::Dir, true),
            entry("forced.log", FileKind::File, true),
            entry("other.log", FileKind::File, true),
            entry("src", FileKind::Dir, false),
        ];
        let tracked = vec!["vendor/forced.php".to_string(), "forced.log".to_string()];
        clear_tracked(&mut entries, &tracked);
        let flags: Vec<bool> = entries.iter().map(|e| e.ignored).collect();
        assert_eq!(flags, vec![false, true, false, true, false]);
    }

    #[test]
    fn a_folder_name_prefix_is_not_a_tracked_folder() {
        let mut entries = vec![entry("vendor", FileKind::Dir, true)];
        clear_tracked(&mut entries, &["vendor-extra/a.php".to_string()]);
        assert!(entries[0].ignored);
    }
}
