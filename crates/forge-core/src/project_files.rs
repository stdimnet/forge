//! Project-relative files that Forge itself keeps in a project (`.forge/settings.toml`,
//! `.forge/dataSources.toml`, `.forge/breakpoints.json`, ...) and project files it reads
//! (`.env`), wherever the project lives: [`LocalFiles`] for a folder on this machine, the
//! project backend (`forge_backend::BackendFiles`) for a project on a remote host.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{ForgeError, Result};

/// Reads and writes project-relative files (`/` separators). Methods block; a remote
/// implementation waits for the host.
pub trait ProjectFiles: Send + Sync {
    /// The file's text, `Ok(None)` when it does not exist.
    fn read(&self, rel: &str) -> Result<Option<String>>;
    /// Write the file, creating its parent folders.
    fn write(&self, rel: &str, text: &str) -> Result<()>;
    /// Where the files are, for messages (`/home/me/shop`, `deploy@host:/var/www/shop`).
    fn describe(&self, rel: &str) -> String;
}

/// Shared handle to a [`ProjectFiles`].
pub type SharedFiles = Arc<dyn ProjectFiles>;

/// Files of a folder on this machine.
#[derive(Debug, Clone)]
pub struct LocalFiles {
    root: PathBuf,
}

impl LocalFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn shared(root: impl Into<PathBuf>) -> SharedFiles {
        Arc::new(Self::new(root))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

impl ProjectFiles for LocalFiles {
    fn read(&self, rel: &str) -> Result<Option<String>> {
        let path = self.path(rel);
        match std::fs::read_to_string(&path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ForgeError::io(&path, e)),
        }
    }

    fn write(&self, rel: &str, text: &str) -> Result<()> {
        let path = self.path(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
        }
        std::fs::write(&path, text).map_err(|e| ForgeError::io(&path, e))
    }

    fn describe(&self, rel: &str) -> String {
        self.path(rel).display().to_string()
    }
}

/// In-memory files, for tests of code that keeps state in a project.
#[derive(Debug, Default)]
pub struct MemoryFiles {
    files: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
}

impl MemoryFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, rel: &str) -> Option<String> {
        self.files.lock().expect("files lock").get(rel).cloned()
    }

    pub fn insert(&self, rel: &str, text: &str) {
        self.files
            .lock()
            .expect("files lock")
            .insert(rel.to_string(), text.to_string());
    }
}

impl ProjectFiles for MemoryFiles {
    fn read(&self, rel: &str) -> Result<Option<String>> {
        Ok(self.get(rel))
    }

    fn write(&self, rel: &str, text: &str) -> Result<()> {
        self.insert(rel, text);
        Ok(())
    }

    fn describe(&self, rel: &str) -> String {
        format!("memory:{rel}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_files_read_missing_as_none_and_create_folders() {
        let dir = tempfile::tempdir().unwrap();
        let files = LocalFiles::new(dir.path());
        assert_eq!(files.read(".forge/settings.toml").unwrap(), None);
        files.write(".forge/settings.toml", "a = 1\n").unwrap();
        assert_eq!(
            files.read(".forge/settings.toml").unwrap().as_deref(),
            Some("a = 1\n")
        );
        assert!(files.describe(".env").ends_with(".env"));
    }
}
