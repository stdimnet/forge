//! Merge conflicts against throwaway repositories: stage versions, sides of merges and
//! rebases, Accept Yours / Theirs and resolving with a merged text.

use std::fs;
use std::path::Path;
use std::process::Command;

use forge_vcs::merge::ChunkKind;
use forge_vcs::{ConflictSide, MergeOperation, Repo, SideChange};
use tempfile::TempDir;

/// Run git with a fixed identity and no user or system configuration.
fn git(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new("git")
        .current_dir(dir)
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
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join(".no-global-config"))
        .env("HOME", dir)
        .env("GIT_EDITOR", "true")
        .output()
        .expect("git runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn sh(dir: &Path, args: &[&str]) -> String {
    let (ok, out) = git(dir, args);
    assert!(ok, "git {args:?}: {out}");
    out
}

fn write(dir: &Path, path: &str, contents: &[u8]) {
    let p = dir.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, contents).unwrap();
}

fn commit_all(dir: &Path, message: &str) {
    sh(dir, &["add", "-A"]);
    sh(dir, &["commit", "-q", "-m", message]);
}

const CART: &str = "<?php\n\nclass Cart\n{\n    public function total(): int\n    {\n        return $price * $qty;\n    }\n}\n";

/// `main` and `feature` both changed `Cart.php`, `Simple.php` and `Gone.php` (deleted on
/// `feature`), and `logo.png` (binary).
fn diverged() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    sh(d, &["init", "-q"]);
    write(d, "src/Cart.php", CART.as_bytes());
    write(d, "src/Simple.php", b"<?php\n$total = $price * $qty;\n");
    write(d, "src/Gone.php", b"<?php\necho 'old';\n");
    write(d, "logo.png", b"PNG\0\x01\x02");
    write(d, "README.md", b"# Shop\n");
    commit_all(d, "Initial commit");

    sh(d, &["checkout", "-q", "-b", "feature"]);
    write(
        d,
        "src/Cart.php",
        CART.replace("class Cart", "class Basket").as_bytes(),
    );
    write(
        d,
        "src/Simple.php",
        b"<?php\n$total = $price * $quantity;\n",
    );
    fs::remove_file(d.join("src/Gone.php")).unwrap();
    write(d, "logo.png", b"PNG\0\x03");
    write(d, "README.md", b"# Shop\n\nFeature docs.\n");
    commit_all(d, "Feature work");

    sh(d, &["checkout", "-q", "main"]);
    write(
        d,
        "src/Cart.php",
        CART.replace("class Cart", "final class Cart").as_bytes(),
    );
    write(d, "src/Simple.php", b"<?php\n$total = $netPrice * $qty;\n");
    write(d, "src/Gone.php", b"<?php\necho 'new';\n");
    write(d, "logo.png", b"PNG\0\x04");
    commit_all(d, "Main work");
    tmp
}

#[test]
fn merge_conflicts_list_files_with_both_sides() {
    let tmp = diverged();
    let d = tmp.path();
    let (ok, out) = git(d, &["merge", "--no-edit", "feature"]);
    assert!(!ok && out.contains("CONFLICT"), "{out}");

    let repo = Repo::open(d).unwrap();
    let set = repo.conflicts().unwrap();
    assert_eq!(set.operation, MergeOperation::Merge);
    assert_eq!(set.description, "Merging branch feature into branch main");
    assert_eq!(
        (set.yours_name.as_str(), set.theirs_name.as_str()),
        ("main", "feature")
    );
    assert_eq!(set.yours_title, "Changes from main");
    assert_eq!(set.theirs_title, "Changes from feature");
    assert!(!set.reversed);
    let paths: Vec<&str> = set.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["logo.png", "src/Cart.php", "src/Gone.php", "src/Simple.php"]
    );
    let gone = set.files.iter().find(|f| f.path == "src/Gone.php").unwrap();
    assert_eq!(
        (gone.yours, gone.theirs),
        (SideChange::Modified, SideChange::Deleted)
    );

    let cart = repo.merge_file("src/Cart.php").unwrap();
    assert!(!cart.binary && !cart.crlf);
    assert_eq!(cart.base.as_deref(), Some(CART));
    assert!(cart.yours.as_deref().unwrap().contains("final class Cart"));
    assert!(cart.theirs.as_deref().unwrap().contains("class Basket"));
    let model = cart.model.unwrap();
    assert_eq!(model.conflicts, 1);
    assert_eq!(model.chunks[0].kind, ChunkKind::Conflict);

    // Different words of the same line: a simple conflict.
    let simple = repo.merge_file("src/Simple.php").unwrap().model.unwrap();
    assert_eq!(simple.simple_conflicts, 1);
    assert_eq!(
        simple.chunks[0].simple.as_deref(),
        Some("$total = $netPrice * $quantity;\n")
    );

    let gone = repo.merge_file("src/Gone.php").unwrap();
    assert_eq!(gone.theirs, None);
    assert!(gone.model.is_none());

    let logo = repo.merge_file("logo.png").unwrap();
    assert!(logo.binary && logo.model.is_none());

    assert!(repo.merge_file("README.md").is_err(), "merged cleanly");
}

