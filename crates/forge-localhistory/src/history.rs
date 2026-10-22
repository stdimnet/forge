//! The Local History of one project: recording change sets, the revisions of a file or
//! folder, contents at a revision, revert plans, selection history, Recent Changes and the
//! cleanup (retention in working days and a size limit).

use std::collections::{BTreeSet, HashMap};
use std::io;

use serde::{Deserialize, Serialize};

use crate::model::{
    is_under, ChangeKind, ChangeSet, Content, Entry, FileState, Limits, NewChange, NewEntry,
    StateKind,
};
use crate::storage::{content_hash, Storage};
use crate::text::{block, map_range, unified_patch, PatchFile};

const DAY_MS: i64 = 86_400_000;
/// Files listed per revision / recent change (the count is complete).
const MAX_LISTED_FILES: usize = 100;
/// Saves this close together are one entry in Recent Changes.
const SAVE_GROUP_MS: i64 = 60 * 60 * 1000;

/// A row of Show History: a change set (or label) that touched the file or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub id: u64,
    pub time: i64,
    pub kind: ChangeKind,
    pub name: String,
    /// The file's path in this revision (renames are followed); the folder for folders.
    pub path: String,
    /// Folder history: the changed files (at most 100).
    pub files: Vec<String>,
    pub file_count: usize,
}

/// A file's content at a revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionContent {
    pub state: StateKind,
    /// The text when `state` is `file` (UTF-8, invalid bytes replaced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// A file in a folder that differs between a revision and now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderChange {
    pub path: String,
    /// At the revision.
    pub then: StateKind,
    /// Now (as last recorded).
    pub now: StateKind,
}

/// A file a change touched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeFile {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub before: StateKind,
    /// `current` when nothing changed the file after the change.
    pub after: StateKind,
}

/// One line of Recent Changes: a change set, or several saves in a row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentChange {
    /// The first and last change set id.
    pub from: u64,
    pub to: u64,
    /// Time of the last change set.
    pub time: i64,
    pub kind: ChangeKind,
    pub name: String,
    pub files: Vec<String>,
    pub file_count: usize,
}

/// A revision of the selected lines (Show History for Selection).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRevision {
    pub id: u64,
    pub time: i64,
    pub kind: ChangeKind,
    pub name: String,
    pub path: String,
    /// The lines as they were (empty when the file did not exist).
    pub text: String,
    /// 1-based line of the block in that revision.
    pub start_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionHistory {
    /// The selected lines now.
    pub current: String,
    pub revisions: Vec<BlockRevision>,
}

/// What a revert does to one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertAction {
    Write(Vec<u8>),
    Delete,
    CreateDir,
    DeleteDir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertOp {
    pub path: String,
    pub action: RevertAction,
}

/// The steps of a revert, in the order to run them, and the paths whose old content is
/// not known.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RevertPlan {
    pub ops: Vec<RevertOp>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeStats {
    pub sets: usize,
    pub contents: usize,
}

/// The Local History of one project.
pub struct LocalHistory<S: Storage> {
    storage: S,
    sets: Vec<ChangeSet>,
    /// The last recorded state of every path.
    known: HashMap<String, FileState>,
    next_id: u64,
    /// Open operations: group → change set id.
    groups: HashMap<u64, u64>,
    pub limits: Limits,
}

impl<S: Storage> LocalHistory<S> {
    pub fn open(mut storage: S, limits: Limits) -> io::Result<Self> {
        let sets = storage.load()?;
        let mut h = LocalHistory {
            storage,
            next_id: sets.last().map_or(1, |s| s.id + 1),
            sets,
            known: HashMap::new(),
            groups: HashMap::new(),
            limits,
        };
        h.rebuild_known();
        Ok(h)
    }

    fn rebuild_known(&mut self) {
        self.known.clear();
        for s in &self.sets {
            for e in &s.entries {
                self.known.insert(e.path.clone(), e.after.clone());
            }
        }
    }

