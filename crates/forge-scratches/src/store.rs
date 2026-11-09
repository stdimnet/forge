//! Scratch files on disk. PHPStorm keeps scratches per user, not per project; Forge stores
//! them in `<config dir>/forge/scratches/` and per-scratch settings (the data source of a
//! SQL scratch) in `<config dir>/forge/scratches.json`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::naming::{self, validate_name};
use crate::run::{runner_for, Runner};
use crate::{Error, Result};

/// A scratch as listed in the Project view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScratchFile {
    /// File name, also the scratch's id.
    pub name: String,
    /// Absolute path on disk.
    pub path: String,
    pub size: u64,
    /// Modification time in milliseconds since the Unix epoch.
    pub modified: u64,
    /// How the scratch runs, if it can.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<Runner>,
    /// Data source attached to a SQL scratch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    #[serde(default)]
    scratches: BTreeMap<String, ScratchMeta>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScratchMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data_source: Option<String>,
}

/// Initial content of new scratches: PHP scratches start in PHP mode, like PHPStorm's.
pub fn template_for(extension: &str) -> &'static str {
    match extension {
        "php" => "<?php\n\n",
        _ => "",
    }
}

/// The scratches of one user.
#[derive(Debug, Clone)]
pub struct ScratchStore {
    dir: PathBuf,
    meta: PathBuf,
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.display().to_string(),
        source,
    }
}

impl ScratchStore {
    /// Store under `forge_config` (`<config dir>/forge`).
    pub fn new(forge_config: &Path) -> Self {
        Self {
            dir: forge_config.join("scratches"),
            meta: forge_config.join("scratches.json"),
        }
    }