#[test]
fn accepting_sides_and_merged_text_resolve_the_files() {
    let tmp = diverged();
    let d = tmp.path();
    git(d, &["merge", "--no-edit", "feature"]);
    let repo = Repo::open(d).unwrap();

    repo.accept_conflict_side(&["src/Gone.php".into()], ConflictSide::Theirs)
        .unwrap();
    assert!(!d.join("src/Gone.php").exists(), "theirs deleted the file");

    repo.accept_conflict_side(
        &["src/Cart.php".into(), "logo.png".into()],
        ConflictSide::Yours,
    )
    .unwrap();
    let cart = fs::read_to_string(d.join("src/Cart.php")).unwrap();
    assert!(cart.contains("final class Cart") && !cart.contains("<<<<<<<"));
    assert_eq!(fs::read(d.join("logo.png")).unwrap(), b"PNG\0\x04");

    write(
        d,
        "src/Simple.php",
        b"<?php\n$total = $netPrice * $quantity;\n",
    );
    repo.stage(&["src/Simple.php".into()]).unwrap();

    assert!(repo.conflicts().unwrap().files.is_empty());
    assert!(repo.status().unwrap().entries.iter().all(|e| !e.conflict));
    sh(d, &["commit", "-q", "--no-edit"]);
}

#[test]
fn keeping_a_file_deleted_on_the_other_side() {
    let tmp = diverged();
    let d = tmp.path();
    git(d, &["merge", "--no-edit", "feature"]);
    let repo = Repo::open(d).unwrap();
    repo.accept_conflict_side(&["src/Gone.php".into()], ConflictSide::Yours)
        .unwrap();
    assert_eq!(
        fs::read_to_string(d.join("src/Gone.php")).unwrap(),
        "<?php\necho 'new';\n"
    );
    let set = repo.conflicts().unwrap();
    assert!(!set.files.iter().any(|f| f.path == "src/Gone.php"));
}

#[test]
fn rebase_conflicts_swap_yours_and_theirs() {
    let tmp = diverged();
    let d = tmp.path();
    sh(d, &["checkout", "-q", "feature"]);
    let (ok, out) = git(d, &["rebase", "main"]);
    assert!(!ok, "{out}");

    let repo = Repo::open(d).unwrap();
    let set = repo.conflicts().unwrap();
    assert_eq!(set.operation, MergeOperation::Rebase);
    assert!(set.reversed);
    assert_eq!(set.description, "Rebasing branch feature onto main");
    assert_eq!(
        set.theirs_title,
        "Already rebased commits and commits from main"
    );
    assert!(set.yours_title.starts_with("Rebasing ") && set.yours_title.ends_with(" from feature"));
    let gone = set.files.iter().find(|f| f.path == "src/Gone.php").unwrap();
    // Yours is the feature commit, which deleted the file.
    assert_eq!(
        (gone.yours, gone.theirs),
        (SideChange::Deleted, SideChange::Modified)
    );

    let cart = repo.merge_file("src/Cart.php").unwrap();
    assert!(cart.yours.unwrap().contains("class Basket"));
    assert!(cart.theirs.unwrap().contains("final class Cart"));

    repo.accept_conflict_side(&["src/Cart.php".into()], ConflictSide::Yours)
        .unwrap();
    assert!(fs::read_to_string(d.join("src/Cart.php"))
        .unwrap()
        .contains("class Basket"));
}

#[test]
fn crlf_files_are_normalized() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    sh(d, &["init", "-q"]);
    write(d, "a.txt", b"one\r\ntwo\r\n");
    commit_all(d, "Initial commit");
    sh(d, &["checkout", "-q", "-b", "feature"]);
    write(d, "a.txt", b"one\r\nTWO\r\n");
    commit_all(d, "Feature");
    sh(d, &["checkout", "-q", "main"]);
    write(d, "a.txt", b"one\r\n2\r\n");
    commit_all(d, "Main");
    git(d, &["merge", "--no-edit", "feature"]);

    let file = Repo::open(d).unwrap().merge_file("a.txt").unwrap();
    assert!(file.crlf);
    assert_eq!(file.yours.as_deref(), Some("one\n2\n"));
    assert_eq!(file.model.unwrap().chunks[0].base_text, "two\n");
}

#[test]
fn unstash_conflicts_without_an_operation_head() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    sh(d, &["init", "-q"]);
    write(d, "a.txt", b"one\n");
    commit_all(d, "Initial commit");
    write(d, "a.txt", b"stashed\n");
    sh(d, &["stash", "push", "-q"]);
    write(d, "a.txt", b"committed\n");
    commit_all(d, "Change");
    let (ok, out) = git(d, &["stash", "pop"]);
    assert!(!ok, "{out}");

    let set = Repo::open(d).unwrap().conflicts().unwrap();
    assert_eq!(set.operation, MergeOperation::Unstash);
    assert_eq!(set.theirs_title, "Changes from stash");
    assert_eq!(set.files.len(), 1);
}
