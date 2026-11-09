//! Running the `git` executable. No shell is involved: arguments are passed as-is, so
//! paths with spaces or quotes need no escaping on any platform.
//!
//! Every invocation goes through a [`GitExec`]: [`LocalGit`] runs git on this machine, other
//! implementations (a remote development agent) run it elsewhere and return the raw output,
//! which is parsed here as usual.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::RwLock;

use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};

/// One git invocation: arguments after `git` (without the fixed `-c` options), working
/// directory and optional stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRequest {
    pub cwd: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub stdin: Option<String>,
    /// Network operations: never prompt for credentials, fail instead.
    #[serde(default)]
    pub non_interactive: bool,
}

/// Raw result of a git invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitOutput {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Result of a git invocation whose stdout is kept as raw bytes ([`GitExec::exec_bytes`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitBytesOutput {
    pub ok: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
    /// Whether stdin and stdout went through unchanged. `false` when the transport only
    /// carries text, so bytes that are not UTF-8 were replaced by U+FFFD on the way.
    pub faithful: bool,
}

/// Runs git somewhere: on this machine ([`LocalGit`]) or on a remote host.
pub trait GitExec: Send + Sync {
    fn exec(&self, req: &GitRequest) -> Result<GitOutput>;

    /// Like [`GitExec::exec`], but with raw bytes for stdin and stdout (patches of files that
    /// are not UTF-8, e.g. Windows-1252 sources). The default goes through the text transport
    /// and reports `faithful: false`; callers must not rely on bytes that are not UTF-8 then.
    fn exec_bytes(&self, req: &GitRequest, stdin: Option<&[u8]>) -> Result<GitBytesOutput> {
        let mut req = req.clone();
        if let Some(input) = stdin {
            req.stdin = Some(String::from_utf8_lossy(input).into_owned());
        }
        let out = self.exec(&req)?;
        Ok(GitBytesOutput {
            ok: out.ok,
            stdout: out.stdout.into_bytes(),
            stderr: out.stderr,
            faithful: false,
        })
    }
}

/// Runs the local `git` binary.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalGit;

impl GitExec for LocalGit {
    fn exec(&self, req: &GitRequest) -> Result<GitOutput> {
        let args: Vec<&str> = req.args.iter().map(String::as_str).collect();
        let out = run_local(
            Path::new(&req.cwd),
            &args,
            Run {
                stdin: req.stdin.as_deref().map(str::as_bytes),
                non_interactive: req.non_interactive,
            },
        )?;
        Ok(GitOutput {
            ok: out.ok,
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: out.stderr,
        })
    }

    fn exec_bytes(&self, req: &GitRequest, stdin: Option<&[u8]>) -> Result<GitBytesOutput> {
        let args: Vec<&str> = req.args.iter().map(String::as_str).collect();
        let out = run_local(
            Path::new(&req.cwd),
            &args,
            Run {
                stdin: stdin.or(req.stdin.as_deref().map(str::as_bytes)),
                non_interactive: req.non_interactive,
            },
        )?;
        Ok(GitBytesOutput {
            ok: out.ok,
            stdout: out.stdout,
            stderr: out.stderr,
            faithful: true,
        })
    }
}

/// The git executable chosen in the settings (`vcs.git.path`), if any.
static PROGRAM: RwLock<Option<String>> = RwLock::new(None);

/// Use `path` as the git executable for every later [`LocalGit`] invocation in this process
/// (Settings | Version Control | Git, "Path to Git executable"). `None`, an empty path or
/// plain `git` use the one on `PATH`. Remote agents are separate processes and keep the git
/// of their host.
pub fn set_git_program(path: Option<&str>) {
    let program = path
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "git")
        .map(str::to_string);
    *PROGRAM.write().unwrap_or_else(|e| e.into_inner()) = program;
}

/// Name of the git binary: the configured path, else `FORGE_GIT` (e.g. a bundled git on
/// Windows), else `git` from `PATH`.
pub fn git_binary() -> String {
    if let Some(p) = PROGRAM.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return p;
    }
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

/// Run git through `exec` (see [`GitExec`]).
pub(crate) fn run_raw(
    exec: &dyn GitExec,
    cwd: &Path,
    args: &[&str],
    opts: Run<'_>,
) -> Result<Output> {
    let out = exec.exec(&GitRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        args: args.iter().map(|a| a.to_string()).collect(),
        stdin: opts.stdin.map(|s| String::from_utf8_lossy(s).into_owned()),
        non_interactive: opts.non_interactive,
    })?;
    Ok(Output {
        ok: out.ok,
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// Run git through `exec`, keeping stdin and stdout as bytes (see [`GitExec::exec_bytes`]).
pub(crate) fn run_raw_bytes(
    exec: &dyn GitExec,
    cwd: &Path,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<GitBytesOutput> {
    exec.exec_bytes(
        &GitRequest {
            cwd: cwd.to_string_lossy().into_owned(),
            args: args.iter().map(|a| a.to_string()).collect(),
            stdin: None,
            non_interactive: false,
        },
        stdin,
    )
}

impl From<GitBytesOutput> for Output {
    fn from(out: GitBytesOutput) -> Self {
        Output {
            ok: out.ok,
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: out.stderr,
        }
    }
}

/// Output of the local git binary; stdout stays raw.
struct LocalOutput {
    ok: bool,
    stdout: Vec<u8>,
    stderr: String,
}

fn run_local(cwd: &Path, args: &[&str], opts: Run<'_>) -> Result<LocalOutput> {
    let program = git_binary();
    let mut cmd = Command::new(&program);
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
        .env("GIT_EDITOR", "true");
    if opts.non_interactive {
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        if forge_hostspawn::env_var("GIT_SSH_COMMAND").is_none() {
            cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
    }
    // In Flatpak, git (and its credential helpers and SSH) run on the host.
    let mut cmd = forge_hostspawn::host_command(cmd);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if opts.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    tracing::debug!(?args, "git");
    let mut child = cmd.spawn().map_err(|e| {
        ForgeError::Other(format!(
            "could not run {program} (is git installed? See Settings | Version Control | Git): {e}"
        ))
    })?;
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
    Ok(LocalOutput {
        ok: out.status.success(),
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Run git and return stdout, turning a non-zero exit into an error carrying stderr.
pub(crate) fn run(exec: &dyn GitExec, cwd: &Path, args: &[&str], opts: Run<'_>) -> Result<String> {
    let out = run_raw(exec, cwd, args, opts)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configured_program_replaces_git_on_path() {
        let default = git_binary();
        set_git_program(Some("  /opt/git/bin/git "));
        assert_eq!(git_binary(), "/opt/git/bin/git");
        // Plain `git` or an empty path mean the one on PATH.
        set_git_program(Some("git"));
        assert_eq!(git_binary(), default);
        set_git_program(Some("/x/git"));
        set_git_program(None);
        assert_eq!(git_binary(), default);
    }
}
