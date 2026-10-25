//! Upload, download, Sync with Deployed, delete on the server and reading deployed files.
//! The project is a [`ServerFs`] too (a [`crate::local::LocalFolder`] at the project root),
//! so both directions run the same code: walk the source, map every project path through
//! the server's mappings, skip excluded paths and names, apply the overwrite rules and copy.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::compare::{self, is_newer_on_target, is_up_to_date, Entry};
use crate::config::{DeployServer, ExcludedSide};
use crate::fs::ServerFs;
use crate::mapping::{is_within, join, name, normalize, parent, ExcludeNames, Resolution};
use crate::options::{CompareBy, DeployOptions, WarnNewer};
use crate::{DeployError, Result};

/// Entries read per side at most by Sync with Deployed.
pub const MAX_SYNC_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Transferred,
    /// "Overwrite up-to-date files" is off and the target is up to date.
    UpToDate,
    /// The target is newer ("Warn when uploading over newer file"); not overwritten.
    NewerOnTarget,
    Excluded,
    NotMapped,
    Deleted,
    Failed,
}

/// What happened to one file or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileResult {
    /// Project-relative path.
    pub local: String,
    /// Where it is on the server (root path included), empty when not mapped.
    pub remote: String,
    pub dir: bool,
    pub status: FileStatus,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The result of an operation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub results: Vec<FileResult>,
    pub transferred: usize,
    pub skipped: usize,
    pub deleted: usize,
    pub failed: usize,
    pub bytes: u64,
    /// Project paths not overwritten because the target is newer (ask, then retry with
    /// `overwrite_newer`).
    pub newer_on_target: Vec<String>,
    pub cancelled: bool,
}

impl Summary {
    fn add(&mut self, r: FileResult) {
        match r.status {
            FileStatus::Transferred => {
                self.transferred += 1;
                self.bytes += r.bytes;
            }
            FileStatus::Deleted => self.deleted += 1,
            FileStatus::Failed => self.failed += 1,
            FileStatus::NewerOnTarget => {
                self.skipped += 1;
                self.newer_on_target.push(r.local.clone());
            }
            _ => self.skipped += 1,
        }
        self.results.push(r);
    }
}

/// Sync with Deployed: the folder diff of a project folder (left) and its deployed folder
/// (right).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCompare {
    /// The compared project folder (`""` for the project root).
    pub local: String,
    /// The deployed folder as the server shows it.
    pub remote: String,
    pub compare: forge_diff::DirCompare,
}

/// One operation on a server.
pub struct Deployer<'a> {
    project: &'a mut dyn ServerFs,
    server_fs: &'a mut dyn ServerFs,
    server: &'a DeployServer,
    options: &'a DeployOptions,
    names: ExcludeNames,
    /// Project folders never uploaded (a local server's folder inside the project).
    skip_local: Vec<String>,
    cancel: Option<&'a AtomicBool>,
    progress: Box<dyn FnMut(&FileResult) + 'a>,
    /// Target folders known to exist.
    made: HashSet<String>,
}

impl<'a> Deployer<'a> {
    pub fn new(
        project: &'a mut dyn ServerFs,
        server_fs: &'a mut dyn ServerFs,
        server: &'a DeployServer,
        options: &'a DeployOptions,
    ) -> Self {
        Self {
            project,
            server_fs,
            server,
            options,
            names: ExcludeNames::parse(&options.exclude_names),
            skip_local: Vec::new(),
            cancel: None,
            progress: Box::new(|_| {}),
            made: HashSet::new(),
        }
    }

    pub fn skip_local(mut self, folder: Option<String>) -> Self {
        self.skip_local.extend(folder.map(|f| normalize(&f)));
        self
    }

    pub fn cancel(mut self, flag: &'a AtomicBool) -> Self {
        self.cancel = Some(flag);
        self
    }

    /// Called after every file or folder.
    pub fn on_progress(mut self, f: impl FnMut(&FileResult) + 'a) -> Self {
        self.progress = Box::new(f);
        self
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }

    fn record(&mut self, summary: &mut Summary, r: FileResult) {
        (self.progress)(&r);
        summary.add(r);
    }

    fn result(
        &self,
        local: &str,
        remote: Option<&str>,
        dir: bool,
        status: FileStatus,
    ) -> FileResult {
        FileResult {
            local: local.to_string(),
            remote: remote
                .map(|r| self.server.display_path(r))
                .unwrap_or_default(),
            dir,
            status,
            bytes: 0,
            message: None,
        }
    }

