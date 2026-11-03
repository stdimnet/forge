//! The host's side of the guest project backend: every [`HostCall`] a guest makes is decoded,
//! checked by the guest's [`Policy`], and only then answered from the host's own
//! [`Backend`].
//!
//! This file is a child module of `host` (see the `#[path]` declaration there), so it can use
//! the session's private state without widening what `host` exports.
//!
//! What is enforced here, whatever the guest's client sends:
//! - the call is a closed, typed [`HostCall`]; garbage does not decode;
//! - [`Policy::check_call`]: permissions, plain relative paths, no `.git/`, no excluded files,
//!   protected files only with Full access;
//! - [`Gate::verify`]: a path is resolved through symlinks and must stay inside the project
//!   and still pass the path rules under its real name (a link to `.env` is `.env`);
//! - results are filtered (listings, search, git status and diffs, language server and
//!   framework locations) so hidden files do not leak through them;
//! - errors never carry the host's absolute paths, answers are size-limited, and a guest
//!   can have only a few calls in flight.

use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forge_backend::frameworks::SymfonyQuery;
use forge_backend::protocol::{encode_value, PhpCall};
use forge_backend::{Backend, FrameworkCall};
use forge_vcs::{GitExec, GitOutput, GitRequest};
use serde::Serialize;
use serde_bytes::ByteBuf;

use super::*;
use crate::audit::{AuditEvent, FileAction};
use crate::guest_backend::calls::{GitAction, GitQuery, HostCall, LspQuery, OpenedProject};
use crate::guest_backend::git::Visible;
use crate::permissions::is_plain_relative;
use crate::reconnect::MissedChanges;

/// Calls a guest may have in flight at once.
const MAX_PENDING_CALLS: usize = 64;
/// Largest file a guest reads through the project backend.
const MAX_READ: u64 = 16 * 1024 * 1024;
/// Largest answer; the frame limit of a guest's connection is [`MAX_HOST_FRAME`].
const MAX_ANSWER: usize = MAX_HOST_FRAME - 1024 * 1024;
const MAX_FIND_LIMIT: usize = 10_000;
const MAX_WALK: usize = 100_000;
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(600);

/// A language server request for the host's servers.
#[derive(Debug, Clone)]
pub struct LspRequest {
    pub query: LspQuery,
    /// The current text of the file the query is about: the shared document, or the file on
    /// disk. The host's servers are synchronized with it before the query runs.
    pub text: Option<String>,
}

/// What the host application provides to answer guests beyond the project's files: its
/// language servers, terminals and runs. The session checks every guest request against the
/// guest's permissions before it calls these; the terminal and run methods default to "not
/// available" for hosts that do not share them.
pub trait HostServices: Send + Sync + 'static {
    /// Answer an [`LspQuery`] with the JSON of the reply type documented on it. Paths in
    /// locations are workspace-relative.
    fn lsp(&self, req: LspRequest) -> BoxFuture<'static, Result<Vec<u8>, String>>;

    /// Open a terminal for a guest in the project folder with the host's own shell, shown in
    /// the host's Terminal tool window as `title`. `key` is the terminal's id in the host
    /// application; its output and exit are reported with
    /// [`HostSession::terminal_output`] and [`HostSession::terminal_exited`].
    fn terminal_open(
        &self,
        _key: String,
        _title: String,
        _cols: u16,
        _rows: u16,
    ) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Err("Terminals are not available in this session.".to_string()) })
    }

    /// Keys a guest typed into a shared terminal of the host application.
    fn terminal_write(&self, _key: &str, _data: &str) -> Result<(), String> {
        Err("Terminals are not available in this session.".into())
    }

    /// Close a guest's terminal (the guest closed it, left, or lost Terminal Full access).
    fn terminal_close(&self, _key: &str) {}

    /// Start the host's run configuration `configuration` as if the host clicked Run (a
    /// running one is rerun). `guest` is who asked, for the host's UI.
    fn run_start(
        &self,
        _configuration: String,
        _guest: String,
    ) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Err("Running is not available in this session.".to_string()) })
    }

    /// Stop a run of the host application (`key` is its run id there).
    fn run_stop(&self, _key: &str) -> Result<(), String> {
        Err("Running is not available in this session.".into())
    }

    /// Console input for a run of the host application.
    fn run_write(&self, _key: &str, _data: &str) -> Result<(), String> {
        Err("Running is not available in this session.".into())
    }
}

/// The host's bookkeeping for guest project backends.
#[derive(Default)]
pub(crate) struct CallState {
    services: Mutex<Option<Arc<dyn HostServices>>>,
    /// Guests that asked for `FsChanged`.
    watchers: Mutex<HashSet<ParticipantId>>,
    /// File changes watching guests missed while their connection was down.
    missed: Mutex<HashMap<ParticipantId, MissedChanges>>,
    /// Files each guest told us it has open (for diagnostics).
    lsp_open: Mutex<HashMap<ParticipantId, HashSet<String>>>,
    pending: Mutex<HashMap<ParticipantId, Arc<AtomicUsize>>>,
    /// The repository root and the project's folder inside it (`""` or `"sub/dir/"`).
    repo: Arc<Mutex<Option<(PathBuf, String)>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().expect("collab call state lock poisoned")
}