    pub fn sets(&self) -> &[ChangeSet] {
        &self.sets
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// The last recorded state of `path`.
    pub fn known(&self, path: &str) -> FileState {
        self.known.get(path).cloned().unwrap_or_default()
    }

    fn store(&mut self, content: Content) -> io::Result<FileState> {
        Ok(match content {
            Content::Unknown => FileState::Unknown,
            Content::Absent => FileState::Absent,
            Content::Dir => FileState::Dir,
            Content::Bytes(b) if self.limits.accepts(&b) => {
                let hash = content_hash(&b);
                self.storage.put_blob(&hash, &b)?;
                FileState::File(hash)
            }
            Content::Bytes(_) => FileState::Unknown,
        })
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.sets.binary_search_by_key(&id, |s| s.id).ok()
    }

    /// Index of the first set with an id of at least `rev`.
    fn index_from(&self, rev: u64) -> usize {
        self.sets.partition_point(|s| s.id < rev)
    }

    /// Record a change. Entries that change nothing are dropped; a change without entries
    /// records nothing. Returns the change set id.
    pub fn record(&mut self, change: NewChange) -> io::Result<Option<u64>> {
        let mut entries = Vec::new();
        for e in change.entries {
            if let Some(entry) = self.entry(e)? {
                entries.push(entry);
            }
        }
        if entries.is_empty() {
            return Ok(None);
        }
        for e in &entries {
            self.known.insert(e.path.clone(), e.after.clone());
        }
        let open = change
            .group
            .and_then(|g| self.groups.get(&g))
            .and_then(|id| self.index_of(*id));
        if let Some(i) = open {
            let set = &mut self.sets[i];
            for e in entries {
                match set.entries.iter_mut().find(|x| x.path == e.path) {
                    Some(x) => {
                        x.after = e.after;
                        if x.from.is_none() {
                            x.from = e.from;
                        }
                    }
                    None => set.entries.push(e),
                }
            }
            set.entries
                .retain(|x| x.before != x.after || x.from.is_some());
            let (id, copy) = (set.id, set.clone());
            self.storage.append(&copy)?;
            return Ok(Some(id));
        }
        let id = self.push(ChangeSet {
            id: 0,
            time: change.time,
            kind: change.kind,
            name: change.name,
            entries,
        })?;
        if let Some(g) = change.group {
            self.groups.insert(g, id);
        }
        Ok(Some(id))
    }

    fn entry(&mut self, e: NewEntry) -> io::Result<Option<Entry>> {
        if self.limits.is_excluded(&e.path) {
            return Ok(None);
        }
        let after = self.store(e.after)?;
        let mut before = self.store(e.before)?;
        if before == FileState::Unknown {
            before = self.known(&e.path);
        }
        if (before == after && e.from.is_none())
            || (before == FileState::Unknown && after == FileState::Unknown)
        {
            return Ok(None);
        }
        Ok(Some(Entry {
            path: e.path,
            before,
            after,
            from: e.from,
        }))
    }

    fn push(&mut self, mut set: ChangeSet) -> io::Result<u64> {
        set.id = self.next_id;
        self.next_id += 1;
        self.storage.append(&set)?;
        let id = set.id;
        self.sets.push(set);
        Ok(id)
    }

    /// The operation of `group` is over: later changes start a new change set.
    pub fn close_group(&mut self, group: u64) {
        self.groups.remove(&group);
    }

    /// The content of a file as Forge reads it (opening it): kept as the "before" of later
    /// changes. A content that differs from the recorded one is an external change, which
    /// is recorded and its id returned.
    pub fn baseline(&mut self, path: &str, bytes: &[u8], time: i64) -> io::Result<Option<u64>> {
        if self.limits.is_excluded(path) {
            return Ok(None);
        }
        let state = self.store(Content::Bytes(bytes.to_vec()))?;
        let known = self.known(path);
        if state == FileState::Unknown || state == known {
            return Ok(None);
        }
        if matches!(known, FileState::File(_) | FileState::Absent) {
            return self.record(NewChange {
                kind: ChangeKind::External,
                name: "External change".into(),
                time,
                group: None,
                entries: vec![NewEntry::new(
                    path,
                    Content::Unknown,
                    Content::Bytes(bytes.to_vec()),
                )],
            });
        }
        self.known.insert(path.to_string(), state.clone());
        let entry = Entry {
            path: path.to_string(),
            before: state.clone(),
            after: state,
            from: None,
        };
        // Baselines in a row share one change set.
        if let Some(last) = self
            .sets
            .last_mut()
            .filter(|s| s.kind == ChangeKind::Baseline)
        {
            last.entries.retain(|e| e.path != path);
            last.entries.push(entry);
            let copy = last.clone();
            self.storage.append(&copy)?;
            return Ok(None);
        }
        self.push(ChangeSet {
            id: 0,
            time,
            kind: ChangeKind::Baseline,
            name: String::new(),
            entries: vec![entry],
        })?;
        Ok(None)
    }

    /// Put Label…: a named point in time for the whole project.
    pub fn put_label(&mut self, name: &str, time: i64) -> io::Result<u64> {
        self.push(ChangeSet {
            id: 0,
            time,
            kind: ChangeKind::Label,
            name: name.to_string(),
            entries: Vec::new(),
        })
    }

    // ---- queries ------------------------------------------------------------------------

    /// The state of `path` just before change set `rev` (for a label: at the label);
    /// `None` when nothing changed the path since (it is as it is now).
    pub fn state_at(&self, path: &str, rev: u64) -> Option<FileState> {
        let i = self.index_from(rev);
        let e = self.sets[i..].iter().find_map(|s| s.entry(path))?;
        if e.before != FileState::Unknown {
            return Some(e.before.clone());
        }
        // Not known then: the state after the last earlier change of the path.
        Some(
            self.sets[..i]
                .iter()
                .rev()
                .find_map(|s| s.entry(path).map(|e| e.after.clone()))
                .unwrap_or_default(),
        )
    }

    fn bytes_of(&self, state: &FileState) -> io::Result<Option<Vec<u8>>> {
        match state {
            FileState::File(h) => self.storage.get_blob(h),
            _ => Ok(None),
        }
    }

    /// The content of `path` at revision `rev`.
    pub fn content(&self, path: &str, rev: u64) -> io::Result<RevisionContent> {
        let Some(state) = self.state_at(path, rev) else {
            return Ok(RevisionContent {
                state: StateKind::Current,
                text: None,
            });
        };
        match self.bytes_of(&state)? {
            Some(b) => Ok(RevisionContent {
                state: StateKind::File,
                text: Some(String::from_utf8_lossy(&b).into_owned()),
            }),
            None if state.hash().is_some() => Ok(RevisionContent {
                state: StateKind::Unknown,
                text: None,
            }),
            None => Ok(RevisionContent {
                state: state.kind(),
                text: None,
            }),
        }
    }

    /// Show History: the revisions of a file (following renames) or a folder (`""` is the
    /// project), newest first, with the labels in between. `filter` keeps revisions whose
    /// name or (for files) content contains it, ignoring case.
    pub fn history(&self, path: &str, dir: bool, filter: &str) -> io::Result<Vec<Revision>> {
        let mut rows = Vec::new();
        let mut cur = path.to_string();
        for s in self.sets.iter().rev() {
            let row = |path: &str, files: Vec<String>, count: usize| Revision {
                id: s.id,
                time: s.time,
                kind: s.kind,
                name: s.name.clone(),
                path: path.to_string(),
                files,
                file_count: count,
            };
            match s.kind {
                ChangeKind::Baseline => {}
                ChangeKind::Label => rows.push(row(&cur, Vec::new(), 0)),
                _ if dir => {
                    let files: Vec<String> = s
                        .entries
                        .iter()
                        .filter(|e| is_under(&e.path, path) && e.after != FileState::Dir)
                        .filter(|e| e.before != FileState::Dir)
                        .map(|e| e.path.clone())
                        .collect();
                    if !files.is_empty() {
                        let count = files.len();
                        rows.push(row(
                            path,
                            files.into_iter().take(MAX_LISTED_FILES).collect(),
                            count,
                        ));
                    }
                }
                _ => {
                    if let Some(e) = s.entry(&cur) {
                        rows.push(row(&cur, vec![cur.clone()], 1));
                        if let Some(from) = &e.from {
                            cur = from.clone();
                        }
                    }
                }
            }
        }
        // Labels older than the first recorded change say nothing about the path.
        while rows.last().is_some_and(|r| r.kind == ChangeKind::Label) {
            rows.pop();
        }
        let needle = filter.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(rows);
        }
        let mut kept = Vec::new();
        for r in rows {
            let by_name = r.name.to_lowercase().contains(&needle);
            let by_text = !dir
                && self
                    .content(&r.path, r.id)?
                    .text
                    .is_some_and(|t| t.to_lowercase().contains(&needle));
            if by_name || by_text {
                kept.push(r);
            }
        }
        Ok(kept)
    }

