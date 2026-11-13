//! Partial commits, commit options and Apply / Create Patch against throwaway repositories.
//! Git runs through a [`GitExec`] that adds a fixed identity and ignores the user's
//! configuration, so nothing touches real settings.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use forge_core::Result;
use forge_vcs::hunks::change_hunks;
use forge_vcs::{CommitFile, CommitOptions, GitExec, GitOutput, GitRequest, Repo};
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

fn fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    sh(d, &["init", "-q"]);
    write(d, "a.txt", "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n");
    write(d, "b.txt", "b\n");
    commit_all(d, "Initial commit");
    tmp
}

fn reader(d: &Path) -> impl Fn(&str) -> Result<String> + '_ {
    move |p: &str| Ok(read(d, p))
}

#[test]
fn commits_only_the_included_hunks() {
    let tmp = fixture();
    let d = tmp.path();
    let repo = open(d);
    write(d, "a.txt", "one\n2\n3\n4\n5\n6\n7\n8\n9\nten\n");
    write(d, "b.txt", "b2\n");
    write(d, "new.txt", "new\n");
    // Something staged beforehand stays staged.
    write(d, "staged.txt", "s\n");
    sh(d, &["add", "staged.txt"]);
    let hunks = repo.file_hunks("a.txt", &read(d, "a.txt")).unwrap();
    assert_eq!(hunks.len(), 2);

    let sha = repo
        .commit_files(
            "Partial",
            &[
                CommitFile {
                    path: "a.txt".into(),
                    hunks: Some(vec![hunks[0].id.clone()]),
                },
                CommitFile {
                    path: "new.txt".into(),
                    hunks: None,
                },
            ],
            &CommitOptions::default(),
            &reader(d),
        )
        .unwrap();
    assert_eq!(sh(d, &["rev-parse", "HEAD"]).trim(), sha);
    assert_eq!(head_subject(d), "Partial");
    assert_eq!(
        sh(d, &["show", "HEAD:a.txt"]),
        "one\n2\n3\n4\n5\n6\n7\n8\n9\n10\n"
    );
    assert_eq!(sh(d, &["show", "HEAD:new.txt"]), "new\n");
    assert!(sh(d, &["show", "--stat", "--format=", "HEAD"]).contains("2 files changed"));
    // The working tree is untouched; the excluded hunk and b.txt are still local changes.
    assert_eq!(read(d, "a.txt"), "one\n2\n3\n4\n5\n6\n7\n8\n9\nten\n");
    let status = sh(d, &["status", "--porcelain"]);
    assert!(status.contains(" M a.txt"), "{status}");
    assert!(status.contains(" M b.txt"), "{status}");
    assert!(status.contains("A  staged.txt"), "{status}");
    assert!(!status.contains("new.txt"), "{status}");
    assert_eq!(
        sh(d, &["diff", "--cached", "--name-only"]).trim(),
        "staged.txt"
    );
    // The remaining hunk is now the only one.
    assert_eq!(
        repo.file_hunks("a.txt", &read(d, "a.txt")).unwrap().len(),
        1
    );
}

#[test]
fn commit_options_reach_git() {
    let tmp = fixture();
    let d = tmp.path();
    let repo = open(d);
    // A failing pre-commit hook: skipped with "Run Git hooks" off.
    write(d, ".git/hooks/pre-commit", "#!/bin/sh\nexit 1\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            d.join(".git/hooks/pre-commit"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        write(d, "b.txt", "hooked\n");
        assert!(repo
            .commit_opts(
                "Blocked",
                Some(&["b.txt".into()]),
                &CommitOptions::default()
            )
            .is_err());
    }
    write(d, "b.txt", "c\n");
    let opts = CommitOptions {
        no_verify: true,
        sign_off: true,
        author: Some("Anna Weber <anna@example.com>".into()),
        ..CommitOptions::default()
    };
    repo.commit_opts("With options", Some(&["b.txt".into()]), &opts)
        .unwrap();
    assert_eq!(
        sh(d, &["log", "-1", "--format=%an <%ae>"]).trim(),
        "Anna Weber <anna@example.com>"
    );
    let body = sh(d, &["log", "-1", "--format=%B"]);
    assert!(body.contains("Signed-off-by: "), "{body}");
    let bad = CommitOptions {
        author: Some("nobody".into()),
        ..CommitOptions::default()
    };
    assert!(repo.commit_opts("x", None, &bad).is_err());

    // Amend a partial commit.
    write(d, "a.txt", "x\n2\n3\n4\n5\n6\n7\n8\n9\ny\n");
    let hunks = change_hunks(&sh(d, &["show", "HEAD:a.txt"]), &read(d, "a.txt"));
    let amend = CommitOptions {
        amend: true,
        no_verify: true,
        ..CommitOptions::default()
    };
    repo.commit_files(
        "",
        &[CommitFile {
            path: "a.txt".into(),
            hunks: Some(vec![hunks[1].id.clone()]),
        }],
        &amend,
        &reader(d),
    )
    .unwrap();
    assert_eq!(head_subject(d), "With options");
    assert_eq!(
        sh(d, &["show", "HEAD:a.txt"]),
        "1\n2\n3\n4\n5\n6\n7\n8\n9\ny\n"
    );
    assert_eq!(sh(d, &["show", "HEAD:b.txt"]), "c\n");
}

