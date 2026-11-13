//! Local History (`forge-localhistory`): every project gets a store in Forge's data dir
//! (`<data dir>/forge/projects/<key>/local-history`, never inside the project) and a
//! recorder thread. File commands, refactorings, VCS operations and the file watcher hand
//! it messages; it reads what it needs through the project's backend (so remote projects
//! work, while the history stays on this machine), batches them and writes the change
//! log. Saving never waits for it.
//!
//! An operation (a refactoring, Update Project, Checkout, Rollback, ...) is bracketed by
//! `local_history_begin` / `local_history_end`: everything recorded meanwhile, including
//! what the watcher reports shortly after, becomes one change set named after it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use forge_backend::Backend;
use forge_core::{FileKind, ForgeError, FsChanges, Result};
use forge_localhistory::{
    ChangeFile, ChangeKind, Content, DiskStorage, FolderChange, Limits, LocalHistory, NewChange,
    NewEntry, RecentChange, RevertAction, RevertPlan, Revision, RevisionContent, SelectionHistory,
};

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Emitted after the recorder wrote new change sets.
pub const CHANGED_EVENT: &str = "local-history://changed";
/// Changes the watcher reports this long after an operation ended still belong to it.
const GRACE: Duration = Duration::from_secs(3);
/// An operation that never ended is closed after this long.
const MAX_OPERATION: Duration = Duration::from_secs(600);
/// Cleanup interval while a project is open.
const PURGE_EVERY: Duration = Duration::from_secs(6 * 3600);
/// Most files read for one folder (delete, rename, copy) or one watcher batch.
const MAX_FILES: usize = 2000;

type Store = Arc<Mutex<Option<LocalHistory<DiskStorage>>>>;

/// What the recorder thread is asked to do.
pub enum Msg {
    Record {
        kind: ChangeKind,
        name: String,
        entries: Vec<NewEntry>,
    },
    /// A file's content as Forge read it.
    Baseline {
        path: String,
        bytes: Vec<u8>,
    },
    /// A file or folder was renamed or moved (it is at `to` now).
    Moved {
        from: String,
        to: String,
        kind: ChangeKind,
        name: String,
    },
    /// A file or folder was copied to `to`.
    Copied {
        to: String,
        name: String,
    },
    Fs(FsChanges),
    Begin {
        token: u64,
        kind: ChangeKind,
        name: String,
    },
    End {
        token: u64,
    },
    /// Answered once everything sent before is recorded.
    Sync(Sender<()>),
}

struct Project {
    store: Store,
    tx: Sender<Msg>,
}

/// The open project's Local History.
#[derive(Default)]
pub struct LocalHistoryState {
    project: Mutex<Option<Project>>,
    tokens: AtomicU64,
}

impl LocalHistoryState {
    /// Hand a message to the recorder (nothing happens without an open project).
    pub fn send(&self, msg: Msg) {
        if let Some(p) = self.project.lock().expect("local history lock").as_ref() {
            let _ = p.tx.send(msg);
        }
    }

    pub fn record(&self, kind: ChangeKind, name: impl Into<String>, entries: Vec<NewEntry>) {
        if !entries.is_empty() {
            self.send(Msg::Record {
                kind,
                name: name.into(),
                entries,
            });
        }
    }

    fn handles(&self) -> Result<(Store, Sender<Msg>)> {
        self.project
            .lock()
            .expect("local history lock")
            .as_ref()
            .map(|p| (p.store.clone(), p.tx.clone()))
            .ok_or_else(ForgeError::no_project)
    }