    /// `<config dir>/forge` of the current user.
    pub fn user_dir(config_dir: &Path) -> PathBuf {
        config_dir.join("forge")
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Absolute path of an existing or new scratch; rejects names that would leave the
    /// scratches directory.
    pub fn path_of(&self, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        Ok(self.dir.join(name))
    }

    fn existing(&self, name: &str) -> Result<PathBuf> {
        let path = self.path_of(name)?;
        if !path.is_file() {
            return Err(Error::NotFound(name.into()));
        }
        Ok(path)
    }

    fn names(&self) -> Result<Vec<(String, u64)>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(io(&self.dir)(e)),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            out.push((name, millis(&meta)));
        }
        Ok(out)
    }

    /// All scratches, sorted by name (case-insensitive).
    pub fn list(&self) -> Result<Vec<ScratchFile>> {
        let meta = self.read_meta();
        let mut out: Vec<ScratchFile> = self
            .names()?
            .into_iter()
            .filter_map(|(name, _)| self.describe(&name, &meta).ok())
            .collect();
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.name.cmp(&b.name))
        });
        Ok(out)
    }

    /// One scratch.
    pub fn get(&self, name: &str) -> Result<ScratchFile> {
        self.existing(name)?;
        self.describe(name, &self.read_meta())
    }

    fn describe(&self, name: &str, meta: &Meta) -> Result<ScratchFile> {
        let path = self.dir.join(name);
        let m = fs::metadata(&path).map_err(io(&path))?;
        Ok(ScratchFile {
            name: name.to_string(),
            path: path.display().to_string(),
            size: m.len(),
            modified: millis(&m),
            runner: runner_for(name),
            data_source: meta.scratches.get(name).and_then(|s| s.data_source.clone()),
        })
    }

    /// Create a scratch with the next free name for `extension` (`scratch_2.php`). Without
    /// `content` it gets the language's template.
    pub fn create(&self, extension: &str, content: Option<&str>) -> Result<ScratchFile> {
        let existing: Vec<String> = self.names()?.into_iter().map(|(n, _)| n).collect();
        let name = naming::next_name(&existing, naming::base_name(extension), extension);
        self.write_new(&name, content.unwrap_or(template_for(extension)))
    }

    /// Create a scratch with an explicit name; fails when it exists.
    pub fn create_named(&self, name: &str, content: &str) -> Result<ScratchFile> {
        if self.path_of(name)?.exists() {
            return Err(Error::Exists(name.into()));
        }
        self.write_new(name, content)
    }

    /// "New Scratch Buffer": the next `bufferN.txt`, or the oldest buffer emptied.
    pub fn create_buffer(&self) -> Result<ScratchFile> {
        let (name, _reused) = naming::next_buffer(&self.names()?);
        self.write_new(&name, "")
    }

    fn write_new(&self, name: &str, content: &str) -> Result<ScratchFile> {
        fs::create_dir_all(&self.dir).map_err(io(&self.dir))?;
        let path = self.path_of(name)?;
        fs::write(&path, content).map_err(io(&path))?;
        self.get(name)
    }

    pub fn read(&self, name: &str) -> Result<String> {
        let path = self.existing(name)?;
        fs::read_to_string(&path).map_err(io(&path))
    }

    /// Save a scratch (created when missing).
    pub fn write(&self, name: &str, content: &str) -> Result<()> {
        fs::create_dir_all(&self.dir).map_err(io(&self.dir))?;
        let path = self.path_of(name)?;
        fs::write(&path, content).map_err(io(&path))
    }

    /// Rename a scratch; its settings move with it.
    pub fn rename(&self, name: &str, new_name: &str) -> Result<ScratchFile> {
        let from = self.existing(name)?;
        let to = self.path_of(new_name)?;
        if name == new_name {
            return self.get(name);
        }
        // A case-only rename on a case-insensitive file system "exists" already (as the file
        // itself). On a case-sensitive one a file listed under exactly that name is another
        // scratch, which the rename must not overwrite.
        let other = !name.eq_ignore_ascii_case(new_name)
            || self.names()?.iter().any(|(n, _)| n == new_name);
        if to.exists() && other {
            return Err(Error::Exists(new_name.into()));
        }
        fs::rename(&from, &to).map_err(io(&from))?;
        let mut meta = self.read_meta();
        if let Some(m) = meta.scratches.remove(name) {
            meta.scratches.insert(new_name.into(), m);
            self.write_meta(&meta)?;
        }
        self.get(new_name)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.existing(name)?;
        fs::remove_file(&path).map_err(io(&path))?;
        let mut meta = self.read_meta();
        if meta.scratches.remove(name).is_some() {
            self.write_meta(&meta)?;
        }
        Ok(())
    }

    /// Attach (or with `None` detach) a data source to a SQL scratch.
    pub fn set_data_source(&self, name: &str, data_source: Option<&str>) -> Result<ScratchFile> {
        self.existing(name)?;
        let mut meta = self.read_meta();
        let entry = meta.scratches.entry(name.into()).or_default();
        entry.data_source = data_source.filter(|d| !d.is_empty()).map(str::to_string);
        if entry.data_source.is_none() {
            meta.scratches.remove(name);
        }
        self.write_meta(&meta)?;
        self.get(name)
    }

    fn read_meta(&self) -> Meta {
        fs::read_to_string(&self.meta)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn write_meta(&self, meta: &Meta) -> Result<()> {
        if let Some(parent) = self.meta.parent() {
            fs::create_dir_all(parent).map_err(io(parent))?;
        }
        let text = serde_json::to_string_pretty(meta).expect("meta serializes");
        fs::write(&self.meta, text).map_err(io(&self.meta))
    }
}

