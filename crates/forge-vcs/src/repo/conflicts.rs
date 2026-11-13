//! Merge conflicts: which files are unmerged, the stage versions (`:1:` base, `:2:` ours,
//! `:3:` theirs) for the merge window, and resolving them (Accept Yours / Accept Theirs /
//! the merged result).
//!
//! "Yours" is the user's side: `ours` in a merge, cherry-pick or unstash, but `theirs`
//! (the commit being replayed) in a rebase, like PHPStorm.

use serde::Serialize;

use super::Repo;
use crate::merge::{looks_binary, merge_three_way, normalize_eol, MergeModel};
use forge_core::{ForgeError, Result};

/// The operation that stopped with conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    /// No operation head: an unstash (`git stash pop/apply`) or `checkout -m`.
    Unstash,
}

/// How one side changed a conflicted file compared with the base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SideChange {
    Modified,
    Added,
    Deleted,
}

/// A conflicted file in the Conflicts dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    /// Workspace-relative path.
    pub path: String,
    pub yours: SideChange,
    pub theirs: SideChange,
}

/// The unmerged files and how to label the two sides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictSet {
    pub operation: MergeOperation,
    /// "Merging branch feature into branch main".
    pub description: String,
    /// Short names for the column headers: "Yours (main)", "Theirs (feature)".
    pub yours_name: String,
    pub theirs_name: String,
    /// Titles of the merge window panes: "Changes from main", "Changes from feature".
    pub yours_title: String,
    pub theirs_title: String,
    /// Yours is git's `theirs` (stage 3): rebases.
    pub reversed: bool,
    pub files: Vec<ConflictFile>,
}

/// Everything the merge window needs for one file. Texts use `\n` line breaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeFile {
    pub path: String,
    pub yours_title: String,
    pub theirs_title: String,
    /// `None` when the side has no version (added on both sides / deleted).
    pub base: Option<String>,
    pub yours: Option<String>,
    pub theirs: Option<String>,
    pub binary: bool,
    /// The file used `\r\n`; the merged result is written back with it.
    pub crlf: bool,
    /// Chunks of the three-way merge, for text files that exist on both sides.
    pub model: Option<MergeModel>,
}

/// Which side to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictSide {
    Yours,
    Theirs,
}

/// Stages present per unmerged path, from `git ls-files -u -z`
/// (`<mode> <object> <stage>\t<path>`), in the order git lists them.
pub fn parse_unmerged(out: &str) -> Vec<(String, [bool; 3])> {
    let mut list: Vec<(String, [bool; 3])> = Vec::new();
    for rec in out.split('\0').filter(|r| !r.is_empty()) {
        let Some((meta, path)) = rec.split_once('\t') else {
            continue;
        };
        let Some(stage) = meta
            .split(' ')
            .nth(2)
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|s| (1..=3).contains(s))
        else {
            continue;
        };
        match list.iter_mut().find(|(p, _)| p == path) {
            Some((_, stages)) => stages[stage - 1] = true,
            None => {
                let mut stages = [false; 3];
                stages[stage - 1] = true;
                list.push((path.to_string(), stages));
            }
        }
    }
    list
}

/// How a side changed the file, from the presence of the base and that side's stage.
fn side_change(base: bool, side: bool) -> SideChange {
    match (base, side) {
        (true, true) => SideChange::Modified,
        (false, _) => SideChange::Added,
        (true, false) => SideChange::Deleted,
    }
}

/// A branch name from `git name-rev` output: `remotes/origin/main~2` → `origin/main`.
fn clean_ref_name(name: &str) -> Option<String> {
    let name = name.trim();
    let name = name
        .strip_prefix("remotes/")
        .or_else(|| name.strip_prefix("tags/"))
        .unwrap_or(name);
    let name = name.split(['~', '^']).next().unwrap_or(name);
    (!name.is_empty() && name != "undefined").then(|| name.to_string())
}

/// `rebasing branch 'feature' on '1a2b3c4'` from the long `git status` text.
pub fn parse_rebase_status(text: &str) -> Option<(String, String)> {
    let rest = text.split("rebasing branch '").nth(1)?;
    let (branch, rest) = rest.split_once("' on '")?;
    let (onto, _) = rest.split_once('\'')?;
    Some((branch.to_string(), onto.to_string()))
}

const SHORT: usize = 7;

fn short(sha: &str) -> String {
    sha.chars().take(SHORT).collect()
}