/// The placeholder root of a guest's project: the host's real path is not sent.
pub fn virtual_root(name: &str) -> PathBuf {
    PathBuf::from("/").join(name)
}

fn enc<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    encode_value(value).map_err(|e| e.to_string())
}

impl HostSession {
    /// Let guests use the host's language servers.
    pub fn set_services(&self, services: Arc<dyn HostServices>) {
        *lock(&self.calls.services) = Some(services);
    }

    /// The host application's services, once set.
    pub(crate) fn services(&self) -> Option<Arc<dyn HostServices>> {
        lock(&self.calls.services).clone()
    }

    /// Language server diagnostics (JSON, a list of `EditorDiagnostic`) for a project file:
    /// sent to the guests that have it open.
    pub fn publish_diagnostics(&self, path: &str, diagnostics: Vec<u8>) {
        let st = self.state();
        let open = lock(&self.calls.lsp_open);
        let mut out = Outbox::default();
        for (id, g) in &st.guests {
            if open.get(id).is_some_and(|files| files.contains(path))
                && g.policy.check_path(path).is_ok()
            {
                out.guests.push((
                    g.tx.clone(),
                    HostMessage::Diagnostics {
                        path: path.to_string(),
                        diagnostics: ByteBuf::from(diagnostics.clone()),
                    },
                ));
            }
        }
        drop(open);
        drop(st);
        self.deliver(out);
    }

    /// The project watcher reported changes: tell the guests that watch, minus what they may
    /// not see. A guest that is away collects them until it resumes (`take_missed_fs`).
    pub(crate) fn forward_fs(&self, changes: &FsChanges) {
        let st = self.state();
        let mut watchers = lock(&self.calls.watchers);
        watchers.retain(|id| st.guests.contains_key(id));
        let mut missed = lock(&self.calls.missed);
        missed.retain(|id, _| watchers.contains(id));
        let mut out = Outbox::default();
        for id in watchers.iter() {
            let Some(g) = st.guests.get(id) else { continue };
            let visible = |paths: &[String]| -> Vec<String> {
                paths
                    .iter()
                    .filter(|p| g.policy.check_path(p).is_ok())
                    .cloned()
                    .collect()
            };
            let (changed, removed) = (visible(&changes.changed), visible(&changes.removed));
            if changed.is_empty() && removed.is_empty() && !changes.git {
                continue;
            }
            if g.participant.reconnecting {
                missed
                    .entry(*id)
                    .or_default()
                    .record(&changed, &removed, changes.git);
                continue;
            }
            out.guests.push((
                g.tx.clone(),
                HostMessage::FsChanged {
                    changed,
                    removed,
                    git: changes.git,
                },
            ));
        }
        drop(missed);
        drop(watchers);
        drop(st);
        self.deliver(out);
    }

    /// What guest `id` missed while it was away, as one `FsChanged`, and forget it.
    pub(crate) fn take_missed_fs(&self, id: ParticipantId) -> Option<HostMessage> {
        let missed = lock(&self.calls.missed).remove(&id)?;
        if missed.is_empty() {
            return None;
        }
        if missed.overflowed() {
            tracing::debug!("Code With Me: a guest missed more file changes than were kept");
        }
        let (changed, removed, git) = missed.into_parts();
        Some(HostMessage::FsChanged {
            changed,
            removed,
            git,
        })
    }

    /// A project call from guest `from`: answered in the background so documents and carets
    /// keep flowing while a search runs.
    pub(crate) fn spawn_call(self: &Arc<Self>, from: ParticipantId, id: u64, payload: ByteBuf) {
        let Some(policy) = self.state().guests.get(&from).map(|g| g.policy.clone()) else {
            return;
        };
        let counter = lock(&self.calls.pending).entry(from).or_default().clone();
        if counter.fetch_add(1, Ordering::SeqCst) >= MAX_PENDING_CALLS {
            counter.fetch_sub(1, Ordering::SeqCst);
            self.reply(
                from,
                HostMessage::CallResult {
                    id,
                    result: Err("Too many requests at once. Try again in a moment.".into()),
                },
            );
            return;
        }
        let session = self.clone();
        tokio::spawn(async move {
            let result = session.answer(from, &policy, &payload).await;
            counter.fetch_sub(1, Ordering::SeqCst);
            let result = result.and_then(|bytes| {
                if bytes.len() > MAX_ANSWER {
                    Err("The answer is too large.".to_string())
                } else {
                    Ok(ByteBuf::from(bytes))
                }
            });
            session.reply(from, HostMessage::CallResult { id, result });
        });
    }

