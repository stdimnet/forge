//! A live connection to the Forge agent on a host: connects (SSH + agent start), measures
//! latency, notices a dead connection and reconnects with backoff. Projects use it as the
//! [`Connection`] of their [`forge_backend::RemoteBackend`], including its SSH tunnels
//! ([`forge_backend::Tunnels`]: databases through `direct-tcpip` channels, Xdebug through a
//! remote listener that is requested again after every reconnect).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use forge_backend::protocol::{BrowseResult, Call, Hello};
use forge_backend::remote::Connection;
use forge_backend::rpc::{connection_lost, RpcClient};
use forge_backend::{PortForward, RemoteHost, Tunnels};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::agent::{start_agent, AgentBinaries};
use crate::session::{ConnectError, ConnectParams, SshSession};

/// How often the latency is measured.
const PING_INTERVAL: Duration = Duration::from_secs(5);
/// A ping slower than this counts as a dead connection.
const PING_TIMEOUT: Duration = Duration::from_secs(15);
/// Waits between reconnect attempts (the last one repeats).
const BACKOFF: &[u64] = &[1, 2, 4, 8, 15, 30];

/// What the status bar shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ConnectionStatus {
    Connecting {
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    Connected {
        latency_ms: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Reconnecting {
        attempt: u32,
        error: String,
        next_retry_secs: u64,
    },
    Disconnected {
        error: Option<String>,
    },
}

type Hook = Arc<dyn Fn() + Send + Sync>;

struct Live {
    session: Arc<SshSession>,
    client: RpcClient,
    hello: Hello,
}

/// A connection to one host's agent.
pub struct RemoteConnection {
    me: Weak<RemoteConnection>,
    host: RemoteHost,
    params: Mutex<ConnectParams>,
    binaries: AgentBinaries,
    live: Mutex<Option<Live>>,
    status: watch::Sender<ConnectionStatus>,
    on_reconnect: Mutex<Vec<Hook>>,
    on_lost: Mutex<Vec<Hook>>,
    closing: AtomicBool,
    /// Remote listeners to request again after a reconnect: id -> (remote, local port).
    remote_forwards: Mutex<HashMap<u64, (u16, u16)>>,
    next_forward: AtomicU64,
}

impl RemoteConnection {
    /// Connect once (errors go straight to the caller, e.g. the fingerprint dialog), then
    /// keep the connection alive in the background.
    pub async fn open(
        host: RemoteHost,
        params: ConnectParams,
        binaries: AgentBinaries,
        progress: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<Arc<Self>, ConnectError> {
        let conn = Arc::new_cyclic(|me| Self {
            me: me.clone(),
            host,
            params: Mutex::new(params),
            binaries,
            live: Mutex::new(None),
            // Updated with `send_replace`: `send` drops the value while nobody subscribed.
            status: watch::channel(ConnectionStatus::Connecting {
                message: "Connecting…".into(),
            })
            .0,
            on_reconnect: Mutex::new(Vec::new()),
            on_lost: Mutex::new(Vec::new()),
            closing: AtomicBool::new(false),
            remote_forwards: Mutex::new(HashMap::new()),
            next_forward: AtomicU64::new(1),
        });
        conn.connect_once(progress).await?;
        // A confirmed fingerprint is in known_hosts now; reconnects must not skip the check.
        conn.params.lock().expect("params lock").accept_fingerprint = None;
        let weak = Arc::downgrade(&conn);
        tokio::spawn(async move { supervise(weak).await });
        Ok(conn)
    }

    async fn connect_once(
        &self,
        progress: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<(), ConnectError> {
        let params = self.params.lock().expect("params lock").clone();
        let _ = self.status.send_replace(ConnectionStatus::Connecting {
            message: format!("Connecting to {}…", self.host.label()),
        });
        progress(&format!("Connecting to {}…", self.host.label()));
        let session = Arc::new(SshSession::connect(&params).await?);
        let forwards: Vec<(u16, u16)> = self
            .remote_forwards
            .lock()
            .expect("forwards lock")
            .values()
            .copied()
            .collect();
        for (remote, local) in forwards {
            if let Err(e) = session.forward_remote(remote, local).await {
                tracing::warn!("port {remote} is not forwarded after reconnecting: {e}");
            }
        }
        let started = start_agent(&session, &self.binaries, &|m: &str| {
            let _ = self.status.send_replace(ConnectionStatus::Connecting {
                message: m.to_string(),
            });
            progress(m);
        })
        .await?;
        *self.live.lock().expect("live lock") = Some(Live {
            session,
            client: started.client,
            hello: started.hello,
        });
        let _ = self
            .status
            .send_replace(ConnectionStatus::Connected { latency_ms: None });
        Ok(())
    }

    pub fn host(&self) -> &RemoteHost {
        &self.host
    }

    pub fn status(&self) -> ConnectionStatus {
        self.status.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<ConnectionStatus> {
        self.status.subscribe()
    }

    /// The agent's hello of the current connection.
    pub fn hello(&self) -> Option<Hello> {
        self.live
            .lock()
            .expect("live lock")
            .as_ref()
            .map(|l| l.hello.clone())
    }

    /// Run `f` (on a blocking thread) after every successful reconnect.
    pub fn on_reconnect(&self, f: impl Fn() + Send + Sync + 'static) {
        self.on_reconnect
            .lock()
            .expect("hooks lock")
            .push(Arc::new(f));
    }

    /// Run `f` (on a blocking thread) when the connection drops.
    pub fn on_lost(&self, f: impl Fn() + Send + Sync + 'static) {
        self.on_lost.lock().expect("hooks lock").push(Arc::new(f));
    }

    /// The SSH session of the current connection.
    pub fn session(&self) -> Option<Arc<SshSession>> {
        self.live
            .lock()
            .expect("live lock")
            .as_ref()
            .map(|l| l.session.clone())
            .filter(|s| !s.is_closed())
    }

    /// Listen on `127.0.0.1:<free port>` here and forward every connection to `host:port`
    /// as the remote host sees it, through the current session (`ssh -L`). Stops when the
    /// returned guard is dropped.
    pub async fn forward_local(&self, host: String, port: u16) -> forge_core::Result<PortForward> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| forge_core::ForgeError::Other(format!("cannot open a local port: {e}")))?;
        let local = listener
            .local_addr()
            .map_err(|e| forge_core::ForgeError::Other(e.to_string()))?
            .port();
        let me = self.me.clone();
        let target = format!("{host}:{port}");
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let Some(session) = me.upgrade().and_then(|c| c.session()) else {
                    // Connection lost: refuse until it is back.
                    continue;
                };
                let host = host.clone();
                tokio::spawn(async move {
                    let _ = socket.set_nodelay(true);
                    match session.open_direct_tcpip(&host, port).await {
                        Ok(mut stream) => {
                            let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                        }
                        Err(e) => tracing::warn!("tunnel to {host}:{port} failed: {e}"),
                    }
                });
            }
        });
        tracing::info!(
            "forwarding 127.0.0.1:{local} to {target} via {}",
            self.host.label()
        );
        Ok(PortForward::new(
            local,
            self.host.label(),
            AbortOnDrop(task),
        ))
    }

    /// Ask the host to listen on `127.0.0.1:remote_port` and forward its connections to
    /// `127.0.0.1:local_port` here (`ssh -R`); requested again after reconnects until the
    /// returned guard is dropped.
    pub async fn forward_remote(
        &self,
        remote_port: u16,
        local_port: u16,
    ) -> forge_core::Result<PortForward> {
        let session = self.session().ok_or_else(connection_lost)?;
        let bound = session.forward_remote(remote_port, local_port).await?;
        let id = self.next_forward.fetch_add(1, Ordering::SeqCst);
        self.remote_forwards
            .lock()
            .expect("forwards lock")
            .insert(id, (bound, local_port));
        let guard = RemoteForwardGuard {
            conn: self.me.clone(),
            id,
            port: bound,
            runtime: tokio::runtime::Handle::current(),
        };
        Ok(PortForward::new(bound, self.host.label(), guard))
    }

    fn current(&self) -> Option<RpcClient> {
        self.live
            .lock()
            .expect("live lock")
            .as_ref()
            .map(|l| l.client.clone())
            .filter(|c| !c.is_closed())
    }

    /// Round trip time to the agent.
    pub async fn ping(&self) -> forge_core::Result<Duration> {
        let client = self.current().ok_or_else(connection_lost)?;
        let start = Instant::now();
        client.call::<()>(Call::Ping, PING_TIMEOUT).await?;
        Ok(start.elapsed())
    }

    /// List an absolute directory on the host (remote folder chooser).
    pub async fn browse(&self, path: &str) -> forge_core::Result<BrowseResult> {
        let client = self.current().ok_or_else(connection_lost)?;
        client
            .call(
                Call::Browse {
                    path: path.to_string(),
                },
                Duration::from_secs(30),
            )
            .await
    }

    /// Drop the connection for good (project closed).
    pub async fn close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        let live = self.live.lock().expect("live lock").take();
        if let Some(l) = live {
            l.client.shutdown();
            l.session.disconnect().await;
        }
        let _ = self
            .status
            .send_replace(ConnectionStatus::Disconnected { error: None });
    }

    /// Simulate a dropped connection (tests and "Reconnect" in the UI).
    pub fn drop_connection(&self) {
        if let Some(c) = self.current() {
            c.shutdown();
        }
    }

    fn run_hooks(hooks: &Mutex<Vec<Hook>>) {
        let hooks: Vec<Hook> = hooks.lock().expect("hooks lock").clone();
        if hooks.is_empty() {
            return;
        }
        tokio::task::spawn_blocking(move || {
            for h in hooks {
                h();
            }
        });
    }
}

