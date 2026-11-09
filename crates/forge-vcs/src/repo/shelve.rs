//! Shelve and unshelve through git: the patch of the selected changes (new files included)
//! goes to the [`Shelf`], then the changes are rolled back; unshelving applies the patch (or
//! some of its files) again, falling back to a three-way merge that leaves conflicts for the
//! conflict resolver.

use serde::Serialize;

use super::Repo;
use crate::cmd::{self, run_raw, run_raw_bytes, Run};
use crate::patch::{apply_file_patch, split_patch, split_patch_bytes, PatchStatus};
use crate::shelf::{Shelf, ShelvedChangelist};
use forge_core::{ForgeError, Result};

/// Before and after of one shelved file (Show Diff in the Shelf tab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfFileContents {
    pub before: Option<String>,
    pub after: Option<String>,
    pub binary: bool,
}

const DIFF: [&str; 6] = [
    "--no-color",
    "--no-ext-diff",
    "--no-renames",
    "--binary",
    "--full-index",
    "--no-textconv",
];

impl Repo {
    /// A changelist with workspace-relative paths (files outside the workspace keep their
    /// repository path).
    fn to_workspace(&self, mut cl: ShelvedChangelist) -> ShelvedChangelist {
        for f in &mut cl.files {
            f.path = self.repo_to_ws(&f.path);
        }
        cl
    }

    /// The shelved changelists of `shelf`, newest first.
    pub fn shelf_list(&self, shelf: &Shelf) -> Vec<ShelvedChangelist> {
        shelf
            .list()
            .into_iter()
            .map(|cl| self.to_workspace(cl))
            .collect()
    }

    /// The patch of the changes in `paths` (workspace-relative files) against HEAD,
    /// unversioned files included as new files.
    pub fn changes_patch(&self, paths: &[String]) -> Result<String> {
        self.changes_patch_with(paths, false)
    }

    /// [`Repo::changes_patch`], optionally reversed (`git diff -R`, Create Patch's "Reverse
    /// patch").
    pub(super) fn changes_patch_with(&self, paths: &[String], reverse: bool) -> Result<String> {
        let (patch, _) = self.changes_patch_bytes(paths, reverse)?;
        Ok(String::from_utf8_lossy(&patch).into_owned())
    }

    /// [`Repo::changes_patch_with`] as raw bytes, plus whether the git transport kept them
    /// unchanged ([`crate::GitExec::exec_bytes`]).
    fn changes_patch_bytes(&self, paths: &[String], reverse: bool) -> Result<(Vec<u8>, bool)> {
        if !self.has_head() {
            return Err(ForgeError::Other(
                "changes can only be shelved after the first commit".into(),
            ));
        }
        let status = self.status()?;
        let mut tracked: Vec<String> = Vec::new();
        let mut untracked: Vec<String> = Vec::new();
        for e in status.entries.iter().filter(|e| paths.contains(&e.path)) {
            if e.conflict {
                return Err(ForgeError::Other(format!(
                    "{} has merge conflicts; resolve them first",
                    e.path
                )));
            }
            if e.is_untracked() {
                untracked.push(self.ws_to_repo(&e.path));
            } else {
                tracked.push(self.ws_to_repo(&e.path));
                if let Some(orig) = &e.orig_path {
                    tracked.push(self.ws_to_repo(orig));
                }
            }
        }
        let mut patch = Vec::new();
        let mut faithful = true;
        if !tracked.is_empty() {
            let mut args = vec!["--literal-pathspecs", "diff"];
            args.extend(DIFF);
            if reverse {
                args.push("-R");
            }
            args.extend(["HEAD", "--"]);
            args.extend(tracked.iter().map(String::as_str));
            let out = run_raw_bytes(self.exec.as_ref(), &self.root, &args, None)?;
            if !out.ok {
                return Err(cmd::error_from(&args, &out.into()));
            }
            faithful &= out.faithful;
            patch.extend(out.stdout);
        }
        for path in &untracked {
            let mut args = vec!["diff", "--no-index"];
            args.extend(DIFF);
            if reverse {
                args.push("-R");
            }
            args.extend(["--", "/dev/null", path.as_str()]);
            // Exit code 1 means "differences found".
            let out = run_raw_bytes(self.exec.as_ref(), &self.root, &args, None)?;
            if !out.stdout.starts_with(b"diff --git") {
                return Err(cmd::error_from(&args, &out.into()));
            }
            faithful &= out.faithful;
            patch.extend(out.stdout);
        }
        Ok((patch, faithful))
    }

