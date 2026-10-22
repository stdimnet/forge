//! [`Repo`]: a git repository driven through the git command line.

mod conflicts;
mod history;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_core::{ForgeError, Result};
use serde::Serialize;

use crate::branch::{self, parse_recent_checkouts, parse_refs, Branches};
use crate::cmd::{self, run, run_raw, GitExec, LocalGit, Run};
use crate::log::{self, parse_log, parse_name_status, Commit, CommitDetails, LogQuery};
use crate::manage::{
    append_ignore_patterns, ignore_patterns_for, parse_remotes, parse_stash_list, Remote,
    StashEntry, STASH_FORMAT,
};
use crate::status::{parse_status, RepoStatus};
use crate::{parse_blame, parse_diff, Blame, FileDiff};

pub use conflicts::{
    parse_rebase_status, parse_unmerged, ConflictFile, ConflictSet, ConflictSide, MergeFile,
    MergeOperation, SideChange,
};

/// Summary of the repository a workspace belongs to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    /// Absolute repository root.
    pub root: String,
    pub branch: Option<String>,
    pub head: Option<String>,
}

/// Result of a network operation (push/pull/fetch): git's human-readable report.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteResult {
    pub output: String,
}

/// A git repository seen from a workspace directory.
#[derive(Clone)]
pub struct Repo {
    /// Repository top level (where commands run).
    root: PathBuf,
    /// Workspace location inside the repository, `""` or `"sub/dir/"`.
    prefix: String,
    /// Where git runs (this machine or a remote host).
    exec: Arc<dyn GitExec>,
}

impl std::fmt::Debug for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repo")
            .field("root", &self.root)
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

impl Repo {
    /// Discover the repository containing `workspace_root`.
    pub fn open(workspace_root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(workspace_root, Arc::new(LocalGit))
    }

    /// Discover the repository containing `workspace_root`, running git through `exec`
    /// (e.g. on the remote host where `workspace_root` lives).
    pub fn open_with(workspace_root: impl AsRef<Path>, exec: Arc<dyn GitExec>) -> Result<Self> {
        let dir = workspace_root.as_ref();
        let out = run_raw(
            exec.as_ref(),
            dir,
            &["rev-parse", "--show-toplevel", "--show-prefix"],
            Run::default(),
        )?;
        if !out.ok {
            return Err(ForgeError::Other(format!(
                "{} is not inside a git repository",
                dir.display()
            )));
        }
        let mut lines = out.stdout.lines();
        let root = PathBuf::from(lines.next().unwrap_or_default().trim());
        let prefix = lines.next().unwrap_or_default().trim().to_string();
        Ok(Self { root, prefix, exec })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        run(self.exec.as_ref(), &self.root, args, Run::default())
    }

    fn git_stdin(&self, args: &[&str], input: &str) -> Result<String> {
        run(
            self.exec.as_ref(),
            &self.root,
            args,
            Run {
                stdin: Some(input.as_bytes()),
                ..Run::default()
            },
        )
    }

    fn git_remote(&self, args: &[&str]) -> Result<RemoteResult> {
        let out = run_raw(
            self.exec.as_ref(),
            &self.root,
            args,
            Run {
                non_interactive: true,
                ..Run::default()
            },
        )?;
        if !out.ok {
            return Err(cmd::error_from(args, &out));
        }
        let text = format!("{}{}", out.stdout, out.stderr);
        Ok(RemoteResult {
            output: text.trim().to_string(),
        })
    }

