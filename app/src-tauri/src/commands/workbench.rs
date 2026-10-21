//! Workbench tool windows: TODO (scanned by `forge-find` through the project's backend, so
//! remote projects are scanned on their host) and Bookmarks (`forge-bookmarks`, stored per
//! project in Forge's state dir, never inside the project).

use std::path::PathBuf;
use std::sync::Mutex;

use forge_bookmarks::{Bookmarks, Op};
use forge_core::search::{TodoRequest, TodoResult};
use forge_core::{ForgeError, Result};
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// TODO tool window: TODO patterns in comments of the whole project or of `request.paths`.
#[tauri::command]
pub async fn todo_scan(request: TodoRequest, state: State<'_, AppState>) -> Result<TodoResult> {
    let backend = state.backend()?;
    blocking(move || backend.find_todos(&request)).await
}

/// Serializes read-modify-write cycles of the bookmark files.
#[derive(Default)]
pub struct BookmarksState(Mutex<()>);

/// FNV-1a: a hash that stays the same across Rust releases (the file name must not change).
fn stable_hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// `<data dir>/forge/projects/<name>-<hash of the root>/<file>`: per-project state that does
/// not belong into the project (bookmarks). Remote projects are keyed by `user@host:/path`.
pub fn project_state_file(name: &str, root: &str, file: &str) -> Option<PathBuf> {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Some(
        dirs::data_dir()?
            .join("forge")
            .join("projects")
            .join(format!("{safe}-{:016x}", stable_hash(root)))
            .join(file),
    )
}

fn bookmarks_file(state: &AppState) -> Result<PathBuf> {
    let info = state
        .project_info()
        .ok_or_else(|| ForgeError::Other("no project is open".into()))?;
    project_state_file(&info.name, &info.display_root(), "bookmarks.json")
        .ok_or_else(|| ForgeError::Other("no data directory on this system".into()))
}

fn load(file: &PathBuf) -> Bookmarks {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// The bookmarks of the open project.
#[tauri::command]
pub fn bookmarks_get(
    state: State<'_, AppState>,
    lock: State<'_, BookmarksState>,
) -> Result<Bookmarks> {
    let _guard = lock.0.lock().expect("bookmarks lock");
    Ok(load(&bookmarks_file(&state)?))
}

/// Apply changes to the open project's bookmarks, save them and return the new state.
#[tauri::command]
pub fn bookmarks_apply(
    ops: Vec<Op>,
    state: State<'_, AppState>,
    lock: State<'_, BookmarksState>,
) -> Result<Bookmarks> {
    let _guard = lock.0.lock().expect("bookmarks lock");
    let file = bookmarks_file(&state)?;
    let mut bookmarks = load(&file);
    for op in ops {
        bookmarks.apply(op);
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
    }
    let json =
        serde_json::to_string_pretty(&bookmarks).map_err(|e| ForgeError::Other(e.to_string()))?;
    std::fs::write(&file, json).map_err(|e| ForgeError::io(&file, e))?;
    Ok(bookmarks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_files_are_stable_per_project() {
        assert_eq!(stable_hash("a"), 0xaf63_dc4c_8601_ec8c);
        let local = project_state_file("shop", "/home/me/shop", "bookmarks.json");
        let remote = project_state_file("shop", "deploy@host:/var/www/shop", "bookmarks.json");
        if let (Some(local), Some(remote)) = (local, remote) {
            assert_ne!(local, remote);
            assert!(local.ends_with("bookmarks.json"));
            assert!(local.to_string_lossy().contains("forge"));
        }
        let odd = project_state_file("my shop/1", "/x", "b.json").unwrap_or_default();
        assert!(!odd.to_string_lossy().contains("my shop/1"));
    }
}
