//! Options of the Git menu dialogs (Pull, Merge, Rebase, Reset HEAD, New Tag) and the git
//! command lines they stand for. Pure, so the combinations PHPStorm forbids are checked and
//! tested without a repository.

use serde::{Deserialize, Serialize};

/// The Pull dialog ("Pull to main"): remote, branches and "Modify options".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PullOptions {
    pub remote: String,
    /// Branches of the remote to pull (`main`, not `origin/main`); empty pulls the upstream.
    pub branches: Vec<String>,
    pub rebase: bool,
    pub ff_only: bool,
    pub no_ff: bool,
    pub squash: bool,
    pub no_commit: bool,
    pub no_verify: bool,
}

/// The Merge dialog ("Merge into main").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MergeOptions {
    /// Branches (or commits) to merge into the current branch.
    pub branches: Vec<String>,
    pub no_ff: bool,
    pub ff_only: bool,
    pub squash: bool,
    /// `-m`: the merge commit message.
    pub message: Option<String>,
    pub no_commit: bool,
    pub allow_unrelated_histories: bool,
    pub no_verify: bool,
}

/// The Rebase dialog: `git rebase [--onto <onto>] <from> [<branch>]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RebaseOptions {
    /// Branch to rebase; empty means the current branch.
    pub branch: Option<String>,
    /// The new base (upstream without `from`; with `--root` optional).
    pub onto: String,
    /// `--onto` mode: the commits after `from` are moved onto `onto`.
    pub from: Option<String>,
    pub root: bool,
    pub rebase_merges: bool,
    pub keep_empty: bool,
    pub update_refs: bool,
}

/// Reset HEAD: how far the reset goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetMode {
    Soft,
    #[default]
    Mixed,
    Hard,
    Keep,
}

/// The New Tag dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TagOptions {
    pub name: String,
    /// Commit to tag; empty means HEAD.
    pub commit: Option<String>,
    /// With a message the tag is annotated.
    pub message: Option<String>,
    pub force: bool,
}

/// What to do with an operation that stopped (Continue Rebase, Abort Merge, Skip Commit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationAction {
    Continue,
    Abort,
    Skip,
}

/// Reject names git would read as options or that are empty.
pub fn check_name(what: &str, name: &str) -> Result<(), String> {
    if name.is_empty() || name.starts_with('-') || name.contains(['\0', '\n']) {
        Err(format!("invalid {what}: {name:?}"))
    } else {
        Ok(())
    }
}

fn some(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// `pull --no-edit [options] <remote> <branch>…`.
pub fn pull_args(o: &PullOptions) -> Result<Vec<String>, String> {
    if o.rebase && (o.ff_only || o.no_ff || o.squash || o.no_commit) {
        return Err(
            "--rebase cannot be combined with --ff-only, --no-ff, --squash or --no-commit".into(),
        );
    }
    if o.ff_only && (o.no_ff || o.squash) {
        return Err("--ff-only cannot be combined with --no-ff or --squash".into());
    }
    if o.no_ff && o.squash {
        return Err("--squash cannot be combined with --no-ff".into());
    }
    let mut args: Vec<String> = vec!["pull".into(), "--no-edit".into()];
    let flags = [
        (o.rebase, "--rebase"),
        (o.ff_only, "--ff-only"),
        (o.no_ff, "--no-ff"),
        (o.squash, "--squash"),
        (o.no_commit, "--no-commit"),
        (o.no_verify, "--no-verify"),
    ];
    args.extend(
        flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, f)| f.to_string()),
    );
    if !o.rebase {
        args.push("--no-rebase".into());
    }
    let remote = o.remote.trim();
    if !remote.is_empty() {
        check_name("remote", remote)?;
        args.push(remote.into());
        for b in o
            .branches
            .iter()
            .map(|b| b.trim())
            .filter(|b| !b.is_empty())
        {
            check_name("branch", b)?;
            // `origin/main` picked from the remote branches means `main` of that remote.
            let b = b.strip_prefix(&format!("{remote}/")).unwrap_or(b);
            args.push(b.into());
        }
    } else if !o.branches.is_empty() {
        return Err("choose the remote to pull from".into());
    }
    Ok(args)
}

