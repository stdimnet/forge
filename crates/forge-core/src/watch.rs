//! Watches the open project for changes made outside the editor (git checkouts, composer,
//! code generators, other editors) and reports them in debounced batches.
//!
//! Paths under the default excludes (`.git`, `node_modules`, ...), `.forge` and
//! everything matched by the root `.gitignore` / `.git/info/exclude` are dropped. Changes to
//! git's own state (`.git/index`, `HEAD`, refs) are reported as a flag so the VCS view can
//! refresh without polling.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{RecursiveMode, Watcher};
use serde::Serialize;

use crate::workspace::DEFAULT_EXCLUDES;
use crate::{ForgeError, Result};

/// One debounced batch of changes; paths are workspace-relative with `/` separators.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsChanges {
    /// Created or modified files and directories (they exist now).
    pub changed: Vec<String>,
    /// Paths that no longer exist.
    pub removed: Vec<String>,
    /// Git's index, HEAD or refs changed (commit, checkout, stage from a terminal).
    pub git: bool,
}

impl FsChanges {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty() && !self.git
    }
}

/// What a raw file system event path means for the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classified {
    Ignored,
    Git,
    Project(String),
}

/// Decides which paths are reported.
pub struct ChangeFilter {
    root: PathBuf,
    gitignore: Gitignore,
}

const GIT_STATE: &[&str] = &["index", "HEAD", "packed-refs", "MERGE_HEAD", "ORIG_HEAD"];

impl ChangeFilter {
    /// Reads `<root>/.gitignore` and `<root>/.git/info/exclude`.
    pub fn new(root: &Path) -> Self {
        let mut builder = GitignoreBuilder::new(root);
        for file in [root.join(".gitignore"), root.join(".git/info/exclude")] {
            if file.is_file() {
                if let Some(e) = builder.add(&file) {
                    tracing::debug!("ignore file {}: {e}", file.display());
                }
            }
        }
        let gitignore = builder.build().unwrap_or_else(|e| {
            tracing::warn!("invalid .gitignore: {e}");
            Gitignore::empty()
        });
        Self {
            root: root.to_path_buf(),
            gitignore,
        }
    }

    pub fn classify(&self, abs: &Path) -> Classified {
        let Ok(rel) = abs.strip_prefix(&self.root) else {
            return Classified::Ignored;
        };
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let Some(first) = parts.first() else {
            return Classified::Ignored;
        };
        if first == ".git" {
            let is_state = match parts.get(1).map(String::as_str) {
                Some("refs") => {
                    parts.len() > 2 && !parts.last().is_some_and(|p| p.ends_with(".lock"))
                }
                Some(name) => parts.len() == 2 && GIT_STATE.contains(&name),
                None => false,
            };
            return if is_state {
                Classified::Git
            } else {
                Classified::Ignored
            };
        }
        if first == ".forge" || parts.iter().any(|p| DEFAULT_EXCLUDES.contains(&p.as_str())) {
            return Classified::Ignored;
        }
        if self
            .gitignore
            .matched_path_or_any_parents(rel, abs.is_dir())
            .is_ignore()
        {
            return Classified::Ignored;
        }
        Classified::Project(parts.join("/"))
    }

    /// Whether `rel` is an ignore file whose change requires a new filter.
    pub fn is_ignore_file(rel: &str) -> bool {
        rel == ".gitignore"
    }
}

/// Collects classified paths until they are flushed as one [`FsChanges`].
#[derive(Default)]
pub struct Batch {
    paths: BTreeSet<String>,
    git: bool,
}

