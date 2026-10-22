//! Git Log actions against throwaway repositories: cherry-pick, revert, undo, edit message,
//! squash, drop, the interactive rebase (with `edit` stops and conflicts) and the pushed /
//! protected detection, with a local bare repository as the remote. Git runs through a
//! [`GitExec`] with a fixed identity that ignores the user's configuration.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use forge_core::Result;
use forge_vcs::{
    GitExec, GitOutput, GitRequest, MergeOperation, OngoingOperation, OperationAction,
    RebaseAction, RebaseEntry, Repo,
};
use tempfile::TempDir;

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
            "init.defaultBranch=main",
            "-c",
            "merge.conflictStyle=merge",
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join(".no-global-config"))
        .env("HOME", dir)
        .env("GIT_EDITOR", "true")
        .env_remove("GIT_SEQUENCE_EDITOR")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
    cmd
}

struct TestGit;

impl GitExec for TestGit {
    fn exec(&self, req: &GitRequest) -> Result<GitOutput> {
        use std::io::Write;
        let mut cmd = git_command(Path::new(&req.cwd));
        cmd.args(&req.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().expect("git runs");
        let mut stdin = child.stdin.take().unwrap();
        if let Some(input) = &req.stdin {
            stdin.write_all(input.as_bytes()).unwrap();
        }
        drop(stdin);
        let out = child.wait_with_output().unwrap();
        Ok(GitOutput {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
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

fn commit(dir: &Path, path: &str, contents: &str, message: &str) -> String {
    write(dir, path, contents);
    sh(dir, &["add", "-A"]);
    sh(dir, &["commit", "-q", "-m", message]);
    rev(dir, "HEAD")
}

fn rev(dir: &Path, r: &str) -> String {
    sh(dir, &["rev-parse", r]).trim().to_string()
}

fn subjects(dir: &Path) -> Vec<String> {
    sh(dir, &["log", "--format=%s"])
        .lines()
        .map(str::to_string)
        .collect()
}

fn open(dir: &Path) -> Repo {
    Repo::open_with(dir, Arc::new(TestGit)).unwrap()
}

/// `main` with commits A..D, each adding a file.
struct Fixture {
    tmp: TempDir,
    a: String,
    b: String,
    c: String,
    d: String,
}

fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    sh(dir, &["init", "-q"]);
    let a = commit(dir, "a.txt", "a\n", "Add a");
    let b = commit(dir, "b.txt", "b\n", "Add b");
    let c = commit(dir, "c.txt", "c\n", "Add c");
    let d = commit(dir, "d.txt", "d\n", "Add d");
    Fixture { tmp, a, b, c, d }
}

fn entry(sha: &str, action: RebaseAction, message: Option<&str>) -> RebaseEntry {
    RebaseEntry {
        sha: sha.to_string(),
        action,
        message: message.map(str::to_string),
    }
}

#[test]
fn interactive_rebase_rewords_squashes_drops_and_reorders() {
    let f = fixture();
    let dir = f.tmp.path();
    let repo = open(dir);
    let plan = repo.rebase_plan(&f.b).unwrap();
    assert_eq!(plan.base.as_deref(), Some(f.a.as_str()));
    assert_eq!(plan.branch.as_deref(), Some("main"));
    let subjects_planned: Vec<&str> = plan.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects_planned, vec!["Add b", "Add c", "Add d"]);
    assert!(plan.entries.iter().all(|e| e.action == RebaseAction::Pick));

    // d moves before b and is reworded; c is squashed into b with a new message.
    let entries = [
        entry(
            &f.d,
            RebaseAction::Reword,
            Some("Add d first\n\nWith a body"),
        ),
        entry(&f.b, RebaseAction::Pick, None),
        entry(&f.c, RebaseAction::Squash, Some("Add b and c")),
    ];
    let res = repo.interactive_rebase(&f.b, &entries).unwrap();
    assert!(!res.stopped);
    assert_eq!(res.old_head, f.d);
    assert_eq!(subjects(dir), vec!["Add b and c", "Add d first", "Add a"]);
    assert_eq!(
        sh(dir, &["log", "-1", "--format=%B", "HEAD~1"]).trim(),
        "Add d first\n\nWith a body"
    );
    assert_eq!(read(dir, "c.txt"), "c\n");
    assert_eq!(repo.operation_state().unwrap().operation, None);

    // Undo puts the branch back.
    repo.undo_rewrite(&res.old_head, res.new_head.as_deref().unwrap())
        .unwrap();
    assert_eq!(rev(dir, "HEAD"), f.d);

    // A plan that misses a commit of the range is refused.
    assert!(repo
        .interactive_rebase(&f.b, &[entry(&f.b, RebaseAction::Pick, None)])
        .is_err());
}

#[test]
fn edit_steps_stop_and_continue_applies_the_rest() {
    let f = fixture();
    let dir = f.tmp.path();
    let repo = open(dir);
    let entries = [
        entry(&f.b, RebaseAction::Edit, None),
        entry(&f.c, RebaseAction::Reword, Some("Add c, reworded")),
        entry(&f.d, RebaseAction::Pick, None),
    ];
    let res = repo.interactive_rebase(&f.b, &entries).unwrap();
    assert!(res.stopped && res.new_head.is_none());
    let state = repo.operation_state().unwrap();
    assert_eq!(state.operation, Some(OngoingOperation::Rebase));
    assert_eq!(state.branch.as_deref(), Some("main"));
    // Amend the stopped commit, then continue.
    write(dir, "b.txt", "b edited\n");
    sh(dir, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    repo.operation_action(OperationAction::Continue).unwrap();
    assert_eq!(repo.operation_state().unwrap().operation, None);
    assert_eq!(
        subjects(dir),
        vec!["Add d", "Add c, reworded", "Add b", "Add a"]
    );
    assert_eq!(read(dir, "b.txt"), "b edited\n");
}

#[test]
fn rebase_conflicts_hand_over_to_the_resolver() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    sh(dir, &["init", "-q"]);
    commit(dir, "a.txt", "1\n", "Base");
    let one = commit(dir, "a.txt", "1\n2\n", "Append 2");
    let two = commit(dir, "a.txt", "1\n2\n3\n", "Append 3");
    let repo = open(dir);
    // Swapping two commits that touch the same lines conflicts.
    let entries = [
        entry(&two, RebaseAction::Reword, Some("Append 3 first")),
        entry(&one, RebaseAction::Pick, None),
    ];
    assert!(repo.interactive_rebase(&one, &entries).is_err());
    assert_eq!(
        repo.operation_state().unwrap().operation,
        Some(OngoingOperation::Rebase)
    );
    let set = repo.conflicts().unwrap();
    assert_eq!(set.operation, MergeOperation::Rebase);
    assert!(set.reversed);
    assert_eq!(set.files.len(), 1);

    write(dir, "a.txt", "1\n3\n");
    repo.stage(&["a.txt".into()]).unwrap();
    // The reword runs after the conflict is resolved; the next pick conflicts again.
    let _ = repo.operation_action(OperationAction::Continue);
    assert_eq!(
        sh(dir, &["log", "-1", "--format=%s", "HEAD"]).trim(),
        "Append 3 first"
    );
    repo.operation_action(OperationAction::Abort).unwrap();
    assert_eq!(rev(dir, "HEAD"), two);
}

#[test]
fn local_changes_survive_a_rewrite() {
    let f = fixture();
    let dir = f.tmp.path();
    write(dir, "a.txt", "local change\n");
    let repo = open(dir);
    repo.drop_commits(std::slice::from_ref(&f.c)).unwrap();
    assert_eq!(subjects(dir), vec!["Add d", "Add b", "Add a"]);
    assert!(!dir.join("c.txt").exists());
    assert_eq!(read(dir, "a.txt"), "local change\n");
}

#[test]
fn edit_message_of_head_and_older_commits() {
    let f = fixture();
    let dir = f.tmp.path();
    write(dir, "staged.txt", "not part of the amend\n");
    sh(dir, &["add", "staged.txt"]);
    let repo = open(dir);

    let res = repo.reword(&f.d, "Add d (better)").unwrap();
    assert_eq!(subjects(dir)[0], "Add d (better)");
    assert_eq!(res.old_head, f.d);
    // --only: the staged file stays staged.
    assert!(sh(dir, &["diff", "--cached", "--name-only"]).contains("staged.txt"));
    assert!(!sh(dir, &["show", "--name-only", "--format=", "HEAD"]).contains("staged.txt"));
    sh(dir, &["reset", "-q"]);

    repo.reword(&f.b, "#12 Add b\n\nIssue references keep their hash")
        .unwrap();
    assert_eq!(
        subjects(dir),
        vec!["Add d (better)", "Add c", "#12 Add b", "Add a"]
    );
    assert!(repo.reword(&f.a, "  ").is_err());
}

#[test]
fn squash_commits_unite_at_the_oldest() {
    let f = fixture();
    let dir = f.tmp.path();
    let repo = open(dir);
    repo.squash_commits(&[f.d.clone(), f.b.clone()], "Add b and d")
        .unwrap();
    assert_eq!(subjects(dir), vec!["Add c", "Add b and d", "Add a"]);
    assert_eq!(
        sh(dir, &["show", "--name-only", "--format=", "HEAD~1"])
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["b.txt", "d.txt"]
    );
    assert!(repo
        .squash_commits(std::slice::from_ref(&f.a), "x")
        .is_err());
}

#[test]
fn rewriting_from_the_root_commit() {
    let f = fixture();
    let dir = f.tmp.path();
    let repo = open(dir);
    let plan = repo.rebase_plan(&f.a).unwrap();
    assert_eq!(plan.base, None);
    assert_eq!(plan.commits.len(), 4);
    repo.reword(&f.a, "Root commit").unwrap();
    assert_eq!(subjects(dir).last().unwrap(), "Root commit");
}

#[test]
fn undo_commit_keeps_the_changes_staged() {
    let f = fixture();
    let dir = f.tmp.path();
    let repo = open(dir);
    assert!(repo.undo_commit(&f.c).is_err(), "only the last commit");
    let undone = repo.undo_commit(&f.d).unwrap();
    assert_eq!(undone.message, "Add d");
    assert_eq!(rev(dir, "HEAD"), f.c);
    assert_eq!(
        sh(dir, &["diff", "--cached", "--name-only"]).trim(),
        "d.txt"
    );

    // The root commit: the branch becomes unborn and the files stay staged.
    let tmp = TempDir::new().unwrap();
    let root_dir = tmp.path();
    sh(root_dir, &["init", "-q"]);
    let root = commit(root_dir, "a.txt", "a\n", "Initial commit");
    let repo = open(root_dir);
    repo.undo_commit(&root).unwrap();
    assert!(sh(root_dir, &["status", "--porcelain"]).contains("A  a.txt"));
}

#[test]
fn cherry_pick_revert_and_skip_empty_picks() {
    let f = fixture();
    let dir = f.tmp.path();
    sh(dir, &["checkout", "-q", "-b", "feature", &f.b]);
    let e = commit(dir, "e.txt", "e\n", "Add e");
    let same_as_c = commit(dir, "c.txt", "c\n", "Add c on feature");
    sh(dir, &["checkout", "-q", "main"]);
    let repo = open(dir);

    // Log order (newest first); the change of `same_as_c` is already on main.
    let out = repo
        .cherry_pick(&[same_as_c.clone(), e.clone()], true, &[])
        .unwrap();
    assert!(out.contains("1 commit was skipped"), "{out}");
    assert_eq!(subjects(dir)[..2], ["Add e", "Add d"]);
    assert_eq!(repo.operation_state().unwrap().operation, None);
    // Not pushed to a protected branch: no "cherry picked from" line.
    assert!(!sh(dir, &["log", "-1", "--format=%B"]).contains("cherry picked from"));

    repo.revert_commits(&[rev(dir, "HEAD"), f.d.clone()])
        .unwrap();
    assert!(!dir.join("e.txt").exists() && !dir.join("d.txt").exists());
    assert_eq!(subjects(dir)[0], "Revert \"Add d\"");
}

#[test]
fn cherry_pick_conflicts_stop_for_the_resolver() {
    let f = fixture();
    let dir = f.tmp.path();
    sh(dir, &["checkout", "-q", "-b", "feature", &f.a]);
    let pick = commit(dir, "d.txt", "other d\n", "Different d");
    sh(dir, &["checkout", "-q", "main"]);
    let repo = open(dir);
    assert!(repo.cherry_pick(&[pick], false, &[]).is_err());
    assert_eq!(
        repo.operation_state().unwrap().operation,
        Some(OngoingOperation::CherryPick)
    );
    assert_eq!(
        repo.conflicts().unwrap().operation,
        MergeOperation::CherryPick
    );
    repo.operation_action(OperationAction::Abort).unwrap();
}

/// `main` pushed to a local bare `origin`, `feature` pushed too, one local commit on top.
fn with_remote() -> (TempDir, TempDir, Fixture, String, String) {
    let f = fixture();
    let dir = f.tmp.path();
    let remote = TempDir::new().unwrap();
    sh(remote.path(), &["init", "-q", "--bare"]);
    let url = remote.path().to_string_lossy().into_owned();
    sh(dir, &["remote", "add", "origin", &url]);
    sh(dir, &["push", "-q", "origin", "main"]);
    sh(dir, &["checkout", "-q", "-b", "feature"]);
    let pushed = commit(dir, "f.txt", "f\n", "Add f");
    sh(dir, &["push", "-q", "origin", "feature"]);
    sh(dir, &["fetch", "-q", "origin"]);
    let local = commit(dir, "g.txt", "g\n", "Add g");
    (remote, TempDir::new().unwrap(), f, pushed, local)
}

#[test]
fn pushed_and_protected_commits() {
    let (_remote, _spare, f, pushed, local) = with_remote();
    let dir = f.tmp.path();
    let repo = open(dir);
    let protected = vec!["main".to_string(), "master".to_string()];

    let s = repo
        .log_action_states(std::slice::from_ref(&local), &protected)
        .unwrap();
    assert!(s.undo_commit.enabled && s.edit_message.enabled);
    assert!(s.edit_message.warning.is_none());

    let s = repo
        .log_action_states(std::slice::from_ref(&pushed), &protected)
        .unwrap();
    assert!(!s.undo_commit.enabled);
    assert!(s.edit_message.enabled);
    assert!(s
        .edit_message
        .warning
        .as_deref()
        .unwrap()
        .contains("'origin/feature'"));

    let s = repo
        .log_action_states(std::slice::from_ref(&f.c), &protected)
        .unwrap();
    assert!(!s.edit_message.enabled && !s.drop_commits.enabled);
    assert_eq!(
        s.interactive_rebase.reason.as_deref(),
        Some("The selected commit is already pushed to protected branch 'origin/main'")
    );
    assert!(s.revert.enabled && !s.cherry_pick.enabled);

    // Several commits: squash needs a line of history; the protected one blocks it.
    let s = repo
        .log_action_states(&[local.clone(), pushed.clone()], &protected)
        .unwrap();
    assert!(s.squash_commits.enabled && !s.edit_message.enabled);

    // Cherry-picking a protected commit adds the suffix.
    sh(dir, &["checkout", "-q", "-b", "other", &f.a]);
    repo.cherry_pick(std::slice::from_ref(&f.c), true, &protected)
        .unwrap();
    assert!(sh(dir, &["log", "-1", "--format=%B"]).contains("cherry picked from commit"));
}

#[test]
fn merges_after_the_selection_block_rebases() {
    let f = fixture();
    let dir = f.tmp.path();
    sh(dir, &["checkout", "-q", "-b", "side", &f.c]);
    commit(dir, "s.txt", "s\n", "Side work");
    sh(dir, &["checkout", "-q", "main"]);
    sh(dir, &["merge", "-q", "--no-ff", "--no-edit", "side"]);
    let repo = open(dir);
    let s = repo
        .log_action_states(std::slice::from_ref(&f.b), &[])
        .unwrap();
    assert!(!s.interactive_rebase.enabled);
    assert!(s.revert.enabled);
    assert!(repo.rebase_plan(&f.b).is_err());
    let merge = rev(dir, "HEAD");
    let s = repo.log_action_states(&[merge], &[]).unwrap();
    assert_eq!(
        s.drop_commits.reason.as_deref(),
        Some("Not available for merge commits")
    );
}

#[test]
fn autosquash_and_patches_and_trees() {
    let f = fixture();
    let dir = f.tmp.path();
    let fix = commit(dir, "b.txt", "b fixed\n", "fixup! Add b");
    let repo = open(dir);
    let plan = repo.rebase_plan(&f.b).unwrap();
    let order: Vec<(&str, RebaseAction)> = plan
        .entries
        .iter()
        .map(|e| (e.sha.as_str(), e.action))
        .collect();
    assert_eq!(
        order,
        vec![
            (f.b.as_str(), RebaseAction::Pick),
            (fix.as_str(), RebaseAction::Fixup),
            (f.c.as_str(), RebaseAction::Pick),
            (f.d.as_str(), RebaseAction::Pick),
        ]
    );
    repo.interactive_rebase(&f.b, &plan.entries).unwrap();
    assert_eq!(subjects(dir), vec!["Add d", "Add c", "Add b", "Add a"]);
    assert_eq!(read(dir, "b.txt"), "b fixed\n");

    let head = rev(dir, "HEAD");
    let patch = repo.commits_patch(std::slice::from_ref(&head)).unwrap();
    assert!(patch.contains("+++ b/d.txt") && !patch.contains("c.txt"));
    let two = repo.commits_patch(&[head, rev(dir, "HEAD~1")]).unwrap();
    assert!(two.contains("+++ b/c.txt") && two.contains("+++ b/d.txt"));
    let root = repo.commits_patch(std::slice::from_ref(&f.a)).unwrap();
    assert!(root.contains("+++ b/a.txt"));

    assert_eq!(repo.tree_files(&f.b).unwrap(), vec!["a.txt", "b.txt"]);
}

#[test]
fn a_stale_rebase_head_does_not_turn_merges_into_rebases() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    sh(dir, &["init", "-q"]);
    commit(dir, "a.txt", "base\n", "Base");
    sh(dir, &["checkout", "-q", "-b", "feature"]);
    commit(dir, "a.txt", "feature\n", "Feature");
    sh(dir, &["checkout", "-q", "main"]);
    commit(dir, "a.txt", "main\n", "Main");
    // A rebase finished with `--skip` outside Forge leaves REBASE_HEAD behind.
    sh(dir, &["checkout", "-q", "feature"]);
    assert!(!git_command(dir)
        .args(["rebase", "main"])
        .output()
        .unwrap()
        .status
        .success());
    sh(dir, &["rebase", "--skip"]);
    assert!(git_command(dir)
        .args(["rev-parse", "-q", "--verify", "REBASE_HEAD"])
        .output()
        .unwrap()
        .status
        .success());

    // Now a merge conflicts: yours is the current branch, not git's `theirs`.
    sh(dir, &["checkout", "-q", "-b", "topic", "main~1"]);
    commit(dir, "a.txt", "topic\n", "Topic");
    sh(dir, &["checkout", "-q", "main"]);
    assert!(!git_command(dir)
        .args(["merge", "--no-edit", "topic"])
        .output()
        .unwrap()
        .status
        .success());
    let repo = open(dir);
    let set = repo.conflicts().unwrap();
    assert_eq!(set.operation, MergeOperation::Merge);
    assert!(!set.reversed);
    assert_eq!(set.yours_name, "main");
    assert_eq!(
        repo.merge_file("a.txt").unwrap().yours.as_deref(),
        Some("main\n")
    );
    repo.accept_conflict_side(&["a.txt".into()], forge_vcs::ConflictSide::Yours)
        .unwrap();
    assert_eq!(read(dir, "a.txt"), "main\n");
}