    /// Paths below `dir` that changes since `rev` touched.
    fn touched_since(&self, dir: &str, rev: u64) -> BTreeSet<String> {
        self.sets[self.index_from(rev)..]
            .iter()
            .filter(|s| s.kind != ChangeKind::Baseline)
            .flat_map(|s| s.entries.iter())
            .filter(|e| is_under(&e.path, dir))
            .map(|e| e.path.clone())
            .collect()
    }

    /// Folder history: the files that differ between revision `rev` and now.
    pub fn folder_changes(&self, dir: &str, rev: u64) -> Vec<FolderChange> {
        self.touched_since(dir, rev)
            .into_iter()
            .filter_map(|path| {
                let then = self.state_at(&path, rev)?;
                let now = self.known(&path);
                if then == now
                    || then == FileState::Unknown
                    || then == FileState::Dir
                    || now == FileState::Dir
                {
                    return None;
                }
                Some(FolderChange {
                    path,
                    then: then.kind(),
                    now: now.kind(),
                })
            })
            .collect()
    }

    /// Revert a file to revision `rev`. `rev_path` is the file's path in that revision
    /// (renames); the content goes to `path`.
    pub fn revert_file(&self, path: &str, rev_path: &str, rev: u64) -> io::Result<RevertPlan> {
        let mut plan = RevertPlan::default();
        match self.state_at(rev_path, rev) {
            None => {}
            Some(FileState::File(h)) => match self.storage.get_blob(&h)? {
                Some(b) => plan.ops.push(RevertOp {
                    path: path.to_string(),
                    action: RevertAction::Write(b),
                }),
                None => plan.skipped.push(path.to_string()),
            },
            Some(FileState::Absent) => plan.ops.push(RevertOp {
                path: path.to_string(),
                action: RevertAction::Delete,
            }),
            Some(_) => plan.skipped.push(path.to_string()),
        }
        Ok(plan)
    }

