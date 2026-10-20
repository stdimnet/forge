//! A project folder on this machine.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use forge_core::{FileEntry, FileMatch, FsChanges, ProjectWatcher, Result, Workspace};
use forge_lsp::{LocalLauncher, ServerLauncher};
use forge_process::{CommandLine, PtyOptions, PtySession, PtySink, RunSink, RunningProcess};
use forge_project::ProjectInfo;
use forge_vcs::{GitExec, LocalGit, Repo};

use crate::php::LocalPhp;
use crate::{
    folder_name, Backend, BackendInfo, FsSink, PhpBackend, Process, Terminal, TerminalSpec,
    WatchGuard,
};

/// Serves a local folder: the desktop app's own projects, and the Forge agent on a remote
/// host.
pub struct LocalBackend {
    info: BackendInfo,
    workspace: Workspace,
    php: Arc<LocalPhp>,
}

impl LocalBackend {
    /// Open `root` (must be an existing directory; symlinks are resolved).
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let workspace = Workspace::open(root)?;
        let root = workspace.root().to_path_buf();
        Ok(Self {
            info: BackendInfo {
                name: folder_name(&root),
                root,
                remote: None,
            },
            php: Arc::new(LocalPhp::new(workspace.clone())),
            workspace,
        })
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn root(&self) -> &Path {
        self.workspace.root()
    }
}

/// Unmark entries that are tracked: a file listed in `tracked`, or a folder with tracked
/// files below it. Ignoring only applies to untracked content.
pub fn clear_tracked(entries: &mut [FileEntry], tracked: &[String]) {
    for e in entries.iter_mut().filter(|e| e.ignored) {
        let prefix = format!("{}/", e.path);
        e.ignored = !tracked
            .iter()
            .any(|t| t == &e.path || t.starts_with(&prefix));
    }
}

struct LocalTerminal {
    session: PtySession,
    cwd: PathBuf,
}

impl Terminal for LocalTerminal {
    fn shell(&self) -> String {
        self.session.shell().to_string()
    }
    fn cwd(&self) -> PathBuf {
        self.cwd.clone()
    }
    fn write(&self, data: &[u8]) -> Result<()> {
        self.session.write(data)
    }
    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.session.resize(cols, rows)
    }
    fn kill(&self) -> Result<()> {
        self.session.kill()
    }
}

impl Process for RunningProcess {
    fn write_stdin(&self, data: &[u8]) -> Result<()> {
        RunningProcess::write_stdin(self, data)
    }
    fn kill(&self, grace: Duration) {
        RunningProcess::kill(self, grace)
    }
}

impl Backend for LocalBackend {
    fn info(&self) -> &BackendInfo {
        &self.info
    }

    fn resolve(&self, rel: &str) -> Result<PathBuf> {
        self.workspace.resolve(rel)
    }

    fn list_dir(&self, rel: &str) -> Result<Vec<FileEntry>> {
        let mut entries = self.workspace.list_dir(rel)?;
        let candidates: Vec<String> = entries
            .iter()
            .filter(|e| e.ignored)
            .map(|e| e.path.clone())
            .collect();
        if !candidates.is_empty() {
            // Files git tracks keep their VCS status even when a pattern matches them, so
            // they must not be grayed out (`git add -f`, files ignored after being committed).
            let tracked = Repo::open(self.root())
                .and_then(|r| r.tracked_among(&candidates))
                .unwrap_or_default();
            clear_tracked(&mut entries, &tracked);
        }
        Ok(entries)
    }

    fn read_file(&self, rel: &str) -> Result<String> {
        self.workspace.read_file(rel)
    }

    fn write_file(&self, rel: &str, contents: &str) -> Result<()> {
        self.workspace.write_file(rel, contents)
    }

    fn find_files(&self, query: &str, limit: usize) -> Result<Vec<FileMatch>> {
        self.workspace.find_files(query, limit)
    }

    fn git(&self) -> Arc<dyn GitExec> {
        Arc::new(LocalGit)
    }

    fn detect_project(&self) -> Result<ProjectInfo> {
        Ok(forge_project::detect(self.root()))
    }

    fn watch(&self, sink: FsSink) -> Result<WatchGuard> {
        let php = self.php.clone();
        let watcher = ProjectWatcher::start(self.root(), move |changes: FsChanges| {
            php.apply_fs_changes(&changes);
            sink(changes);
        })?;
        Ok(Box::new(watcher))
    }

    fn open_terminal(&self, spec: TerminalSpec, sink: PtySink) -> Result<Arc<dyn Terminal>> {
        let cwd = if spec.cwd.is_dir() {
            spec.cwd
        } else {
            self.root().to_path_buf()
        };
        let session = PtySession::spawn(
            PtyOptions {
                shell: spec.shell.filter(|s| !s.trim().is_empty()),
                args: None,
                cwd: cwd.clone(),
                env: spec.env,
                cols: spec.cols,
                rows: spec.rows,
            },
            sink,
        )?;
        Ok(Arc::new(LocalTerminal { session, cwd }))
    }

    fn start_process(&self, cmd: &CommandLine, sink: RunSink) -> Result<Arc<dyn Process>> {
        Ok(Arc::new(RunningProcess::start(cmd, sink)?))
    }

    fn lsp_launcher(&self) -> Arc<dyn ServerLauncher> {
        Arc::new(LocalLauncher)
    }

    fn php(&self) -> &dyn PhpBackend {
        self.php.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::FileKind;

    fn entry(path: &str, kind: FileKind, ignored: bool) -> FileEntry {
        FileEntry {
            name: path.rsplit('/').next().unwrap().to_string(),
            path: path.to_string(),
            kind,
            ignored,
        }
    }

    #[test]
    fn tracked_files_and_folders_with_tracked_files_are_not_ignored() {
        let mut entries = vec![
            entry("vendor", FileKind::Dir, true),
            entry("cache", FileKind::Dir, true),
            entry("forced.log", FileKind::File, true),
            entry("other.log", FileKind::File, true),
            entry("src", FileKind::Dir, false),
        ];
        let tracked = vec!["vendor/forced.php".to_string(), "forced.log".to_string()];
        clear_tracked(&mut entries, &tracked);
        let flags: Vec<bool> = entries.iter().map(|e| e.ignored).collect();
        assert_eq!(flags, vec![false, true, false, true, false]);
    }

    #[test]
    fn a_folder_name_prefix_is_not_a_tracked_folder() {
        let mut entries = vec![entry("vendor", FileKind::Dir, true)];
        clear_tracked(&mut entries, &["vendor-extra/a.php".to_string()]);
        assert!(entries[0].ignored);
    }

    #[test]
    fn serves_files_of_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.php"), "<?php\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        backend.write_file("src/A.php", "<?php class A {}").unwrap();
        let names: Vec<String> = backend
            .list_dir("")
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["src", "index.php"]);
        assert_eq!(backend.read_file("src/A.php").unwrap(), "<?php class A {}");
        assert!(backend.resolve("../x").is_err());
        let found = backend.find_files("A", 10).unwrap();
        assert_eq!(found[0].path, "src/A.php");
    }
}