    /// The store, once everything sent before is recorded.
    fn synced(&self) -> Result<Store> {
        let (store, tx) = self.handles()?;
        let (done, wait) = mpsc::channel();
        if tx.send(Msg::Sync(done)).is_ok() {
            let _ = wait.recv_timeout(Duration::from_secs(5));
        }
        Ok(store)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn parent_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

fn limits(app: &AppHandle) -> Limits {
    state_limits(&app.state::<AppState>())
}

fn retention(app: &AppHandle) -> (u32, u64) {
    let state = app.state::<AppState>();
    let s = state.read_settings();
    let days = s.get_as::<u32>("localHistory.keepDays").unwrap_or(5);
    let mb = s.get_as::<u64>("localHistory.maxStorageMb").unwrap_or(512);
    (days, mb.max(1) * 1024 * 1024)
}

fn store_dir(state: &AppState) -> Option<PathBuf> {
    let info = state.project_info()?;
    crate::commands::workbench::project_state_file(
        &info.name,
        &info.display_root(),
        "local-history",
    )
}

/// Start the Local History of the project that was just opened (the previous project's
/// recorder stops).
pub fn project_opened(app: &AppHandle) {
    let lh = app.state::<LocalHistoryState>();
    let mut slot = lh.project.lock().expect("local history lock");
    *slot = None;
    let state = app.state::<AppState>();
    let (Some(dir), Ok(backend)) = (store_dir(&state), state.backend()) else {
        return;
    };
    let store: Store = Arc::default();
    let (tx, rx) = mpsc::channel();
    let worker = Worker {
        app: app.clone(),
        backend,
        store: store.clone(),
        action: None,
        groups: 0,
    };
    std::thread::Builder::new()
        .name("local-history".into())
        .spawn(move || worker.run(dir, rx))
        .map_err(|e| tracing::warn!("local history not started: {e}"))
        .ok();
    *slot = Some(Project { store, tx });
}

/// Feed the file watcher's batches to the recorder.
pub fn fs_changed(app: &AppHandle, changes: &FsChanges) {
    if !changes.changed.is_empty() || !changes.removed.is_empty() {
        app.state::<LocalHistoryState>()
            .send(Msg::Fs(changes.clone()));
    }
}

struct Action {
    token: u64,
    kind: ChangeKind,
    name: String,
    group: u64,
    started: Instant,
    ended: Option<Instant>,
}

struct Worker {
    app: AppHandle,
    backend: Arc<dyn Backend>,
    store: Store,
    action: Option<Action>,
    groups: u64,
}

impl Worker {
    fn run(mut self, dir: PathBuf, rx: Receiver<Msg>) {
        match LocalHistory::open(DiskStorage::new(&dir), limits(&self.app)) {
            Ok(h) => *self.store.lock().expect("local history store") = Some(h),
            Err(e) => {
                tracing::warn!("local history at {} not opened: {e}", dir.display());
                // Answer queries so nobody waits, but record nothing.
                for msg in rx {
                    if let Msg::Sync(done) = msg {
                        let _ = done.send(());
                    }
                }
                return;
            }
        }
        self.purge();
        let mut last_purge = Instant::now();
        loop {
            let msg = match self.deadline() {
                Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                    Ok(m) => m,
                    Err(RecvTimeoutError::Timeout) => {
                        self.close_action();
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None => match rx.recv() {
                    Ok(m) => m,
                    Err(_) => break,
                },
            };
            let batch: Vec<Msg> = std::iter::once(msg).chain(rx.try_iter()).collect();
            let limits = limits(&self.app);
            if let Some(h) = self.store.lock().expect("local history store").as_mut() {
                h.limits = limits.clone();
            }
            let mut changed = false;
            let mut replies = Vec::new();
            for msg in batch {
                match msg {
                    Msg::Sync(done) => replies.push(done),
                    Msg::Begin { token, kind, name } => {
                        self.close_action();
                        self.groups += 1;
                        self.action = Some(Action {
                            token,
                            kind,
                            name,
                            group: self.groups,
                            started: Instant::now(),
                            ended: None,
                        });
                    }
                    Msg::End { token } => {
                        if let Some(a) = self.action.as_mut().filter(|a| a.token == token) {
                            a.ended = Some(Instant::now());
                        }
                    }
                    Msg::Baseline { path, bytes } => {
                        let mut guard = self.store.lock().expect("local history store");
                        if let Some(h) = guard.as_mut() {
                            match h.baseline(&path, &bytes, now_ms()) {
                                Ok(id) => changed |= id.is_some(),
                                Err(e) => tracing::warn!("local history: {e}"),
                            }
                        }
                    }
                    other => {
                        if let Some((kind, name, entries)) = self.entries(other, &limits) {
                            changed |= self.write(kind, name, entries);
                        }
                    }
                }
            }
            if last_purge.elapsed() > PURGE_EVERY {
                self.purge();
                last_purge = Instant::now();
            }
            if changed {
                let _ = self.app.emit(CHANGED_EVENT, ());
            }
            for done in replies {
                let _ = done.send(());
            }
        }
    }

    /// When the open operation must be closed.
    fn deadline(&self) -> Option<Instant> {
        self.action.as_ref().map(|a| match a.ended {
            Some(end) => end + GRACE,
            None => a.started + MAX_OPERATION,
        })
    }

    fn close_action(&mut self) {
        if let Some(a) = self.action.take() {
            if let Some(h) = self.store.lock().expect("local history store").as_mut() {
                h.close_group(a.group);
            }
        }
    }

    fn purge(&self) {
        let (days, bytes) = retention(&self.app);
        if let Some(h) = self.store.lock().expect("local history store").as_mut() {
            if let Err(e) = h.purge(days, bytes) {
                tracing::warn!("local history cleanup failed: {e}");
            }
        }
    }

    fn write(&self, kind: ChangeKind, name: String, entries: Vec<NewEntry>) -> bool {
        if entries.is_empty() {
            return false;
        }
        let (kind, name, group) = match &self.action {
            Some(a) if kind != ChangeKind::Revert => (a.kind, a.name.clone(), Some(a.group)),
            _ => (kind, name, None),
        };
        let mut guard = self.store.lock().expect("local history store");
        let Some(h) = guard.as_mut() else {
            return false;
        };
        match h.record(NewChange {
            kind,
            name,
            time: now_ms(),
            group,
            entries,
        }) {
            Ok(id) => id.is_some(),
            Err(e) => {
                tracing::warn!("local history: {e}");
                false
            }
        }
    }

    fn known_is_set(&self, path: &str) -> bool {
        self.store
            .lock()
            .expect("local history store")
            .as_ref()
            .is_some_and(|h| {
                matches!(
                    h.known(path),
                    forge_localhistory::FileState::File(_) | forge_localhistory::FileState::Dir
                )
            })
    }

    /// The entries of a message, read through the backend (outside the store lock).
    fn entries(&self, msg: Msg, limits: &Limits) -> Option<(ChangeKind, String, Vec<NewEntry>)> {
        match msg {
            Msg::Record {
                kind,
                name,
                entries,
            } => Some((kind, name, entries)),
            Msg::Fs(changes) => {
                let mut entries = Vec::new();
                for path in changes.changed.iter().take(MAX_FILES) {
                    if limits.is_excluded(path) {
                        continue;
                    }
                    // Folders cannot be read as files; their files are reported themselves.
                    if let Ok(bytes) = self.backend.read_bytes(path) {
                        entries.push(NewEntry::new(
                            path.clone(),
                            Content::Unknown,
                            Content::Bytes(bytes),
                        ));
                    }
                }
                for path in changes.removed.iter().take(MAX_FILES) {
                    if !limits.is_excluded(path) && self.known_is_set(path) {
                        entries.push(NewEntry::new(
                            path.clone(),
                            Content::Unknown,
                            Content::Absent,
                        ));
                    }
                }
                Some((ChangeKind::External, "External change".into(), entries))
            }
            Msg::Moved {
                from,
                to,
                kind,
                name,
            } => {
                let mut entries = Vec::new();
                for (path, content) in snapshot(&*self.backend, &to, limits) {
                    let old = format!("{from}{}", &path[to.len()..]);
                    entries.push(NewEntry::new(old.clone(), content.clone(), Content::Absent));
                    let mut moved = NewEntry::new(path, Content::Absent, content);
                    moved.from = Some(old);
                    entries.push(moved);
                }
                Some((kind, name, entries))
            }
            Msg::Copied { to, name } => {
                let entries = snapshot(&*self.backend, &to, limits)
                    .into_iter()
                    .map(|(path, content)| NewEntry::new(path, Content::Absent, content))
                    .collect();
                Some((ChangeKind::Copy, name, entries))
            }
            Msg::Baseline { .. } | Msg::Begin { .. } | Msg::End { .. } | Msg::Sync(_) => None,
        }
    }
}

/// The content of a file, or of every file and folder below a folder (at most
/// [`MAX_FILES`]), read through the backend. Files that are too large or binary are
/// [`Content::Unknown`].
pub fn snapshot(backend: &dyn Backend, path: &str, limits: &Limits) -> Vec<(String, Content)> {
    let mut out = Vec::new();
    if limits.is_excluded(path) {
        return out;
    }
    match backend.list_dir(path) {
        Ok(_) => {
            let mut dirs = vec![path.to_string()];
            while let Some(dir) = dirs.pop() {
                out.push((dir.clone(), Content::Dir));
                let Ok(children) = backend.list_dir(&dir) else {
                    continue;
                };
                for c in children {
                    if out.len() >= MAX_FILES {
                        return out;
                    }
                    if limits.is_excluded(&c.path) {
                        continue;
                    }
                    match c.kind {
                        FileKind::Dir => dirs.push(c.path),
                        FileKind::File => {
                            out.push((c.path.clone(), read(backend, &c.path, limits)))
                        }
                    }
                }
            }
        }
        Err(_) => out.push((path.to_string(), read(backend, path, limits))),
    }
    out
}

fn read(backend: &dyn Backend, path: &str, limits: &Limits) -> Content {
    match backend.read_bytes(path) {
        Ok(b) if limits.accepts(&b) => Content::Bytes(b),
        _ => Content::Unknown,
    }
}

// ---- hooks for the file commands ---------------------------------------------------------

/// The content before a write: read first for local projects (cheap); remote projects use
/// the last recorded content instead of an extra round trip.
pub fn before_write(state: &AppState, backend: &dyn Backend, path: &str) -> Content {
    if state.is_remote() {
        return Content::Unknown;
    }
    match backend.read_bytes(path) {
        Ok(b) => Content::Bytes(b),
        Err(_) => Content::Absent,
    }
}

pub fn saved(lh: &LocalHistoryState, path: &str, before: Content, after: Vec<u8>) {
    lh.record(
        ChangeKind::Save,
        "Saved",
        vec![NewEntry::new(path, before, Content::Bytes(after))],
    );
}

/// A new file (with its content) or folder ([`Content::Dir`]).
pub fn created(lh: &LocalHistoryState, path: &str, content: Content) {
    let what = if content == Content::Dir {
        "folder"
    } else {
        "file"
    };
    lh.record(
        ChangeKind::Create,
        format!("Create {what} {}", name_of(path)),
        vec![NewEntry::new(path, Content::Absent, content)],
    );
}

/// Before deleting: what is about to be deleted (read now, recorded with [`deleted`]).
pub fn before_delete(
    state: &AppState,
    backend: &dyn Backend,
    path: &str,
) -> Vec<(String, Content)> {
    snapshot(backend, path, &state_limits(state))
}

/// What is recorded (Settings | Version Control | Local History).
fn state_limits(state: &AppState) -> Limits {
    let s = state.read_settings();
    let mut l = Limits::default();
    if let Some(kb) = s.get_as::<u64>("localHistory.maxFileSizeKb") {
        l.max_file_size = kb.max(1) * 1024;
    }
    if let Some(ex) = s.get_as::<Vec<String>>("localHistory.excludedFolders") {
        l.excluded = ex;
    }
    drop(s);
    // The project's excluded folders (Settings | Directories) are not recorded either.
    let excludes = crate::commands::project_structure::excludes(state);
    for folder in excludes
        .user_folders()
        .iter()
        .cloned()
        .chain(excludes.default_names())
    {
        if !l.excluded.contains(&folder) {
            l.excluded.push(folder);
        }
    }
    l
}

pub fn deleted(lh: &LocalHistoryState, path: &str, contents: Vec<(String, Content)>) {
    let entries = contents
        .into_iter()
        .map(|(p, c)| NewEntry::new(p, c, Content::Absent))
        .collect();
    lh.record(
        ChangeKind::Delete,
        format!("Delete {}", name_of(path)),
        entries,
    );
}

pub fn moved(lh: &LocalHistoryState, from: &str, to: &str) {
    let (kind, name) = if parent_of(from) == parent_of(to) {
        (
            ChangeKind::Rename,
            format!("Rename {} to {}", name_of(from), name_of(to)),
        )
    } else {
        let dest = parent_of(to);
        (
            ChangeKind::Move,
            format!(
                "Move {} to {}",
                name_of(from),
                if dest.is_empty() {
                    "the project root"
                } else {
                    dest
                }
            ),
        )
    };
    lh.send(Msg::Moved {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        name,
    });
}

pub fn copied(lh: &LocalHistoryState, from: &str, to: &str) {
    lh.send(Msg::Copied {
        to: to.to_string(),
        name: format!("Copy {} to {}", name_of(from), to),
    });
}

/// Files an edit changed (texts before and after; `None` = the file did not exist).
pub fn edited<'a>(
    lh: &LocalHistoryState,
    kind: ChangeKind,
    name: &str,
    files: impl IntoIterator<Item = (&'a str, Option<&'a str>, Option<&'a str>)>,
) {
    let entries = files
        .into_iter()
        .map(|(path, before, after)| {
            NewEntry::new(
                path,
                Content::text(before.map(String::from)),
                Content::text(after.map(String::from)),
            )
        })
        .collect();
    lh.record(kind, name, entries);
}

/// Replace in Files / Structural Replace: the changed files.
pub fn replaced(lh: &LocalHistoryState, name: &str, files: &[forge_backend::replace::Restore]) {
    edited(
        lh,
        ChangeKind::Edit,
        name,
        files.iter().map(|r| {
            (
                r.path.as_str(),
                Some(r.before.as_str()),
                Some(r.after.as_str()),
            )
        }),
    );
}

// ---- IPC ---------------------------------------------------------------------------------

/// A file change recorded by the UI (editor changes of a refactoring).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedFile {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

fn kind_of(kind: &str) -> ChangeKind {
    serde_json::from_value(serde_json::Value::String(kind.to_string())).unwrap_or(ChangeKind::Edit)
}

/// Record changes the UI made (e.g. the editor part of a refactoring).
#[tauri::command]
pub fn local_history_record(
    kind: String,
    name: String,
    files: Vec<RecordedFile>,
    lh: State<'_, LocalHistoryState>,
) {
    let entries = files
        .into_iter()
        .map(|f| NewEntry::new(f.path, Content::text(f.before), Content::text(f.after)))
        .collect();
    lh.record(kind_of(&kind), name, entries);
}

/// Start an operation (refactoring, VCS update, ...): what is recorded until
/// [`local_history_end`] becomes one change set called `name`. Returns its token.
#[tauri::command]
pub fn local_history_begin(kind: String, name: String, lh: State<'_, LocalHistoryState>) -> u64 {
    let token = lh.tokens.fetch_add(1, Ordering::Relaxed) + 1;
    lh.send(Msg::Begin {
        token,
        kind: kind_of(&kind),
        name,
    });
    token
}

#[tauri::command]
pub fn local_history_end(token: u64, lh: State<'_, LocalHistoryState>) {
    lh.send(Msg::End { token });
}

/// Record the current content of files an operation is about to change (Rollback,
/// Unshelve), so their old content can be restored.
#[tauri::command]
pub async fn local_history_snapshot(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    let backend = state.backend()?;
    let limits = state_limits(&state);
    blocking(move || {
        let lh = app.state::<LocalHistoryState>();
        for path in paths {
            for (p, c) in snapshot(&*backend, &path, &limits) {
                if let Content::Bytes(bytes) = c {
                    lh.send(Msg::Baseline { path: p, bytes });
                }
            }
        }
        Ok(())
    })
    .await
}

/// Show History: the revisions of a file or folder (`""` = the project), newest first.
#[tauri::command]
pub async fn local_history_list(
    path: String,
    dir: bool,
    filter: Option<String>,
    app: AppHandle,
) -> Result<Vec<Revision>> {
    query(app, move |h| {
        h.history(&path, dir, filter.as_deref().unwrap_or(""))
    })
    .await
}

/// The content of a file at a revision.
#[tauri::command]
pub async fn local_history_content(
    path: String,
    rev: u64,
    app: AppHandle,
) -> Result<RevisionContent> {
    query(app, move |h| h.content(&path, rev)).await
}

/// The files of a folder that differ between a revision and now.
#[tauri::command]
pub async fn local_history_folder_changes(
    path: String,
    rev: u64,
    app: AppHandle,
) -> Result<Vec<FolderChange>> {
    query(app, move |h| Ok(h.folder_changes(&path, rev))).await
}

/// Show History for Selection.
#[tauri::command]
pub async fn local_history_selection(
    path: String,
    text: String,
    start: usize,
    end: usize,
    app: AppHandle,
) -> Result<SelectionHistory> {
    query(app, move |h| h.selection_history(&path, &text, start, end)).await
}

/// Recent Changes (Alt+Shift+C).
#[tauri::command]
pub async fn local_history_recent(
    limit: Option<usize>,
    app: AppHandle,
) -> Result<Vec<RecentChange>> {
    query(app, move |h| Ok(h.recent_changes(limit.unwrap_or(50)))).await
}

/// The files of a Recent Changes entry.
#[tauri::command]
pub async fn local_history_change_files(
    from: u64,
    to: u64,
    app: AppHandle,
) -> Result<Vec<ChangeFile>> {
    query(app, move |h| Ok(h.change_files(from, to))).await
}

/// Put Label…
#[tauri::command]
pub async fn local_history_put_label(name: String, app: AppHandle) -> Result<u64> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ForgeError::Other("The label name is empty".into()));
    }
    let result = query(app.clone(), move |h| h.put_label(&name, now_ms())).await;
    let _ = app.emit(CHANGED_EVENT, ());
    result
}