/// "Update" of a local branch that is not checked out: `fetch <remote> <merge>:refs/heads/<name>`.
/// Without `+` git only fast-forwards the branch and rejects the fetch when it has diverged.
/// `merge` is the branch's `branch.<name>.merge` (a full ref name on the remote).
pub fn update_branch_args(remote: &str, merge: &str, name: &str) -> Result<Vec<String>, String> {
    check_name("remote", remote)?;
    check_name("branch", merge)?;
    check_name("branch", name)?;
    if merge.starts_with('+') || merge.contains(':') || name.contains(':') {
        return Err(format!("invalid branch: {merge:?}"));
    }
    Ok(vec![
        "fetch".into(),
        remote.into(),
        format!("{merge}:refs/heads/{name}"),
    ])
}

/// `merge [options] <branch>…`.
pub fn merge_args(o: &MergeOptions) -> Result<Vec<String>, String> {
    if o.branches.iter().all(|b| b.trim().is_empty()) {
        return Err("choose the branch to merge".into());
    }
    if o.ff_only && (o.no_ff || o.squash || some(&o.message).is_some()) {
        return Err("--ff-only cannot be combined with --no-ff, --squash or -m".into());
    }
    if o.squash && (o.no_ff || some(&o.message).is_some()) {
        return Err("--squash cannot be combined with --no-ff or -m".into());
    }
    let mut args: Vec<String> = vec!["merge".into()];
    let flags = [
        (o.no_ff, "--no-ff"),
        (o.ff_only, "--ff-only"),
        (o.squash, "--squash"),
        (o.no_commit, "--no-commit"),
        (o.allow_unrelated_histories, "--allow-unrelated-histories"),
        (o.no_verify, "--no-verify"),
    ];
    args.extend(
        flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, f)| f.to_string()),
    );
    match some(&o.message) {
        Some(m) => args.extend(["-m".into(), m.to_string()]),
        None => args.push("--no-edit".into()),
    }
    for b in o
        .branches
        .iter()
        .map(|b| b.trim())
        .filter(|b| !b.is_empty())
    {
        check_name("branch", b)?;
        args.push(b.into());
    }
    Ok(args)
}

/// `rebase [options] [--root | [--onto <onto>] <from>] [<branch>]`.
pub fn rebase_args(o: &RebaseOptions) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec!["rebase".into()];
    let flags = [
        (o.rebase_merges, "--rebase-merges"),
        (o.keep_empty, "--keep-empty"),
        (o.update_refs, "--update-refs"),
    ];
    args.extend(
        flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, f)| f.to_string()),
    );
    let onto = o.onto.trim();
    if o.root {
        args.push("--root".into());
        if !onto.is_empty() {
            check_name("revision", onto)?;
            args.extend(["--onto".into(), onto.into()]);
        }
    } else {
        check_name("revision", onto)?;
        match some(&o.from) {
            Some(from) => {
                check_name("revision", from)?;
                args.extend(["--onto".into(), onto.into(), from.into()]);
            }
            None => args.push(onto.into()),
        }
    }
    if let Some(branch) = some(&o.branch) {
        check_name("branch", branch)?;
        args.push(branch.into());
    }
    Ok(args)
}

/// `reset --soft|--mixed|--hard|--keep <rev>`.
pub fn reset_args(rev: &str, mode: ResetMode) -> Result<Vec<String>, String> {
    let rev = if rev.trim().is_empty() {
        "HEAD"
    } else {
        rev.trim()
    };
    check_name("revision", rev)?;
    let flag = match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard => "--hard",
        ResetMode::Keep => "--keep",
    };
    Ok(vec!["reset".into(), "-q".into(), flag.into(), rev.into()])
}

