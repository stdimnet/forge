//! Compare Directories: the folder diff of two project folders (computed where the files
//! are, see `forge_backend::Backend::compare_dirs`) and its copy actions.

use forge_core::Result;
use forge_diff::DirCompare;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

#[tauri::command]
pub async fn compare_dirs(
    left: String,
    right: String,
    state: State<'_, AppState>,
) -> Result<DirCompare> {
    let backend = state.backend()?;
    blocking(move || backend.compare_dirs(&left, &right)).await
}

/// Copy a file or folder over another one (Copy to Right / Copy to Left).
#[tauri::command]
pub async fn compare_copy(
    from: String,
    to: String,
    dir: bool,
    state: State<'_, AppState>,
) -> Result<()> {
    let backend = state.backend()?;
    blocking(move || forge_backend::compare::copy_entry(backend.as_ref(), &from, &to, dir)).await
}
