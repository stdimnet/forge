//! File, folder and selection history against throwaway repositories. Identity and options
//! come from environment variables and `-c` flags, so no git config is written.

use std::fs;
use std::path::Path;
use std::process::Command;

use forge_vcs::{HistoryQuery, Repo};
use tempfile::TempDir;

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join(".no-global-config"))
        .env("HOME", dir)
        .env("GIT_AUTHOR_NAME", "Marlon")
        .env("GIT_AUTHOR_EMAIL", "marlon@example.com")
        .env("GIT_COMMITTER_NAME", "Marlon")
        .env("GIT_COMMITTER_EMAIL", "marlon@example.com")
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

/// `src/a.php` is created, edited, renamed to `src/b.php` and edited again; `docs/x.md`
/// is committed in between.
fn fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    sh(d, &["init", "-q"]);
    write(d, "src/a.php", "<?php\nfunction a() {\n    return 1;\n}\n");
    sh(d, &["add", "-A"]);
    sh(d, &["commit", "-q", "-m", "Add a"]);
    write(d, "src/a.php", "<?php\nfunction a() {\n    return 2;\n}\n");
    sh(d, &["commit", "-q", "-am", "Return 2"]);
    write(d, "docs/x.md", "# Docs\n");
    sh(d, &["add", "-A"]);
    sh(d, &["commit", "-q", "-m", "Add docs"]);
    sh(d, &["mv", "src/a.php", "src/b.php"]);
    sh(d, &["commit", "-q", "-m", "Rename a to b"]);
    write(
        d,
        "src/b.php",
        "<?php\n// b\nfunction a() {\n    return 3;\n}\n",
    );
    sh(d, &["commit", "-q", "-am", "Return 3"]);
    tmp
}

#[test]
fn file_history_follows_renames() {
    let tmp = fixture();
    let repo = Repo::open(tmp.path()).unwrap();
    let h = repo
        .file_history("src/b.php", false, &HistoryQuery::default())
        .unwrap();
    let subjects: Vec<&str> = h.iter().map(|r| r.subject.as_str()).collect();
    assert_eq!(subjects, ["Return 3", "Rename a to b", "Return 2", "Add a"]);
    assert_eq!(h[1].files[0].status, "R");
    assert_eq!(h[1].files[0].orig_path.as_deref(), Some("src/a.php"));
    assert_eq!(h[2].files[0].path, "src/a.php");
    assert_eq!(h[3].files[0].status, "A");
    assert_eq!(h[0].author, "Marlon");

    // The content of each revision is found under its path in that revision.
    let old = repo.show(&h[2].sha, &h[2].files[0].path).unwrap().unwrap();
    assert!(old.contains("return 2;"));

    // Paging.
    let page = repo
        .file_history("src/b.php", false, &HistoryQuery { skip: 1, limit: 2 })
        .unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].subject, "Rename a to b");
}

#[test]
fn folder_history_lists_changed_files() {
    let tmp = fixture();
    let repo = Repo::open(tmp.path()).unwrap();
    let h = repo
        .file_history("docs", true, &HistoryQuery::default())
        .unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].files[0].path, "docs/x.md");
    let all = repo
        .file_history("", true, &HistoryQuery::default())
        .unwrap();
    assert_eq!(all.len(), 5);
}

#[test]
fn workspace_in_a_subfolder_gets_relative_paths() {
    let tmp = fixture();
    let repo = Repo::open(tmp.path().join("src")).unwrap();
    let h = repo
        .file_history("b.php", false, &HistoryQuery::default())
        .unwrap();
    assert_eq!(h[0].files[0].path, "b.php");
    assert_eq!(h[3].files[0].path, "a.php");
    let block = repo.block_history("b.php", 4, 4, 0).unwrap();
    assert_eq!(block[0].path, "b.php");
}

#[test]
fn block_history_shows_the_evolution_of_lines() {
    let tmp = fixture();
    let repo = Repo::open(tmp.path()).unwrap();
    // Line 4 of HEAD is `    return 3;`.
    let h = repo.block_history("src/b.php", 4, 4, 0).unwrap();
    let subjects: Vec<&str> = h.iter().map(|r| r.subject.as_str()).collect();
    assert_eq!(subjects, ["Return 3", "Return 2", "Add a"]);
    assert_eq!(h[0].after, "    return 3;\n");
    assert_eq!(h[0].before.as_deref(), Some("    return 2;\n"));
    assert_eq!(h[0].start_line, 4);
    assert_eq!(h[1].path, "src/a.php");
    assert_eq!(h[2].before, None);
    assert_eq!(h[2].after, "    return 1;\n");

    assert!(repo.block_history("src/b.php", 3, 2, 0).is_err());
}

#[test]
fn blame_of_a_revision_and_changes_since_a_branch() {
    let tmp = fixture();
    let d = tmp.path();
    let repo = Repo::open(d).unwrap();
    let h = repo
        .file_history("src/b.php", false, &HistoryQuery::default())
        .unwrap();
    let blame = repo.blame_at(&h[2].sha, "src/a.php").unwrap();
    assert_eq!(blame.lines.len(), 4);
    assert_eq!(blame.commit_for_line(2).unwrap().summary, "Return 2");
    assert_eq!(blame.commit_for_line(0).unwrap().summary, "Add a");
    assert!(repo.blame_at("-x", "src/a.php").is_err());

    // A branch at "Add docs", then local edits.
    sh(d, &["branch", "old", "HEAD~2"]);
    write(d, "docs/x.md", "# Docs\nmore\n");
    let mut changes = repo.changes_since("old", "").unwrap();
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    let summary: Vec<(String, String)> = changes
        .iter()
        .map(|c| (c.status.clone(), c.path.clone()))
        .collect();
    assert_eq!(
        summary,
        [
            ("M".to_string(), "docs/x.md".to_string()),
            ("R".to_string(), "src/b.php".to_string()),
        ]
    );
    let docs = repo.changes_since("old", "docs").unwrap();
    assert_eq!(docs.len(), 1);
}