fn millis(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ScratchStore) {
        let tmp = tempfile::tempdir().unwrap();
        let s = ScratchStore::new(&tmp.path().join("forge"));
        (tmp, s)
    }

    fn names(s: &ScratchStore) -> Vec<String> {
        s.list().unwrap().into_iter().map(|f| f.name).collect()
    }

    #[test]
    fn creates_numbered_scratches_with_templates() {
        let (_tmp, s) = store();
        assert!(s.list().unwrap().is_empty());
        let a = s.create("php", None).unwrap();
        assert_eq!(a.name, "scratch.php");
        assert_eq!(a.runner, Some(Runner::Php));
        assert_eq!(s.read("scratch.php").unwrap(), "<?php\n\n");
        assert_eq!(
            s.create("php", Some("<?php echo 1;")).unwrap().name,
            "scratch_1.php"
        );
        assert_eq!(s.read("scratch_1.php").unwrap(), "<?php echo 1;");
        assert_eq!(s.create("http", None).unwrap().name, "rest-api.http");
        assert_eq!(s.create("js", None).unwrap().name, "scratch.js");
        assert_eq!(
            names(&s),
            [
                "rest-api.http",
                "scratch.js",
                "scratch.php",
                "scratch_1.php"
            ]
        );
        assert!(Path::new(&a.path).starts_with(s.dir()));
    }

    #[test]
    fn buffers() {
        let (_tmp, s) = store();
        assert_eq!(s.create_buffer().unwrap().name, "buffer1.txt");
        s.write("buffer1.txt", "notes").unwrap();
        assert_eq!(s.create_buffer().unwrap().name, "buffer2.txt");
        // The existing buffer keeps its text.
        assert_eq!(s.read("buffer1.txt").unwrap(), "notes");
    }

    #[test]
    fn rename_delete_and_data_sources() {
        let (_tmp, s) = store();
        s.create("sql", Some("select 1;")).unwrap();
        s.create("php", None).unwrap();
        let f = s.set_data_source("scratch.sql", Some("shop")).unwrap();
        assert_eq!(f.data_source.as_deref(), Some("shop"));

        let renamed = s.rename("scratch.sql", "orders.sql").unwrap();
        assert_eq!(renamed.data_source.as_deref(), Some("shop"));
        assert_eq!(s.read("orders.sql").unwrap(), "select 1;");
        assert!(matches!(s.read("scratch.sql"), Err(Error::NotFound(_))));
        assert!(matches!(
            s.rename("orders.sql", "scratch.php"),
            Err(Error::Exists(_))
        ));
        assert!(matches!(
            s.rename("orders.sql", "../evil.sql"),
            Err(Error::Invalid(_))
        ));

        s.delete("orders.sql").unwrap();
        assert_eq!(names(&s), ["scratch.php"]);
        // Its settings are gone: a new scratch with that name starts detached.
        s.create_named("orders.sql", "").unwrap();
        assert_eq!(s.get("orders.sql").unwrap().data_source, None);
        assert!(matches!(s.delete("missing.sql"), Err(Error::NotFound(_))));
    }

    #[test]
    fn case_only_renames_keep_other_scratches() {
        let (_tmp, s) = store();
        s.create_named("notes.txt", "mine").unwrap();
        // Fails on a case-insensitive file system, where both are one file.
        if s.create_named("Notes.txt", "other").is_ok() {
            assert!(matches!(
                s.rename("notes.txt", "Notes.txt"),
                Err(Error::Exists(_))
            ));
            assert_eq!(s.read("Notes.txt").unwrap(), "other");
        }
        // A case-only rename of a scratch without such a sibling works everywhere.
        s.create_named("todo.txt", "x").unwrap();
        assert_eq!(s.rename("todo.txt", "TODO.txt").unwrap().name, "TODO.txt");
        assert_eq!(s.read("TODO.txt").unwrap(), "x");
    }

    #[test]
    fn names_cannot_escape_the_directory() {
        let (_tmp, s) = store();
        for bad in ["../x.php", "/etc/passwd", "a/b.php", ".."] {
            assert!(s.write(bad, "x").is_err(), "{bad}");
            assert!(s.read(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ignores_directories_in_the_scratch_dir() {
        let (_tmp, s) = store();
        fs::create_dir_all(s.dir().join("sub")).unwrap();
        s.write("a.txt", "").unwrap();
        assert_eq!(names(&s), ["a.txt"]);
    }
}
