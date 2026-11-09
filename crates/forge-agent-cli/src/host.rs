//! Where an agent CLI runs: this machine ([`LocalHost`]) or, for a project on a remote host,
//! that host (the app implements [`AgentHost`] over the project backend's process API, so
//! the CLI sees the project's files and its own login there).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;

use forge_core::{ForgeError, Result};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::Command;
use tokio::sync::oneshot;

use crate::cli::{AgentCli, CommandSpec};

/// A started CLI process.
pub struct SpawnedAgent {
    pub stdin: Box<dyn AsyncWrite + Send + Unpin>,
    pub stdout: Box<dyn AsyncRead + Send + Unpin>,
    pub stderr: Box<dyn AsyncRead + Send + Unpin>,
    /// Resolves when the process ended: its exit code, `None` when a signal ended it.
    pub exit: Pin<Box<dyn Future<Output = Option<i32>> + Send>>,
    /// Stop the process; `exit` resolves afterwards.
    pub kill: Box<dyn FnOnce() + Send>,
}

pub type SpawnFuture = Pin<Box<dyn Future<Output = Result<SpawnedAgent>> + Send>>;

/// Runs agent CLIs.
pub trait AgentHost: Send + Sync {
    /// The adapter's executable: `override_path` when it exists, else a search of `PATH`
    /// and the usual install locations. May block (call off the async runtime).
    fn detect(&self, adapter: &dyn AgentCli, override_path: Option<&str>) -> Option<PathBuf>;

    /// Start `spec` in `cwd` with piped stdio.
    fn spawn(&self, spec: CommandSpec, cwd: PathBuf) -> SpawnFuture;

    /// `user@host` for a remote host, `None` for this machine.
    fn remote_label(&self) -> Option<String> {
        None
    }
}

/// This machine.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalHost;

impl AgentHost for LocalHost {
    fn detect(&self, adapter: &dyn AgentCli, override_path: Option<&str>) -> Option<PathBuf> {
        adapter.detect(override_path)
    }

    fn spawn(&self, spec: CommandSpec, cwd: PathBuf) -> SpawnFuture {
        Box::pin(async move { spawn_local(&spec, &cwd) })
    }
}

fn spawn_local(spec: &CommandSpec, cwd: &Path) -> Result<SpawnedAgent> {
    let mut std_cmd = std::process::Command::new(&spec.program);
    std_cmd.args(&spec.args).current_dir(cwd);
    // In Flatpak the agent CLI runs on the host.
    let mut cmd: Command = forge_hostspawn::host_command(std_cmd).into();
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| ForgeError::io(&spec.program, e))?;
    let stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (exit_tx, exit_rx) = oneshot::channel::<Option<i32>>();
    let (kill_tx, kill_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let code = tokio::select! {
            status = child.wait() => status.ok().and_then(|s| s.code()),
            _ = kill_rx => {
                let _ = child.kill().await;
                None
            }
        };
        let _ = exit_tx.send(code);
    });
    Ok(SpawnedAgent {
        stdin: Box::new(stdin),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        exit: Box::pin(async move { exit_rx.await.ok().flatten() }),
        kill: Box::new(move || {
            let _ = kill_tx.send(());
        }),
    })
}
