use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::gitignore::IgnoreRules;
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
    /// Matched by `.gitignore` rules; the project tree shows these grayed out.
    #[serde(default)]
    pub ignored: bool,
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
    /// Entries matched by `.gitignore` are flagged `ignored` but still listed.
    pub fn list_dir(&self, rel: &str) -> Result<Vec<FileEntry>> {
        let dir = self.resolve(rel)?;
        let read = fs::read_dir(&dir).map_err(|e| ForgeError::io(&dir, e))?;
        let rules = IgnoreRules::for_dir(&self.root, &dir);
        let mut entries: Vec<FileEntry> = read
            .filter_map(|e| e.ok())
            .filter(|e| !DEFAULT_EXCLUDES.contains(&e.file_name().to_string_lossy().as_ref()))
            .map(|e| {
                let kind = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    FileKind::Dir
                } else {
                    FileKind::File
                };
                let ignored = rules
                    .as_ref()
                    .is_some_and(|r| r.is_ignored(&e.path(), kind == FileKind::Dir));
                FileEntry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path: self.relative(&e.path()),
                    kind,
                    ignored,
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
        self.write_bytes(rel, contents.as_bytes())
    }

    /// The file's raw bytes (for files that are not UTF-8).
    pub fn read_bytes(&self, rel: &str) -> Result<Vec<u8>> {
        let p = self.resolve(rel)?;
        fs::read(&p).map_err(|e| ForgeError::io(&p, e))
    }

    pub fn write_bytes(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        let p = self.resolve(rel)?;
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(|e| ForgeError::io(parent, e))?;
        }
        fs::write(&p, bytes).map_err(|e| ForgeError::io(&p, e))
    }

    /// Resolve a path the project tree may change: not the project root itself.
    fn resolve_entry(&self, rel: &str) -> Result<PathBuf> {
        let p = self.resolve(rel)?;
        if p == self.root {
            return Err(ForgeError::Other(
                "the project root cannot be changed".into(),
            ));
        }
        Ok(p)
    }

    /// Create an empty file (and missing parent folders). Fails if the path exists.
    pub fn create_file(&self, rel: &str) -> Result<()> {
        let p = self.resolve_entry(rel)?;
        if p.exists() {
            return Err(ForgeError::Other(format!("{rel} already exists")));
        }
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(|e| ForgeError::io(parent, e))?;
        }
        fs::File::create_new(&p)
            .map(|_| ())
            .map_err(|e| ForgeError::io(&p, e))
    }

    /// Create a folder (and missing parents). Fails if the path exists.
    pub fn create_dir(&self, rel: &str) -> Result<()> {
        let p = self.resolve_entry(rel)?;
        if p.exists() {
            return Err(ForgeError::Other(format!("{rel} already exists")));
        }
        fs::create_dir_all(&p).map_err(|e| ForgeError::io(&p, e))
    }

    /// Rename or move a file or folder inside the project. Never replaces an existing path.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        let src = self.resolve_entry(from)?;
        let dst = self.resolve_entry(to)?;
        if !src.exists() {
            return Err(ForgeError::NotFound(src));
        }
        if dst.exists() {
            return Err(ForgeError::Other(format!("{to} already exists")));
        }
        if dst.starts_with(&src) {
            return Err(ForgeError::Other(format!("cannot move {from} into itself")));
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| ForgeError::io(parent, e))?;
        }
        fs::rename(&src, &dst).map_err(|e| ForgeError::io(&src, e))
    }

    /// Delete a file, or a folder with everything in it.
    pub fn delete(&self, rel: &str) -> Result<()> {
        let p = self.resolve_entry(rel)?;
        let meta = fs::symlink_metadata(&p).map_err(|e| ForgeError::io(&p, e))?;
        // A symlink to a folder is removed as a link, never followed.
        if meta.is_dir() {
            fs::remove_dir_all(&p).map_err(|e| ForgeError::io(&p, e))
        } else {
            fs::remove_file(&p).map_err(|e| ForgeError::io(&p, e))
        }
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
    fn flags_gitignored_entries() {
        let (dir, ws) = ws();
        fs::write(dir.path().join(".gitignore"), "/vendor\n*.log\n").unwrap();
        fs::create_dir(dir.path().join("vendor")).unwrap();
        fs::write(dir.path().join("vendor/autoload.php"), "<?php").unwrap();
        fs::write(dir.path().join("app.log"), "").unwrap();
        let flags = |rel: &str| -> Vec<(String, bool)> {
            ws.list_dir(rel)
                .unwrap()
                .into_iter()
                .map(|e| (e.name, e.ignored))
                .collect()
        };
        assert_eq!(
            flags(""),
            vec![
                ("src".to_string(), false),
                ("vendor".to_string(), true),
                (".gitignore".to_string(), false),
                ("app.log".to_string(), true),
                ("composer.json".to_string(), false),
            ]
        );
        assert_eq!(flags("vendor"), vec![("autoload.php".to_string(), true)]);
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

    #[test]
    fn creates_files_and_folders_with_missing_parents() {
        let (dir, ws) = ws();
        ws.create_file("src/Models/Order.php").unwrap();
        ws.create_dir("storage/cache").unwrap();
        assert!(dir.path().join("src/Models/Order.php").is_file());
        assert!(dir.path().join("storage/cache").is_dir());
        assert!(ws.create_file("src/User.php").is_err());
        assert!(ws.create_dir("src").is_err());
    }

    #[test]
    fn renames_without_replacing_or_escaping() {
        let (dir, ws) = ws();
        ws.rename("src/User.php", "src/Account.php").unwrap();
        assert!(dir.path().join("src/Account.php").is_file());
        assert!(!dir.path().join("src/User.php").exists());
        ws.create_file("src/Other.php").unwrap();
        assert!(ws.rename("src/Other.php", "src/Account.php").is_err());
        assert!(ws.rename("src", "src/inner").is_err());
        assert!(ws.rename("src/Other.php", "../outside.php").is_err());
        assert!(ws.rename("missing.php", "x.php").is_err());
    }

    #[test]
    fn deletes_files_and_folders_but_not_the_root() {
        let (dir, ws) = ws();
        ws.delete("src/User.php").unwrap();
        assert!(!dir.path().join("src/User.php").exists());
        ws.delete("src").unwrap();
        assert!(!dir.path().join("src").exists());
        assert!(ws.delete("").is_err());
        assert!(ws.delete("../x").is_err());
        assert!(ws.delete("nope").is_err());
    }
}