    /// Decode, check and answer one call. The error is shown to the guest.
    pub(crate) async fn answer(
        &self,
        from: ParticipantId,
        policy: &Policy,
        payload: &[u8],
    ) -> Result<Vec<u8>, String> {
        let call = HostCall::decode(payload).map_err(|e| {
            tracing::warn!("Code With Me: undecodable call from {from:?}: {e}");
            "The host could not read the request.".to_string()
        })?;
        // Files the guest may not see read as missing, so settings lookups keep working.
        if let HostCall::ReadOptional { path } = &call {
            if is_plain_relative(path) && policy.check_path(path).is_err() {
                return enc(&None::<String>);
            }
        }
        if let Err(denied) = policy.check_call(&call) {
            tracing::warn!(
                "Code With Me: denied {} from {from:?}: {denied}",
                call.name()
            );
            self.audit(AuditEvent::Denied {
                participant: from,
                name: self.guest_name(from),
                request: call.name().to_string(),
                reason: denied.to_string(),
            });
            return Err(denied.to_string());
        }
        let timeout = match &call {
            HostCall::GitAction(a) if a.non_interactive() => GIT_NETWORK_TIMEOUT,
            _ => CALL_TIMEOUT,
        };
        let logged = self.change_of(from, &call);
        let result = match tokio::time::timeout(timeout, self.dispatch(from, policy, call)).await {
            Ok(result) => result,
            Err(_) => Err("The host took too long to answer.".into()),
        };
        if let (Some(event), Ok(_)) = (logged, &result) {
            self.audit(event);
        }
        result
    }

    /// The session log entry of a call that changes the project, written once it succeeded.
    fn change_of(&self, from: ParticipantId, call: &HostCall) -> Option<AuditEvent> {
        let file = |action, path: &str, to: Option<&str>| {
            Some(AuditEvent::File {
                participant: from,
                name: self.guest_name(from),
                action,
                path: path.to_string(),
                to: to.map(str::to_string),
            })
        };
        match call {
            HostCall::WriteFile { path, .. } | HostCall::WriteBytes { path, .. } => {
                file(FileAction::Written, path, None)
            }
            HostCall::CreateFile { path } => file(FileAction::Created, path, None),
            HostCall::CreateDir { path } => file(FileAction::CreatedFolder, path, None),
            HostCall::Rename { from: a, to: b } => file(FileAction::Renamed, a, Some(b)),
            HostCall::Delete { path } => file(FileAction::Deleted, path, None),
            HostCall::GitAction(a) => Some(AuditEvent::Git {
                participant: from,
                name: self.guest_name(from),
                action: a.label(),
            }),
            _ => None,
        }
    }

    async fn dispatch(
        &self,
        from: ParticipantId,
        policy: &Policy,
        call: HostCall,
    ) -> Result<Vec<u8>, String> {
        let backend = self.backend.clone();
        let policy = policy.clone();
        match call {
            HostCall::Ping => enc(&()),
            HostCall::Watch => {
                lock(&self.calls.watchers).insert(from);
                enc(&())
            }
            HostCall::Lsp(q) => self.lsp(from, &policy, q).await,
            HostCall::TerminalOpen { cols, rows } => {
                enc(&self.open_guest_terminal(from, cols, rows).await?)
            }
            HostCall::TerminalClose { terminal } => {
                self.close_guest_terminal(from, terminal)?;
                enc(&())
            }
            HostCall::RunStart { configuration } => {
                self.start_run_for(from, configuration).await?;
                enc(&())
            }
            HostCall::RunStop { run } => {
                self.stop_run_for(from, run)?;
                enc(&())
            }
            HostCall::Rename { from: a, to: b } => {
                let (x, y) = (a.clone(), b.clone());
                let r = blocking(move || {
                    Gate::new(&*backend, &policy).run(HostCall::Rename { from: x, to: y })
                })
                .await;
                if r.is_ok() {
                    self.on_renamed(&a, &b);
                }
                r
            }
            HostCall::Delete { path } => {
                let p = path.clone();
                let r = blocking(move || {
                    Gate::new(&*backend, &policy).run(HostCall::Delete { path: p })
                })
                .await;
                if r.is_ok() {
                    self.on_deleted(&path);
                }
                r
            }
            HostCall::Git(q) => {
                let repo = self.calls.repo.clone();
                blocking(move || run_git(&*backend, &policy, &repo, GitCall::Query(q))).await
            }
            HostCall::GitAction(a) => {
                let repo = self.calls.repo.clone();
                blocking(move || run_git(&*backend, &policy, &repo, GitCall::Action(a))).await
            }
            other => {
                let limit = self.max_file_size;
                blocking(move || {
                    let mut gate = Gate::new(&*backend, &policy);
                    gate.max_file = limit;
                    gate.run(other)
                })
                .await
            }
        }
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("The host failed to answer: {e}"))?
}

/// Everything a file call may touch, resolved against one guest's policy.
struct Gate<'a> {
    backend: &'a dyn Backend,
    policy: &'a Policy,
    /// The canonical project root, when the project's files are on this machine.
    root: Option<PathBuf>,
    max_file: usize,
}

impl<'a> Gate<'a> {
    fn new(backend: &'a dyn Backend, policy: &'a Policy) -> Self {
        let root = backend
            .info()
            .is_local()
            .then(|| backend.info().root.canonicalize().ok())
            .flatten();
        Self {
            backend,
            policy,
            root,
            max_file: 5 * 1024 * 1024,
        }
    }

