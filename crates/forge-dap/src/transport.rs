//! How Forge reaches a debug adapter: a child process on stdio, a TCP connection to an
//! adapter that listens (started by Forge or already running, "Attach to DAP server"), or
//! a TCP server Forge opens for an adapter that connects back.

use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncWrite, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, ChildStderr, Command};

use crate::error::{DapError, DapResult};

/// A program Forge starts for the adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterCommand {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<(String, String)>,
}

impl AdapterCommand {
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
            ..Default::default()
        }
    }

    /// The command with `${port}` replaced in its arguments.
    fn with_port(&self, port: u16) -> Self {
        let mut out = self.clone();
        for a in &mut out.args {
            *a = a.replace("${port}", &port.to_string());
        }
        out
    }

    fn spawn(&self, stdio: bool) -> DapResult<Child> {
        let mut std_cmd = std::process::Command::new(&self.command);
        std_cmd.args(&self.args);
        if let Some(cwd) = &self.cwd {
            std_cmd.current_dir(cwd);
        }
        for (k, v) in &self.env {
            std_cmd.env(k, v);
        }
        // In Flatpak the debug adapter runs on the host.
        let mut cmd: Command = forge_hostspawn::host_command(std_cmd).into();
        cmd.stdin(if stdio { Stdio::piped() } else { Stdio::null() })
            .stdout(if stdio { Stdio::piped() } else { Stdio::null() })
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd.spawn().map_err(|e| {
            DapError::Other(if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "Cannot start the debug adapter '{}': it is not installed or not on the PATH",
                    self.command
                )
            } else {
                format!("Cannot start the debug adapter '{}': {e}", self.command)
            })
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Transport {
    /// Start the adapter and speak DAP over its stdin/stdout.
    Stdio { adapter: AdapterCommand },
    /// Connect to `host:port`. With `adapter`, Forge first starts it (`${port}` in its
    /// arguments; port 0 picks a free one) and retries until it listens.
    Connect {
        host: String,
        port: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        adapter: Option<AdapterCommand>,
    },
    /// Listen on a free local port, start the adapter with `${port}`; it connects back.
    Listen { adapter: AdapterCommand },
}

impl Transport {
    pub fn describe(&self) -> String {
        match self {
            Self::Stdio { adapter } => adapter.command.clone(),
            Self::Connect { host, port, .. } => format!("{host}:{port}"),
            Self::Listen { adapter } => format!("{} (connects back)", adapter.command),
        }
    }
}

/// An open connection to an adapter.
pub struct Connection {
    pub reader: Box<dyn AsyncBufRead + Send + Unpin>,
    pub writer: Box<dyn AsyncWrite + Send + Unpin>,
    /// The adapter process Forge started (killed when the session ends).
    pub child: Option<Child>,
    /// The adapter's stderr (shown in the debug console).
    pub stderr: Option<ChildStderr>,
    pub peer: String,
}

fn from_tcp(stream: TcpStream, child: Option<Child>, peer: String) -> Connection {
    let _ = stream.set_nodelay(true);
    let (r, w) = stream.into_split();
    let mut child = child;
    let stderr = child.as_mut().and_then(|c| c.stderr.take());
    Connection {
        reader: Box::new(BufReader::new(r)),
        writer: Box::new(w),
        child,
        stderr,
        peer,
    }
}

/// A free port for an adapter Forge starts with `${port}`: outside the system's ephemeral
/// range and probed without listening (see `forge-ports`).
fn free_port() -> DapResult<u16> {
    Ok(forge_ports::free_port()?)
}

/// Open `transport`, waiting at most `timeout` for TCP adapters to come up.
pub async fn open(transport: &Transport, timeout: Duration) -> DapResult<Connection> {
    match transport {
        Transport::Stdio { adapter } => {
            let mut child = adapter.spawn(true)?;
            let stdout = child.stdout.take().ok_or(DapError::Closed)?;
            let stdin = child.stdin.take().ok_or(DapError::Closed)?;
            let stderr = child.stderr.take();
            Ok(Connection {
                reader: Box::new(BufReader::new(stdout)),
                writer: Box::new(stdin),
                child: Some(child),
                stderr,
                peer: adapter.command.clone(),
            })
        }
        Transport::Connect {
            host,
            port,
            adapter,
        } => {
            let Some(adapter) = adapter else {
                let addr = format!("{host}:{port}");
                let stream = tokio::time::timeout(timeout, TcpStream::connect(&addr))
                    .await
                    .map_err(|_| DapError::Timeout(format!("connect to {addr}")))?
                    .map_err(|e| DapError::Other(format!("Cannot connect to {addr}: {e}")))?;
                return Ok(from_tcp(stream, None, addr));
            };
            let port = if *port == 0 { free_port()? } else { *port };
            let mut child = adapter.with_port(port).spawn(false)?;
            let addr = format!("{host}:{port}");
            let deadline = Instant::now() + timeout;
            loop {
                let connected = TcpStream::connect(&addr).await;
                // An adapter that exited did not accept this connection: another program
                // listens on the port.
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(DapError::Other(format!(
                        "The debug adapter '{}' exited ({status}) before it listened on {addr}",
                        adapter.command
                    )));
                }
                match connected {
                    Ok(stream) => return Ok(from_tcp(stream, Some(child), addr)),
                    Err(e) => {
                        if Instant::now() >= deadline {
                            return Err(DapError::Other(format!(
                                "The debug adapter does not listen on {addr}: {e}"
                            )));
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
        }
        Transport::Listen { adapter } => {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let port = listener.local_addr()?.port();
            let mut child = adapter.with_port(port).spawn(false)?;
            let accepted = tokio::select! {
                r = listener.accept() => Some(r?),
                _ = tokio::time::sleep(timeout) => None,
                status = child.wait() => {
                    return Err(DapError::Other(format!(
                        "The debug adapter '{}' exited ({}) before it connected",
                        adapter.command,
                        status.map(|s| s.to_string()).unwrap_or_default()
                    )));
                }
            };
            let (stream, peer) = accepted.ok_or_else(|| {
                DapError::Timeout(format!("the adapter to connect to port {port}"))
            })?;
            Ok(from_tcp(stream, Some(child), peer.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ports::{ephemeral_ports, PORTS as ADAPTER_PORTS};
    use std::net::SocketAddr;
    use tokio::net::TcpSocket;

    #[test]
    fn port_placeholders_are_replaced() {
        let c = AdapterCommand::new(
            "dlv",
            vec!["dap".into(), "--listen=127.0.0.1:${port}".into()],
        );
        assert_eq!(c.with_port(4711).args[1], "--listen=127.0.0.1:4711");
    }

    #[tokio::test]
    async fn missing_adapters_explain_themselves() {
        let t = Transport::Stdio {
            adapter: AdapterCommand::new("forge-no-such-adapter-xyz", vec![]),
        };
        let err = open(&t, Duration::from_secs(1)).await.err().unwrap();
        assert!(err.to_string().contains("not installed"), "{err}");
    }

    #[tokio::test]
    async fn adapter_ports_avoid_the_ephemeral_range() {
        let port = free_port().unwrap();
        // Unless the system's ephemeral range was moved down over these ports.
        if !matches!(ephemeral_ports(), Some(r) if r.contains(&ADAPTER_PORTS.start)) {
            assert!(ADAPTER_PORTS.contains(&port), "{port}");
        }
    }

    /// A process spawned while Forge probes a port keeps a copy of the probe socket until
    /// it executes its program: the copy must neither accept connections nor keep the
    /// adapter from listening.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_copy_of_the_probe_neither_accepts_nor_blocks_the_adapter() {
        let port = free_port().unwrap();
        let copy = TcpSocket::new_v4().unwrap();
        copy.set_reuseaddr(true).unwrap();
        copy.bind(SocketAddr::from(([127, 0, 0, 1], port))).unwrap();
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
        let adapter = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
        let (accepted, connected) =
            tokio::join!(adapter.accept(), TcpStream::connect(("127.0.0.1", port)));
        assert!(accepted.is_ok() && connected.is_ok());
        drop(copy);
    }

    #[tokio::test]
    async fn connect_reports_refused_ports() {
        let port = free_port().unwrap();
        let t = Transport::Connect {
            host: "127.0.0.1".into(),
            port,
            adapter: None,
        };
        assert!(open(&t, Duration::from_secs(2)).await.is_err());
    }
}
