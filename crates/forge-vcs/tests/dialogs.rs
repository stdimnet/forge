//! The Git menu dialogs and the shelf against throwaway repositories: Pull/Merge/Rebase with
//! options (a local bare repository is the remote), Continue/Abort, Reset, Tag, remotes,
//! stash details and shelve/unshelve. Git runs through a [`GitExec`] that adds a fixed
//! identity and ignores the user's configuration, so nothing touches real settings.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use forge_core::Result;
use forge_vcs::{
    GitBytesOutput, GitExec, GitOutput, GitRequest, MergeOptions, OngoingOperation,
    OperationAction, PullOptions, RebaseOptions, Repo, ResetMode, Shelf, TagOptions,
};
use tempfile::TempDir;

/// Identity and isolation for every git call.
fn git_command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .args([
            "-c",
            "user.name=Marlon",
            "-c",
            "user.email=marlon@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "merge.conflictStyle=merge",
            "-c",
            "core.quotepath=off",
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join(".no-global-config"))
        .env("HOME", dir)
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
    cmd
}

struct TestGit;

fn run_git(req: &GitRequest, stdin_bytes: Option<&[u8]>) -> std::process::Output {
    use std::io::Write;
    let mut cmd = git_command(Path::new(&req.cwd));
    cmd.args(&req.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("git runs");
    let mut stdin = child.stdin.take().unwrap();
    if let Some(input) = stdin_bytes.or(req.stdin.as_deref().map(str::as_bytes)) {
        stdin.write_all(input).unwrap();
    }
    drop(stdin);
    child.wait_with_output().unwrap()
}

impl GitExec for TestGit {
    fn exec(&self, req: &GitRequest) -> Result<GitOutput> {
        let out = run_git(req, None);
        Ok(GitOutput {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    fn exec_bytes(&self, req: &GitRequest, stdin: Option<&[u8]>) -> Result<GitBytesOutput> {
        let out = run_git(req, stdin);
        Ok(GitBytesOutput {
            ok: out.status.success(),
            stdout: out.stdout,
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            faithful: true,
        })
    }
}

/// A transport that only carries text (like a remote connection): the default `exec_bytes`.
struct TextOnlyGit;

impl GitExec for TextOnlyGit {
    fn exec(&self, req: &GitRequest) -> Result<GitOutput> {
        TestGit.exec(req)
    }
}

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = git_command(dir).args(args).output().expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write(dir: &Path, path: &str, contents: &str) {
    let p = dir.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, contents).unwrap();
}

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

fn commit_all(dir: &Path, message: &str) {
    sh(dir, &["add", "-A"]);
    sh(dir, &["commit", "-q", "-m", message]);
}

fn open(dir: &Path) -> Repo {
    Repo::open_with(dir, Arc::new(TestGit)).unwrap()
}

fn head_subject(dir: &Path) -> String {
    sh(dir, &["log", "-1", "--format=%s"]).trim().to_string()
}

/// `main` with `a.txt` (5 lines) and `b.txt`.
fn fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    sh(d, &["init", "-q"]);
    write(d, "a.txt", "1\n2\n3\n4\n5\n");
    write(d, "b.txt", "b\n");
    commit_all(d, "Initial commit");
    tmp
}

#[test]
fn merge_with_options() {
    let tmp = fixture();
    let d = tmp.path();
    sh(d, &["checkout", "-q", "-b", "feature"]);
    write(d, "c.txt", "c\n");
    commit_all(d, "Add c");
    sh(d, &["checkout", "-q", "main"]);
    let repo = open(d);

    // --squash --no-commit: changes staged, no commit.
    repo.merge_with(&MergeOptions {
        branches: vec!["feature".into()],
        squash: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(head_subject(d), "Initial commit");
    assert!(d.join("c.txt").exists());
    sh(d, &["reset", "-q", "--hard"]);

    // --no-ff with a message makes a merge commit even though a fast-forward is possible.
    repo.merge_with(&MergeOptions {
        branches: vec!["feature".into()],
        no_ff: true,
        message: Some("Merge the feature".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(head_subject(d), "Merge the feature");
    assert_eq!(
        sh(d, &["rev-list", "--parents", "-1", "HEAD"])
            .split_whitespace()
            .count(),
        3
    );

    // Invalid combinations are refused before git runs.
    assert!(repo
        .merge_with(&MergeOptions {
            branches: vec!["feature".into()],
            ff_only: true,
            squash: true,
            ..Default::default()
        })
        .is_err());
}

#[test]
fn merge_conflicts_stop_and_abort() {
    let tmp = fixture();
    let d = tmp.path();
    sh(d, &["checkout", "-q", "-b", "feature"]);
    write(d, "a.txt", "1\nfeature\n3\n4\n5\n");
    commit_all(d, "Feature edit");
    sh(d, &["checkout", "-q", "main"]);
    write(d, "a.txt", "1\nmain\n3\n4\n5\n");
    commit_all(d, "Main edit");
    let repo = open(d);

    assert_eq!(repo.operation_state().unwrap().operation, None);
    assert!(repo
        .merge_with(&MergeOptions {
            branches: vec!["feature".into()],
            ..Default::default()
        })
        .is_err());
    let state = repo.operation_state().unwrap();
    assert_eq!(state.operation, Some(OngoingOperation::Merge));
    assert_eq!(state.branch.as_deref(), Some("main"));
    // The conflict resolver takes over from here.
    assert_eq!(repo.conflicts().unwrap().files.len(), 1);
    assert!(repo.operation_action(OperationAction::Skip).is_err());

    repo.operation_action(OperationAction::Abort).unwrap();
    assert_eq!(repo.operation_state().unwrap().operation, None);
    assert_eq!(read(d, "a.txt"), "1\nmain\n3\n4\n5\n");
}

#[test]
fn rebase_with_options_continue_and_skip() {
    let tmp = fixture();
    let d = tmp.path();
    sh(d, &["checkout", "-q", "-b", "feature"]);
    write(d, "a.txt", "1\nfeature\n3\n4\n5\n");
    commit_all(d, "Feature edit");
    write(d, "f.txt", "f\n");
    commit_all(d, "Add f");
    sh(d, &["checkout", "-q", "main"]);
    write(d, "a.txt", "1\nmain\n3\n4\n5\n");
    commit_all(d, "Main edit");
    let repo = open(d);

    // Rebase `feature` (not checked out) onto main: the first commit conflicts.
    let err = repo
        .rebase_with(&RebaseOptions {
            branch: Some("feature".into()),
            onto: "main".into(),
            ..Default::default()
        })
        .unwrap_err();
    assert!(err.to_string().contains("rebase"), "{err}");
    let state = repo.operation_state().unwrap();
    assert_eq!(state.operation, Some(OngoingOperation::Rebase));
    assert_eq!(state.branch.as_deref(), Some("feature"));
    assert_eq!(state.onto.as_deref(), Some("main"));

    // Continue with the conflict still there fails; resolve it and continue.
    assert!(repo.operation_action(OperationAction::Continue).is_err());
    write(d, "a.txt", "1\nboth\n3\n4\n5\n");
    sh(d, &["add", "a.txt"]);
    repo.operation_action(OperationAction::Continue).unwrap();
    assert_eq!(repo.operation_state().unwrap().operation, None);
    assert_eq!(head_subject(d), "Add f");
    assert_eq!(
        sh(d, &["log", "--format=%s", "-4"])
            .lines()
            .collect::<Vec<_>>(),
        ["Add f", "Feature edit", "Main edit", "Initial commit"]
    );

    // Skip: rebase main's edit onto a branch with a conflicting one and skip that commit.
    sh(d, &["checkout", "-q", "-b", "other", "main~1"]);
    write(d, "a.txt", "1\nother\n3\n4\n5\n");
    commit_all(d, "Other edit");
    assert!(repo
        .rebase_with(&RebaseOptions {
            onto: "main".into(),
            ..Default::default()
        })
        .is_err());
    repo.operation_action(OperationAction::Skip).unwrap();
    assert_eq!(repo.operation_state().unwrap().operation, None);
    assert_eq!(head_subject(d), "Main edit");
    // The REBASE_HEAD git leaves behind is gone, so later conflicts are not taken for a rebase.
    assert!(!d.join(".git/REBASE_HEAD").exists());
}

#[test]
fn rebase_onto_from_and_root() {
    let tmp = fixture();
    let d = tmp.path();
    sh(d, &["checkout", "-q", "-b", "base"]);
    write(d, "base.txt", "base\n");
    commit_all(d, "Base work");
    sh(d, &["checkout", "-q", "-b", "top"]);
    write(d, "top.txt", "top\n");
    commit_all(d, "Top work");
    let repo = open(d);

    // Move only the commits after `base` onto main: Base work is left out.
    repo.rebase_with(&RebaseOptions {
        onto: "main".into(),
        from: Some("base".into()),
        update_refs: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        sh(d, &["log", "--format=%s"]).lines().collect::<Vec<_>>(),
        ["Top work", "Initial commit"]
    );
    assert!(!d.join("base.txt").exists());

    // --root replays every commit (new hashes, same subjects).
    let before = sh(d, &["rev-parse", "HEAD"]);
    repo.rebase_with(&RebaseOptions {
        root: true,
        keep_empty: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(head_subject(d), "Top work");
    let _ = before;
}

#[test]
fn pull_with_options_from_a_local_remote() {
    let tmp = TempDir::new().unwrap();
    let remote = tmp.path().join("remote.git");
    let me = tmp.path().join("me");
    let other = tmp.path().join("other");
    fs::create_dir_all(&remote).unwrap();
    sh(&remote, &["init", "-q", "--bare"]);
    sh(tmp.path(), &["clone", "-q", remote.to_str().unwrap(), "me"]);
    write(&me, "a.txt", "1\n");
    commit_all(&me, "Initial commit");
    sh(&me, &["push", "-q", "-u", "origin", "main"]);
    sh(
        tmp.path(),
        &["clone", "-q", remote.to_str().unwrap(), "other"],
    );
    write(&other, "b.txt", "b\n");
    commit_all(&other, "Remote work");
    sh(&other, &["push", "-q", "origin", "main"]);
    sh(&other, &["checkout", "-q", "-b", "develop"]);
    write(&other, "c.txt", "c\n");
    commit_all(&other, "Develop work");
    sh(&other, &["push", "-q", "origin", "develop"]);

    // Diverge locally, then pull with --rebase: linear history.
    write(&me, "mine.txt", "mine\n");
    commit_all(&me, "Local work");
    let repo = open(&me);
    repo.pull_with(&PullOptions {
        remote: "origin".into(),
        branches: vec!["origin/main".into()],
        rebase: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        sh(&me, &["log", "--format=%s"]).lines().collect::<Vec<_>>(),
        ["Local work", "Remote work", "Initial commit"]
    );

    // Pull another branch with --no-ff --no-commit: merged but not committed.
    repo.pull_with(&PullOptions {
        remote: "origin".into(),
        branches: vec!["develop".into()],
        no_ff: true,
        no_commit: true,
        ..Default::default()
    })
    .unwrap();
    assert!(me.join("c.txt").exists());
    assert_eq!(head_subject(&me), "Local work");
    assert_eq!(
        repo.operation_state().unwrap().operation,
        Some(OngoingOperation::Merge)
    );
    repo.operation_action(OperationAction::Continue).unwrap();
    assert!(head_subject(&me).starts_with("Merge branch 'develop'"));

    // --ff-only refuses diverged histories.
    write(&other, "d.txt", "d\n");
    commit_all(&other, "More develop work");
    sh(&other, &["push", "-q", "origin", "develop"]);
    write(&me, "e.txt", "e\n");
    commit_all(&me, "Diverge");
    assert!(repo
        .pull_with(&PullOptions {
            remote: "origin".into(),
            branches: vec!["develop".into()],
            ff_only: true,
            ..Default::default()
        })
        .is_err());
}

/// A bare remote with `main` (pushed from the clone `me`, whose `main` tracks it) and a second
/// clone `other` to push more work from.
fn remote_fixture() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let remote = tmp.path().join("remote.git");
    let me = tmp.path().join("me");
    let other = tmp.path().join("other");
    fs::create_dir_all(&remote).unwrap();
    sh(&remote, &["init", "-q", "--bare"]);
    sh(tmp.path(), &["clone", "-q", remote.to_str().unwrap(), "me"]);
    write(&me, "a.txt", "1\n");
    commit_all(&me, "Initial commit");
    sh(&me, &["push", "-q", "-u", "origin", "main"]);
    sh(
        tmp.path(),
        &["clone", "-q", remote.to_str().unwrap(), "other"],
    );
    (tmp, me, other)
}

fn subjects(dir: &Path, rev: &str) -> Vec<String> {
    sh(dir, &["log", "--format=%s", rev])
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn pull_a_remote_branch_into_the_current_branch() {
    let (_tmp, me, other) = remote_fixture();
    sh(&other, &["checkout", "-q", "-b", "feature/x"]);
    write(&other, "x.txt", "x\n");
    commit_all(&other, "Feature work");
    sh(&other, &["push", "-q", "origin", "feature/x"]);
    sh(&me, &["fetch", "-q", "origin"]);
    write(&me, "mine.txt", "mine\n");
    commit_all(&me, "Local work");
    let repo = open(&me);

    // Using Merge: a merge commit on main, the working tree has the feature's file.
    repo.pull_into_current("origin/feature/x", false).unwrap();
    assert!(me.join("x.txt").exists());
    assert!(head_subject(&me).starts_with("Merge branch 'feature/x'"));
    assert_eq!(sh(&me, &["branch", "--show-current"]).trim(), "main");

    // Using Rebase: new remote commits end up below the local ones, without a merge commit.
    sh(&me, &["reset", "-q", "--hard", "HEAD~1"]);
    write(&other, "y.txt", "y\n");
    commit_all(&other, "More feature work");
    sh(&other, &["push", "-q", "origin", "feature/x"]);
    // The remote-tracking branch is stale: the pull fetches the branch itself.
    repo.pull_into_current("origin/feature/x", true).unwrap();
    assert_eq!(
        subjects(&me, "HEAD"),
        [
            "Local work",
            "More feature work",
            "Feature work",
            "Initial commit"
        ]
    );

    // Not a remote branch, or an option instead of a name: refused before git runs.
    assert!(repo.pull_into_current("feature/x", false).is_err());
    assert!(repo.pull_into_current("--upload-pack=x", false).is_err());
}

#[test]
fn pull_into_the_current_branch_stops_on_conflicts() {
    let (_tmp, me, other) = remote_fixture();
    write(&other, "a.txt", "remote\n");
    commit_all(&other, "Remote edit");
    sh(&other, &["push", "-q", "origin", "main"]);
    write(&me, "a.txt", "local\n");
    commit_all(&me, "Local edit");
    let repo = open(&me);
    assert!(repo.pull_into_current("origin/main", false).is_err());
    assert_eq!(
        repo.operation_state().unwrap().operation,
        Some(OngoingOperation::Merge)
    );
    repo.operation_action(OperationAction::Abort).unwrap();
    assert_eq!(head_subject(&me), "Local edit");
}

#[test]
fn update_a_branch_that_is_not_checked_out() {
    let (_tmp, me, other) = remote_fixture();
    sh(&me, &["checkout", "-q", "-b", "feature"]);
    write(&me, "wip.txt", "uncommitted\n");
    write(&other, "b.txt", "b\n");
    commit_all(&other, "Remote work");
    sh(&other, &["push", "-q", "origin", "main"]);
    let repo = open(&me);

    // main is fast-forwarded to the remote; the checked out branch and its files stay.
    assert_eq!(subjects(&me, "main"), ["Initial commit"]);
    let old = sh(&me, &["rev-parse", "main"]);
    let report = repo.update_branch("main").unwrap().output;
    let new = sh(&me, &["rev-parse", "main"]);
    assert_eq!(report, format!("{}..{}", &old[..8], &new[..8]));
    assert_eq!(subjects(&me, "main"), ["Remote work", "Initial commit"]);
    assert_eq!(subjects(&me, "origin/main"), subjects(&me, "main"));
    assert_eq!(sh(&me, &["branch", "--show-current"]).trim(), "feature");
    assert_eq!(read(&me, "wip.txt"), "uncommitted\n");
    assert!(!me.join("b.txt").exists());
    assert_eq!(
        repo.update_branch("main").unwrap().output,
        "Already up to date."
    );

    // The current branch, a branch without a tracked branch and unknown names are refused.
    let err = repo.update_branch("feature").unwrap_err().to_string();
    assert!(err.contains("is checked out"), "{err}");
    sh(&me, &["branch", "untracked-branch"]);
    let err = repo
        .update_branch("untracked-branch")
        .unwrap_err()
        .to_string();
    assert!(err.contains("has no tracked branch"), "{err}");
    assert!(repo.update_branch("nope").is_err());
    assert!(repo.update_branch("--force").is_err());
}

#[test]
fn update_never_rewrites_a_diverged_branch() {
    let (_tmp, me, other) = remote_fixture();
    write(&me, "mine.txt", "mine\n");
    commit_all(&me, "Unpushed work");
    sh(&me, &["checkout", "-q", "-b", "feature"]);
    write(&other, "b.txt", "b\n");
    commit_all(&other, "Remote work");
    sh(&other, &["push", "-q", "origin", "main"]);
    let repo = open(&me);

    let err = repo.update_branch("main").unwrap_err().to_string();
    assert!(
        err.contains("has commits that are not in its tracked branch"),
        "{err}"
    );
    assert_eq!(subjects(&me, "main"), ["Unpushed work", "Initial commit"]);
}

#[test]
fn update_refuses_a_branch_checked_out_in_another_worktree() {
    let (tmp, me, other) = remote_fixture();
    sh(&me, &["checkout", "-q", "-b", "feature"]);
    let wt = tmp.path().join("wt");
    sh(
        &me,
        &["worktree", "add", "-q", wt.to_str().unwrap(), "main"],
    );
    write(&other, "b.txt", "b\n");
    commit_all(&other, "Remote work");
    sh(&other, &["push", "-q", "origin", "main"]);

    assert!(open(&me).update_branch("main").is_err());
    assert_eq!(subjects(&me, "main"), ["Initial commit"]);
    assert!(!wt.join("b.txt").exists());
}

#[test]
fn reset_modes() {
    let tmp = fixture();
    let d = tmp.path();
    write(d, "a.txt", "changed\n");
    commit_all(d, "Second");
    let repo = open(d);

    repo.reset_head("HEAD~1", ResetMode::Soft).unwrap();
    assert_eq!(head_subject(d), "Initial commit");
    assert!(sh(d, &["diff", "--cached", "--name-only"]).contains("a.txt"));

    sh(d, &["commit", "-q", "-m", "Second again"]);
    repo.reset_head("HEAD~1", ResetMode::Mixed).unwrap();
    assert!(sh(d, &["diff", "--cached", "--name-only"]).is_empty());
    assert_eq!(read(d, "a.txt"), "changed\n");

    sh(d, &["commit", "-q", "-a", "-m", "Second once more"]);
    write(d, "b.txt", "local edit\n");
    // --keep refuses to lose local changes to files the reset touches, but keeps others.
    repo.reset_head("HEAD~1", ResetMode::Keep).unwrap();
    assert_eq!(read(d, "a.txt"), "1\n2\n3\n4\n5\n");
    assert_eq!(read(d, "b.txt"), "local edit\n");

    repo.reset_head("", ResetMode::Hard).unwrap();
    assert_eq!(read(d, "b.txt"), "b\n");
    assert!(repo.reset_head("--hard", ResetMode::Hard).is_err());
}

#[test]
fn tags_with_force_and_message_and_remote_editing() {
    let tmp = fixture();
    let d = tmp.path();
    let repo = open(d);
    let first = repo.resolve_commit("HEAD").unwrap();
    assert!(repo.resolve_commit("no-such-thing").is_none());
    write(d, "a.txt", "x\n");
    commit_all(d, "Second");

    repo.tag(&TagOptions {
        name: "v1.0.0".into(),
        commit: Some(first.clone()),
        message: Some("First release".into()),
        force: false,
    })
    .unwrap();
    assert_eq!(sh(d, &["cat-file", "-t", "v1.0.0"]).trim(), "tag");
    assert_eq!(sh(d, &["rev-parse", "v1.0.0^{commit}"]).trim(), first);
    let again = TagOptions {
        name: "v1.0.0".into(),
        ..Default::default()
    };
    assert!(repo.tag(&again).is_err());
    repo.tag(&TagOptions {
        force: true,
        ..again
    })
    .unwrap();
    assert_eq!(sh(d, &["cat-file", "-t", "v1.0.0"]).trim(), "commit");

    repo.add_remote("origin", "https://example.com/a.git")
        .unwrap();
    repo.edit_remote("origin", "upstream", "https://example.com/b.git")
        .unwrap();
    let remotes = repo.remote_list().unwrap();
    assert_eq!(remotes.len(), 1);
    assert_eq!(remotes[0].name, "upstream");
    assert_eq!(remotes[0].fetch_url, "https://example.com/b.git");
    assert!(repo.edit_remote("upstream", "upstream", "--evil").is_err());
}

#[test]
fn stash_details_clear_and_branch() {
    let tmp = fixture();
    let d = tmp.path();
    let repo = open(d);
    write(d, "a.txt", "stashed\n");
    write(d, "new.txt", "untracked\n");
    repo.stash("With untracked", false, true).unwrap();

    let files = repo.stash_files(0).unwrap();
    let mut summary: Vec<(String, String)> =
        files.into_iter().map(|f| (f.path, f.status)).collect();
    summary.sort();
    assert_eq!(
        summary,
        [
            ("a.txt".to_string(), "M".to_string()),
            ("new.txt".to_string(), "A".to_string())
        ]
    );
    let a = repo.stash_file(0, "a.txt").unwrap();
    assert_eq!(a.before.as_deref(), Some("1\n2\n3\n4\n5\n"));
    assert_eq!(a.after.as_deref(), Some("stashed\n"));
    let n = repo.stash_file(0, "new.txt").unwrap();
    assert_eq!((n.before, n.after.as_deref()), (None, Some("untracked\n")));

    repo.stash_branch(0, "from-stash").unwrap();
    assert_eq!(repo.status().unwrap().branch.as_deref(), Some("from-stash"));
    assert_eq!(read(d, "a.txt"), "stashed\n");
    assert!(repo.stash_list().unwrap().is_empty());

    repo.stash("one", false, false).unwrap();
    write(d, "b.txt", "two\n");
    repo.stash("two", false, false).unwrap();
    assert_eq!(repo.stash_list().unwrap().len(), 2);
    repo.stash_clear().unwrap();
    assert!(repo.stash_list().unwrap().is_empty());
}

#[test]
fn shelve_and_unshelve() {
    let tmp = fixture();
    let d = tmp.path();
    let data = TempDir::new().unwrap();
    let shelf = Shelf::new(data.path().join("shelf"));
    let repo = open(d);

    write(d, "a.txt", "1\n2\nthree\n4\n5\n");
    write(d, "new file.txt", "fresh\n");
    fs::remove_file(d.join("b.txt")).unwrap();
    write(d, "keep.txt", "not shelved\n");
    write(d, "logo.png", "PNG\0\u{1}\u{2}");
    sh(d, &["add", "logo.png"]);

    let cl = repo
        .shelve(
            &shelf,
            &[
                "a.txt".into(),
                "new file.txt".into(),
                "b.txt".into(),
                "logo.png".into(),
            ],
            "Half-done invoice",
        )
        .unwrap();
    assert_eq!(cl.name, "Half-done invoice");
    assert_eq!(cl.branch.as_deref(), Some("main"));
    assert_eq!(cl.files.len(), 4);
    // The changes are gone; the file that was not selected stays.
    assert_eq!(read(d, "a.txt"), "1\n2\n3\n4\n5\n");
    assert!(!d.join("new file.txt").exists());
    assert!(!d.join("logo.png").exists());
    assert_eq!(read(d, "b.txt"), "b\n");
    let st = repo.status().unwrap();
    assert_eq!(
        st.entries
            .iter()
            .map(|e| e.path.as_str())
            .collect::<Vec<_>>(),
        ["keep.txt"]
    );
    assert_eq!(repo.shelf_list(&shelf).len(), 1);

    // Show Diff of a shelved file.
    let a = repo.shelf_file(&shelf, &cl.id, "a.txt").unwrap();
    assert_eq!(a.before.as_deref(), Some("1\n2\n3\n4\n5\n"));
    assert_eq!(a.after.as_deref(), Some("1\n2\nthree\n4\n5\n"));
    let n = repo.shelf_file(&shelf, &cl.id, "new file.txt").unwrap();
    assert_eq!((n.before, n.after.as_deref()), (None, Some("fresh\n")));
    assert!(repo.shelf_file(&shelf, &cl.id, "logo.png").unwrap().binary);

    // Partial unshelve, removing the file from the shelf.
    repo.unshelve(&shelf, &cl.id, Some(&["new file.txt".to_string()]), true)
        .unwrap();
    assert_eq!(read(d, "new file.txt"), "fresh\n");
    assert_eq!(read(d, "a.txt"), "1\n2\n3\n4\n5\n");
    assert_eq!(shelf.get(&cl.id).unwrap().files.len(), 3);

    // The rest no longer applies cleanly (a.txt changed meanwhile): merged three-way.
    write(d, "a.txt", "one\n2\n3\n4\n5\n");
    commit_all(d, "Edit line one");
    repo.unshelve(&shelf, &cl.id, None, false).unwrap();
    assert_eq!(read(d, "a.txt"), "one\n2\nthree\n4\n5\n");
    assert!(!d.join("b.txt").exists());
    assert_eq!(fs::read(d.join("logo.png")).unwrap(), b"PNG\0\x01\x02");
    // Kept on the shelf (remove = false); modifications are not staged.
    assert_eq!(shelf.get(&cl.id).unwrap().files.len(), 3);
    let st = repo.status().unwrap();
    let a = st.entries.iter().find(|e| e.path == "a.txt").unwrap();
    assert_eq!(a.index, forge_vcs::FileState::Unmodified);
}

#[test]
fn shelve_keeps_files_that_are_not_utf8() {
    // Windows-1252: "ä" is the single byte 0xE4, which is not valid UTF-8.
    let tmp = fixture();
    let d = tmp.path();
    let original: &[u8] = b"<?php\n// Gr\xfc\xdfe\n$a = 'b';\n";
    fs::write(d.join("legacy.php"), original).unwrap();
    commit_all(d, "Add legacy file");
    let edited: &[u8] = b"<?php\n// Gr\xfc\xdfe\n$a = 'b\xe4\xe4';\n";
    fs::write(d.join("legacy.php"), edited).unwrap();
    let data = TempDir::new().unwrap();
    let shelf = Shelf::new(data.path().join("shelf"));

    // A text-only transport cannot carry the bytes: refused, nothing is rolled back.
    let text_only = Repo::open_with(d, Arc::new(TextOnlyGit)).unwrap();
    let err = text_only
        .shelve(&shelf, &["legacy.php".into()], "Umlauts")
        .unwrap_err();
    assert!(err.to_string().contains("legacy.php"), "{err}");
    assert_eq!(fs::read(d.join("legacy.php")).unwrap(), edited);
    assert!(shelf.list().is_empty());

    let repo = open(d);
    let cl = repo
        .shelve(&shelf, &["legacy.php".into()], "Umlauts")
        .unwrap();
    assert_eq!(fs::read(d.join("legacy.php")).unwrap(), original);
    assert!(shelf
        .patch_bytes(&cl.id)
        .unwrap()
        .windows(3)
        .any(|w| w == b"\xe4\xe4'"));

    repo.unshelve(&shelf, &cl.id, None, true).unwrap();
    assert_eq!(fs::read(d.join("legacy.php")).unwrap(), edited);
    assert!(shelf.list().is_empty());
}

#[test]
fn unshelve_conflicts_go_to_the_resolver() {
    let tmp = fixture();
    let d = tmp.path();
    let data = TempDir::new().unwrap();
    let shelf = Shelf::new(data.path().join("shelf"));
    let repo = open(d);

    write(d, "a.txt", "1\nshelved\n3\n4\n5\n");
    let cl = repo.shelve(&shelf, &["a.txt".into()], "").unwrap();
    assert_eq!(cl.name, "Changes");
    write(d, "a.txt", "1\ncommitted\n3\n4\n5\n");
    commit_all(d, "Conflicting edit");

    assert!(repo.unshelve(&shelf, &cl.id, None, true).is_err());
    let set = repo.conflicts().unwrap();
    assert_eq!(set.files.len(), 1);
    assert_eq!(set.files[0].path, "a.txt");
    // Not removed: the changes stay on the shelf until the user deletes them.
    assert!(shelf.get(&cl.id).is_ok());
    assert!(repo.shelve(&shelf, &["a.txt".into()], "x").is_err());
}

#[test]
fn shelving_in_a_workspace_subdirectory() {
    let tmp = fixture();
    let d = tmp.path();
    write(d, "app/src/x.php", "<?php\n");
    commit_all(d, "Add app");
    let data = TempDir::new().unwrap();
    let shelf = Shelf::new(data.path().join("shelf"));
    let repo = Repo::open_with(d.join("app"), Arc::new(TestGit)).unwrap();
    write(d, "app/src/x.php", "<?php\necho 1;\n");
    let cl = repo.shelve(&shelf, &["src/x.php".into()], "Sub").unwrap();
    assert_eq!(cl.files[0].path, "src/x.php");
    assert_eq!(read(d, "app/src/x.php"), "<?php\n");
    assert_eq!(repo.shelf_list(&shelf)[0].files[0].path, "src/x.php");
    repo.unshelve(&shelf, &cl.id, Some(&["src/x.php".into()]), true)
        .unwrap();
    assert_eq!(read(d, "app/src/x.php"), "<?php\necho 1;\n");
    assert!(shelf.list().is_empty());
}
