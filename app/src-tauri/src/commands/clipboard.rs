//! Clipboard history for Paste from History (`forge_editing::clipboard`). The history lives
//! in memory for the session; the UI passes the configured limit
//! (`editor.clipboard.maxContents`) with every call.

use std::sync::Mutex;

use forge_editing::{ClipEntry, ClipItem, ClipboardHistory};
use tauri::State;

#[derive(Default)]
pub struct ClipboardState(Mutex<ClipboardHistory>);

impl ClipboardState {
    fn with<T>(&self, max: Option<usize>, f: impl FnOnce(&mut ClipboardHistory) -> T) -> T {
        let mut history = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(max) = max {
            history.set_max(max);
        }
        f(&mut history)
    }
}

/// Remember a text that was copied or cut in Forge.
#[tauri::command]
pub fn clipboard_history_add(
    entry: ClipEntry,
    max: Option<usize>,
    state: State<'_, ClipboardState>,
) -> bool {
    state.with(max, |h| h.add(entry))
}

/// The entries matching `query`, newest first.
#[tauri::command]
pub fn clipboard_history_list(
    query: Option<String>,
    max: Option<usize>,
    state: State<'_, ClipboardState>,
) -> Vec<ClipItem> {
    state.with(max, |h| h.items(query.as_deref().unwrap_or("")))
}

/// Remove an entry (Delete in the popup).
#[tauri::command]
pub fn clipboard_history_remove(index: usize, state: State<'_, ClipboardState>) -> bool {
    state.with(None, |h| h.remove(index).is_some())
}

/// Move a pasted entry to the top and return it.
#[tauri::command]
pub fn clipboard_history_promote(
    index: usize,
    state: State<'_, ClipboardState>,
) -> Option<ClipEntry> {
    state.with(None, |h| h.promote(index))
}