    /// An error message without the host's absolute paths.
    fn msg(&self, e: impl Display) -> String {
        let mut text = e.to_string();
        let info = self.backend.info();
        let mut roots = vec![info.root.to_string_lossy().into_owned()];
        if let Some(r) = &self.root {
            roots.push(r.to_string_lossy().into_owned());
        }
        for r in roots {
            if r.len() > 1 {
                text = text
                    .replace(&format!("{r}/"), "")
                    .replace(&r, "the project");
            }
        }
        text
    }

    fn allowed(&self, path: &str) -> bool {
        self.policy.check_path(path).is_ok()
    }

    /// Check `rel` against the file system: resolved through symlinks (for a path that does
    /// not exist yet, its nearest existing folder) it must stay inside the project, and the
    /// real name must pass the same path rules, so a link to `.env` or `.git/config` is
    /// treated as what it points to. A no-op for projects the host itself reaches over SSH:
    /// their symlinks cannot be resolved from here.
    fn verify(&self, rel: &str) -> Result<(), String> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        let outside = || "The path is outside the project.".to_string();
        let abs = self.backend.resolve(rel).map_err(|e| self.msg(e))?;
        let mut existing = abs.clone();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        let real = loop {
            match existing.canonicalize() {
                Ok(real) => break real,
                Err(_) => {
                    let name = existing.file_name().ok_or_else(outside)?.to_os_string();
                    tail.push(name);
                    if !existing.pop() {
                        return Err(outside());
                    }
                }
            }
        };
        let mut real = real;
        real.extend(tail.iter().rev());
        let rest = real.strip_prefix(root).map_err(|_| outside())?;
        let real_rel = rest
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if !real_rel.is_empty() {
            self.policy
                .check_path(&real_rel)
                .map_err(|d| d.to_string())?;
        }
        Ok(())
    }

    fn file_size_ok(&self, rel: &str, limit: u64) -> Result<(), String> {
        if self.root.is_none() {
            return Ok(());
        }
        let abs = self.backend.resolve(rel).map_err(|e| self.msg(e))?;
        match std::fs::metadata(&abs) {
            Ok(m) if m.len() > limit => Err("The file is too large to open as a guest.".into()),
            _ => Ok(()),
        }
    }

    /// Whether anything below a folder is hidden from the guest (deleting the folder would
    /// destroy files the guest cannot see).
    fn hides_content(&self, rel: &str) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let Ok(abs) = self.backend.resolve(rel) else {
            return false;
        };
        let mut stack = vec![abs];
        let mut seen = 0;
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in read.flatten() {
                seen += 1;
                if seen > MAX_WALK {
                    return true;
                }
                let path = e.path();
                let Ok(rest) = path.strip_prefix(root) else {
                    continue;
                };
                let rel = rest
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if self.policy.check_path(&rel).is_err() {
                    return true;
                }
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push(path);
                }
            }
        }
        false
    }

    fn keep<T>(&self, items: Vec<T>, path: impl Fn(&T) -> &str) -> Vec<T> {
        items
            .into_iter()
            .filter(|i| self.allowed(path(i)))
            .collect()
    }

    /// Answer a call that works on files and the project (not git, not language servers).
    fn run(&self, call: HostCall) -> Result<Vec<u8>, String> {
        let b = self.backend;
        let e = |err: forge_core::ForgeError| self.msg(err);
        match call {
            HostCall::Open => enc(&OpenedProject {
                name: b.info().name.clone(),
            }),
            HostCall::ListDir { path } => {
                if !path.is_empty() {
                    self.verify(&path)?;
                }
                let entries = b.list_dir(&path).map_err(e)?;
                enc(&self.keep(entries, |f| &f.path))
            }
            HostCall::ReadFile { path } => {
                self.verify(&path)?;
                self.file_size_ok(&path, MAX_READ)?;
                enc(&b.read_file(&path).map_err(e)?)
            }
            HostCall::ReadBytes { path } => {
                self.verify(&path)?;
                self.file_size_ok(&path, MAX_READ)?;
                enc(&ByteBuf::from(b.read_bytes(&path).map_err(e)?))
            }
            HostCall::WriteBytes { path, bytes } => {
                self.verify(&path)?;
                enc(&b.write_bytes(&path, &bytes).map_err(e)?)
            }
            HostCall::ReadOptional { path } => {
                self.verify(&path)?;
                self.file_size_ok(&path, MAX_READ)?;
                enc(&b.read_optional(&path).map_err(e)?)
            }
            HostCall::WriteFile { path, contents } => {
                self.verify(&path)?;
                enc(&b.write_file(&path, &contents).map_err(e)?)
            }
            HostCall::CreateFile { path } => {
                self.verify(&path)?;
                enc(&b.create_file(&path).map_err(e)?)
            }
            HostCall::CreateDir { path } => {
                self.verify(&path)?;
                enc(&b.create_dir(&path).map_err(e)?)
            }
            HostCall::Rename { from, to } => {
                self.verify(&from)?;
                self.verify(&to)?;
                enc(&b.rename(&from, &to).map_err(e)?)
            }
            HostCall::Delete { path } => {
                self.verify(&path)?;
                if self.hides_content(&path) {
                    return Err(
                        "The folder contains files that are not shared with guests, so it cannot be deleted."
                            .into(),
                    );
                }
                enc(&b.delete(&path).map_err(e)?)
            }
            HostCall::FindFiles {
                query,
                limit,
                exact,
            } => {
                let limit = limit.min(MAX_FIND_LIMIT);
                // Hidden files are dropped afterwards; ask for a few more.
                let found = b.find_files(&query, limit + 100, exact).map_err(e)?;
                let mut found = self.keep(found, |m| &m.path);
                found.truncate(limit);
                enc(&found)
            }
            HostCall::FindInFiles(mut req) => {
                if !req.directory.is_empty() {
                    self.verify(&req.directory)?;
                }
                req.limit = req.limit.min(MAX_FIND_LIMIT);
                let mut result = b.find_in_files(&req).map_err(e)?;
                self.filter_find(&mut result);
                enc(&result)
            }
            HostCall::Detect => enc(&b.detect_project().map_err(e)?),
            HostCall::Php(p) => self.php(p),
            HostCall::Framework(f) => self.framework(f),
            HostCall::Ping
            | HostCall::Watch
            | HostCall::Git(_)
            | HostCall::GitAction(_)
            | HostCall::Lsp(_)
            | HostCall::TerminalOpen { .. }
            | HostCall::TerminalClose { .. }
            | HostCall::RunStart { .. }
            | HostCall::RunStop { .. } => Err("Unexpected request.".into()),
        }
    }

    /// Drop the hits in hidden files and count again, so the totals do not betray them.
    fn filter_find(&self, result: &mut forge_core::search::FindResult) {
        let truncated = result.truncated;
        result.files.retain(|f| self.allowed(&f.path));
        result.match_count = result
            .files
            .iter()
            .flat_map(|f| &f.matches)
            .map(|m| m.ranges.len().max(1))
            .sum();
        result.truncated = truncated && result.match_count > 0;
    }

    fn php(&self, call: PhpCall) -> Result<Vec<u8>, String> {
        let php = self.backend.php();
        let e = |err: forge_core::ForgeError| self.msg(err);
        match call {
            PhpCall::Index => enc(&php.index_workspace(Arc::new(|_, _| {})).map_err(e)?),
            PhpCall::Search {
                query,
                kind,
                limit,
                exact,
            } => {
                let hits = php
                    .search(&query, kind, limit.min(1000), exact)
                    .map_err(e)?;
                enc(&self.keep(hits, |h| &h.path))
            }
            PhpCall::Structure {
                path,
                content,
                inherited,
            } => {
                self.verify(&path)?;
                enc(&php.file_structure(&path, content, inherited).map_err(e)?)
            }
            PhpCall::Hover {
                path,
                offset,
                content,
            } => {
                self.verify(&path)?;
                enc(&php.hover(&path, offset, content).map_err(e)?)
            }
            PhpCall::GenerateTargets {
                path,
                offset,
                content,
            } => {
                self.verify(&path)?;
                enc(&php.generate_targets(&path, offset, content).map_err(e)?)
            }
            PhpCall::Generate {
                path,
                offset,
                kind,
                options,
                content,
            } => {
                self.verify(&path)?;
                enc(&php
                    .generate(&path, offset, kind, options, content)
                    .map_err(e)?)
            }
            PhpCall::Usages {
                path,
                offset,
                options,
                content,
            } => {
                self.verify(&path)?;
                self.scrubbed(&php.usages(&path, offset, options, content).map_err(e)?)
            }
            PhpCall::Highlights {
                path,
                offset,
                content,
            } => {
                self.verify(&path)?;
                self.scrubbed(&php.highlights(&path, offset, content).map_err(e)?)
            }
            PhpCall::Navigate {
                path,
                offset,
                kind,
                content,
            } => {
                self.verify(&path)?;
                self.scrubbed(&php.navigate(&path, offset, kind, content).map_err(e)?)
            }
            PhpCall::Definition {
                path,
                offset,
                content,
            } => {
                self.verify(&path)?;
                self.scrubbed(&php.definition(&path, offset, content).map_err(e)?)
            }
            PhpCall::LineMarkers { path, content } => {
                self.verify(&path)?;
                self.scrubbed(&php.line_markers(&path, content).map_err(e)?)
            }
            PhpCall::Hierarchy {
                path,
                offset,
                query,
                content,
            } => {
                self.verify(&path)?;
                self.scrubbed(&php.hierarchy(&path, offset, query, content).map_err(e)?)
            }
            PhpCall::Intentions {
                path,
                offset,
                options,
                content,
            } => {
                self.verify(&path)?;
                self.scrubbed(&php.intentions(&path, offset, options, content).map_err(e)?)
            }
            PhpCall::UnresolvedClasses { path, content } => {
                self.verify(&path)?;
                enc(&php.unresolved_classes(&path, content).map_err(e)?)
            }
            PhpCall::Inspect { path, content } => {
                self.verify(&path)?;
                enc(&php.inspect(&path, content).map_err(e)?)
            }
            PhpCall::Complete {
                path,
                offset,
                content,
            } => {
                self.verify(&path)?;
                enc(&php.complete(&path, offset, content).map_err(e)?)
            }
            PhpCall::LanguagePlaces => enc(&php.language_places().map_err(e)?),
            PhpCall::LspConfig { .. } | PhpCall::DetectServers { .. } => {
                Err("Language servers run on the host.".into())
            }
            PhpCall::Configure { .. } => Err("The PHP include paths are set on the host.".into()),
            PhpCall::Libraries | PhpCall::LibraryList { .. } | PhpCall::LibraryRead { .. } => {
                Err("External libraries are not shared with guests.".into())
            }
        }
    }

    /// The reply of a query that may point into other files, without what the guest may not
    /// see (the same filter as framework analysis).
    fn scrubbed<T: Serialize>(&self, reply: &T) -> Result<Vec<u8>, String> {
        scrub_msgpack(&enc(reply)?, self.policy)
    }

    /// Framework analysis. Nothing a guest sends can run project code or name a program:
    /// Laravel routes always come from the static analysis (no `artisan`), and the guest's
    /// settings only name folders inside the project.
    fn framework(&self, call: FrameworkCall) -> Result<Vec<u8>, String> {
        let call = match call {
            FrameworkCall::LaravelRoutes { .. } => FrameworkCall::LaravelRoutes {
                source: "static".into(),
                php: String::new(),
            },
            FrameworkCall::LaravelArtisanCommands { .. } => {
                FrameworkCall::LaravelArtisanCommands { php: String::new() }
            }
            FrameworkCall::LaravelComplete { ref path, .. }
            | FrameworkCall::LaravelGoto { ref path, .. }
            | FrameworkCall::LaravelGutter { ref path, .. }
            | FrameworkCall::Related { ref path, .. }
            | FrameworkCall::TestTargets { ref path, .. } => {
                self.verify(path)?;
                call
            }
            FrameworkCall::NewTestPlan {
                ref path,
                ref composer_path,
                ..
            } => {
                self.verify(path)?;
                if !composer_path.is_empty() {
                    self.verify(composer_path)?;
                }
                call
            }
            FrameworkCall::Symfony {
                ref config,
                ref query,
            } => {
                for dir in [
                    &config.app_directory,
                    &config.templates_directory,
                    &config.console_path,
                    &config.container_xml,
                ] {
                    if !dir.is_empty() {
                        self.verify(dir)?;
                    }
                }
                if let SymfonyQuery::Complete { path, .. }
                | SymfonyQuery::Goto { path, .. }
                | SymfonyQuery::Gutter { path, .. } = query
                {
                    self.verify(path)?;
                }
                call
            }
            FrameworkCall::Doctrine { ref config, .. } => {
                for dir in config
                    .source_directories
                    .iter()
                    .chain(&config.mapping_directories)
                {
                    if !dir.is_empty() {
                        self.verify(dir)?;
                    }
                }
                call
            }
            FrameworkCall::LaravelLogs { ref path, .. } => {
                if !path.is_empty() {
                    self.verify(&format!("storage/logs/{path}"))?;
                }
                call
            }
            FrameworkCall::WordPress { ref query } => {
                use phpforge_fw_wordpress::api::Query as Wp;
                if let Wp::Complete { path, .. }
                | Wp::Goto { path, .. }
                | Wp::Gutter { path, .. }
                | Wp::Usages { path, .. }
                | Wp::Inspect { path, .. } = query
                {
                    self.verify(path)?;
                }
                call
            }
            FrameworkCall::Cms { ref query } => {
                use phpforge_fw_cms::api::Query as Cms;
                if let Cms::Complete { path, .. }
                | Cms::Goto { path, .. }
                | Cms::Gutter { path, .. } = query
                {
                    self.verify(path)?;
                }
                call
            }
            other => other,
        };
        let bytes = self.backend.framework(call).map_err(|e| self.msg(e))?;
        scrub_msgpack(&bytes, self.policy)
    }
}

