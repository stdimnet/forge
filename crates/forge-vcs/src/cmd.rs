//! Running the `git` executable. No shell is involved: arguments are passed as-is, so
//! paths with spaces or quotes need no escaping on any platform.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use forge_core::{ForgeError, Result};

/// Name of the git binary; `FORGE_GIT` overrides it (e.g. a bundled git on Windows).
fn git_binary() -> String {
    std::env::var("FORGE_GIT").unwrap_or_else(|_| "git".to_string())
}

/// Options for a single git invocation.
#[derive(Default)]
pub(crate) struct Run<'a> {
    pub stdin: Option<&'a [u8]>,
    /// Network operations: never prompt for credentials, fail instead.
    pub non_interactive: bool,
}

/// Raw result of a git invocation that may legitimately fail.
pub(crate) struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) fn run_raw(cwd: &Path, args: &[&str], opts: Run<'_>) -> Result<Output> {
    let mut cmd = Command::new(git_binary());
    cmd.current_dir(cwd)
        .args([
            "-c",
            "core.quotepath=off",
            "-c",
            "color.ui=never",
            "-c",
            "log.showSignature=false",
        ])
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "true")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if opts.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    if opts.non_interactive {
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        if std::env::var_os("GIT_SSH_COMMAND").is_none() {
            cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    tracing::debug!(?args, "git");
    let mut child = cmd
        .spawn()
        .map_err(|e| ForgeError::Other(format!("could not run git (is it installed?): {e}")))?;
    if let Some(input) = opts.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(input)
                .map_err(|e| ForgeError::Other(format!("git stdin: {e}")))?;
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| ForgeError::Other(format!("git: {e}")))?;
    Ok(Output {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Run git and return stdout, turning a non-zero exit into an error carrying stderr.
pub(crate) fn run(cwd: &Path, args: &[&str], opts: Run<'_>) -> Result<String> {
    let out = run_raw(cwd, args, opts)?;
    if out.ok {
        Ok(out.stdout)
    } else {
        Err(error_from(args, &out))
    }
}

pub(crate) fn error_from(args: &[&str], out: &Output) -> ForgeError {
    let msg = if out.stderr.trim().is_empty() {
        out.stdout.trim()
    } else {
        out.stderr.trim()
    };
    let msg = msg
        .lines()
        .map(|l| l.strip_prefix("fatal: ").unwrap_or(l))
        .map(|l| l.strip_prefix("error: ").unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n");
    let sub = args.first().copied().unwrap_or("git");
    if msg.is_empty() {
        ForgeError::Other(format!("git {sub} failed"))
    } else {
        ForgeError::Other(format!("git {sub}: {msg}"))
    }
}