impl Connection for RemoteConnection {
    fn client(&self) -> forge_core::Result<RpcClient> {
        self.current().ok_or_else(connection_lost)
    }

    fn tunnels(&self) -> Option<Arc<dyn Tunnels>> {
        Some(Arc::new(SshTunnels {
            conn: self.me.clone(),
        }))
    }
}

/// Aborts a task when dropped.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Stops a remote listener when dropped.
struct RemoteForwardGuard {
    conn: Weak<RemoteConnection>,
    id: u64,
    port: u16,
    runtime: tokio::runtime::Handle,
}

impl Drop for RemoteForwardGuard {
    fn drop(&mut self) {
        let Some(conn) = self.conn.upgrade() else {
            return;
        };
        conn.remote_forwards
            .lock()
            .expect("forwards lock")
            .remove(&self.id);
        if let Some(session) = conn.session() {
            let port = self.port;
            self.runtime
                .spawn(async move { session.cancel_forward(port).await });
        }
    }
}

type ForwardFuture = Pin<Box<dyn Future<Output = forge_core::Result<PortForward>> + Send>>;

/// [`Tunnels`] of a project's connection.
struct SshTunnels {
    conn: Weak<RemoteConnection>,
}

impl Tunnels for SshTunnels {
    fn forward_local(&self, host: String, port: u16) -> ForwardFuture {
        let conn = self.conn.clone();
        Box::pin(async move {
            let conn = conn.upgrade().ok_or_else(connection_lost)?;
            conn.forward_local(host, port).await
        })
    }