    /// Shelve Changes: store the changes of `paths` in the shelf as `name`, then remove them
    /// from the working tree and the index (new files are deleted).
    ///
    /// The patch is kept as raw bytes, so files that are not UTF-8 (Windows-1252, Latin-1)
    /// shelve exactly. Nothing is rolled back unless the stored patch provably restores the
    /// working tree: a transport that can only carry text refuses such files, and the patch
    /// must apply in reverse to the files as they are now.
    pub fn shelve(&self, shelf: &Shelf, paths: &[String], name: &str) -> Result<ShelvedChangelist> {
        let (patch, faithful) = self.changes_patch_bytes(paths, false)?;
        if patch.trim_ascii().is_empty() {
            return Err(ForgeError::Other("no changes to shelve".into()));
        }
        if !faithful {
            if let Some(f) = split_patch_bytes(&patch)
                .into_iter()
                .find(|(f, _)| f.text.contains('\u{FFFD}'))
            {
                return Err(ForgeError::Other(format!(
                    "{} is not UTF-8 and cannot be shelved over this connection",
                    self.repo_to_ws(&f.0.path)
                )));
            }
        }
        // The rollback below is only safe when the stored patch can bring the changes back.
        let check = run_raw_bytes(
            self.exec.as_ref(),
            &self.root,
            &["apply", "-R", "--check", "--whitespace=nowarn", "-"],
            Some(&patch),
        )?;
        if !check.ok {
            return Err(ForgeError::Other(format!(
                "the changes could not be shelved safely, nothing was changed ({})",
                check.stderr.trim()
            )));
        }
        let branch = self.status()?.branch;
        let cl = shelf.create(name, branch.as_deref(), &patch)?;
        self.discard(&patch)?;
        Ok(self.to_workspace(cl))
    }

    /// Roll back what `patch` changes: files known to HEAD are restored, new files removed.
    fn discard(&self, patch: &[u8]) -> Result<()> {
        let files: Vec<_> = split_patch_bytes(patch)
            .into_iter()
            .map(|(f, _)| f)
            .collect();
        let (added, existing): (Vec<_>, Vec<_>) =
            files.iter().partition(|f| f.status == PatchStatus::Added);
        if !existing.is_empty() {
            let mut args = vec![
                "--literal-pathspecs",
                "restore",
                "--source=HEAD",
                "--staged",
                "--worktree",
                "--",
            ];
            args.extend(existing.iter().map(|f| f.path.as_str()));
            self.git(&args)?;
        }
        if !added.is_empty() {
            let mut args = vec![
                "--literal-pathspecs",
                "rm",
                "--cached",
                "-q",
                "-f",
                "--ignore-unmatch",
                "--",
            ];
            args.extend(added.iter().map(|f| f.path.as_str()));
            self.git(&args)?;
            let mut args = vec!["--literal-pathspecs", "clean", "-f", "-q", "--"];
            args.extend(added.iter().map(|f| f.path.as_str()));
            self.git(&args)?;
        }
        Ok(())
    }

