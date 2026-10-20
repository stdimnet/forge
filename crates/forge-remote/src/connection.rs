//! A live connection to the Forge agent on a host: connects (SSH + agent start), measures
//! latency, notices a dead connection and reconnects with backoff. Projects use it as the
//! [`Connection`] of their [`forge_backend::RemoteBackend`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::protocol::{BrowseResult, Call, Hello};
use forge_backend::remote::Connection;
use forge_backend::rpc::{connection_lost, RpcClient};
use forge_backend::RemoteHost;
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
    host: RemoteHost,
    params: Mutex<ConnectParams>,
    binaries: AgentBinaries,
    live: Mutex<Option<Live>>,
    status: watch::Sender<ConnectionStatus>,
    on_reconnect: Mutex<Vec<Hook>>,
    on_lost: Mutex<Vec<Hook>>,
    closing: AtomicBool,
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
        let conn = Arc::new(Self {
            host,
            params: Mutex::new(params),
            binaries,
            live: Mutex::new(None),
            status: watch::channel(ConnectionStatus::Connecting {
                message: "Connecting…".into(),
            })
            .0,
            on_reconnect: Mutex::new(Vec::new()),
            on_lost: Mutex::new(Vec::new()),
            closing: AtomicBool::new(false),
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
        let _ = self.status.send(ConnectionStatus::Connecting {
            message: format!("Connecting to {}…", self.host.label()),
        });
        progress(&format!("Connecting to {}…", self.host.label()));
        let session = Arc::new(SshSession::connect(&params).await?);
        let started = start_agent(&session, &self.binaries, &|m: &str| {
            let _ = self.status.send(ConnectionStatus::Connecting {
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
            .send(ConnectionStatus::Connected { latency_ms: None });
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
            .send(ConnectionStatus::Disconnected { error: None });
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
                    let _ = conn.status.send(ConnectionStatus::Connected {
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
                let _ = conn.status.send(ConnectionStatus::Reconnecting {
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
                    let _ = conn.status.send(ConnectionStatus::Disconnected {
                        error: Some(e.to_string()),
                    });
                    return;
                }
                Err(e) => {
                    let _ = conn.status.send(ConnectionStatus::Reconnecting {
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
