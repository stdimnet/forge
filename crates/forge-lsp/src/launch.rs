//! Starting language server processes. [`LocalLauncher`] spawns them on this machine; other
//! launchers start them elsewhere (on a remote development host) and hand back their stdio
//! as streams, so [`LspClient`](crate::LspClient) works the same either way.
//!
//! A server that already runs and listens on a TCP port (the Godot editor's GDScript server
//! on 6005) is reached with the command `tcp://<host>:<port>`: [`LocalLauncher`] connects
//! to it instead of spawning a process, and stopping the client only closes the
//! connection.

use std::collections::BTreeMap;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;

use forge_core::{ForgeError, Result};
use futures_util::future::BoxFuture;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::{Child, Command};

use crate::config::{find_on_path, LspServerConfig};

pub type BoxRead = Pin<Box<dyn AsyncRead + Send>>;
pub type BoxWrite = Pin<Box<dyn AsyncWrite + Send>>;

/// A running server: its stdio and a handle to stop it.
pub struct ServerIo {
    pub stdin: BoxWrite,
    pub stdout: BoxRead,
    /// Diagnostics output, logged at debug level when present.
    pub stderr: Option<BoxRead>,
    pub process: Box<dyn ServerProcess>,
    /// `processId` sent in `initialize`: a process on the server's machine whose exit tells
    /// the server to exit too. `None` for a server on another machine, where Forge's own pid
    /// means nothing (some servers exit when that process does not exist).
    pub parent_pid: Option<u32>,
}

/// The process behind a [`ServerIo`].
pub trait ServerProcess: Send + Sync {
    /// Wait up to `grace` for the process to exit on its own, then kill it.
    fn stop(&mut self, grace: Duration) -> BoxFuture<'_, ()>;
    /// Kill the process right away.
    fn kill(&mut self) -> BoxFuture<'_, ()>;
}

/// Starts language servers for a project root.
pub trait ServerLauncher: Send + Sync {
    fn launch<'a>(
        &'a self,
        config: &'a LspServerConfig,
        root: &'a Path,
    ) -> BoxFuture<'a, Result<ServerIo>>;
}

/// Spawns servers as child processes of Forge.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalLauncher;

struct LocalProcess(Child);

impl ServerProcess for LocalProcess {
    fn stop(&mut self, grace: Duration) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            if tokio::time::timeout(grace, self.0.wait()).await.is_err() {
                let _ = self.0.kill().await;
            }
        })
    }

    fn kill(&mut self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = self.0.kill().await;
        })
    }
}

impl ServerLauncher for LocalLauncher {
    fn launch<'a>(
        &'a self,
        config: &'a LspServerConfig,
        root: &'a Path,
    ) -> BoxFuture<'a, Result<ServerIo>> {
        Box::pin(async move {
            if let Some(address) = tcp_address(&config.command) {
                return connect_tcp(address).await;
            }
            spawn_local_with_env(&config.command, &config.args, root, &config.env)
        })
    }
}

/// The `host:port` of a `tcp://host:port` server command.
pub fn tcp_address(command: &str) -> Option<&str> {
    command
        .trim()
        .strip_prefix("tcp://")
        .map(|a| a.trim_end_matches('/'))
        .filter(|a| !a.is_empty())
}

/// A connection to a server that listens on TCP; the server outlives it. Stopping closes
/// the socket (a handle to the same socket as the client's streams).
struct TcpConnection(Option<std::net::TcpStream>);

impl TcpConnection {
    fn close(&mut self) {
        if let Some(s) = self.0.take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl ServerProcess for TcpConnection {
    fn stop(&mut self, _grace: Duration) -> BoxFuture<'_, ()> {
        self.close();
        Box::pin(async {})
    }

    fn kill(&mut self) -> BoxFuture<'_, ()> {
        self.close();
        Box::pin(async {})
    }
}

/// Connect to a language server that listens on `address` (`127.0.0.1:6005`).
pub async fn connect_tcp(address: &str) -> Result<ServerIo> {
    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(address),
    )
    .await
    .map_err(|_| ForgeError::Other(format!("could not connect to {address}: timed out")))?
    .map_err(|e| ForgeError::Other(format!("could not connect to {address}: {e}")))?;
    let _ = stream.set_nodelay(true);
    let other = |e: std::io::Error| ForgeError::Other(format!("{address}: {e}"));
    let std_stream = stream.into_std().map_err(other)?;
    let handle = std_stream.try_clone().map_err(other)?;
    let stream = tokio::net::TcpStream::from_std(std_stream).map_err(other)?;
    let (read, write) = stream.into_split();
    Ok(ServerIo {
        stdin: Box::pin(write),
        stdout: Box::pin(read),
        stderr: None,
        process: Box::new(TcpConnection(Some(handle))),
        // The server is not Forge's child: it must not exit with Forge.
        parent_pid: None,
    })
}

/// Spawn `command` with piped stdio in `cwd` (looked up on `PATH` like a shell would).
pub fn spawn_local(command: &str, args: &[String], cwd: &Path) -> Result<ServerIo> {
    spawn_local_with_env(command, args, cwd, &BTreeMap::new())
}

/// [`spawn_local`] with extra environment variables.
pub fn spawn_local_with_env(
    command: &str,
    args: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
) -> Result<ServerIo> {
    let program = find_on_path(command).unwrap_or_else(|| command.into());
    let mut child = Command::new(&program)
        .args(args)
        .envs(env)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ForgeError::Other(format!("could not start {command}: {e}")))?;
    let stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    Ok(ServerIo {
        stdin: Box::pin(stdin),
        stdout: Box::pin(stdout),
        stderr: Some(Box::pin(stderr)),
        process: Box::new(LocalProcess(child)),
        parent_pid: Some(std::process::id()),
    })
}
