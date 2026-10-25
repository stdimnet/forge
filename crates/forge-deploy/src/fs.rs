//! The file operations of a deployment server. Paths are relative to the server's root
//! path (normalized, `""` is the root); each server type joins them with its root. The
//! project side of a transfer is a [`crate::local::LocalFolder`] at the project root, so
//! uploads and downloads are the same code in both directions.

use crate::compare::Entry;
use crate::Result;

pub trait ServerFs: Send {
    /// The file or folder at `path`, `None` when it does not exist.
    fn stat(&mut self, path: &str) -> Result<Option<Entry>>;
    /// The entries of a folder (their `path`s include `dir`).
    fn list(&mut self, dir: &str) -> Result<Vec<Entry>>;
    fn read(&mut self, path: &str) -> Result<Vec<u8>>;
    /// Create or replace a file; its folder exists.
    fn write(&mut self, path: &str, data: &[u8]) -> Result<()>;
    /// Create one folder (its parent exists); an existing folder is fine.
    fn mkdir(&mut self, path: &str) -> Result<()>;
    fn remove_file(&mut self, path: &str) -> Result<()>;
    /// Remove an empty folder.
    fn remove_dir(&mut self, path: &str) -> Result<()>;
    /// Set the modification time; `Ok(false)` when the server cannot.
    fn set_modified(&mut self, path: &str, secs: i64) -> Result<bool>;
    /// Where `path` is, for messages (`sftp://deploy@shop/var/www/x`).
    fn describe(&self, path: &str) -> String;

    /// Create `path` and its missing parents.
    fn mkdir_all(&mut self, path: &str) -> Result<()> {
        if path.is_empty() {
            return Ok(());
        }
        match self.stat(path)? {
            Some(e) if e.dir => Ok(()),
            Some(_) => Err(crate::DeployError::new(format!(
                "{} is a file, not a folder",
                self.describe(path)
            ))),
            None => {
                self.mkdir_all(crate::mapping::parent(path))?;
                self.mkdir(path)
            }
        }
    }

    /// Remove a file, or a folder with everything in it.
    fn remove_all(&mut self, path: &str) -> Result<()> {
        match self.stat(path)? {
            None => Ok(()),
            Some(e) if !e.dir => self.remove_file(path),
            Some(_) => {
                for child in self.list(path)? {
                    if child.dir {
                        self.remove_all(&child.path)?;
                    } else {
                        self.remove_file(&child.path)?;
                    }
                }
                self.remove_dir(path)
            }
        }
    }
}
