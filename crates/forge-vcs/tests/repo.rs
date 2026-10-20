//! End-to-end tests against throwaway repositories created with the real git CLI.

use std::fs;
use std::path::Path;
use std::process::Command;

use forge_vcs::{FileState, LogQuery, RefKind, Repo};
use tempfile::TempDir;

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", dir)
        .output()
        .expect("git runs");
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

/// A repo with one commit on `main` containing `a.php` and `src/b.php`.
fn fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    sh(d, &["init", "-q", "-b", "main"]);
    sh(d, &["config", "user.name", "Marlon"]);
    sh(d, &["config", "user.email", "marlon@example.com"]);
    sh(d, &["config", "commit.gpgsign", "false"]);
    write(d, "a.php", "<?php\necho 1;\necho 2;\n");
    write(d, "src/b.php", "<?php\nclass B {}\n");
    sh(d, &["add", "-A"]);
    sh(d, &["commit", "-q", "-m", "Initial commit"]);
    tmp
}

#[test]
fn status_reports_staged_unstaged_untracked() {
    let tmp = fixture();
    let d = tmp.path();
    write(d, "a.php", "<?php\necho 1;\necho 3;\n");
    write(d, "new file.php", "<?php\n");
    write(d, "staged.php", "<?php\n");
    sh(d, &["add", "staged.php"]);
    fs::remove_file(d.join("src/b.php")).unwrap();

    let repo = Repo::open(d).unwrap();
    let st = repo.status().unwrap();
    assert_eq!(st.branch.as_deref(), Some("main"));
    assert!(st.head.is_some());
    let get = |p: &str| st.entries.iter().find(|e| e.path == p).unwrap();
    assert_eq!(get("a.php").worktree, FileState::Modified);
    assert!(get("new file.php").is_untracked());
    assert_eq!(get("staged.php").index, FileState::Added);
    assert_eq!(get("src/b.php").worktree, FileState::Deleted);
}

#[test]
fn diff_show_and_blame() {
    let tmp = fixture();
    let d = tmp.path();
    write(d, "a.php", "<?php\necho 1;\necho 3;\n");
    let repo = Repo::open(d).unwrap();

    let head = repo.show("HEAD", "a.php").unwrap().unwrap();
    assert_eq!(head, "<?php\necho 1;\necho 2;\n");
    assert_eq!(repo.show("HEAD", "missing.php").unwrap(), None);

    let diff = repo.diff(&["a.php".into()], false).unwrap();
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].path, "a.php");
    assert!(repo.diff(&[], true).unwrap().is_empty());
    assert!(repo.diff_head_text(&[]).unwrap().contains("+echo 3;"));

    let blame = repo.blame("a.php", None).unwrap();
    assert_eq!(blame.lines.len(), 3);
    let c = blame.commit_for_line(0).unwrap();
    assert_eq!(c.author, "Marlon");
    assert_eq!(c.summary, "Initial commit");
    assert!(blame.commit_for_line(2).unwrap().uncommitted);

    let buffer = repo
        .blame("a.php", Some("<?php\necho 1;\necho 2;\nnew();\n"))
        .unwrap();
    assert_eq!(buffer.lines.len(), 4);
    assert!(!buffer.commit_for_line(2).unwrap().uncommitted);
    assert!(buffer.commit_for_line(3).unwrap().uncommitted);
}

#[test]
fn stage_unstage_rollback() {
    let tmp = fixture();
    let d = tmp.path();
    write(d, "a.php", "changed\n");
    write(d, "added.php", "<?php\n");
    let repo = Repo::open(d).unwrap();

    repo.stage(&["a.php".into(), "added.php".into()]).unwrap();
    let st = repo.status().unwrap();
    assert!(st.entries.iter().all(|e| e.is_staged()));

    repo.unstage(&["a.php".into()]).unwrap();
    let st = repo.status().unwrap();
    let a = st.entries.iter().find(|e| e.path == "a.php").unwrap();
    assert!(!a.is_staged() && a.is_unstaged());

    repo.rollback(&["a.php".into(), "added.php".into()])
        .unwrap();
    assert_eq!(
        fs::read_to_string(d.join("a.php")).unwrap(),
        "<?php\necho 1;\necho 2;\n"
    );
    // The added file is unstaged but kept on disk.
    assert!(d.join("added.php").exists());
    let st = repo.status().unwrap();
    assert_eq!(st.entries.len(), 1);
    assert!(st.entries[0].is_untracked());
}