    /// Unshelve: apply the changelist (or only `paths` of it) to the working tree. A patch that
    /// no longer applies cleanly is merged three-way; conflicts are left for the resolver
    /// (the error says so and the changes stay on the shelf). With `remove`, successfully
    /// applied files leave the shelf.
    pub fn unshelve(
        &self,
        shelf: &Shelf,
        id: &str,
        paths: Option<&[String]>,
        remove: bool,
    ) -> Result<String> {
        let wanted: Option<Vec<String>> =
            paths.map(|p| p.iter().map(|p| self.ws_to_repo(p)).collect());
        let (files, raw): (Vec<_>, Vec<_>) = split_patch_bytes(&shelf.patch_bytes(id)?)
            .into_iter()
            .filter(|(f, _)| match &wanted {
                Some(w) => w.contains(&f.path),
                None => true,
            })
            .unzip();
        if files.is_empty() {
            return Err(ForgeError::Other("nothing to unshelve".into()));
        }
        let patch: Vec<u8> = raw.concat();
        let check: cmd::Output = run_raw_bytes(
            self.exec.as_ref(),
            &self.root,
            &["apply", "--check", "--whitespace=nowarn", "-"],
            Some(&patch),
        )?
        .into();
        let added: Vec<&str> = files
            .iter()
            .filter(|f| f.status == PatchStatus::Added)
            .map(|f| f.path.as_str())
            .collect();
        if check.ok {
            let args = ["apply", "--whitespace=nowarn", "-"];
            let out = run_raw_bytes(self.exec.as_ref(), &self.root, &args, Some(&patch))?;
            if !out.ok {
                return Err(cmd::error_from(&args, &out.into()));
            }
            if !added.is_empty() {
                // New files come back as added files, like PHPStorm.
                let mut args = vec!["--literal-pathspecs", "add", "--"];
                args.extend(added.iter().copied());
                self.git(&args)?;
            }
        } else {
            let merged: cmd::Output = run_raw_bytes(
                self.exec.as_ref(),
                &self.root,
                &["apply", "--3way", "--whitespace=nowarn", "-"],
                Some(&patch),
            )?
            .into();
            if !merged.ok {
                let conflicted = self
                    .status()
                    .map(|st| st.entries.iter().any(|e| e.conflict))
                    .unwrap_or(false);
                return Err(if conflicted {
                    cmd::error_from(&["apply"], &merged)
                } else {
                    cmd::error_from(&["apply"], &check)
                });
            }
            // `--3way` also staged the files; keep modifications unstaged as before shelving.
            let modified: Vec<&str> = files
                .iter()
                .filter(|f| f.status != PatchStatus::Added)
                .map(|f| f.path.as_str())
                .collect();
            if !modified.is_empty() {
                let mut args = vec!["--literal-pathspecs", "reset", "-q", "--"];
                args.extend(modified.iter().copied());
                self.git(&args)?;
            }
        }
        if remove {
            let applied: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
            shelf.remove_files(id, &applied)?;
        }
        let n = files.len();
        Ok(format!(
            "{n} file{} unshelved",
            if n == 1 { "" } else { "s" }
        ))
    }

    /// One shelved file before and after its change.
    pub fn shelf_file(&self, shelf: &Shelf, id: &str, path: &str) -> Result<ShelfFileContents> {
        let repo_path = self.ws_to_repo(path);
        let file = split_patch(&shelf.patch(id)?)
            .into_iter()
            .find(|f| f.path == repo_path)
            .ok_or_else(|| {
                ForgeError::Other(format!("{path} is not in this shelved changelist"))
            })?;
        if file.binary {
            return Ok(ShelfFileContents {
                before: None,
                after: None,
                binary: true,
            });
        }
        let before = match (&file.old_blob, file.status) {
            (_, PatchStatus::Added) => None,
            (Some(blob), _) => {
                let out = run_raw(
                    self.exec.as_ref(),
                    &self.root,
                    &["cat-file", "blob", blob],
                    Run::default(),
                )?;
                if out.ok {
                    Some(out.stdout)
                } else {
                    self.show("HEAD", path)?
                }
            }
            (None, _) => self.show("HEAD", path)?,
        };
        let after = match file.status {
            PatchStatus::Deleted => None,
            _ => Some(
                apply_file_patch(before.as_deref().unwrap_or_default(), &file.text)
                    .map_err(ForgeError::Other)?,
            ),
        };
        Ok(ShelfFileContents {
            before,
            after,
            binary: false,
        })
    }
}