    fn failed(&self, local: &str, remote: Option<&str>, dir: bool, e: DeployError) -> FileResult {
        FileResult {
            message: Some(e.to_string()),
            ..self.result(local, remote, dir, FileStatus::Failed)
        }
    }

    /// Excluded by name or skipped as a deployment folder inside the project.
    fn skipped_locally(&self, local: &str) -> bool {
        self.names.matches_path(local) || self.skip_local.iter().any(|s| is_within(local, s))
    }

    /// Upload or download project paths (files or folders).
    pub fn transfer(
        &mut self,
        direction: Direction,
        paths: &[String],
        overwrite_newer: bool,
    ) -> Summary {
        let mut summary = Summary::default();
        for path in paths {
            if self.cancelled() {
                summary.cancelled = true;
                break;
            }
            let local = normalize(path);
            if self.skipped_locally(&local) {
                let r = self.result(&local, None, false, FileStatus::Excluded);
                self.record(&mut summary, r);
                continue;
            }
            self.copy_entry(direction, &local, overwrite_newer, &mut summary);
        }
        summary.cancelled |= self.cancelled();
        summary
    }

    fn sides(&mut self, direction: Direction) -> (&mut dyn ServerFs, &mut dyn ServerFs) {
        match direction {
            Direction::Upload => (&mut *self.project, &mut *self.server_fs),
            Direction::Download => (&mut *self.server_fs, &mut *self.project),
        }
    }