impl Repo {
    /// The commit a ref points at, if it exists (`MERGE_HEAD`, `REBASE_HEAD`, …).
    fn ref_sha(&self, name: &str) -> Option<String> {
        self.git(&["rev-parse", "-q", "--verify", name])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// A readable name for a commit: its branch (or tag), else the short hash.
    fn name_of(&self, sha: &str) -> String {
        self.git(&["name-rev", "--name-only", "--no-undefined", sha])
            .ok()
            .and_then(|n| clean_ref_name(&n))
            .unwrap_or_else(|| short(sha))
    }

    fn current_branch_name(&self) -> String {
        self.git(&["symbolic-ref", "--short", "-q", "HEAD"])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| self.ref_sha("HEAD").map(|s| short(&s)))
            .unwrap_or_else(|| "HEAD".into())
    }

    /// Stages of the unmerged paths (repository paths) in the workspace.
    pub(super) fn unmerged(&self, paths: &[String]) -> Result<Vec<(String, [bool; 3])>> {
        let specs = self.pathspecs(paths);
        let mut args = vec!["--literal-pathspecs", "ls-files", "-u", "-z", "--"];
        if specs.is_empty() && !self.prefix.is_empty() {
            args.push(self.prefix.as_str());
        }
        args.extend(specs.iter().map(String::as_str));
        let mut list = parse_unmerged(&self.git(&args)?);
        list.retain(|(p, _)| self.in_workspace(p));
        Ok(list)
    }

    /// The unmerged files and the labels of both sides (the Conflicts dialog).
    pub fn conflicts(&self) -> Result<ConflictSet> {
        let current = self.current_branch_name();
        let (operation, description, yours_name, theirs_name, yours_title, theirs_title) =
            if let Some((sha, progress)) = self
                .ref_sha("REBASE_HEAD")
                .and_then(|sha| Some((sha, self.rebase_in_progress()?)))
            {
                let (branch, onto) = match progress.branch_onto {
                    Some((branch, onto)) => (branch, self.name_of(&onto)),
                    None => (self.name_of(&sha), "upstream".to_string()),
                };
                (
                    MergeOperation::Rebase,
                    format!("Rebasing branch {branch} onto {onto}"),
                    branch.clone(),
                    onto.clone(),
                    format!("Rebasing {} from {branch}", short(&sha)),
                    format!("Already rebased commits and commits from {onto}"),
                )
            } else if let Some(sha) = self.ref_sha("MERGE_HEAD") {
                let theirs = self.name_of(&sha);
                (
                    MergeOperation::Merge,
                    format!("Merging branch {theirs} into branch {current}"),
                    current.clone(),
                    theirs.clone(),
                    format!("Changes from {current}"),
                    format!("Changes from {theirs}"),
                )
            } else if let Some(sha) = self.ref_sha("CHERRY_PICK_HEAD") {
                (
                    MergeOperation::CherryPick,
                    format!(
                        "Cherry-picking commit {} into branch {current}",
                        short(&sha)
                    ),
                    current.clone(),
                    short(&sha),
                    format!("Changes from {current}"),
                    format!("Changes from {}", short(&sha)),
                )
            } else if let Some(sha) = self.ref_sha("REVERT_HEAD") {
                (
                    MergeOperation::Revert,
                    format!("Reverting commit {} in branch {current}", short(&sha)),
                    current.clone(),
                    format!("revert of {}", short(&sha)),
                    format!("Changes from {current}"),
                    format!("Changes from revert of {}", short(&sha)),
                )
            } else {
                (
                    MergeOperation::Unstash,
                    format!("Unstashing changes into branch {current}"),
                    current.clone(),
                    "stash".to_string(),
                    format!("Changes from {current}"),
                    "Changes from stash".to_string(),
                )
            };
        let reversed = operation == MergeOperation::Rebase;
        let files = self
            .unmerged(&[])?
            .into_iter()
            .map(|(path, [base, ours, theirs])| {
                let (yours, theirs) = if reversed {
                    (theirs, ours)
                } else {
                    (ours, theirs)
                };
                ConflictFile {
                    path: self.repo_to_ws(&path),
                    yours: side_change(base, yours),
                    theirs: side_change(base, theirs),
                }
            })
            .collect();
        Ok(ConflictSet {
            operation,
            description,
            yours_name,
            theirs_name,
            yours_title,
            theirs_title,
            reversed,
            files,
        })
    }

