use std::sync::RwLock;

use forge_core::{ForgeError, Workspace};

/// Shared application state managed by Tauri.
#[derive(Default)]
pub struct AppState {
    workspace: RwLock<Option<Workspace>>,
}

impl AppState {
    pub fn set_workspace(&self, ws: Workspace) {
        *self.workspace.write().expect("workspace lock poisoned") = Some(ws);
    }

    /// Run `f` with the open workspace, or fail if no project is open.
    pub fn with_workspace<T>(
        &self,
        f: impl FnOnce(&Workspace) -> forge_core::Result<T>,
    ) -> forge_core::Result<T> {
        let guard = self.workspace.read().expect("workspace lock poisoned");
        match guard.as_ref() {
            Some(ws) => f(ws),
            None => Err(ForgeError::Other("no project is open".into())),
        }
    }
}