/// Create Patch: the difference between a revision and now.
#[tauri::command]
pub async fn local_history_patch(
    path: String,
    dir: bool,
    rev_path: String,
    rev: u64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    let backend = state.backend()?;
    query(app, move |h| {
        h.patch(&path, dir, &rev_path, rev, |p| {
            backend.read_optional(p).ok().flatten()
        })
    })
    .await
}

/// What a revert changed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertResult {
    /// Files written or deleted (open editors reload them).
    pub changed: Vec<String>,
    /// Files whose old content is not known.
    pub skipped: Vec<String>,
    /// Files that could not be written or deleted.
    pub failed: Vec<String>,
}

/// What to revert.
#[derive(Debug, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum RevertTarget {
    /// A file to revision `rev` (`revPath` is its path then).
    File {
        path: String,
        rev_path: String,
        rev: u64,
    },
    /// A folder (`""` = the project) to revision `rev` or a label.
    Folder { path: String, rev: u64 },
    /// The files of a Recent Changes entry to their state before it.
    Change { from: u64, to: u64 },
}

/// Revert, recorded as a change set called `name`.
#[tauri::command]
pub async fn local_history_revert(
    target: RevertTarget,
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RevertResult> {
    let backend = state.backend()?;
    let plan = query(app.clone(), move |h| match target {
        RevertTarget::File {
            path,
            rev_path,
            rev,
        } => h.revert_file(&path, &rev_path, rev),
        RevertTarget::Folder { path, rev } => h.revert_folder(&path, rev),
        RevertTarget::Change { from, to } => h.revert_change(from, to),
    })
    .await?;
    let app2 = app.clone();
    blocking(move || Ok(apply(&app2, &*backend, plan, &name))).await
}

fn apply(app: &AppHandle, backend: &dyn Backend, plan: RevertPlan, name: &str) -> RevertResult {
    let mut result = RevertResult {
        changed: Vec::new(),
        skipped: plan.skipped,
        failed: Vec::new(),
    };
    let mut entries = Vec::new();
    for op in plan.ops {
        let (done, after) = match op.action {
            RevertAction::Write(bytes) => {
                (backend.write_bytes(&op.path, &bytes), Content::Bytes(bytes))
            }
            RevertAction::CreateDir => (
                backend.create_dir(&op.path).or_else(|e| {
                    // It may exist already.
                    backend.list_dir(&op.path).map(|_| ()).map_err(|_| e)
                }),
                Content::Dir,
            ),
            RevertAction::Delete | RevertAction::DeleteDir => {
                (backend.delete(&op.path), Content::Absent)
            }
        };
        match done {
            Ok(()) => {
                if after != Content::Dir {
                    result.changed.push(op.path.clone());
                }
                entries.push(NewEntry::new(op.path, Content::Unknown, after));
            }
            Err(e) => {
                tracing::warn!("revert of {} failed: {e}", op.path);
                result.failed.push(op.path);
            }
        }
    }
    app.state::<LocalHistoryState>()
        .record(ChangeKind::Revert, name, entries);
    result
}

async fn query<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&mut LocalHistory<DiskStorage>) -> std::io::Result<T> + Send + 'static,
) -> Result<T> {
    blocking(move || {
        let store = app.state::<LocalHistoryState>().synced()?;
        let mut guard = store.lock().expect("local history store");
        let h = guard
            .as_mut()
            .ok_or_else(|| ForgeError::Other("Local History is not available".into()))?;
        f(h).map_err(|e| ForgeError::Other(format!("Local History: {e}")))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_and_move_names_follow_phpstorm() {
        assert_eq!(name_of("app/Models/User.php"), "User.php");
        assert_eq!(parent_of("app/Models/User.php"), "app/Models");
        assert_eq!(parent_of("User.php"), "");
        assert_eq!(kind_of("refactoring"), ChangeKind::Refactoring);
        assert_eq!(kind_of("vcs"), ChangeKind::Vcs);
        assert_eq!(kind_of("nonsense"), ChangeKind::Edit);
    }
}