/// The reply of a framework analysis without what the guest may not see: entries located in
/// hidden files are dropped, and the values of `env` completions (they come from `.env`) are
/// removed.
fn scrub_msgpack(bytes: &[u8], policy: &Policy) -> Result<Vec<u8>, String> {
    let mut value: serde_json::Value = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?;
    scrub_value(&mut value, policy);
    rmp_serde::to_vec_named(&value).map_err(|e| e.to_string())
}

/// Same for a JSON reply (language servers).
pub(crate) fn scrub_json(bytes: &[u8], policy: &Policy) -> Result<Vec<u8>, String> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    scrub_value(&mut value, policy);
    serde_json::to_vec(&value).map_err(|e| e.to_string())
}

fn scrub_value(value: &mut serde_json::Value, policy: &Policy) {
    use serde_json::Value;
    match value {
        Value::Array(items) => {
            items.retain(|item| !located_in_hidden_file(item, policy));
            items.iter_mut().for_each(|i| scrub_value(i, policy));
        }
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("env") {
                map.remove("detail");
                map.remove("documentation");
            }
            // A single location (a declaration) in a hidden file.
            map.values_mut().for_each(|v| {
                if located_in_hidden_file(v, policy) {
                    *v = Value::Null;
                }
            });
            map.values_mut().for_each(|v| scrub_value(v, policy));
        }
        _ => {}
    }
}