    /// Revert a folder (`""` = the project) to revision `rev`: changed files get their old
    /// content back, deleted ones are recreated and new ones deleted.
    pub fn revert_folder(&self, dir: &str, rev: u64) -> io::Result<RevertPlan> {
        self.plan(self.touched_since(dir, rev), rev)
    }

    fn plan(&self, paths: impl IntoIterator<Item = String>, rev: u64) -> io::Result<RevertPlan> {
        let mut plan = RevertPlan::default();
        for path in paths {
            let Some(then) = self.state_at(&path, rev) else {
                continue;
            };
            let now = self.known(&path);
            if then == now && then != FileState::Unknown {
                continue;
            }
            let action = match then {
                FileState::File(h) => match self.storage.get_blob(&h)? {
                    Some(b) => RevertAction::Write(b),
                    None => {
                        plan.skipped.push(path);
                        continue;
                    }
                },
                FileState::Absent if now == FileState::Dir => RevertAction::DeleteDir,
                FileState::Absent => RevertAction::Delete,
                FileState::Dir => RevertAction::CreateDir,
                FileState::Unknown => {
                    plan.skipped.push(path);
                    continue;
                }
            };
            plan.ops.push(RevertOp { path, action });
        }
        let depth = |p: &str| p.matches('/').count();
        let rank = |a: &RevertAction| match a {
            RevertAction::CreateDir => 0,
            RevertAction::Write(_) => 1,
            RevertAction::Delete => 2,
            RevertAction::DeleteDir => 3,
        };
        plan.ops.sort_by(|a, b| {
            rank(&a.action).cmp(&rank(&b.action)).then_with(|| {
                if a.action == RevertAction::DeleteDir {
                    depth(&b.path).cmp(&depth(&a.path))
                } else {
                    depth(&a.path).cmp(&depth(&b.path))
                }
            })
        });
        Ok(plan)
    }