    fn forward_remote(&self, remote_port: u16, local_port: u16) -> ForwardFuture {
        let conn = self.conn.clone();
        Box::pin(async move {
            let conn = conn.upgrade().ok_or_else(connection_lost)?;
            conn.forward_remote(remote_port, local_port).await
        })
    }
}

/// Watch the connection: measure latency, reconnect when it drops.
async fn supervise(weak: std::sync::Weak<RemoteConnection>) {
    loop {
        // Connected: ping until the connection dies.
        loop {
            let Some(conn) = weak.upgrade() else { return };
            if conn.closing.load(Ordering::SeqCst) {
                return;
            }
            let Some(client) = conn.current() else { break };
            match conn.ping().await {
                Ok(rtt) => {
                    let _ = conn.status.send_replace(ConnectionStatus::Connected {
                        latency_ms: Some(rtt.as_millis() as u64),
                    });
                }
                Err(_) => {
                    client.shutdown();
                    break;
                }
            }
            drop(conn);
            tokio::select! {
                _ = client.closed() => break,
                _ = tokio::time::sleep(PING_INTERVAL) => {}
            }
        }

        let Some(conn) = weak.upgrade() else { return };
        if conn.closing.load(Ordering::SeqCst) {
            return;
        }
        tracing::info!("connection to {} lost, reconnecting", conn.host.label());
        RemoteConnection::run_hooks(&conn.on_lost);
        drop(conn);

        let mut attempt = 0u32;
        loop {
            let wait = BACKOFF[(attempt as usize).min(BACKOFF.len() - 1)];
            attempt += 1;
            {
                let Some(conn) = weak.upgrade() else { return };
                let error = "Connection lost".to_string();
                let _ = conn.status.send_replace(ConnectionStatus::Reconnecting {
                    attempt,
                    error,
                    next_retry_secs: wait,
                });
            }
            tokio::time::sleep(Duration::from_secs(wait)).await;
            let Some(conn) = weak.upgrade() else { return };
            if conn.closing.load(Ordering::SeqCst) {
                return;
            }
            match conn.connect_once(&|_| {}).await {
                Ok(()) => {
                    tracing::info!("reconnected to {}", conn.host.label());
                    RemoteConnection::run_hooks(&conn.on_reconnect);
                    break;
                }
                Err(e @ (ConnectError::HostKey { .. } | ConnectError::Auth { .. })) => {
                    // Needs the user (changed host key, password changed): stop retrying.
                    let _ = conn.status.send_replace(ConnectionStatus::Disconnected {
                        error: Some(e.to_string()),
                    });
                    return;
                }
                Err(e) => {
                    let _ = conn.status.send_replace(ConnectionStatus::Reconnecting {
                        attempt,
                        error: e.to_string(),
                        next_retry_secs: BACKOFF[(attempt as usize).min(BACKOFF.len() - 1)],
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_for_the_status_bar() {
        let s = ConnectionStatus::Reconnecting {
            attempt: 2,
            error: "Connection lost".into(),
            next_retry_secs: 4,
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["state"], "reconnecting");
        assert_eq!(json["nextRetrySecs"], 4);
        let c = serde_json::to_value(ConnectionStatus::Connected {
            latency_ms: Some(12),
        })
        .unwrap();
        assert_eq!(c["latencyMs"], 12);
    }
}
