//! Show History, Show History for Selection, Annotate Revision and Compare with Branch on a
//! folder: the git invocations (parsers in `crate::history`).

use forge_core::{ForgeError, Result};

use super::{check_rev, Repo};
use crate::blame::{parse_blame, Blame};
use crate::history::{
    parse_block_history, parse_file_history, BlockRevision, FileRevision, HistoryQuery,
};
use crate::log::{parse_name_status, ChangedFile};

/// Header of one commit in history output: a record separator, then the fields separated
/// by unit separators, then the per-commit part (name-status or patch).
const HISTORY_FORMAT: &str = "--format=%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s%x1f";

impl Repo {
    /// History of a file (following renames) or of a folder: the commits that changed it,
    /// newest first, with what each commit changed below the path.
    pub fn file_history(
        &self,
        path: &str,
        dir: bool,
        q: &HistoryQuery,
    ) -> Result<Vec<FileRevision>> {
        if !self.has_head() {
            return Ok(Vec::new());
        }
        let limit = if q.limit == 0 { 200 } else { q.limit };
        let spec = if path.is_empty() {
            if self.prefix.is_empty() {
                ".".to_string()
            } else {
                self.prefix.clone()
            }
        } else {
            self.ws_to_repo(path)
        };
        let mut args: Vec<String> = vec![
            "log".into(),
            "-z".into(),
            "--name-status".into(),
            "-M".into(),
            HISTORY_FORMAT.into(),
            format!("--max-count={limit}"),
            format!("--skip={}", q.skip),
        ];
        if !dir {
            args.push("--follow".into());
        }
        args.extend(["HEAD".into(), "--".into(), spec]);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut revs = parse_file_history(&self.git(&refs)?);
        for rev in &mut revs {
            for f in &mut rev.files {
                f.path = self.repo_to_ws(&f.path);
                f.orig_path = f.orig_path.as_deref().map(|p| self.repo_to_ws(p));
            }
        }
        Ok(revs)
    }

    /// History of lines `start..=end` (1-based, as in HEAD) of a file: every commit that
    /// changed them, with the block before and after (`git log -L`).
    pub fn block_history(
        &self,
        path: &str,
        start: u32,
        end: u32,
        limit: usize,
    ) -> Result<Vec<BlockRevision>> {
        if start == 0 || end < start {
            return Err(ForgeError::Other(format!(
                "invalid line range {start}..{end}"
            )));
        }
        if !self.has_head() {
            return Ok(Vec::new());
        }
        let limit = if limit == 0 { 100 } else { limit };
        let range = format!("-L{start},{end}:{}", self.ws_to_repo(path));
        let max = format!("--max-count={limit}");
        let out = self.git(&[
            "log",
            "--no-ext-diff",
            "--no-color",
            HISTORY_FORMAT,
            &max,
            &range,
            "HEAD",
        ])?;
        let mut revs = parse_block_history(&out);
        for r in &mut revs {
            r.path = self.repo_to_ws(&r.path);
        }
        Ok(revs)
    }

    /// Blame of `path` as it was in `rev` (Annotate Revision).
    pub fn blame_at(&self, rev: &str, path: &str) -> Result<Blame> {
        check_rev(rev)?;
        let rp = self.ws_to_repo(path);
        Ok(parse_blame(&self.git(&[
            "blame",
            "--porcelain",
            rev,
            "--",
            &rp,
        ])?))
    }

    /// Files below `path` (a folder, `""` for the workspace) that differ between `rev` and
    /// the working tree (Compare with Branch on a folder). Statuses read from `rev` to the
    /// working tree: `A` exists only locally, `D` only in `rev`.
    pub fn changes_since(&self, rev: &str, path: &str) -> Result<Vec<ChangedFile>> {
        check_rev(rev)?;
        let spec = if path.is_empty() {
            if self.prefix.is_empty() {
                ".".to_string()
            } else {
                self.prefix.clone()
            }
        } else {
            self.ws_to_repo(path)
        };
        let out = self.git(&[
            "diff",
            "--no-ext-diff",
            "-z",
            "--name-status",
            "-M",
            rev,
            "--",
            &spec,
        ])?;
        Ok(parse_name_status(&out)
            .into_iter()
            .map(|mut c| {
                c.path = self.repo_to_ws(&c.path);
                c.orig_path = c.orig_path.as_deref().map(|p| self.repo_to_ws(p));
                c
            })
            .collect())
    }
}
