//! "Local or mounted folder" servers, and the project side of every transfer.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::compare::Entry;
use crate::fs::ServerFs;
use crate::mapping::{join, normalize};
use crate::{DeployError, Result};

/// A folder on this machine; paths are relative to it.
#[derive(Debug, Clone)]
pub struct LocalFolder {
    root: PathBuf,
}

impl LocalFolder {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn full(&self, path: &str) -> Result<PathBuf> {
        let path = normalize(path);
        if path.split('/').any(|p| p == "..") {
            return Err(DeployError::new(format!(
                "{path} is outside of {}",
                self.root.display()
            )));
        }
        Ok(if path.is_empty() {
            self.root.clone()
        } else {
            self.root.join(path)
        })
    }

    fn io(&self, path: &str, e: std::io::Error) -> DeployError {
        DeployError::context(self.describe(path), e)
    }
}

fn modified(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

fn entry(path: String, meta: &std::fs::Metadata) -> Entry {
    if meta.is_dir() {
        Entry::dir(path, modified(meta))
    } else {
        Entry::file(path, meta.len(), modified(meta))
    }
}

impl ServerFs for LocalFolder {
    fn stat(&mut self, path: &str) -> Result<Option<Entry>> {
        match std::fs::metadata(self.full(path)?) {
            Ok(meta) => Ok(Some(entry(normalize(path), &meta))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(self.io(path, e)),
        }
    }

    fn list(&mut self, dir: &str) -> Result<Vec<Entry>> {
        let dir = normalize(dir);
        let mut out = Vec::new();
        for item in std::fs::read_dir(self.full(&dir)?).map_err(|e| self.io(&dir, e))? {
            let item = item.map_err(|e| self.io(&dir, e))?;
            let name = item.file_name().to_string_lossy().into_owned();
            // Follows symbolic links like a web server does; broken links are skipped.
            if let Ok(meta) = std::fs::metadata(item.path()) {
                out.push(entry(join(&dir, &name), &meta));
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        std::fs::read(self.full(path)?).map_err(|e| self.io(path, e))
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<()> {
        std::fs::write(self.full(path)?, data).map_err(|e| self.io(path, e))
    }

    fn mkdir(&mut self, path: &str) -> Result<()> {
        let full = self.full(path)?;
        match std::fs::create_dir(&full) {
            Ok(()) => Ok(()),
            Err(_) if full.is_dir() => Ok(()),
            Err(e) => Err(self.io(path, e)),
        }
    }

    fn remove_file(&mut self, path: &str) -> Result<()> {
        std::fs::remove_file(self.full(path)?).map_err(|e| self.io(path, e))
    }

    fn remove_dir(&mut self, path: &str) -> Result<()> {
        std::fs::remove_dir(self.full(path)?).map_err(|e| self.io(path, e))
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        std::fs::rename(self.full(from)?, self.full(to)?).map_err(|e| self.io(from, e))
    }

    fn set_modified(&mut self, path: &str, secs: i64) -> Result<bool> {
        let time = filetime::FileTime::from_unix_time(secs, 0);
        filetime::set_file_mtime(self.full(path)?, time).map_err(|e| self.io(path, e))?;
        Ok(true)
    }

    fn describe(&self, path: &str) -> String {
        let path = normalize(path);
        if path.is_empty() {
            self.root.display().to_string()
        } else {
            self.root.join(path).display().to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_operations_stay_below_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let mut fs = LocalFolder::new(dir.path());
        assert_eq!(fs.stat("a").unwrap(), None);
        fs.mkdir_all("a/b").unwrap();
        fs.mkdir("a/b").unwrap();
        fs.write("a/b/c.txt", b"hello").unwrap();
        assert!(fs.set_modified("a/b/c.txt", 1_700_000_000).unwrap());
        let e = fs.stat("/a/b/c.txt").unwrap().unwrap();
        assert_eq!(
            (e.path.as_str(), e.size, e.modified),
            ("a/b/c.txt", 5, Some(1_700_000_000))
        );
        assert_eq!(fs.read("a/b/c.txt").unwrap(), b"hello");
        let listed = fs.list("a").unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].dir && listed[0].path == "a/b");
        assert!(fs.read("../outside").is_err());
        assert!(fs.mkdir_all("a/b/c.txt/d").is_err());
        fs.rename("a/b/c.txt", "a/d.txt").unwrap();
        assert_eq!(fs.read("a/d.txt").unwrap(), b"hello");
        assert!(fs.rename("a/missing", "a/x").is_err());
        fs.remove_all("a").unwrap();
        assert_eq!(fs.stat("a").unwrap(), None);
        assert!(fs.describe("x").ends_with("x"));
    }
}
