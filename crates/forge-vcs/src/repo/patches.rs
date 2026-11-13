//! Git ▸ Patch: Create Patch from local changes or commits (paths relative to the project
//! root, like PHPStorm) and Apply Patch, which applies what fits and merges the rest three-way
//! so conflicts end up in the conflict resolver.

use serde::Serialize;

use super::Repo;
use crate::cmd::{run_raw, Output, Run};
use crate::patch::{
    join_base, join_patch, normalize_patch, split_patch, strip_path_prefix, PatchStatus,
};
use forge_core::{ForgeError, Result};

/// What Apply Patch did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPatchResult {
    /// Files the patch was applied to (workspace-relative).
    pub applied: Vec<String>,
    /// Files merged three-way that now have conflicts (for the conflict resolver).
    pub conflicts: Vec<String>,
    /// Files that could not be patched, with git's reason.
    pub failed: Vec<(String, String)>,
}

impl Repo {
    /// Create Patch from local changes: the changes of `paths` against HEAD (new files
    /// included), with paths relative to the project root. `reverse` swaps both sides.
    pub fn create_patch(&self, paths: &[String], reverse: bool) -> Result<String> {
        let patch = self.changes_patch_with(paths, reverse)?;
        Ok(strip_path_prefix(&patch, &self.prefix))
    }

    fn apply(&self, args: &[&str], patch: &str) -> Result<Output> {
        run_raw(
            self.exec.as_ref(),
            &self.root,
            args,
            Run {
                stdin: Some(patch.as_bytes()),
                ..Run::default()
            },
        )
    }

    /// Apply Patch: apply `patch` (git or foreign unified diff, paths relative to the project
    /// root) to the files under `base` (`""` = project root). `paths` limits it to some
    /// target files (as listed by the preview). Files that do not apply cleanly are merged
    /// three-way when the patch names their base versions; conflicts stay for the resolver.
    pub fn apply_patch(
        &self,
        patch: &str,
        base: &str,
        paths: Option<&[String]>,
    ) -> Result<ApplyPatchResult> {
        let base = base.trim_matches('/');
        let files: Vec<_> = split_patch(&normalize_patch(patch))
            .into_iter()
            .filter(|f| paths.is_none_or(|p| p.contains(&join_base(base, &f.path))))
            .collect();
        if files.is_empty() {
            return Err(ForgeError::Other("the patch changes no files".into()));
        }
        let dir = format!("{}{}", self.prefix, base);
        let dir = dir.trim_end_matches('/');
        let directory = format!("--directory={dir}");
        let mut args = vec!["apply", "--whitespace=nowarn"];
        if !dir.is_empty() {
            args.push(&directory);
        }
        let with = |extra: &[&'static str]| -> Vec<&str> {
            let mut a = args.clone();
            a.extend_from_slice(extra);
            a.push("-");
            a
        };
        let mut result = ApplyPatchResult::default();
        let target = |f: &crate::patch::PatchFile| join_base(base, &f.path);
        // Everything that fits goes in one go; the rest file by file.
        let fits: Vec<bool> = if self.apply(&with(&["--check"]), &join_patch(&files))?.ok {
            vec![true; files.len()]
        } else {
            files
                .iter()
                .map(|f| self.apply(&with(&["--check"]), &f.text).map(|o| o.ok))
                .collect::<Result<_>>()?
        };
        let clean: Vec<_> = files
            .iter()
            .zip(&fits)
            .filter(|(_, ok)| **ok)
            .map(|(f, _)| f)
            .collect();
        if !clean.is_empty() {
            let out = self.apply(&with(&[]), &join_patch(clean.iter().copied()))?;
            if !out.ok {
                return Err(crate::cmd::error_from(&["apply"], &out));
            }
            result.applied.extend(clean.iter().map(|f| target(f)));
        }
        for f in files
            .iter()
            .zip(&fits)
            .filter(|(_, ok)| !**ok)
            .map(|(f, _)| f)
        {
            let out = self.apply(&with(&["--3way"]), &f.text)?;
            let path = target(f);
            if out.ok {
                // A clean three-way merge is staged; keep it a local change like the rest.
                let rp = self.ws_to_repo(&path);
                self.git(&["--literal-pathspecs", "reset", "-q", "--", &rp])?;
                result.applied.push(path);
                continue;
            }
            let conflicted = self
                .status()?
                .entries
                .iter()
                .any(|e| e.conflict && e.path == path);
            if conflicted {
                result.conflicts.push(path);
            } else {
                let msg = format!("{}{}", out.stderr.trim(), out.stdout.trim());
                result.failed.push((path, msg));
            }
        }
        // New files become added files, like after unshelving.
        let added: Vec<String> = files
            .iter()
            .filter(|f| f.status == PatchStatus::Added)
            .map(target)
            .filter(|p| result.applied.contains(p))
            .map(|p| self.ws_to_repo(&p))
            .collect();
        if !added.is_empty() {
            let mut args = vec!["--literal-pathspecs", "add", "--"];
            args.extend(added.iter().map(String::as_str));
            self.git(&args)?;
        }
        Ok(result)
    }
}