/// An item with a `path` or `uri`-less location in a file the guest may not see (or outside
/// the project).
fn located_in_hidden_file(item: &serde_json::Value, policy: &Policy) -> bool {
    let Some(path) = item.get("path").and_then(serde_json::Value::as_str) else {
        return false;
    };
    policy.check_path(path).is_err()
}

// ---- git -------------------------------------------------------------------------------

use crate::guest_backend::git::GitCall;

fn git_output(out: GitOutput) -> Result<Vec<u8>, String> {
    enc(&out)
}

/// Run a typed git call on the host's backend; see [`crate::guest_backend::git`].
fn run_git(
    backend: &dyn Backend,
    policy: &Policy,
    repo: &Mutex<Option<(PathBuf, String)>>,
    call: GitCall,
) -> Result<Vec<u8>, String> {
    let gate = Gate::new(backend, policy);
    let exec = backend.git();
    let project = &backend.info().root;
    let cached = lock(repo).clone();
    let (top, prefix) = match (&call, cached) {
        (GitCall::Query(GitQuery::Discover), _) | (_, None) => {
            match discover(&*exec, project, &gate)? {
                Ok(found) => {
                    *lock(repo) = Some(found.clone());
                    found
                }
                Err(failed) => return git_output(failed),
            }
        }
        (_, Some(cached)) => cached,
    };
    if let GitCall::Query(GitQuery::Discover) = call {
        let name = &backend.info().name;
        return git_output(GitOutput {
            ok: true,
            stdout: format!("{}\n{}\n", virtual_root(name).display(), prefix),
            stderr: String::new(),
        });
    }
    let allowed = |p: &str| policy.check_path(p).is_ok();
    let vis = Visible {
        prefix: &prefix,
        allowed: &allowed,
    };
    let refuse = |p: &str| format!("\"{p}\" is not a path in the project");
    match &call {
        GitCall::Query(q) => {
            let specs: Vec<&String> = match q {
                GitQuery::Diff { paths, .. }
                | GitQuery::TrackedAmong { paths }
                | GitQuery::LsFiles { paths }
                | GitQuery::LsTreeHead { paths } => paths.iter().collect(),
                GitQuery::Show { path, .. } | GitQuery::Blame { path, .. } => vec![path],
                GitQuery::Log { path: Some(p), .. } => vec![p],
                _ => Vec::new(),
            };
            for p in specs {
                if !vis.pathspec(p) {
                    return Err(refuse(p));
                }
            }
            // A file's content or blame needs a file, not the project folder.
            if let GitQuery::Show { path, .. } | GitQuery::Blame { path, .. } = q {
                if !vis.path(path) {
                    return Err(refuse(path));
                }
            }
        }
        GitCall::Action(a) => {
            let specs: &[String] = match a {
                GitAction::Stage { paths }
                | GitAction::AddNew { paths }
                | GitAction::Unstage { paths }
                | GitAction::UnstageInitial { paths }
                | GitAction::RestoreFromHead { paths }
                | GitAction::ForgetAdded { paths }
                | GitAction::Commit { paths, .. } => paths,
                _ => &[],
            };
            for p in specs {
                if !vis.pathspec(p) {
                    return Err(refuse(p));
                }
            }
        }
    }
    let (args, stdin, non_interactive) = match &call {
        GitCall::Query(q) => (q.to_args(), q.stdin(), false),
        GitCall::Action(a) => (a.to_args(), a.stdin(), a.non_interactive()),
    };
    let mut out = exec
        .exec(&GitRequest {
            cwd: top.to_string_lossy().into_owned(),
            args,
            stdin,
            non_interactive,
        })
        .map_err(|e| gate.msg(e))?;
    if let GitCall::Query(q) = &call {
        out.stdout = match q {
            GitQuery::Status => vis.status(&out.stdout),
            GitQuery::Diff { .. } => vis.diff(&out.stdout),
            GitQuery::CommitFiles { .. } => vis.name_status(&out.stdout),
            GitQuery::TrackedAmong { .. }
            | GitQuery::LsFiles { .. }
            | GitQuery::LsTreeHead { .. } => vis.names(&out.stdout),
            _ => out.stdout,
        };
    }
    out.stdout = gate.msg(out.stdout);
    out.stderr = gate.msg(out.stderr);
    git_output(out)
}

