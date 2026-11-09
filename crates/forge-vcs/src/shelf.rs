//! The shelf: PHPStorm's VCS-independent store of set-aside changes. Every shelved changelist
//! is a folder in Forge's per-project data directory (PHPStorm uses `.idea/shelf`; Forge
//! writes nothing into the project):
//!
//! ```text
//! <shelf dir>/<id>/changes.json    name, date, branch and files
//! <shelf dir>/<id>/shelved.patch   `git diff --binary --full-index` of the files
//! ```
//!
//! Paths in the store are repository-relative; [`crate::Repo`] converts them for the UI. The
//! patch is stored as raw bytes, so files in other encodings than UTF-8 come back unchanged.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};

use crate::patch::{split_patch_bytes, PatchStatus};

/// One shelved file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelvedFile {
    pub path: String,
    pub status: PatchStatus,
    #[serde(default)]
    pub binary: bool,
}

/// A shelved changelist ("Shelf" tab of the Commit tool window).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelvedChangelist {
    pub id: String,
    /// The description shown in the Shelf tab.
    pub name: String,
    /// Seconds since the Unix epoch.
    pub date: i64,
    /// Branch the changes were shelved on.
    #[serde(default)]
    pub branch: Option<String>,
    pub files: Vec<ShelvedFile>,
}

const META: &str = "changes.json";
const PATCH: &str = "shelved.patch";

/// The shelf of one project.
#[derive(Debug, Clone)]
pub struct Shelf {
    dir: PathBuf,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn check_id(id: &str) -> Result<&str> {
    if !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(id)
    } else {
        Err(ForgeError::Other(format!("invalid shelf id {id:?}")))
    }
}

/// The files a patch changes.
pub fn shelved_files(patch: &[u8]) -> Vec<ShelvedFile> {
    split_patch_bytes(patch)
        .into_iter()
        .map(|(f, _)| ShelvedFile {
            path: f.path,
            status: f.status,
            binary: f.binary,
        })
        .collect()
}

impl Shelf {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn folder(&self, id: &str) -> Result<PathBuf> {
        Ok(self.dir.join(check_id(id)?))
    }