    /// The tracked files among `paths`, as workspace-relative paths. A directory in `paths`
    /// stands for every tracked file below it, so callers can tell a folder that holds
    /// tracked files from one that does not. Paths are matched literally.
    pub fn tracked_among(&self, paths: &[String]) -> Result<Vec<String>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let specs = self.pathspecs(paths);
        let mut args = vec!["--literal-pathspecs", "ls-files", "-z", "--cached", "--"];
        args.extend(specs.iter().map(String::as_str));
        Ok(self
            .git(&args)?
            .split('\0')
            .filter(|p| !p.is_empty() && self.in_workspace(p))
            .map(|p| self.repo_to_ws(p))
            .collect())
    }

    /// Workspace-relative path to a repository path.
    fn ws_to_repo(&self, path: &str) -> String {
        format!("{}{}", self.prefix, path.trim_start_matches("./"))
    }

    /// Repository path to a workspace-relative one (paths outside the workspace keep a
    /// `../`-free repository path so they stay identifiable).
    fn repo_to_ws(&self, path: &str) -> String {
        path.strip_prefix(&self.prefix).unwrap_or(path).to_string()
    }

    fn in_workspace(&self, path: &str) -> bool {
        path.starts_with(&self.prefix)
    }

    fn pathspecs(&self, paths: &[String]) -> Vec<String> {
        paths.iter().map(|p| self.ws_to_repo(p)).collect()
    }

    fn has_head(&self) -> bool {
        run_raw(
            self.exec.as_ref(),
            &self.root,
            &["rev-parse", "--verify", "-q", "HEAD"],
            Run::default(),
        )
        .map(|o| o.ok)
        .unwrap_or(false)
    }

    pub fn info(&self) -> Result<RepoInfo> {
        let st = self.status()?;
        Ok(RepoInfo {
            root: self.root.display().to_string(),
            branch: st.branch,
            head: st.head,
        })
    }

    // ---- status -------------------------------------------------------------------------

    pub fn status(&self) -> Result<RepoStatus> {
        let out = self.git(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
        ])?;
        let mut st = parse_status(&out);
        st.entries.retain(|e| self.in_workspace(&e.path));
        for e in &mut st.entries {
            e.path = self.repo_to_ws(&e.path);
            e.orig_path = e.orig_path.as_deref().map(|p| self.repo_to_ws(p));
        }
        Ok(st)
    }

    // ---- content and diffs --------------------------------------------------------------

    /// Content of `path` at `rev` (e.g. `HEAD`, or `""` for the index). `None` if the file
    /// does not exist there (new file, unborn branch).
    pub fn show(&self, rev: &str, path: &str) -> Result<Option<String>> {
        let spec = format!("{rev}:{}", self.ws_to_repo(path));
        let out = run_raw(
            self.exec.as_ref(),
            &self.root,
            &["show", &spec],
            Run::default(),
        )?;
        Ok(out.ok.then_some(out.stdout))
    }

    /// Unified diff text. `staged` compares index to HEAD, otherwise working tree to index.
    pub fn diff_text(&self, paths: &[String], staged: bool) -> Result<String> {
        let mut args: Vec<String> = vec![
            "diff".into(),
            "--no-ext-diff".into(),
            "--no-color".into(),
            "-M".into(),
        ];
        if staged {
            args.push("--cached".into());
        }
        args.push("--".into());
        if paths.is_empty() {
            args.push(if self.prefix.is_empty() {
                ".".into()
            } else {
                self.prefix.clone()
            });
        } else {
            args.extend(self.pathspecs(paths));
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.git(&refs)?;
        Ok(self.strip_diff_prefix(out))
    }

    /// Unified diff of the working tree against HEAD (staged and unstaged together).
    pub fn diff_head_text(&self, paths: &[String]) -> Result<String> {
        if !self.has_head() {
            return self.diff_text(paths, true);
        }
        let mut args: Vec<String> = vec![
            "diff".into(),
            "--no-ext-diff".into(),
            "--no-color".into(),
            "-M".into(),
            "HEAD".into(),
            "--".into(),
        ];
        args.extend(self.pathspecs(paths));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.git(&refs)?;
        Ok(self.strip_diff_prefix(out))
    }

    pub fn diff(&self, paths: &[String], staged: bool) -> Result<Vec<FileDiff>> {
        Ok(parse_diff(&self.diff_text(paths, staged)?))
    }

    fn strip_diff_prefix(&self, out: String) -> String {
        if self.prefix.is_empty() {
            return out;
        }
        out.replace(&format!("a/{}", self.prefix), "a/")
            .replace(&format!("b/{}", self.prefix), "b/")
    }

    // ---- blame --------------------------------------------------------------------------

    /// Blame `path`. With `contents`, blames that buffer instead of the file on disk, so
    /// unsaved edits show up as uncommitted lines.
    pub fn blame(&self, path: &str, contents: Option<&str>) -> Result<Blame> {
        let rp = self.ws_to_repo(path);
        let out = match contents {
            Some(c) => {
                self.git_stdin(&["blame", "--porcelain", "--contents", "-", "--", &rp], c)?
            }
            None => self.git(&["blame", "--porcelain", "--", &rp])?,
        };
        Ok(parse_blame(&out))
    }

    // ---- history ------------------------------------------------------------------------

    pub fn log(&self, q: &LogQuery) -> Result<Vec<Commit>> {
        if !self.has_head() {
            return Ok(Vec::new());
        }
        let limit = if q.limit == 0 { 200 } else { q.limit };
        let mut args: Vec<String> = vec![
            "log".into(),
            log::LOG_FORMAT.into(),
            "--decorate=full".into(),
            "--date-order".into(),
            format!("--max-count={limit}"),
            format!("--skip={}", q.skip),
        ];
        if let Some(t) = q.text.as_deref().filter(|t| !t.is_empty()) {
            args.push("-i".into());
            args.push(format!("--grep={t}"));
        }
        if let Some(a) = q.author.as_deref().filter(|t| !t.is_empty()) {
            args.push(format!("--author={a}"));
        }
        match q.rev.as_deref().filter(|r| !r.is_empty()) {
            Some(rev) => {
                if rev.starts_with('-') {
                    return Err(ForgeError::Other(format!("invalid revision: {rev}")));
                }
                args.push(rev.into());
            }
            None if q.all => {
                args.push("--branches".into());
                args.push("--remotes".into());
                args.push("--tags".into());
                args.push("HEAD".into());
            }
            None => args.push("HEAD".into()),
        }
        args.push("--".into());
        if let Some(p) = q.path.as_deref().filter(|p| !p.is_empty()) {
            args.push(self.ws_to_repo(p));
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        Ok(parse_log(&self.git(&refs)?))
    }

    pub fn commit_details(&self, sha: &str) -> Result<CommitDetails> {
        check_rev(sha)?;
        let meta = self.git(&[
            "show",
            "-s",
            "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ct%x1f%B",
            sha,
        ])?;
        let f: Vec<&str> = meta.splitn(8, '\x1f').collect();
        if f.len() < 8 {
            return Err(ForgeError::Other(format!(
                "unexpected git show output for {sha}"
            )));
        }
        let parents: Vec<String> = f[1].split_whitespace().map(str::to_string).collect();
        let tree = match parents.first() {
            Some(p) => self.git(&["diff-tree", "-r", "-z", "--name-status", "-M", p, sha])?,
            None => self.git(&["diff-tree", "-r", "-z", "--name-status", "--root", sha])?,
        };
        let files = parse_name_status(&tree)
            .into_iter()
            .map(|mut c| {
                c.path = self.repo_to_ws(&c.path);
                c.orig_path = c.orig_path.as_deref().map(|p| self.repo_to_ws(p));
                c
            })
            .collect();
        Ok(CommitDetails {
            sha: f[0].to_string(),
            parents,
            author: f[2].to_string(),
            email: f[3].to_string(),
            time: f[4].parse().unwrap_or(0),
            committer: f[5].to_string(),
            commit_time: f[6].parse().unwrap_or(0),
            message: f[7].trim_end().to_string(),
            files,
        })
    }

    // ---- branches -----------------------------------------------------------------------

    pub fn branches(&self) -> Result<Branches> {
        let out = self.git(&[
            "for-each-ref",
            branch::REF_FORMAT,
            "--sort=-committerdate",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])?;
        let mut b = parse_refs(&out);
        if let Ok(reflog) = self.git(&["reflog", "--format=%gs", "-n", "300"]) {
            b.recent = parse_recent_checkouts(&reflog, &b.local, b.current.as_deref());
        }
        Ok(b)
    }

    pub fn checkout(&self, rev: &str) -> Result<()> {
        check_rev(rev)?;
        self.git(&["checkout", rev, "--"]).map(drop)
    }

    /// Check out a remote branch (`origin/feature`): switch to the local branch of the same
    /// name, creating it with tracking if needed.
    pub fn checkout_remote(&self, remote_branch: &str) -> Result<()> {
        check_rev(remote_branch)?;
        let local = remote_branch
            .split_once('/')
            .map(|(_, b)| b)
            .unwrap_or(remote_branch);
        let exists = self.branches()?.local.iter().any(|b| b.name == local);
        if exists {
            self.checkout(local)
        } else {
            self.git(&["checkout", "-b", local, "--track", remote_branch])
                .map(drop)
        }
    }

    pub fn create_branch(&self, name: &str, start: Option<&str>, checkout: bool) -> Result<()> {
        check_rev(name)?;
        let mut args = if checkout {
            vec!["checkout", "-b", name]
        } else {
            vec!["branch", name]
        };
        if let Some(s) = start.filter(|s| !s.is_empty()) {
            check_rev(s)?;
            args.push(s);
        }
        self.git(&args).map(drop)
    }

    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        check_rev(name)?;
        self.git(&["branch", if force { "-D" } else { "-d" }, name])
            .map(drop)
    }

    pub fn rename_branch(&self, old: &str, new: &str) -> Result<()> {
        check_rev(old)?;
        check_rev(new)?;
        self.git(&["branch", "-m", old, new]).map(drop)
    }

    pub fn merge(&self, rev: &str) -> Result<String> {
        check_rev(rev)?;
        self.git(&["merge", "--no-edit", rev])
    }

    pub fn rebase(&self, onto: &str) -> Result<String> {
        check_rev(onto)?;
        self.git(&["rebase", onto])
    }

    // ---- index and working tree ---------------------------------------------------------

    pub fn stage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let specs = self.pathspecs(paths);
        let mut args = vec!["add", "-A", "--"];
        args.extend(specs.iter().map(String::as_str));
        self.git(&args).map(drop)
    }

    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let specs = self.pathspecs(paths);
        let mut args = if self.has_head() {
            vec!["restore", "--staged", "--"]
        } else {
            vec!["rm", "--cached", "-r", "-q", "--"]
        };
        args.extend(specs.iter().map(String::as_str));
        self.git(&args).map(drop)
    }

    /// Discard local changes (PHPStorm "Rollback"): files known to HEAD are restored in the
    /// index and working tree; newly added files are unstaged but kept on disk. Untracked
    /// files are left alone.
    pub fn rollback(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let specs = self.pathspecs(paths);
        let in_head: Vec<String> = if self.has_head() {
            let mut args = vec!["ls-tree", "-r", "-z", "--name-only", "HEAD", "--"];
            args.extend(specs.iter().map(String::as_str));
            self.git(&args)?
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let indexed: Vec<String> = {
            let mut args = vec!["ls-files", "-z", "--cached", "--"];
            args.extend(specs.iter().map(String::as_str));
            self.git(&args)?
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        };
        let added: Vec<&String> = indexed.iter().filter(|p| !in_head.contains(p)).collect();
        if !in_head.is_empty() {
            let mut args = vec!["restore", "--source=HEAD", "--staged", "--worktree", "--"];
            args.extend(in_head.iter().map(String::as_str));
            self.git(&args)?;
        }
        if !added.is_empty() {
            let mut args = vec!["rm", "--cached", "-f", "-q", "--"];
            args.extend(added.iter().map(|s| s.as_str()));
            self.git(&args)?;
        }
        Ok(())
    }

    /// Commit and return the new HEAD sha. With `paths`, exactly those files are committed
    /// (untracked ones are added first) regardless of what else is staged, like PHPStorm's
    /// changelist checkboxes; without, the index is committed as is.
    pub fn commit(&self, message: &str, amend: bool, paths: Option<&[String]>) -> Result<String> {
        self.commit_with(message, amend, paths, false)
    }

    /// [`Repo::commit`] that can add a `Signed-off-by` trailer (`git commit --signoff`,
    /// Settings | Version Control | Git, "Sign-off commits").
    pub fn commit_with(
        &self,
        message: &str,
        amend: bool,
        paths: Option<&[String]>,
        sign_off: bool,
    ) -> Result<String> {
        if message.trim().is_empty() && !amend {
            return Err(ForgeError::Other("commit message is empty".into()));
        }
        let specs = paths.map(|p| self.pathspecs(p)).unwrap_or_default();
        if !specs.is_empty() {
            // Make new files known so `--only` can commit them; deletions are fine as is.
            let untracked: Vec<String> = self
                .status()?
                .entries
                .into_iter()
                .filter(|e| e.is_untracked())
                .map(|e| self.ws_to_repo(&e.path))
                .filter(|p| specs.contains(p))
                .collect();
            if !untracked.is_empty() {
                let mut args = vec!["add", "--"];
                args.extend(untracked.iter().map(String::as_str));
                self.git(&args)?;
            }
        }
        let mut args = vec!["commit", "-q", "--cleanup=strip"];
        if amend {
            args.push("--amend");
        }
        if sign_off {
            args.push("--signoff");
        }
        if message.trim().is_empty() {
            args.push("--no-edit");
        } else {
            args.extend(["-F", "-"]);
        }
        if !specs.is_empty() {
            args.extend(["--only", "--"]);
            args.extend(specs.iter().map(String::as_str));
        }
        self.git_stdin(&args, message)?;
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_string())
    }

    // ---- ignore rules, tags and stashes -------------------------------------------------

    /// Add `paths` (workspace-relative files or directories) to the `.gitignore` at the
    /// repository root, like PHPStorm's "Add to .gitignore". Returns the patterns written;
    /// patterns the file already has are not repeated.
    pub fn add_to_gitignore(&self, paths: &[String]) -> Result<Vec<String>> {
        let repo_paths: Vec<String> = paths
            .iter()
            .map(|p| self.ws_to_repo(p.trim_matches('/')))
            .collect();
        if paths.iter().any(|p| p.trim_matches('/').is_empty()) {
            return Err(ForgeError::Other("invalid path: \"\"".into()));
        }
        let patterns = ignore_patterns_for(&repo_paths, |p| self.root.join(p).is_dir())?;
        let file = self.root.join(".gitignore");
        let existing = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(ForgeError::io(&file, e)),
        };
        if let Some(text) = append_ignore_patterns(&existing, &patterns) {
            std::fs::write(&file, text).map_err(|e| ForgeError::io(&file, e))?;
        }
        Ok(patterns)
    }

    /// Create a tag at `rev` (default HEAD); with a message it is an annotated tag.
    pub fn create_tag(&self, name: &str, rev: Option<&str>, message: Option<&str>) -> Result<()> {
        check_rev(name)?;
        let mut args = vec!["tag"];
        let message = message.map(str::trim).filter(|m| !m.is_empty());
        if message.is_some() {
            args.extend(["-a", "-F", "-"]);
        }
        args.push(name);
        if let Some(rev) = rev {
            check_rev(rev)?;
            args.push(rev);
        }
        self.git_stdin(&args, message.unwrap_or_default()).map(drop)
    }

    /// Stash local changes ("Stash Changes…"). Returns git's report, e.g. "No local
    /// changes to save".
    pub fn stash(
        &self,
        message: &str,
        keep_index: bool,
        include_untracked: bool,
    ) -> Result<String> {
        let mut args = vec!["stash", "push"];
        if keep_index {
            args.push("--keep-index");
        }
        if include_untracked {
            args.push("--include-untracked");
        }
        let message = message.trim();
        if !message.is_empty() {
            args.extend(["-m", message]);
        }
        Ok(self.git(&args)?.trim().to_string())
    }

    /// The stash list, most recent first.
    pub fn stash_list(&self) -> Result<Vec<StashEntry>> {
        Ok(parse_stash_list(&self.git(&[
            "stash",
            "list",
            STASH_FORMAT,
        ])?))
    }

    /// Apply the stash at `index` ("Unstash Changes…"); `pop` also drops it, and
    /// `reinstate_index` restores what was staged.
    pub fn unstash(&self, index: usize, pop: bool, reinstate_index: bool) -> Result<String> {
        let selector = format!("stash@{{{index}}}");
        let mut args = vec!["stash", if pop { "pop" } else { "apply" }];
        if reinstate_index {
            args.push("--index");
        }
        args.push(&selector);
        Ok(self.git(&args)?.trim().to_string())
    }

    /// Drop the stash at `index` without applying it.
    pub fn drop_stash(&self, index: usize) -> Result<()> {
        let selector = format!("stash@{{{index}}}");
        self.git(&["stash", "drop", "-q", &selector]).map(drop)
    }

    // ---- remotes ------------------------------------------------------------------------

    pub fn remotes(&self) -> Result<Vec<String>> {
        Ok(self
            .git(&["remote"])?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Push the current branch; sets the upstream on first push.
    pub fn push(&self, force: bool) -> Result<RemoteResult> {
        let st = self.status()?;
        let mut args = vec!["push"];
        if force {
            args.push("--force-with-lease");
        }
        let remote;
        if st.upstream.is_none() {
            let remotes = self.remotes()?;
            remote = remotes
                .iter()
                .find(|r| *r == "origin")
                .or(remotes.first())
                .cloned()
                .ok_or_else(|| ForgeError::Other("no remote is configured".into()))?;
            if st.branch.is_none() {
                return Err(ForgeError::Other("cannot push a detached HEAD".into()));
            }
            args.extend(["-u", remote.as_str(), "HEAD"]);
        }
        self.git_remote(&args)
    }

    /// Pull the current branch: `Some(true)` rebases, `Some(false)` merges, `None` follows
    /// the branch's own configuration (`pull.rebase`, `branch.<name>.rebase`).
    pub fn pull(&self, rebase: Option<bool>) -> Result<RemoteResult> {
        let mut args = vec!["pull", "--no-edit"];
        match rebase {
            Some(true) => args.push("--rebase"),
            Some(false) => args.push("--no-rebase"),
            None => {}
        }
        self.git_remote(&args)
    }

    /// The effective value of a git config key, if set (`git config --get`).
    pub fn config_value(&self, key: &str) -> Option<String> {
        self.git(&["config", "--get", key])
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    pub fn fetch(&self) -> Result<RemoteResult> {
        self.git_remote(&["fetch", "--all", "--prune"])
    }

    /// `git remote -v` output (`name<TAB>url (fetch|push)` lines).
    pub fn remote_verbose(&self) -> Result<String> {
        self.git(&["remote", "-v"])
    }

    /// Configured remotes with their URLs ("Manage Remotes…").
    pub fn remote_list(&self) -> Result<Vec<Remote>> {
        Ok(parse_remotes(&self.remote_verbose()?))
    }

    pub fn add_remote(&self, name: &str, url: &str) -> Result<()> {
        check_rev(name)?;
        let url = url.trim();
        if url.is_empty() || url.starts_with('-') {
            return Err(ForgeError::Other(format!("invalid remote URL: {url:?}")));
        }
        self.git(&["remote", "add", name, url]).map(drop)
    }

    pub fn remove_remote(&self, name: &str) -> Result<()> {
        check_rev(name)?;
        self.git(&["remote", "remove", name]).map(drop)
    }

    /// Fetch one refspec from a remote, e.g. `+refs/pull/7/head:refs/heads/pr-7`.
    pub fn fetch_refspec(&self, remote: &str, refspec: &str) -> Result<RemoteResult> {
        check_rev(remote)?;
        check_rev(refspec)?;
        self.git_remote(&["fetch", remote, refspec])
    }

    /// Whether a local branch exists.
    pub fn has_branch(&self, name: &str) -> bool {
        run_raw(
            self.exec.as_ref(),
            &self.root,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ],
            Run::default(),
        )
        .map(|o| o.ok)
        .unwrap_or(false)
    }
}

/// Reject revisions that git would parse as options.
fn check_rev(rev: &str) -> Result<()> {
    if rev.is_empty() || rev.starts_with('-') || rev.contains('\0') {
        Err(ForgeError::Other(format!("invalid revision: {rev:?}")))
    } else {
        Ok(())
    }
}