impl Batch {
    pub fn add(&mut self, c: Classified) {
        match c {
            Classified::Ignored => {}
            Classified::Git => self.git = true,
            Classified::Project(rel) => {
                self.paths.insert(rel);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && !self.git
    }

    /// Split into existing and removed paths (`exists` checks a relative path).
    pub fn take(&mut self, exists: impl Fn(&str) -> bool) -> FsChanges {
        let mut out = FsChanges {
            git: std::mem::take(&mut self.git),
            ..Default::default()
        };
        for p in std::mem::take(&mut self.paths) {
            if exists(&p) {
                out.changed.push(p);
            } else {
                out.removed.push(p);
            }
        }
        out
    }
}

/// Quiet time before a batch is reported, and the longest a busy batch is held back.
pub const DEBOUNCE: Duration = Duration::from_millis(250);
const MAX_DELAY: Duration = Duration::from_millis(1500);

/// A running watcher; dropping it stops watching.
pub struct ProjectWatcher {
    _watcher: notify::RecommendedWatcher,
    root: PathBuf,
}

impl ProjectWatcher {
    /// Watch `root` recursively and call `on_change` (from a background thread) with
    /// every non-empty debounced batch.
    pub fn start(
        root: impl Into<PathBuf>,
        on_change: impl Fn(FsChanges) + Send + 'static,
    ) -> Result<Self> {
        let root: PathBuf = root.into();
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher = notify::recommended_watcher(tx)
            .map_err(|e| ForgeError::Other(format!("cannot watch files: {e}")))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| ForgeError::Other(format!("cannot watch {}: {e}", root.display())))?;
        let thread_root = root.clone();
        std::thread::Builder::new()
            .name("forge-fs-watch".into())
            .spawn(move || run(thread_root, rx, on_change))
            .map_err(|e| ForgeError::io(&root, e))?;
        Ok(Self {
            _watcher: watcher,
            root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn run(
    root: PathBuf,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    on_change: impl Fn(FsChanges),
) {
    let mut filter = ChangeFilter::new(&root);
    let mut batch = Batch::default();
    let mut first_at: Option<Instant> = None;
    loop {
        let timeout = match first_at {
            Some(t) => DEBOUNCE.min(MAX_DELAY.saturating_sub(t.elapsed())),
            None => Duration::from_secs(3600),
        };
        match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => {
                if matches!(event.kind, notify::EventKind::Access(_)) {
                    continue;
                }
                for path in &event.paths {
                    let c = filter.classify(path);
                    if let Classified::Project(rel) = &c {
                        if ChangeFilter::is_ignore_file(rel) {
                            filter = ChangeFilter::new(&root);
                        }
                    }
                    batch.add(c);
                }
                if !batch.is_empty() && first_at.is_none() {
                    first_at = Some(Instant::now());
                }
                if first_at.is_some_and(|t| t.elapsed() < MAX_DELAY) {
                    continue;
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("file watcher: {e}");
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if first_at.take().is_some() && !batch.is_empty() {
            let changes = batch.take(|rel| root.join(rel).exists());
            if !changes.is_empty() {
                on_change(changes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "/vendor/\n*.log\n").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join(".git/refs/heads")).unwrap();
        dir
    }

    #[test]
    fn classifies_paths() {
        let dir = project();
        let root = dir.path();
        let f = ChangeFilter::new(root);
        assert_eq!(
            f.classify(&root.join("src/User.php")),
            Classified::Project("src/User.php".into())
        );
        assert_eq!(
            f.classify(&root.join("vendor/a/b.php")),
            Classified::Ignored
        );
        assert_eq!(
            f.classify(&root.join("storage/app.log")),
            Classified::Ignored
        );
        assert_eq!(
            f.classify(&root.join("node_modules/x.js")),
            Classified::Ignored
        );
        assert_eq!(
            f.classify(&root.join(".forge/cache/x")),
            Classified::Ignored
        );
        assert_eq!(f.classify(&root.join(".git/index")), Classified::Git);
        assert_eq!(
            f.classify(&root.join(".git/refs/heads/main")),
            Classified::Git
        );
        assert_eq!(
            f.classify(&root.join(".git/index.lock")),
            Classified::Ignored
        );
        assert_eq!(
            f.classify(&root.join(".git/objects/ab/cd")),
            Classified::Ignored
        );
        assert_eq!(f.classify(Path::new("/elsewhere/x")), Classified::Ignored);
    }

    #[test]
    fn batches_split_changed_and_removed() {
        let mut b = Batch::default();
        assert!(b.is_empty());
        b.add(Classified::Project("a.php".into()));
        b.add(Classified::Project("a.php".into()));
        b.add(Classified::Project("gone.php".into()));
        b.add(Classified::Ignored);
        b.add(Classified::Git);
        let c = b.take(|p| p == "a.php");
        assert_eq!(c.changed, vec!["a.php"]);
        assert_eq!(c.removed, vec!["gone.php"]);
        assert!(c.git);
        assert!(b.is_empty());
    }

    #[test]
    fn reports_changes_on_disk() {
        let dir = project();
        let root = dir.path().canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = ProjectWatcher::start(&root, move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(root.join("src/New.php"), "<?php").unwrap();
        std::fs::write(root.join("debug.log"), "x").unwrap();
        let mut seen = FsChanges::default();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !seen.changed.iter().any(|p| p == "src/New.php") && Instant::now() < deadline {
            if let Ok(c) = rx.recv_timeout(Duration::from_millis(500)) {
                seen.changed.extend(c.changed);
            }
        }
        assert!(seen.changed.iter().any(|p| p == "src/New.php"), "{seen:?}");
        assert!(!seen.changed.iter().any(|p| p == "debug.log"));
    }
}