    /// Show History for Selection: how lines `start..=end` (1-based) of `current` looked in
    /// earlier revisions; only revisions where they differ are listed.
    pub fn selection_history(
        &self,
        path: &str,
        current: &str,
        start: usize,
        end: usize,
    ) -> io::Result<SelectionHistory> {
        let (mut s, mut e) = (start.max(1) - 1, end.max(start.max(1)));
        let mut newer = current.to_string();
        let now = block(&newer, s, e);
        let mut prev = now.clone();
        let mut revisions = Vec::new();
        for r in self.history(path, false, "")? {
            if r.kind == ChangeKind::Label {
                continue;
            }
            let c = self.content(&r.path, r.id)?;
            let row = |text: String, line: usize| BlockRevision {
                id: r.id,
                time: r.time,
                kind: r.kind,
                name: r.name.clone(),
                path: r.path.clone(),
                text,
                start_line: line,
            };
            let older = match (c.state, c.text) {
                (StateKind::File, Some(t)) => t,
                (StateKind::Current, _) => continue,
                (StateKind::Absent, _) => {
                    if !prev.is_empty() {
                        revisions.push(row(String::new(), 1));
                    }
                    break;
                }
                _ => break,
            };
            let (ns, ne) = map_range(&older, &newer, s, e);
            let b = block(&older, ns, ne);
            if b != prev {
                revisions.push(row(b.clone(), ns + 1));
            }
            prev = b;
            newer = older;
            (s, e) = (ns, ne);
        }
        Ok(SelectionHistory {
            current: now,
            revisions,
        })
    }

    /// Recent Changes: the latest change sets and labels, newest first; saves in a row are
    /// one entry.
    pub fn recent_changes(&self, limit: usize) -> Vec<RecentChange> {
        let mut out: Vec<RecentChange> = Vec::new();
        let mut files_of: Vec<BTreeSet<String>> = Vec::new();
        for s in self.sets.iter().rev() {
            if s.kind == ChangeKind::Baseline {
                continue;
            }
            let paths = s
                .entries
                .iter()
                .filter(|e| e.after != FileState::Dir && e.before != FileState::Dir)
                .map(|e| e.path.clone());
            let joins = s.kind == ChangeKind::Save
                && out.last().is_some_and(|last| {
                    last.kind == ChangeKind::Save && last.time - s.time < SAVE_GROUP_MS
                });
            if joins {
                let last = out.last_mut().expect("checked");
                last.from = s.id;
                files_of.last_mut().expect("parallel").extend(paths);
                continue;
            }
            if out.len() >= limit {
                break;
            }
            out.push(RecentChange {
                from: s.id,
                to: s.id,
                time: s.time,
                kind: s.kind,
                name: s.name.clone(),
                files: Vec::new(),
                file_count: 0,
            });
            files_of.push(paths.collect());
        }
        for (r, files) in out.iter_mut().zip(files_of) {
            r.file_count = files.len();
            r.files = files.into_iter().take(MAX_LISTED_FILES).collect();
        }
        out
    }