    /// Stage versions of one conflicted file plus its three-way merge (the merge window).
    pub fn merge_file(&self, path: &str) -> Result<MergeFile> {
        let set = self.conflicts()?;
        let stages = self
            .unmerged(&[path.to_string()])?
            .into_iter()
            .next()
            .map(|(_, s)| s)
            .ok_or_else(|| ForgeError::Other(format!("{path} has no merge conflict")))?;
        let read = |stage: usize| -> Result<Option<String>> {
            if stages[stage - 1] {
                self.show(&format!(":{stage}"), path)
            } else {
                Ok(None)
            }
        };
        let base = read(1)?;
        let (ours, theirs) = (read(2)?, read(3)?);
        let (yours, theirs) = if set.reversed {
            (theirs, ours)
        } else {
            (ours, theirs)
        };
        let binary = [&base, &yours, &theirs]
            .iter()
            .any(|t| t.as_deref().is_some_and(looks_binary));
        let crlf = [&yours, &theirs, &base]
            .iter()
            .find_map(|t| t.as_deref())
            .is_some_and(|t| t.contains("\r\n"));
        let norm = |t: Option<String>| t.map(|t| if binary { t } else { normalize_eol(&t).0 });
        let (base, yours, theirs) = (norm(base), norm(yours), norm(theirs));
        let model = match (&yours, &theirs) {
            (Some(y), Some(t)) if !binary => {
                Some(merge_three_way(base.as_deref().unwrap_or_default(), y, t))
            }
            _ => None,
        };
        Ok(MergeFile {
            path: path.to_string(),
            yours_title: set.yours_title,
            theirs_title: set.theirs_title,
            base,
            yours,
            theirs,
            binary,
            crlf,
            model,
        })
    }

    /// Accept Yours / Accept Theirs: take that side's version of each file (or delete the
    /// file when that side deleted it) and mark it resolved.
    pub fn accept_conflict_side(&self, paths: &[String], side: ConflictSide) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let reversed = self.ref_sha("REBASE_HEAD").is_some() && self.rebase_in_progress().is_some();
        // git's stage of the chosen side: 2 is `--ours`, 3 is `--theirs`.
        let ours = (side == ConflictSide::Yours) != reversed;
        let mut keep = Vec::new();
        let mut delete = Vec::new();
        for (path, stages) in self.unmerged(paths)? {
            if stages[if ours { 1 } else { 2 }] {
                keep.push(path);
            } else {
                delete.push(path);
            }
        }
        if !keep.is_empty() {
            let mut args = vec!["checkout", if ours { "--ours" } else { "--theirs" }, "--"];
            args.extend(keep.iter().map(String::as_str));
            self.git(&args)?;
            let mut args = vec!["add", "--"];
            args.extend(keep.iter().map(String::as_str));
            self.git(&args)?;
        }
        if !delete.is_empty() {
            let mut args = vec!["rm", "-q", "-f", "--"];
            args.extend(delete.iter().map(String::as_str));
            self.git(&args)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unmerged_stages() {
        let out = [
            "100644 aaa 1\tboth.php",
            "100644 bbb 2\tboth.php",
            "100644 ccc 3\tboth.php",
            "100644 ddd 1\tdeleted by them.php",
            "100644 eee 2\tdeleted by them.php",
            "100644 fff 2\tadded.php",
            "100644 ggg 3\tadded.php",
            "",
        ]
        .join("\0");
        let list = parse_unmerged(&out);
        assert_eq!(
            list,
            vec![
                ("both.php".to_string(), [true, true, true]),
                ("deleted by them.php".to_string(), [true, true, false]),
                ("added.php".to_string(), [false, true, true]),
            ]
        );
        assert_eq!(side_change(true, true), SideChange::Modified);
        assert_eq!(side_change(true, false), SideChange::Deleted);
        assert_eq!(side_change(false, true), SideChange::Added);
    }

    #[test]
    fn cleans_name_rev_output() {
        assert_eq!(clean_ref_name("feature/x\n").as_deref(), Some("feature/x"));
        assert_eq!(
            clean_ref_name("remotes/origin/main~2").as_deref(),
            Some("origin/main")
        );
        assert_eq!(clean_ref_name("tags/v1.0^0").as_deref(), Some("v1.0"));
        assert_eq!(clean_ref_name("undefined"), None);
    }

    #[test]
    fn parses_the_rebase_status_line() {
        let text = "interactive rebase in progress; onto 1a2b3c4\nYou are currently rebasing branch 'feature/x' on '1a2b3c4'.\n";
        assert_eq!(
            parse_rebase_status(text),
            Some(("feature/x".to_string(), "1a2b3c4".to_string()))
        );
        assert_eq!(parse_rebase_status("On branch main\n"), None);
    }
}
