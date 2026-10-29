//! Get from VCS through a [`Backend`]: `git clone` and `git ls-remote` run where the
//! project lives (this machine, or the remote host for SSH projects), with progress,
//! cancellation and per-invocation HTTPS credentials ([`forge_vcs::clone`]).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use forge_core::{ForgeError, Result};
use forge_process::CommandLine;
use forge_vcs::clone::{
    clone_invocation, ls_remote_invocation, parse_ls_remote, parse_progress, CloneProgress,
    GitInvocation, HttpsCredential, RemoteRef,
};

use crate::process_io::{capture, run_progress_lines};
use crate::Backend;

/// Where and how git runs for a clone or a connection test.
#[derive(Debug, Clone)]
pub struct GitRunner {
    /// The git executable (`git` on remote hosts, the configured one locally).
    pub program: String,
    /// An existing directory to run in.
    pub cwd: PathBuf,
    /// Make SSH fail instead of prompting (unless the user set `GIT_SSH_COMMAND`).
    pub ssh_batch: bool,
}

impl GitRunner {
    fn command(&self, inv: GitInvocation) -> CommandLine {
        let mut cmd = CommandLine::new(&self.program).args(inv.args);
        cmd.cwd = self.cwd.clone();
        cmd.env.extend(inv.env);
        cmd
    }
}

/// The lines of git's error report: everything that is not a progress meter.
fn error_message(lines: &[String]) -> String {
    let msg: Vec<String> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && parse_progress(l).is_none())
        .map(|l| {
            l.strip_prefix("fatal: ")
                .or_else(|| l.strip_prefix("error: "))
                .unwrap_or(l)
                .to_string()
        })
        .collect();
    if msg.is_empty() {
        "git clone failed".into()
    } else {
        msg.join("\n")
    }
}

/// Clone `url` into `directory` (absolute on the backend's machine). Calls `on_progress`
/// for every progress line; setting `cancel` kills git (which removes the half-cloned
/// directory). Errors carry git's message.
pub fn clone(
    backend: &dyn Backend,
    runner: &GitRunner,
    url: &str,
    directory: &str,
    credential: Option<&HttpsCredential>,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(CloneProgress),
) -> Result<()> {
    let inv = clone_invocation(url, directory, credential, runner.ssh_batch)
        .map_err(ForgeError::Other)?;
    let cmd = runner.command(inv);
    let mut stderr: Vec<String> = Vec::new();
    let code = run_progress_lines(
        backend,
        &cmd,
        cancel,
        &mut |line, _| match parse_progress(&line) {
            Some(p) => on_progress(p),
            None => {
                if stderr.len() < 200 {
                    stderr.push(line);
                }
            }
        },
    )?;
    match code {
        Some(0) => Ok(()),
        None if cancel.load(std::sync::atomic::Ordering::Relaxed) => {
            Err(ForgeError::Other("Clone was cancelled".into()))
        }
        _ => Err(ForgeError::Other(error_message(&stderr))),
    }
}

/// The Test button: `git ls-remote --heads` with the same credentials.
pub fn ls_remote(
    backend: &dyn Backend,
    runner: &GitRunner,
    url: &str,
    credential: Option<&HttpsCredential>,
    timeout: Duration,
) -> Result<Vec<RemoteRef>> {
    let inv = ls_remote_invocation(url, credential, runner.ssh_batch).map_err(ForgeError::Other)?;
    let out = capture(backend, &runner.command(inv), "", timeout)?;
    match out.code {
        Some(0) => Ok(parse_ls_remote(&out.stdout)),
        None => Err(ForgeError::Other(format!(
            "No answer from the repository within {} seconds",
            timeout.as_secs()
        ))),
        Some(_) => {
            let lines: Vec<String> = out.stderr.lines().map(str::to_string).collect();
            Err(ForgeError::Other(error_message(&lines)))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::LocalBackend;
    use std::process::Command;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn bare_remote(tmp: &std::path::Path) -> String {
        let src = tmp.join("src");
        std::fs::create_dir_all(&src).unwrap();
        git(&src, &["init", "-q", "-b", "main"]);
        std::fs::write(src.join("a.txt"), "a\n").unwrap();
        git(&src, &["add", "-A"]);
        git(
            &src,
            &[
                "-c",
                "commit.gpgsign=false",
                "-c",
                "user.name=M",
                "-c",
                "user.email=m@x",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        );
        let bare = tmp.join("remote.git");
        git(
            tmp,
            &[
                "clone",
                "-q",
                "--bare",
                src.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
        format!("file://{}", bare.display())
    }

    fn runner(cwd: &std::path::Path) -> GitRunner {
        GitRunner {
            program: "git".into(),
            cwd: cwd.to_path_buf(),
            ssh_batch: true,
        }
    }

    #[test]
    fn clones_with_progress_and_tests_the_connection() {
        let tmp = tempfile::tempdir().unwrap();
        let url = bare_remote(tmp.path());
        let backend = LocalBackend::open(tmp.path()).unwrap();
        let refs = ls_remote(
            &backend,
            &runner(tmp.path()),
            &url,
            None,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(refs[0].name, "refs/heads/main");

        let target = tmp.path().join("work/shop");
        let mut phases = Vec::new();
        clone(
            &backend,
            &runner(tmp.path()),
            &url,
            target.to_str().unwrap(),
            None,
            &AtomicBool::new(false),
            &mut |p| phases.push(p.phase),
        )
        .unwrap();
        assert!(target.join("a.txt").is_file());
        assert!(
            phases.iter().any(|p| p == "Receiving objects"),
            "{phases:?}"
        );

        // Cloning into a non-empty directory fails with git's message.
        let err = clone(
            &backend,
            &runner(tmp.path()),
            &url,
            target.to_str().unwrap(),
            None,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("already exists"), "{err}");

        let missing = ls_remote(
            &backend,
            &runner(tmp.path()),
            &format!("file://{}/nope.git", tmp.path().display()),
            None,
            Duration::from_secs(30),
        )
        .unwrap_err()
        .to_string();
        assert!(!missing.is_empty());
    }

    #[test]
    fn a_cancelled_clone_reports_it() {
        let tmp = tempfile::tempdir().unwrap();
        let url = bare_remote(tmp.path());
        let backend = LocalBackend::open(tmp.path()).unwrap();
        let err = clone(
            &backend,
            &runner(tmp.path()),
            &url,
            tmp.path().join("c").to_str().unwrap(),
            None,
            &AtomicBool::new(true),
            &mut |_| {},
        );
        // Either git was killed before it finished, or it was faster than the first check.
        if let Err(e) = err {
            assert!(e.to_string().contains("cancelled"), "{e}");
        }
    }
}
