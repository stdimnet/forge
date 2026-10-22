//! Shelve and unshelve through git: the patch of the selected changes (new files included)
//! goes to the [`Shelf`], then the changes are rolled back; unshelving applies the patch (or
//! some of its files) again, falling back to a three-way merge that leaves conflicts for the
//! conflict resolver.

use serde::Serialize;

use super::Repo;
use crate::cmd::{self, run_raw, Run};
use crate::patch::{apply_file_patch, join_patch, split_patch, PatchStatus};
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
        let mut patch = String::new();
        if !tracked.is_empty() {
            let mut args = vec!["--literal-pathspecs", "diff"];
            args.extend(DIFF);
            if reverse {
                args.push("-R");
            }
            args.extend(["HEAD", "--"]);
            args.extend(tracked.iter().map(String::as_str));
            patch.push_str(&self.git(&args)?);
        }
        for path in &untracked {
            let mut args = vec!["diff", "--no-index"];
            args.extend(DIFF);
            if reverse {
                args.push("-R");
            }
            args.extend(["--", "/dev/null", path.as_str()]);
            // Exit code 1 means "differences found".
            let out = run_raw(self.exec.as_ref(), &self.root, &args, Run::default())?;
            if !out.stdout.starts_with("diff --git") {
                return Err(cmd::error_from(&args, &out));
            }
            patch.push_str(&out.stdout);
        }
        Ok(patch)
    }

    /// Shelve Changes: store the changes of `paths` in the shelf as `name`, then remove them
    /// from the working tree and the index (new files are deleted).
    pub fn shelve(&self, shelf: &Shelf, paths: &[String], name: &str) -> Result<ShelvedChangelist> {
        let patch = self.changes_patch(paths)?;
        if patch.trim().is_empty() {
            return Err(ForgeError::Other("no changes to shelve".into()));
        }
        let branch = self.status()?.branch;
        let cl = shelf.create(name, branch.as_deref(), &patch)?;
        self.discard(&patch)?;
        Ok(self.to_workspace(cl))
    }

    /// Roll back what `patch` changes: files known to HEAD are restored, new files removed.
    fn discard(&self, patch: &str) -> Result<()> {
        let files = split_patch(patch);
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
        let files: Vec<_> = split_patch(&shelf.patch(id)?)
            .into_iter()
            .filter(|f| match &wanted {
                Some(w) => w.contains(&f.path),
                None => true,
            })
            .collect();
        if files.is_empty() {
            return Err(ForgeError::Other("nothing to unshelve".into()));
        }
        let patch = join_patch(&files);
        let check = run_raw(
            self.exec.as_ref(),
            &self.root,
            &["apply", "--check", "--whitespace=nowarn", "-"],
            Run {
                stdin: Some(patch.as_bytes()),
                ..Run::default()
            },
        )?;
        let added: Vec<&str> = files
            .iter()
            .filter(|f| f.status == PatchStatus::Added)
            .map(|f| f.path.as_str())
            .collect();
        if check.ok {
            self.git_stdin(&["apply", "--whitespace=nowarn", "-"], &patch)?;
            if !added.is_empty() {
                // New files come back as added files, like PHPStorm.
                let mut args = vec!["--literal-pathspecs", "add", "--"];
                args.extend(added.iter().copied());
                self.git(&args)?;
            }
        } else {
            let merged = run_raw(
                self.exec.as_ref(),
                &self.root,
                &["apply", "--3way", "--whitespace=nowarn", "-"],
                Run {
                    stdin: Some(patch.as_bytes()),
                    ..Run::default()
                },
            )?;
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
