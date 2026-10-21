//! A project folder on this machine.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use forge_core::search::{FindRequest, FindResult};
use forge_core::{
    CollectRequest, FileEntry, FileMatch, FsChanges, ProjectWatcher, Result, SourceFile, Workspace,
};
use forge_lsp::{LocalLauncher, ServerLauncher};
use forge_process::{CommandLine, PtyOptions, PtySession, PtySink, RunSink, RunningProcess};
use forge_project::ProjectInfo;
use forge_vcs::{GitExec, LocalGit, Repo};

use crate::frameworks::{FrameworkCall, LocalFrameworks};
use crate::langservers::{
    self, InstallRequest, InstallSink, LangServersAnswer, LangServersRequest, LocalLangServers,
};
use crate::php::LocalPhp;
use crate::web::{LocalWeb, WebCall};
use crate::{
    folder_name, Backend, BackendInfo, FsSink, PhpBackend, Process, Terminal, TerminalSpec,
    WatchGuard,
};
use forge_core::ForgeError;
use forge_langservers::install::InstallOutcome;
use forge_langservers::managed::PackageUpdate;

/// Serves a local folder: the desktop app's own projects, and the Forge agent on a remote
/// host.
pub struct LocalBackend {
    info: BackendInfo,
    workspace: Workspace,
    php: Arc<LocalPhp>,
    frameworks: LocalFrameworks,
    langservers: LocalLangServers,
    web: LocalWeb,
}

impl LocalBackend {
    /// Open `root` (must be an existing directory; symlinks are resolved).
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let workspace = Workspace::open(root)?;
        let root = workspace.root().to_path_buf();
        Ok(Self {
            info: BackendInfo {
                name: folder_name(&root),
                root: root.clone(),
                remote: None,
            },
            php: Arc::new(LocalPhp::new(workspace.clone())),
            frameworks: LocalFrameworks::new(workspace.clone()),
            langservers: LocalLangServers::default(),
            web: LocalWeb::new(&root),
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
    fn close_stdin(&self) -> Result<()> {
        RunningProcess::close_stdin(self);
        Ok(())
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

    fn read_optional(&self, rel: &str) -> Result<Option<String>> {
        if self.workspace.resolve(rel)?.is_file() {
            self.workspace.read_file(rel).map(Some)
        } else {
            Ok(None)
        }
    }

    fn write_file(&self, rel: &str, contents: &str) -> Result<()> {
        self.workspace.write_file(rel, contents)
    }

    fn create_file(&self, rel: &str) -> Result<()> {
        self.workspace.create_file(rel)
    }

    fn create_dir(&self, rel: &str) -> Result<()> {
        self.workspace.create_dir(rel)
    }

    fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.workspace.rename(from, to)
    }

    fn delete(&self, rel: &str) -> Result<()> {
        self.workspace.delete(rel)
    }

    fn find_files(&self, query: &str, limit: usize) -> Result<Vec<FileMatch>> {
        self.workspace.find_files(query, limit)
    }

    fn find_in_files(&self, req: &FindRequest) -> Result<FindResult> {
        self.workspace.find_in_files(req)
    }

    fn collect_files(&self, req: &CollectRequest) -> Result<Vec<SourceFile>> {
        self.workspace.collect_files(req)
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
                args: spec.args,
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

    fn framework(&self, call: FrameworkCall) -> Result<Vec<u8>> {
        self.frameworks.call(call)
    }

    fn langservers(&self, req: LangServersRequest) -> Result<LangServersAnswer> {
        self.langservers.plan(self.root(), &req)
    }

    fn install_langserver(
        &self,
        req: InstallRequest,
        cancel: Arc<tokio::sync::Notify>,
        sink: InstallSink,
    ) -> Result<InstallOutcome> {
        let mut on = |e| sink(e);
        langservers::block_on(langservers::install(&req, cancel, &mut on))?
            .map_err(ForgeError::Other)
    }

    fn langserver_updates(
        &self,
        registry: Option<String>,
        node_mirror: Option<String>,
        force: bool,
    ) -> Result<Option<Vec<PackageUpdate>>> {
        langservers::block_on(langservers::check_updates(
            registry.as_deref(),
            node_mirror.as_deref(),
            force,
        ))?
        .map_err(ForgeError::Other)
    }

    fn web(&self, call: WebCall) -> Result<Vec<u8>> {
        self.web.call(call)
    }

    fn find_executable(&self, names: &[String], custom: Option<&str>) -> Result<Option<PathBuf>> {
        Ok(find_executable(self.root(), names, custom))
    }
}

/// Find one of `names` like the language server detection does (project `node_modules/.bin`,
/// `PATH`, npm/nvm/volta, `~/.local/bin`, cargo, go, ...), plus the places agent CLIs install
/// themselves (`~/.claude/local`, `~/.npm-global/bin`). An existing `custom` file wins.
pub fn find_executable(root: &Path, names: &[String], custom: Option<&str>) -> Option<PathBuf> {
    if let Some(c) = custom.map(str::trim).filter(|c| !c.is_empty()) {
        let p = PathBuf::from(c);
        return p.is_file().then_some(p);
    }
    let detector = forge_langservers::Detector::for_project(root);
    if let Some(found) = names.iter().find_map(|n| detector.find(n)) {
        return Some(found);
    }
    let home = detector.env().home.clone()?;
    let extra = [home.join(".claude/local"), home.join(".npm-global/bin")];
    extra
        .iter()
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
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
        assert_eq!(backend.read_optional("missing.php").unwrap(), None);
        assert_eq!(
            backend.read_optional("index.php").unwrap().as_deref(),
            Some("<?php\n")
        );
    }
}