/// The repository root and the project's folder inside it. `Err` is git's own failure (not a
/// repository), which the guest's version control code reports.
fn discover(
    exec: &dyn GitExec,
    project: &Path,
    gate: &Gate<'_>,
) -> Result<Result<(PathBuf, String), GitOutput>, String> {
    let out = exec
        .exec(&GitRequest {
            cwd: project.to_string_lossy().into_owned(),
            args: GitQuery::Discover.to_args(),
            stdin: None,
            non_interactive: false,
        })
        .map_err(|e| gate.msg(e))?;
    if !out.ok {
        return Ok(Err(GitOutput {
            ok: false,
            stdout: String::new(),
            stderr: "not a git repository".into(),
        }));
    }
    let mut lines = out.stdout.lines();
    let top = PathBuf::from(lines.next().unwrap_or_default().trim());
    let prefix = lines.next().unwrap_or_default().trim().to_string();
    Ok(Ok((top, prefix)))
}

impl HostSession {
    /// A guest's edit or save of a shared document is held to the same rules as a file call:
    /// the file must be one the guest may change (visible, Files: Edit, and Full access for
    /// files that configure what runs). Other messages are not about a file's content.
    pub(crate) fn document_write_denied(
        &self,
        policy: &Policy,
        msg: &GuestMessage,
    ) -> Option<crate::permissions::Denied> {
        let (GuestMessage::DocUpdate { doc, .. } | GuestMessage::DocSave { doc, .. }) = msg else {
            return None;
        };
        let path = self.state().docs.get(doc).map(|d| d.path.clone())?;
        policy.check_write_path(&path).err()
    }