    fn change_paths(&self, from: u64, to: u64) -> Vec<(String, Option<String>)> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for s in self.sets[self.index_from(from)..]
            .iter()
            .take_while(|s| s.id <= to)
            .filter(|s| s.kind != ChangeKind::Baseline)
        {
            for e in &s.entries {
                if e.after == FileState::Dir || e.before == FileState::Dir {
                    continue;
                }
                if seen.insert(e.path.clone()) {
                    out.push((e.path.clone(), e.from.clone()));
                }
            }
        }
        out.sort();
        out
    }

    /// The files change sets `from..=to` touched, with their state before and after.
    pub fn change_files(&self, from: u64, to: u64) -> Vec<ChangeFile> {
        self.change_paths(from, to)
            .into_iter()
            .map(|(path, origin)| ChangeFile {
                before: self
                    .state_at(&path, from)
                    .map_or(StateKind::Current, |s| s.kind()),
                after: self
                    .state_at(&path, to + 1)
                    .map_or(StateKind::Current, |s| s.kind()),
                path,
                from: origin,
            })
            .collect()
    }

    /// Revert of a Recent Changes entry: the files it touched get their state before it.
    pub fn revert_change(&self, from: u64, to: u64) -> io::Result<RevertPlan> {
        self.plan(
            self.change_paths(from, to).into_iter().map(|(p, _)| p),
            from,
        )
    }

    /// Create Patch: the difference between revision `rev` and now, as a unified diff.
    /// `current` reads a file as it is now (`None` = it does not exist).
    pub fn patch(
        &self,
        path: &str,
        dir: bool,
        rev_path: &str,
        rev: u64,
        mut current: impl FnMut(&str) -> Option<String>,
    ) -> io::Result<String> {
        let mut files = Vec::new();
        let pairs: Vec<(String, String)> = if dir {
            self.folder_changes(path, rev)
                .into_iter()
                .map(|c| (c.path.clone(), c.path))
                .collect()
        } else {
            vec![(path.to_string(), rev_path.to_string())]
        };
        for (now_path, then_path) in pairs {
            let then = self.content(&then_path, rev)?;
            let before = match then.state {
                StateKind::File => then.text,
                StateKind::Absent => None,
                _ => continue,
            };
            files.push(PatchFile {
                path: now_path.clone(),
                before,
                after: current(&now_path),
            });
        }
        Ok(unified_patch(&files))
    }

    /// Cleanup: keep the change sets of the last `keep_days` days with changes ("working
    /// days"), then drop the oldest until the stored contents fit in `max_bytes`, and delete
    /// contents nothing refers to any more.
    pub fn purge(&mut self, keep_days: u32, max_bytes: u64) -> io::Result<PurgeStats> {
        let before = self.sets.len();
        let mut days: Vec<i64> = self
            .sets
            .iter()
            .map(|s| s.time.div_euclid(DAY_MS))
            .collect();
        days.dedup();
        days.sort_unstable();
        days.dedup();
        let keep = keep_days.max(1) as usize;
        if days.len() > keep {
            let first_kept = days[days.len() - keep];
            self.sets
                .retain(|s| s.time.div_euclid(DAY_MS) >= first_kept);
        }
        let sizes = self.storage.blob_sizes()?;
        let mut refs: HashMap<String, usize> = HashMap::new();
        for s in &self.sets {
            for h in set_hashes(s) {
                *refs.entry(h).or_default() += 1;
            }
        }
        let mut total: u64 = refs
            .keys()
            .map(|h| sizes.get(h).copied().unwrap_or(0))
            .sum();
        let mut drop = 0;
        while total > max_bytes && drop + 1 < self.sets.len() {
            for h in set_hashes(&self.sets[drop]) {
                let n = refs.get_mut(&h).expect("counted");
                *n -= 1;
                if *n == 0 {
                    refs.remove(&h);
                    total = total.saturating_sub(sizes.get(&h).copied().unwrap_or(0));
                }
            }
            drop += 1;
        }
        self.sets.drain(..drop);
        let removed_sets = before - self.sets.len();
        if removed_sets > 0 {
            self.storage.rewrite(&self.sets)?;
            self.rebuild_known();
            self.groups.clear();
        }
        let mut contents = 0;
        for h in sizes.keys().filter(|h| !refs.contains_key(*h)) {
            self.storage.delete_blob(h)?;
            contents += 1;
        }
        Ok(PurgeStats {
            sets: removed_sets,
            contents,
        })
    }
}

fn set_hashes(s: &ChangeSet) -> BTreeSet<String> {
    s.entries
        .iter()
        .flat_map(|e| [e.before.hash(), e.after.hash()])
        .flatten()
        .map(String::from)
        .collect()
}
