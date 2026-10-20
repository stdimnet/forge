use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ForgeError, Result};

/// Directory names hidden from the project tree by default (like PHPStorm's "Excluded").
pub(crate) const DEFAULT_EXCLUDES: &[&str] =
    &[".git", "node_modules", "target", ".idea", ".DS_Store"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    File,
    Dir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    /// Path relative to the workspace root, always with `/` separators.
    pub path: String,
    pub kind: FileKind,
}

/// An opened project root. All paths handed in from the UI are resolved against it
/// and rejected if they escape it.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let root = root.canonicalize().map_err(|e| ForgeError::io(root, e))?;
        if !root.is_dir() {
            return Err(ForgeError::NotFound(root));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.display().to_string())
    }

    /// Resolve a workspace-relative path, refusing `..` escapes and absolute paths.
    pub fn resolve(&self, rel: &str) -> Result<PathBuf> {
        let rel_path = Path::new(rel);
        let mut out = self.root.clone();
        for c in rel_path.components() {
            match c {
                Component::Normal(p) => out.push(p),
                Component::CurDir => {}
                _ => return Err(ForgeError::OutsideWorkspace(rel_path.to_path_buf())),
            }
        }
        Ok(out)
    }

    pub fn relative(&self, abs: &Path) -> String {
        abs.strip_prefix(&self.root)
            .unwrap_or(abs)
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// List one directory level: folders first, then files, both alphabetical (case-insensitive).
    pub fn list_dir(&self, rel: &str) -> Result<Vec<FileEntry>> {
        let dir = self.resolve(rel)?;
        let read = fs::read_dir(&dir).map_err(|e| ForgeError::io(&dir, e))?;
        let mut entries: Vec<FileEntry> = read
            .filter_map(|e| e.ok())
            .filter(|e| !DEFAULT_EXCLUDES.contains(&e.file_name().to_string_lossy().as_ref()))
            .map(|e| {
                let kind = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    FileKind::Dir
                } else {
                    FileKind::File
                };
                FileEntry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path: self.relative(&e.path()),
                    kind,
                }
            })
            .collect();
        entries.sort_by(|a, b| {
            (a.kind != FileKind::Dir, a.name.to_lowercase())
                .cmp(&(b.kind != FileKind::Dir, b.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub fn read_file(&self, rel: &str) -> Result<String> {
        let p = self.resolve(rel)?;
        fs::read_to_string(&p).map_err(|e| ForgeError::io(&p, e))
    }

    pub fn write_file(&self, rel: &str, contents: &str) -> Result<()> {
        let p = self.resolve(rel)?;
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(|e| ForgeError::io(parent, e))?;
        }
        fs::write(&p, contents).map_err(|e| ForgeError::io(&p, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join("composer.json"), "{}").unwrap();
        fs::write(dir.path().join("src/User.php"), "<?php").unwrap();
        let ws = Workspace::open(dir.path()).unwrap();
        (dir, ws)
    }

    #[test]
    fn lists_dirs_first_and_hides_excluded() {
        let (_d, ws) = ws();
        let names: Vec<_> = ws
            .list_dir("")
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["src", "composer.json"]);
    }

    #[test]
    fn rejects_escaping_paths() {
        let (_d, ws) = ws();
        assert!(matches!(
            ws.resolve("../etc/passwd"),
            Err(ForgeError::OutsideWorkspace(_))
        ));
        assert!(ws.resolve("/etc/passwd").is_err());
    }

    #[test]
    fn read_write_roundtrip() {
        let (_d, ws) = ws();
        ws.write_file("src/new/Thing.php", "<?php // hi").unwrap();
        assert_eq!(ws.read_file("src/new/Thing.php").unwrap(), "<?php // hi");
        assert_eq!(ws.list_dir("src").unwrap()[0].path, "src/new");
    }
}