    /// Make sure the shared document of `path` exists for a guest who opens it: created from
    /// the file on disk if nobody has it open yet. The file is checked like any other call
    /// (symlinks resolved, hidden and oversized files refused).
    pub(crate) async fn ensure_doc(&self, policy: &Policy, path: &str) -> Result<(), String> {
        if self.state().by_path.contains_key(path) {
            return Ok(());
        }
        let backend = self.backend.clone();
        let (limit, policy_c, rel) = (self.max_file_size, policy.clone(), path.to_string());
        let disk = blocking(move || {
            let gate = Gate::new(&*backend, &policy_c);
            gate.verify(&rel)?;
            read_shared(&*backend, &rel, limit).map_err(|e| gate.msg(e))
        })
        .await?;
        let mut st = self.state();
        if !st.by_path.contains_key(path) {
            let id = DocId(st.next_doc);
            st.next_doc += 1;
            st.by_path.insert(path.to_string(), id);
            st.docs
                .insert(id, SharedDoc::new(id, path.to_string(), &disk, None));
        }
        Ok(())
    }

    /// Language server requests: synchronize the host's servers with the file's current text,
    /// then ask.
    async fn lsp(
        &self,
        from: ParticipantId,
        policy: &Policy,
        query: LspQuery,
    ) -> Result<Vec<u8>, String> {
        let services = lock(&self.calls.services)
            .clone()
            .ok_or_else(|| "Language servers are not available in this session.".to_string())?;
        let path = match &query {
            LspQuery::WorkspaceSymbols { .. } => None,
            LspQuery::Sync { path }
            | LspQuery::Close { path }
            | LspQuery::Hover { path, .. }
            | LspQuery::Completion { path, .. }
            | LspQuery::CompletionResolve { path, .. }
            | LspQuery::Definition { path, .. }
            | LspQuery::References { path, .. }
            | LspQuery::DocumentSymbols { path }
            | LspQuery::SignatureHelp { path, .. }
            | LspQuery::Format { path, .. } => Some(path.clone()),
        };
        if let Some(path) = &path {
            let mut open = lock(&self.calls.lsp_open);
            let files = open.entry(from).or_default();
            match &query {
                LspQuery::Close { .. } => {
                    files.remove(path);
                }
                LspQuery::Sync { .. } => {
                    if files.len() >= 256 && !files.contains(path) {
                        return Err("Too many open files.".into());
                    }
                    files.insert(path.clone());
                }
                _ => {}
            }
        }
        let text = match (&path, &query) {
            (Some(path), q) if !matches!(q, LspQuery::Close { .. }) => {
                Some(self.current_text(policy, path).await?)
            }
            _ => None,
        };
        let reply = services.lsp(LspRequest { query, text }).await?;
        scrub_json(&reply, policy)
    }

    /// The file's text as the host's servers should see it: the shared document, else the
    /// file on disk.
    async fn current_text(&self, policy: &Policy, path: &str) -> Result<String, String> {
        if let Some(text) = {
            let st = self.state();
            st.by_path.get(path).map(|id| st.docs[id].text())
        } {
            return Ok(text);
        }
        let backend = self.backend.clone();
        let policy = policy.clone();
        let path = path.to_string();
        let limit = self.max_file_size as u64;
        blocking(move || {
            let gate = Gate::new(&*backend, &policy);
            gate.verify(&path)?;
            gate.file_size_ok(&path, limit)?;
            Ok(backend
                .read_optional(&path)
                .map_err(|e| gate.msg(e))?
                .unwrap_or_default())
        })
        .await
    }
}