#[test]
fn commit_selected_paths_and_amend() {
    let tmp = fixture();
    let d = tmp.path();
    write(d, "a.php", "one\n");
    write(d, "src/b.php", "two\n");
    write(d, "new.php", "three\n");
    let repo = Repo::open(d).unwrap();

    let sha = repo
        .commit(
            "Add new file\n\nWith a body.",
            false,
            Some(&["a.php".into(), "new.php".into()]),
        )
        .unwrap();
    assert_eq!(sha.len(), 40);
    let st = repo.status().unwrap();
    assert_eq!(st.entries.len(), 1, "only src/b.php stays modified");
    assert_eq!(st.entries[0].path, "src/b.php");

    let details = repo.commit_details(&sha).unwrap();
    assert_eq!(details.message, "Add new file\n\nWith a body.");
    let mut files: Vec<_> = details.files.iter().map(|f| f.path.as_str()).collect();
    files.sort();
    assert_eq!(files, vec!["a.php", "new.php"]);

    repo.stage(&["src/b.php".into()]).unwrap();
    let amended = repo.commit("Add new file and edit b", true, None).unwrap();
    assert_ne!(amended, sha);
    let log = repo.log(&LogQuery::default()).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].subject, "Add new file and edit b");
    assert!(repo.status().unwrap().entries.is_empty());

    assert!(repo.commit("  ", false, None).is_err());
}

#[test]
fn branches_checkout_merge_and_log_graph() {
    let tmp = fixture();
    let d = tmp.path();
    let repo = Repo::open(d).unwrap();

    repo.create_branch("feature/login", None, true).unwrap();
    write(d, "login.php", "<?php\n");
    repo.commit("Add login", false, Some(&["login.php".into()]))
        .unwrap();
    repo.checkout("main").unwrap();
    write(d, "a.php", "main change\n");
    repo.commit("Change a", false, Some(&["a.php".into()]))
        .unwrap();
    repo.merge("feature/login").unwrap();
    sh(d, &["tag", "v1.0"]);

    let b = repo.branches().unwrap();
    assert_eq!(b.current.as_deref(), Some("main"));
    assert_eq!(b.local.len(), 2);
    assert_eq!(b.tags[0].name, "v1.0");
    assert_eq!(b.recent, vec!["feature/login"]);

    let log = repo
        .log(&LogQuery {
            all: true,
            ..LogQuery::default()
        })
        .unwrap();
    assert_eq!(log.len(), 4);
    assert_eq!(log[0].parents.len(), 2, "merge commit first");
    let head_ref = log[0].refs.iter().find(|r| r.current).unwrap();
    assert_eq!(head_ref.name, "main");
    assert!(log[0]
        .refs
        .iter()
        .any(|r| r.kind == RefKind::Tag && r.name == "v1.0"));

    let page = repo
        .log(&LogQuery {
            skip: 1,
            limit: 2,
            ..LogQuery::default()
        })
        .unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].sha, log[1].sha);

    let merge = repo.commit_details(&log[0].sha).unwrap();
    assert_eq!(merge.files.len(), 1);
    assert_eq!(merge.files[0].path, "login.php");

    let filtered = repo
        .log(&LogQuery {
            path: Some("login.php".into()),
            ..LogQuery::default()
        })
        .unwrap();
    assert_eq!(filtered.len(), 1);

    repo.delete_branch("feature/login", false).unwrap();
    assert_eq!(repo.branches().unwrap().local.len(), 1);
    assert!(repo.checkout("--orphan").is_err());
}

#[test]
fn push_pull_fetch_against_local_remote() {
    let remote = TempDir::new().unwrap();
    sh(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    let tmp = fixture();
    let d = tmp.path();
    sh(
        d,
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    let repo = Repo::open(d).unwrap();

    repo.push(false).unwrap();
    let st = repo.status().unwrap();
    assert_eq!(st.upstream.as_deref(), Some("origin/main"));

    write(d, "a.php", "pushed\n");
    repo.commit("Second", false, Some(&["a.php".into()]))
        .unwrap();
    assert_eq!(repo.status().unwrap().ahead, 1);
    repo.push(false).unwrap();
    repo.fetch().unwrap();
    repo.pull(false).unwrap();
    let st = repo.status().unwrap();
    assert_eq!((st.ahead, st.behind), (0, 0));
    assert!(repo
        .branches()
        .unwrap()
        .remote
        .iter()
        .any(|b| b.name == "origin/main"));
}

#[test]
fn push_without_remote_reports_error() {
    let tmp = fixture();
    let repo = Repo::open(tmp.path()).unwrap();
    let err = repo.push(false).unwrap_err().to_string();
    assert!(err.contains("no remote"), "{err}");
}

#[test]
fn workspace_in_subdirectory_uses_relative_paths() {
    let tmp = fixture();
    let d = tmp.path();
    write(d, "src/b.php", "<?php\nclass B { }\n");
    write(d, "a.php", "outside\n");
    let repo = Repo::open(d.join("src")).unwrap();
    let st = repo.status().unwrap();
    assert_eq!(st.entries.len(), 1);
    assert_eq!(st.entries[0].path, "b.php");
    assert_eq!(
        repo.show("HEAD", "b.php").unwrap().as_deref(),
        Some("<?php\nclass B {}\n")
    );
    let diff = repo.diff(&[], false).unwrap();
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].path, "b.php");
}

#[test]
fn open_outside_repository_fails() {
    let tmp = TempDir::new().unwrap();
    assert!(Repo::open(tmp.path()).is_err());
}
