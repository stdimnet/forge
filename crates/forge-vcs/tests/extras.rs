//! Get from VCS, Commit Message History, worktrees and commit signing against throwaway
//! repositories (local bare remotes, fake `gpg` / `ssh-keygen` scripts, no network).

#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use forge_vcs::clone::{clone_invocation, parse_progress, HttpsCredential};
use forge_vcs::{CommitOptions, LogQuery, NewWorktree, Repo, SignatureStatus, Signing, SigningKey};
use tempfile::TempDir;

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
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

/// A repo with one commit on `main`, inside `<tmp>/shop`.
fn fixture() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path().join("shop");
    fs::create_dir_all(&d).unwrap();
    sh(&d, &["init", "-q", "-b", "main"]);
    sh(&d, &["config", "user.name", "Marlon"]);
    sh(&d, &["config", "user.email", "marlon@example.com"]);
    sh(&d, &["config", "commit.gpgsign", "false"]);
    write(&d, "a.php", "<?php\necho 1;\n");
    sh(&d, &["add", "-A"]);
    sh(&d, &["commit", "-q", "-m", "Initial commit"]);
    (tmp, d)
}

fn script(dir: &Path, name: &str, body: &str) -> String {
    let p = dir.join(name);
    fs::write(&p, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn clones_from_a_local_bare_remote_with_progress() {
    let (tmp, d) = fixture();
    let bare = tmp.path().join("remote.git");
    sh(
        tmp.path(),
        &[
            "clone",
            "-q",
            "--bare",
            d.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    let url = format!("file://{}", bare.display());
    let target = tmp.path().join("clones/shop-copy");
    let inv = clone_invocation(&url, target.to_str().unwrap(), None, true).unwrap();
    let out = Command::new("git")
        .current_dir(tmp.path())
        .args(&inv.args)
        .envs(inv.env.iter().map(|(k, v)| (k, v)))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let phases: Vec<String> = stderr
        .split(['\r', '\n'])
        .filter_map(parse_progress)
        .map(|p| p.phase)
        .collect();
    assert_eq!(phases.first().map(String::as_str), Some("Cloning"));
    assert!(
        phases.iter().any(|p| p == "Receiving objects"),
        "{phases:?}"
    );
    let repo = Repo::open(&target).unwrap();
    assert_eq!(repo.info().unwrap().branch.as_deref(), Some("main"));
    // Nothing about credentials ends up in the clone's config.
    let config = fs::read_to_string(target.join(".git/config")).unwrap();
    assert!(!config.contains("credential"));
}

#[test]
fn the_credential_helper_answers_from_the_environment_only() {
    let tmp = TempDir::new().unwrap();
    let cred = HttpsCredential {
        username: "oauth2".into(),
        password: "glpat-secret".into(),
    };
    let inv =
        clone_invocation("https://gitlab.example.com/a/b.git", "b", Some(&cred), true).unwrap();
    let mut args: Vec<String> = inv.args[..4].to_vec();
    args.extend(["credential".into(), "fill".into()]);
    let mut child = Command::new("git")
        .current_dir(tmp.path())
        .args(&args)
        .envs(inv.env.iter().map(|(k, v)| (k, v)))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"protocol=https\nhost=gitlab.example.com\n\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("username=oauth2"), "{text}");
    assert!(text.contains("password=glpat-secret"), "{text}");
}

#[test]
fn worktrees_are_listed_created_locked_and_removed() {
    let (tmp, d) = fixture();
    sh(&d, &["branch", "feature/x"]);
    let repo = Repo::open(&d).unwrap();
    let list = repo.worktrees().unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].main && list[0].current);

    let existing = tmp.path().join("shop-x");
    repo.add_worktree(&NewWorktree {
        path: existing.to_string_lossy().into(),
        branch: Some("feature/x".into()),
        ..NewWorktree::default()
    })
    .unwrap();
    let fresh = tmp.path().join("shop-y");
    repo.add_worktree(&NewWorktree {
        path: fresh.to_string_lossy().into(),
        new_branch: Some("feature/y".into()),
        base: Some("main".into()),
        ..NewWorktree::default()
    })
    .unwrap();
    let list = repo.worktrees().unwrap();
    assert_eq!(list.len(), 3);
    let x = list
        .iter()
        .find(|w| w.branch.as_deref() == Some("feature/x"))
        .unwrap();
    assert!(!x.current && !x.main);

    // The branches popup marks branches checked out in other worktrees.
    let branches = repo.branches().unwrap();
    let wt = |name: &str| {
        branches
            .local
            .iter()
            .find(|b| b.name == name)
            .unwrap()
            .worktree
            .clone()
    };
    assert_eq!(wt("main"), None);
    assert!(wt("feature/x").unwrap().ends_with("shop-x"));
    assert!(wt("feature/y").unwrap().ends_with("shop-y"));

    // From inside a linked worktree, that one is current.
    let inside = Repo::open(&existing).unwrap();
    assert!(inside
        .worktrees()
        .unwrap()
        .iter()
        .any(|w| w.current && w.branch.as_deref() == Some("feature/x")));

    repo.lock_worktree(&x.path, Some("on a USB drive")).unwrap();
    let locked = repo.worktrees().unwrap();
    let x = locked.iter().find(|w| w.path == x.path).unwrap();
    assert!(x.locked);
    assert_eq!(x.lock_reason.as_deref(), Some("on a USB drive"));
    assert!(repo.remove_worktree(&x.path, false).is_err());
    repo.unlock_worktree(&x.path).unwrap();
    repo.remove_worktree(&x.path, false).unwrap();

    // A worktree whose directory vanished is prunable.
    fs::remove_dir_all(&fresh).unwrap();
    assert!(repo.worktrees().unwrap().iter().any(|w| w.prunable));
    repo.prune_worktrees().unwrap();
    assert_eq!(repo.worktrees().unwrap().len(), 1);
}

#[test]
fn own_commit_messages_come_from_the_configured_author() {
    let (_tmp, d) = fixture();
    write(&d, "b.php", "<?php\n");
    sh(&d, &["add", "-A"]);
    let out = Command::new("git")
        .current_dir(&d)
        .args(["commit", "-q", "-m", "By Anna"])
        .env("GIT_AUTHOR_EMAIL", "anna@example.com")
        .output()
        .unwrap();
    assert!(out.status.success());
    write(&d, "c.php", "<?php\n");
    sh(&d, &["add", "-A"]);
    sh(&d, &["commit", "-q", "-m", "Add C\n\nWith a body."]);
    let repo = Repo::open(&d).unwrap();
    assert_eq!(
        repo.own_commit_messages(10).unwrap(),
        vec!["Add C\n\nWith a body.", "Initial commit"]
    );
    assert_eq!(repo.own_commit_messages(1).unwrap().len(), 1);
}

const FAKE_GPG: &str = r#"
for a; do echo "$a" >> "$(dirname "$0")/gpg-args.log"; done
case " $* " in
  *" --verify "*)
    echo "[GNUPG:] NEWSIG"
    echo "[GNUPG:] GOODSIG 3AA5C34371567BD2 Marlon <marlon@example.com>"
    echo "[GNUPG:] VALIDSIG 0123456789ABCDEF01233AA5C34371567BD2 2026-10-22 1700000000 0 4 0 22 10 00 0123456789ABCDEF01233AA5C34371567BD2"
    echo "[GNUPG:] TRUST_ULTIMATE 0 pgp"
    exit 0;;
esac
cat > /dev/null
echo "[GNUPG:] BEGIN_SIGNING H10" >&2
echo "[GNUPG:] SIG_CREATED S 22 10 00 1700000000 0123456789ABCDEF01233AA5C34371567BD2" >&2
echo "-----BEGIN PGP SIGNATURE-----"
echo ""
echo "ZmFrZQ=="
echo "-----END PGP SIGNATURE-----"
"#;

#[test]
fn commits_are_signed_with_the_chosen_gpg_key_and_the_log_shows_it() {
    let (tmp, d) = fixture();
    let gpg = script(tmp.path(), "fake-gpg", FAKE_GPG);
    // Only the throwaway repository is configured; the key comes from the commit options.
    sh(&d, &["config", "gpg.program", &gpg]);
    let repo = Repo::open(&d).unwrap();
    write(&d, "b.php", "<?php\n");
    let opts = CommitOptions {
        signing: Signing {
            sign: true,
            key: SigningKey::from_setting("openpgp:3AA5C34371567BD2"),
        },
        ..CommitOptions::default()
    };
    let sha = repo
        .commit_opts("Signed commit", Some(&["b.php".to_string()]), &opts)
        .unwrap();
    let raw = sh(&d, &["cat-file", "commit", &sha]);
    assert!(
        raw.contains("gpgsig -----BEGIN PGP SIGNATURE-----"),
        "{raw}"
    );
    let args = fs::read_to_string(tmp.path().join("gpg-args.log")).unwrap();
    assert!(args.lines().any(|l| l == "3AA5C34371567BD2"), "{args}");
    // Not written to the repository's config.
    let config = fs::read_to_string(d.join(".git/config")).unwrap();
    assert!(!config.contains("signingkey"), "{config}");

    let log = repo.log(&LogQuery::default()).unwrap();
    assert_eq!(log[0].signature, SignatureStatus::Good);
    assert_eq!(log[1].signature, SignatureStatus::None);
    let details = repo.commit_details(&sha).unwrap();
    assert_eq!(details.signature, SignatureStatus::Good);
    assert_eq!(
        details.signer.as_deref(),
        Some("Marlon <marlon@example.com>")
    );
    assert_eq!(details.signing_key.as_deref(), Some("3AA5C34371567BD2"));
    assert_eq!(details.message, "Signed commit");
}

#[test]
fn commits_are_signed_with_an_ssh_key() {
    let (tmp, d) = fixture();
    let keygen = script(
        tmp.path(),
        "fake-ssh-keygen",
        r#"for a; do last="$a"; echo "$a" >> "$(dirname "$0")/ssh-args.log"; done
printf -- '-----BEGIN SSH SIGNATURE-----\nZmFrZQ==\n-----END SSH SIGNATURE-----\n' > "$last.sig"
"#,
    );
    sh(&d, &["config", "gpg.ssh.program", &keygen]);
    let key = tmp.path().join("id_ed25519.pub");
    fs::write(
        &key,
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI marlon@laptop\n",
    )
    .unwrap();
    let repo = Repo::open(&d).unwrap();
    write(&d, "a.php", "<?php\necho 2;\n");
    let opts = CommitOptions {
        signing: Signing {
            sign: true,
            key: Some(SigningKey {
                format: forge_vcs::SigningFormat::Ssh,
                key: key.to_string_lossy().into(),
            }),
        },
        ..CommitOptions::default()
    };
    let sha = repo
        .commit_files(
            "SSH signed",
            &[forge_vcs::CommitFile {
                path: "a.php".into(),
                hunks: None,
            }],
            &opts,
            &|p| Ok(fs::read_to_string(d.join(p)).unwrap()),
        )
        .unwrap();
    let raw = sh(&d, &["cat-file", "commit", &sha]);
    assert!(
        raw.contains("gpgsig -----BEGIN SSH SIGNATURE-----"),
        "{raw}"
    );
    let args = fs::read_to_string(tmp.path().join("ssh-args.log")).unwrap();
    assert!(args.contains(&*key.to_string_lossy()), "{args}");
    let config = fs::read_to_string(d.join(".git/config")).unwrap();
    assert!(
        !config.contains("format = ssh") && !config.contains("signingkey"),
        "{config}"
    );
    // The repository turns signing off; Forge only reads that.
    assert!(!repo.signing_config().gpg_sign);
}