    /// Shelved changelists, newest first. Unreadable entries are skipped.
    pub fn list(&self) -> Vec<ShelvedChangelist> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<ShelvedChangelist> = entries
            .flatten()
            .filter_map(|e| fs::read_to_string(e.path().join(META)).ok())
            .filter_map(|t| serde_json::from_str(&t).ok())
            .collect();
        out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.id.cmp(&a.id)));
        out
    }

    pub fn get(&self, id: &str) -> Result<ShelvedChangelist> {
        let file = self.folder(id)?.join(META);
        let text = fs::read_to_string(&file).map_err(|e| ForgeError::io(&file, e))?;
        serde_json::from_str(&text)
            .map_err(|e| ForgeError::Other(format!("{}: {e}", file.display())))
    }

    /// The stored patch as text, for showing it (bytes that are not UTF-8 become U+FFFD).
    pub fn patch(&self, id: &str) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.patch_bytes(id)?).into_owned())
    }

    /// The stored patch exactly as git printed it; use this to apply it.
    pub fn patch_bytes(&self, id: &str) -> Result<Vec<u8>> {
        let file = self.folder(id)?.join(PATCH);
        fs::read(&file).map_err(|e| ForgeError::io(&file, e))
    }

    fn write(&self, cl: &ShelvedChangelist, patch: Option<&[u8]>) -> Result<()> {
        let folder = self.folder(&cl.id)?;
        fs::create_dir_all(&folder).map_err(|e| ForgeError::io(&folder, e))?;
        if let Some(patch) = patch {
            let file = folder.join(PATCH);
            fs::write(&file, patch).map_err(|e| ForgeError::io(&file, e))?;
        }
        let file = folder.join(META);
        let json =
            serde_json::to_string_pretty(cl).map_err(|e| ForgeError::Other(e.to_string()))?;
        fs::write(&file, json).map_err(|e| ForgeError::io(&file, e))
    }

    /// Store a patch as a new shelved changelist.
    pub fn create(
        &self,
        name: &str,
        branch: Option<&str>,
        patch: &[u8],
    ) -> Result<ShelvedChangelist> {
        let files = shelved_files(patch);
        if files.is_empty() {
            return Err(ForgeError::Other("no changes to shelve".into()));
        }
        let date = now();
        let mut n = 0;
        let id = loop {
            let id = if n == 0 {
                format!("{date}")
            } else {
                format!("{date}-{n}")
            };
            if !self.dir.join(&id).exists() {
                break id;
            }
            n += 1;
        };
        let name = name.trim();
        let cl = ShelvedChangelist {
            id,
            name: if name.is_empty() {
                "Changes".into()
            } else {
                name.into()
            },
            date,
            branch: branch.map(str::to_string),
            files,
        };
        self.write(&cl, Some(patch))?;
        Ok(cl)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<ShelvedChangelist> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ForgeError::Other("the name must not be empty".into()));
        }
        let mut cl = self.get(id)?;
        cl.name = name.into();
        self.write(&cl, None)?;
        Ok(cl)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let folder = self.folder(id)?;
        match fs::remove_dir_all(&folder) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(ForgeError::io(&folder, e)),
        }
    }

    /// Drop files (repository paths) from a changelist, e.g. after they were unshelved.
    /// Deletes the changelist when nothing is left; returns what remains.
    pub fn remove_files(&self, id: &str, paths: &[String]) -> Result<Option<ShelvedChangelist>> {
        let mut cl = self.get(id)?;
        let rest: Vec<u8> = split_patch_bytes(&self.patch_bytes(id)?)
            .into_iter()
            .filter(|(f, _)| !paths.contains(&f.path))
            .flat_map(|(_, raw)| raw)
            .collect();
        if rest.is_empty() {
            self.delete(id)?;
            return Ok(None);
        }
        let patch = rest;
        cl.files = shelved_files(&patch);
        self.write(&cl, Some(&patch))?;
        Ok(Some(cl))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/a.txt b/a.txt\nindex 1111111..2222222 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/n.txt b/n.txt\nnew file mode 100644\nindex 0000000..3333333\n--- /dev/null\n+++ b/n.txt\n@@ -0,0 +1 @@\n+new\n";

    #[test]
    fn stores_renames_and_deletes_changelists() {
        let tmp = tempfile::TempDir::new().unwrap();
        let shelf = Shelf::new(tmp.path().join("shelf"));
        assert!(shelf.list().is_empty());
        let first = shelf.create("  ", Some("main"), PATCH.as_bytes()).unwrap();
        let second = shelf.create("Second", None, PATCH.as_bytes()).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.name, "Changes");
        assert_eq!(first.files.len(), 2);
        assert_eq!(first.files[1].status, PatchStatus::Added);
        assert_eq!(shelf.list().len(), 2);
        assert_eq!(shelf.patch(&first.id).unwrap(), PATCH);

        assert_eq!(
            shelf.rename(&first.id, "Invoice work").unwrap().name,
            "Invoice work"
        );
        assert!(shelf.rename(&first.id, " ").is_err());

        let rest = shelf
            .remove_files(&first.id, &["a.txt".into()])
            .unwrap()
            .unwrap();
        assert_eq!(rest.files.len(), 1);
        assert!(shelf
            .patch(&first.id)
            .unwrap()
            .starts_with("diff --git a/n.txt"));
        assert!(shelf
            .remove_files(&first.id, &["n.txt".into()])
            .unwrap()
            .is_none());
        assert!(shelf.get(&first.id).is_err());

        shelf.delete(&second.id).unwrap();
        assert!(shelf.list().is_empty());
        assert!(shelf.get("../escape").is_err());
        assert!(shelf.create("x", None, b"").is_err());
    }
}