/// `tag [-f] [-a -F -] <name> [<commit>]`; the message (if any) goes to stdin.
pub fn tag_args(o: &TagOptions) -> Result<Vec<String>, String> {
    let name = o.name.trim();
    check_name("tag name", name)?;
    if name.contains([' ', '~', '^', ':', '?', '*', '[', '\\']) || name.contains("..") {
        return Err(format!("{name:?} is not a valid tag name"));
    }
    let mut args: Vec<String> = vec!["tag".into()];
    if o.force {
        args.push("-f".into());
    }
    if some(&o.message).is_some() {
        args.extend(["-a".into(), "-F".into(), "-".into()]);
    }
    args.push(name.into());
    if let Some(commit) = some(&o.commit) {
        check_name("commit", commit)?;
        args.push(commit.into());
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[String]) -> String {
        v.join(" ")
    }

    #[test]
    fn pull_arguments() {
        let o = PullOptions {
            remote: "origin".into(),
            branches: vec!["origin/main".into(), "develop".into()],
            no_ff: true,
            no_verify: true,
            ..Default::default()
        };
        assert_eq!(
            s(&pull_args(&o).unwrap()),
            "pull --no-edit --no-ff --no-verify --no-rebase origin main develop"
        );
        let rebase = PullOptions {
            remote: "origin".into(),
            rebase: true,
            ..Default::default()
        };
        assert_eq!(
            s(&pull_args(&rebase).unwrap()),
            "pull --no-edit --rebase origin"
        );
        assert!(pull_args(&PullOptions {
            squash: true,
            ..rebase.clone()
        })
        .is_err());
        assert!(pull_args(&PullOptions {
            remote: "--upload-pack=x".into(),
            ..Default::default()
        })
        .is_err());
        assert_eq!(
            s(&pull_args(&PullOptions::default()).unwrap()),
            "pull --no-edit --no-rebase"
        );
    }

    #[test]
    fn update_branch_arguments() {
        assert_eq!(
            s(&update_branch_args("origin", "refs/heads/main", "main").unwrap()),
            "fetch origin refs/heads/main:refs/heads/main"
        );
        // A forced or redirected refspec is never built from a branch name.
        assert!(update_branch_args("origin", "+refs/heads/main", "main").is_err());
        assert!(update_branch_args("origin", "refs/heads/main", "a:b").is_err());
        assert!(update_branch_args("origin", "-f", "main").is_err());
    }

    #[test]
    fn merge_arguments() {
        let o = MergeOptions {
            branches: vec!["feature/x".into()],
            no_ff: true,
            message: Some("Merge feature x".into()),
            ..Default::default()
        };
        assert_eq!(
            s(&merge_args(&o).unwrap()),
            "merge --no-ff -m Merge feature x feature/x"
        );
        let squash = MergeOptions {
            branches: vec!["a".into(), "b".into()],
            squash: true,
            no_commit: true,
            allow_unrelated_histories: true,
            ..Default::default()
        };
        assert_eq!(
            s(&merge_args(&squash).unwrap()),
            "merge --squash --no-commit --allow-unrelated-histories --no-edit a b"
        );
        assert!(merge_args(&MergeOptions::default()).is_err());
        assert!(merge_args(&MergeOptions {
            ff_only: true,
            no_ff: true,
            ..o
        })
        .is_err());
    }

    #[test]
    fn rebase_arguments() {
        let simple = RebaseOptions {
            onto: "main".into(),
            ..Default::default()
        };
        assert_eq!(s(&rebase_args(&simple).unwrap()), "rebase main");
        let onto = RebaseOptions {
            onto: "main".into(),
            from: Some("feature/base".into()),
            branch: Some("feature/top".into()),
            update_refs: true,
            ..Default::default()
        };
        assert_eq!(
            s(&rebase_args(&onto).unwrap()),
            "rebase --update-refs --onto main feature/base feature/top"
        );
        let root = RebaseOptions {
            root: true,
            keep_empty: true,
            rebase_merges: true,
            ..Default::default()
        };
        assert_eq!(
            s(&rebase_args(&root).unwrap()),
            "rebase --rebase-merges --keep-empty --root"
        );
        assert!(rebase_args(&RebaseOptions::default()).is_err());
    }

    #[test]
    fn reset_and_tag_arguments() {
        assert_eq!(
            s(&reset_args("", ResetMode::Hard).unwrap()),
            "reset -q --hard HEAD"
        );
        assert_eq!(
            s(&reset_args("HEAD~2", ResetMode::Keep).unwrap()),
            "reset -q --keep HEAD~2"
        );
        assert!(reset_args("--hard", ResetMode::Soft).is_err());
        let tag = TagOptions {
            name: "v2.0.0".into(),
            commit: Some("abc1234".into()),
            message: Some("Release".into()),
            force: true,
        };
        assert_eq!(s(&tag_args(&tag).unwrap()), "tag -f -a -F - v2.0.0 abc1234");
        assert!(tag_args(&TagOptions {
            name: "bad name".into(),
            ..Default::default()
        })
        .is_err());
    }
}