#[test]
fn creates_and_applies_patches() {
    let tmp = fixture();
    let d = tmp.path();
    let repo = open(d);
    write(d, "a.txt", "1\n2\nthree\n4\n5\n6\n7\n8\n9\n10\n");
    write(d, "n.txt", "new file\n");
    let patch = repo
        .create_patch(&["a.txt".into(), "n.txt".into()], false)
        .unwrap();
    assert!(patch.contains("diff --git a/a.txt b/a.txt"));
    assert!(patch.contains("+++ b/n.txt"));
    let reverse = repo.create_patch(&["a.txt".into()], true).unwrap();
    assert!(
        reverse.contains("-three") && reverse.contains("+3"),
        "{reverse}"
    );

    // Undo the changes, then apply the patch again.
    sh(d, &["checkout", "--", "a.txt"]);
    fs::remove_file(d.join("n.txt")).unwrap();
    let res = repo.apply_patch(&patch, "", None).unwrap();
    assert_eq!(res.applied, vec!["a.txt".to_string(), "n.txt".to_string()]);
    assert!(res.conflicts.is_empty() && res.failed.is_empty());
    assert_eq!(read(d, "a.txt"), "1\n2\nthree\n4\n5\n6\n7\n8\n9\n10\n");
    assert!(sh(d, &["status", "--porcelain"]).contains("A  n.txt"));

    // A foreign patch into a base directory, limited to one file.
    write(d, "sub/x.txt", "x\n");
    commit_all(d, "Add sub");
    let foreign = "--- x.txt\t2026-01-01\n+++ x.txt\t2026-01-02\n@@ -1 +1 @@\n-x\n+X\n--- y.txt\n+++ y.txt\n@@ -1 +1 @@\n-y\n+Y\n";
    let res = repo
        .apply_patch(foreign, "sub", Some(&["sub/x.txt".into()]))
        .unwrap();
    assert_eq!(res.applied, vec!["sub/x.txt".to_string()]);
    assert_eq!(read(d, "sub/x.txt"), "X\n");
}

#[test]
fn conflicting_patches_are_merged_three_way() {
    let tmp = fixture();
    let d = tmp.path();
    let repo = open(d);
    write(d, "a.txt", "1\n2\nmine\n4\n5\n6\n7\n8\n9\n10\n");
    let patch = repo.create_patch(&["a.txt".into()], false).unwrap();
    sh(d, &["checkout", "--", "a.txt"]);
    // Somebody else changed the same line meanwhile.
    write(d, "a.txt", "1\n2\ntheirs\n4\n5\n6\n7\n8\n9\n10\n");
    commit_all(d, "Change line 3");
    let res = repo.apply_patch(&patch, "", None).unwrap();
    assert_eq!(res.conflicts, vec!["a.txt".to_string()]);
    assert!(read(d, "a.txt").contains("<<<<<<<"));
    assert!(repo
        .conflicts()
        .unwrap()
        .files
        .iter()
        .any(|f| f.path == "a.txt"));

    // A patch without base versions that does not fit cannot be merged.
    sh(d, &["checkout", "-f", "HEAD", "--"]);
    sh(d, &["reset", "-q", "--hard"]);
    let foreign = "--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-zzz\n+b2\n";
    let res = repo.apply_patch(foreign, "", None).unwrap();
    assert_eq!(res.failed.len(), 1);
    assert_eq!(read(d, "b.txt"), "b\n");
}