    fn copy_entry(
        &mut self,
        direction: Direction,
        local: &str,
        overwrite_newer: bool,
        summary: &mut Summary,
    ) {
        let remote = match self.server.resolve(local) {
            Resolution::Mapped(r) => r,
            Resolution::NotMapped => {
                let r = self.result(local, None, false, FileStatus::NotMapped);
                return self.record(summary, r);
            }
            Resolution::Excluded => {
                let r = self.result(local, None, false, FileStatus::Excluded);
                return self.record(summary, r);
            }
        };
        let (src_path, dst_path) = match direction {
            Direction::Upload => (local.to_string(), remote.clone()),
            Direction::Download => (remote.clone(), local.to_string()),
        };
        let (src, _) = self.sides(direction);
        let entry = match src.stat(&src_path) {
            Ok(Some(e)) => e,
            Ok(None) => {
                let e = DeployError::new(format!("{} does not exist", src.describe(&src_path)));
                let r = self.failed(local, Some(&remote), false, e);
                return self.record(summary, r);
            }
            Err(e) => {
                let r = self.failed(local, Some(&remote), false, e);
                return self.record(summary, r);
            }
        };
        if entry.dir {
            self.copy_dir(
                direction,
                local,
                &remote,
                &src_path,
                &dst_path,
                overwrite_newer,
                summary,
            );
        } else {
            let r = self.copy_file(
                direction,
                local,
                &remote,
                &entry,
                &dst_path,
                overwrite_newer,
            );
            self.record(summary, r);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_dir(
        &mut self,
        direction: Direction,
        local: &str,
        remote: &str,
        src_path: &str,
        dst_path: &str,
        overwrite_newer: bool,
        summary: &mut Summary,
    ) {
        let create_empty_dirs = self.options.create_empty_dirs;
        let (src, dst) = self.sides(direction);
        let children = match src.list(src_path) {
            Ok(c) => c,
            Err(e) => {
                let r = self.failed(local, Some(remote), true, e);
                return self.record(summary, r);
            }
        };
        if create_empty_dirs {
            if let Err(e) = dst.mkdir_all(dst_path) {
                let r = self.failed(local, Some(remote), true, e);
                return self.record(summary, r);
            }
            self.made.insert(dst_path.to_string());
        }
        let mut names: HashSet<String> = HashSet::new();
        for child in &children {
            if self.cancelled() {
                return;
            }
            let child_name = name(&child.path).to_string();
            let child_local = join(local, &child_name);
            names.insert(child_name);
            if self.skipped_locally(&child_local) {
                continue;
            }
            match direction {
                // Excluded deployment paths below a downloaded folder stay where they are.
                Direction::Download
                    if self
                        .server
                        .is_excluded(ExcludedSide::Deployment, &child.path) =>
                {
                    continue
                }
                _ => {}
            }
            if self.server.is_excluded(ExcludedSide::Local, &child_local) {
                continue;
            }
            self.copy_entry(direction, &child_local, overwrite_newer, summary);
        }
        if !self.options.delete_target_items {
            return;
        }
        // "Delete target items when source ones do not exist".
        let (_, dst) = self.sides(direction);
        let Ok(existing) = dst.list(dst_path) else {
            return;
        };
        for item in existing {
            let item_name = name(&item.path).to_string();
            if names.contains(&item_name) || self.names.matches(&item_name) {
                continue;
            }
            let (item_local, item_remote) = match direction {
                Direction::Upload => (join(local, &item_name), item.path.clone()),
                Direction::Download => (item.path.clone(), join(remote, &item_name)),
            };
            if self.server.is_excluded(ExcludedSide::Local, &item_local)
                || self
                    .server
                    .is_excluded(ExcludedSide::Deployment, &item_remote)
                || self.skipped_locally(&item_local)
            {
                continue;
            }
            let (_, dst) = self.sides(direction);
            let r = match dst.remove_all(&item.path) {
                Ok(()) => self.result(
                    &item_local,
                    Some(&item_remote),
                    item.dir,
                    FileStatus::Deleted,
                ),
                Err(e) => self.failed(&item_local, Some(&item_remote), item.dir, e),
            };
            self.record(summary, r);
        }
    }

    fn copy_file(
        &mut self,
        direction: Direction,
        local: &str,
        remote: &str,
        source: &Entry,
        dst_path: &str,
        overwrite_newer: bool,
    ) -> FileResult {
        let src_path = source.path.clone();
        let options = self.options;
        let made_parent = self.made.contains(parent(dst_path));
        let (src, dst) = self.sides(direction);
        let outcome = (|| -> Result<FileResult> {
            let target = dst.stat(dst_path)?;
            let mut data: Option<Vec<u8>> = None;
            if let Some(target) = &target {
                if target.dir {
                    return Err(DeployError::new(format!(
                        "{} is a folder",
                        dst.describe(dst_path)
                    )));
                }
                if !options.overwrite_up_to_date && is_up_to_date(source, target) {
                    return Ok(FileResult {
                        local: local.to_string(),
                        remote: String::new(),
                        dir: false,
                        status: FileStatus::UpToDate,
                        bytes: 0,
                        message: None,
                    });
                }
                if !overwrite_newer && options.warn_newer != WarnNewer::No {
                    let by_time =
                        matches!((source.modified, target.modified), (Some(s), Some(t)) if t > s);
                    let differ = if by_time && options.warn_newer == WarnNewer::Content {
                        let bytes = src.read(&src_path)?;
                        let differ = dst.read(dst_path)? != bytes;
                        data = Some(bytes);
                        Some(differ)
                    } else {
                        None
                    };
                    if is_newer_on_target(source, target, options.warn_newer, differ) {
                        return Ok(FileResult {
                            local: local.to_string(),
                            remote: String::new(),
                            dir: false,
                            status: FileStatus::NewerOnTarget,
                            bytes: 0,
                            message: None,
                        });
                    }
                }
            } else if !made_parent {
                dst.mkdir_all(parent(dst_path))?;
            }
            let data = match data {
                Some(d) => d,
                None => src.read(&src_path)?,
            };
            dst.write(dst_path, &data)?;
            if options.preserve_timestamps {
                if let Some(secs) = source.modified {
                    // Best effort: some FTP servers cannot set times.
                    let _ = dst.set_modified(dst_path, secs);
                }
            }
            Ok(FileResult {
                local: local.to_string(),
                remote: String::new(),
                dir: false,
                status: FileStatus::Transferred,
                bytes: data.len() as u64,
                message: None,
            })
        })();
        self.made.insert(parent(dst_path).to_string());
        match outcome {
            Ok(r) => FileResult {
                remote: self.server.display_path(remote),
                ..r
            },
            Err(e) => self.failed(local, Some(remote), false, e),
        }
    }

    /// Delete project paths' deployed files and folders from the server.
    pub fn delete_remote(&mut self, paths: &[String]) -> Summary {
        let mut summary = Summary::default();
        for path in paths {
            let local = normalize(path);
            let r = match self.server.resolve(&local) {
                Resolution::Mapped(remote) if remote.is_empty() => self.failed(
                    &local,
                    Some(&remote),
                    true,
                    DeployError::new("The root path itself is never deleted"),
                ),
                Resolution::Mapped(remote) => {
                    let dir = matches!(self.server_fs.stat(&remote), Ok(Some(e)) if e.dir);
                    match self.server_fs.remove_all(&remote) {
                        Ok(()) => self.result(&local, Some(&remote), dir, FileStatus::Deleted),
                        Err(e) => self.failed(&local, Some(&remote), dir, e),
                    }
                }
                Resolution::NotMapped => self.result(&local, None, false, FileStatus::NotMapped),
                Resolution::Excluded => self.result(&local, None, false, FileStatus::Excluded),
            };
            self.record(&mut summary, r);
        }
        summary
    }

    /// The deployed version of a project file (`None` when it is not on the server) and
    /// where it is.
    pub fn read_remote(&mut self, local: &str) -> Result<(Option<Vec<u8>>, String)> {
        let local = normalize(local);
        let remote = match self.server.resolve(&local) {
            Resolution::Mapped(r) => r,
            Resolution::NotMapped => {
                return Err(DeployError::new(format!(
                    "{local} is not mapped to {}",
                    self.server.label()
                )))
            }
            Resolution::Excluded => {
                return Err(DeployError::new(format!(
                    "{local} is excluded from {}",
                    self.server.label()
                )))
            }
        };
        let display = self.server.display_path(&remote);
        match self.server_fs.stat(&remote)? {
            None => Ok((None, display)),
            Some(e) if e.dir => Err(DeployError::new(format!("{display} is a folder"))),
            Some(_) => Ok((Some(self.server_fs.read(&remote)?), display)),
        }
    }

    /// Sync with Deployed: compare a project folder with its deployed folder.
    pub fn sync_compare(&mut self, local_dir: &str) -> Result<SyncCompare> {
        let local_dir = normalize(local_dir);
        let remote_dir = match self.server.resolve(&local_dir) {
            Resolution::Mapped(r) => r,
            _ => {
                return Err(DeployError::new(format!(
                    "{} is not mapped to {}",
                    if local_dir.is_empty() {
                        "The project root"
                    } else {
                        &local_dir
                    },
                    self.server.label()
                )))
            }
        };
        let mut truncated = false;
        let mut local_entries = Vec::new();
        let server = self.server;
        let names = self.names.clone();
        let skip = self.skip_local.clone();
        let keep_local = |rel: &str| {
            let full = join(&local_dir, rel);
            !names.matches(name(rel))
                && !server.is_excluded(ExcludedSide::Local, &full)
                && !skip.iter().any(|s| is_within(&full, s))
        };
        walk(
            &mut *self.project,
            &local_dir,
            "",
            &keep_local,
            &mut local_entries,
            &mut truncated,
        )?;
        let mut remote_entries = Vec::new();
        if self.server_fs.stat(&remote_dir)?.is_some() {
            let keep_remote = |rel: &str| {
                !names.matches(name(rel))
                    && !server.is_excluded(ExcludedSide::Deployment, &join(&remote_dir, rel))
            };
            walk(
                &mut *self.server_fs,
                &remote_dir,
                "",
                &keep_remote,
                &mut remote_entries,
                &mut truncated,
            )?;
        }
        if self.options.compare_by == CompareBy::Content {
            let sizes: std::collections::HashMap<&str, u64> = remote_entries
                .iter()
                .filter(|e| !e.dir)
                .map(|e| (e.path.as_str(), e.size))
                .collect();
            let both: HashSet<String> = local_entries
                .iter()
                .filter(|e| !e.dir && sizes.get(e.path.as_str()) == Some(&e.size))
                .map(|e| e.path.clone())
                .collect();
            for e in local_entries.iter_mut().filter(|e| both.contains(&e.path)) {
                e.digest = Some(forge_diff::digest(
                    &self.project.read(&join(&local_dir, &e.path))?,
                ));
            }
            for e in remote_entries.iter_mut().filter(|e| both.contains(&e.path)) {
                e.digest = Some(forge_diff::digest(
                    &self.server_fs.read(&join(&remote_dir, &e.path))?,
                ));
            }
        }
        let mut result = compare::compare(&local_entries, &remote_entries, self.options.compare_by);
        result.truncated |= truncated;
        Ok(SyncCompare {
            local: local_dir,
            remote: self.server.display_path(&remote_dir),
            compare: result,
        })
    }
}

/// Every entry below `base/rel`, paths relative to `base`.
fn walk(
    fs: &mut dyn ServerFs,
    base: &str,
    rel: &str,
    keep: &dyn Fn(&str) -> bool,
    out: &mut Vec<Entry>,
    truncated: &mut bool,
) -> Result<()> {
    for child in fs.list(&join(base, rel))? {
        if out.len() >= MAX_SYNC_ENTRIES {
            *truncated = true;
            return Ok(());
        }
        let child_rel = join(rel, name(&child.path));
        if !keep(&child_rel) {
            continue;
        }
        let dir = child.dir;
        out.push(Entry {
            path: child_rel.clone(),
            ..child
        });
        if dir {
            walk(fs, base, &child_rel, keep, out, truncated)?;
        }
    }
    Ok(())
}
