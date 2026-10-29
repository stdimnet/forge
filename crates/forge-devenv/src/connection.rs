//! A live connection to an agent started over a process's stdio (a dev container or a WSL
//! distribution). It is the [`Connection`] of the project's
//! [`forge_backend::RemoteBackend`]; when the process ends (container stopped, distribution
//! shut down) it starts the agent again with backoff and reattaches the project, like an
//! SSH connection.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use forge_backend::protocol::{BrowseResult, Call, Hello};
use forge_backend::remote::Connection;
use forge_backend::rpc::{connection_lost, RpcClient};
use forge_remote::connection::ConnectionStatus;
use tokio::sync::watch;

use crate::agent::StartedAgent;
use crate::runner::BoxFuture;

/// Waits between restart attempts (the last one repeats).
const BACKOFF: &[u64] = &[1, 2, 4, 8, 15, 30];

/// Starts the agent (again).
pub type Starter = Arc<dyn Fn() -> BoxFuture<'static, Result<StartedAgent, String>> + Send + Sync>;

type Hook = Arc<dyn Fn() + Send + Sync>;

struct Live {
    client: RpcClient,
    hello: Hello,
    _process: Box<dyn Send + Sync>,
}

/// A connection to one agent process.
pub struct StdioConnection {
    label: String,
    starter: Starter,
    live: Mutex<Option<Live>>,
    status: watch::Sender<ConnectionStatus>,
    on_reconnect: Mutex<Vec<Hook>>,
    on_lost: Mutex<Vec<Hook>>,
    closing: AtomicBool,
}

impl StdioConnection {
    /// Start the agent once (errors go to the caller), then watch it in the background.
    pub async fn open(label: impl Into<String>, starter: Starter) -> Result<Arc<Self>, String> {
        let conn = Arc::new(Self {
            label: label.into(),
            starter,
            live: Mutex::new(None),
            status: watch::channel(ConnectionStatus::Connecting {
                message: "Starting the Forge agent…".into(),
            })
            .0,
            on_reconnect: Mutex::new(Vec::new()),
            on_lost: Mutex::new(Vec::new()),
            closing: AtomicBool::new(false),
        });
        conn.start_once().await?;
        let weak = Arc::downgrade(&conn);
        tokio::spawn(async move { supervise(weak).await });
        Ok(conn)
    }

    async fn start_once(&self) -> Result<(), String> {
        let started = (self.starter)().await?;
        *self.live.lock().expect("live lock") = Some(Live {
            client: started.client,
            hello: started.hello,
            _process: started.process,
        });
        let _ = self
            .status
            .send_replace(ConnectionStatus::Connected { latency_ms: None });
        Ok(())
    }

    /// "Dev Container: shop", "WSL: Ubuntu".
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn status(&self) -> ConnectionStatus {
        self.status.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<ConnectionStatus> {
        self.status.subscribe()
    }

    pub fn hello(&self) -> Option<Hello> {
        self.live
            .lock()
            .expect("live lock")
            .as_ref()
            .map(|l| l.hello.clone())
    }

    fn current(&self) -> Option<RpcClient> {
        self.live
            .lock()
            .expect("live lock")
            .as_ref()
            .map(|l| l.client.clone())
            .filter(|c| !c.is_closed())
    }

    /// Run `f` (on a blocking thread) after every successful restart.
    pub fn on_reconnect(&self, f: impl Fn() + Send + Sync + 'static) {
        self.on_reconnect
            .lock()
            .expect("hooks lock")
            .push(Arc::new(f));
    }

    /// Run `f` (on a blocking thread) when the agent goes away.
    pub fn on_lost(&self, f: impl Fn() + Send + Sync + 'static) {
        self.on_lost.lock().expect("hooks lock").push(Arc::new(f));
    }

    /// List an absolute directory (folder chooser).
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

    /// End the agent for good (project closed).
    pub async fn close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        let live = self.live.lock().expect("live lock").take();
        if let Some(l) = live {
            l.client.shutdown();
        }
        let _ = self
            .status
            .send_replace(ConnectionStatus::Disconnected { error: None });
    }

    /// Drop the agent so it is started again ("Reconnect").
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

impl Connection for StdioConnection {
    fn client(&self) -> forge_core::Result<RpcClient> {
        self.current().ok_or_else(connection_lost)
    }
}

async fn supervise(weak: Weak<StdioConnection>) {
    loop {
        let client = {
            let Some(conn) = weak.upgrade() else { return };
            conn.current()
        };
        if let Some(client) = client {
            client.closed().await;
        }
        let Some(conn) = weak.upgrade() else { return };
        if conn.closing.load(Ordering::SeqCst) {
            return;
        }
        tracing::info!("the Forge agent in {} ended, restarting", conn.label);
        // The old process goes away with its client.
        conn.live.lock().expect("live lock").take();
        StdioConnection::run_hooks(&conn.on_lost);
        drop(conn);

        let mut attempt = 0u32;
        let mut error = "The Forge agent ended".to_string();
        loop {
            let wait = BACKOFF[(attempt as usize).min(BACKOFF.len() - 1)];
            attempt += 1;
            {
                let Some(conn) = weak.upgrade() else { return };
                let _ = conn.status.send_replace(ConnectionStatus::Reconnecting {
                    attempt,
                    error: error.clone(),
                    next_retry_secs: wait,
                });
            }
            tokio::time::sleep(Duration::from_secs(wait)).await;
            let Some(conn) = weak.upgrade() else { return };
            if conn.closing.load(Ordering::SeqCst) {
                return;
            }
            match conn.start_once().await {
                Ok(()) => {
                    tracing::info!("the Forge agent in {} runs again", conn.label);
                    StdioConnection::run_hooks(&conn.on_reconnect);
                    break;
                }
                Err(e) => error = e,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::start_agent;
    use crate::agent::tests::{binaries, docker_target, FakeRunner, PROBE_EMPTY};
    use std::sync::atomic::AtomicUsize;

    #[tokio::test(flavor = "multi_thread")]
    async fn serves_a_remote_backend_and_restarts_a_dropped_agent() {
        let runner = FakeRunner::new(PROBE_EMPTY);
        let (dir, bins) = binaries();
        let starts = Arc::new(AtomicUsize::new(0));
        let starter: Starter = {
            let (runner, bins, starts) = (runner.clone(), bins.clone(), starts.clone());
            Arc::new(move || {
                let (runner, bins, starts) = (runner.clone(), bins.clone(), starts.clone());
                Box::pin(async move {
                    starts.fetch_add(1, Ordering::SeqCst);
                    start_agent(runner.as_ref(), &docker_target(), &bins, &|_| {}).await
                })
            })
        };
        let conn = StdioConnection::open("Dev Container: Shop", starter)
            .await
            .unwrap();
        assert!(matches!(conn.status(), ConnectionStatus::Connected { .. }));
        assert!(conn.hello().is_some());

        // A project in the "container" (a temporary folder here).
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("index.php"), "<?php echo 1;\n").unwrap();
        let root = project.path().to_string_lossy().into_owned();
        let host = forge_backend::RemoteHost {
            host: "Shop".into(),
            port: 0,
            user: "vscode".into(),
            config_id: None,
            kind: forge_backend::RemoteKind::DevContainer {
                container: "c0ffee".into(),
                source: "/src/shop".into(),
                config: "/src/shop/.devcontainer/devcontainer.json".into(),
            },
        };
        let source: Arc<dyn Connection> = conn.clone();
        let backend = {
            let root = root.clone();
            tokio::task::spawn_blocking(move || {
                forge_backend::RemoteBackend::open(source, host, &root)
            })
            .await
            .unwrap()
            .unwrap()
        };
        let reattached = Arc::new(AtomicUsize::new(0));
        {
            let (b, n) = (Arc::new(backend), reattached.clone());
            let b2 = b.clone();
            conn.on_lost(move || b2.connection_lost());
            conn.on_reconnect(move || {
                b.reattach().unwrap();
                n.fetch_add(1, Ordering::SeqCst);
                let text = forge_backend::Backend::read_file(b.as_ref(), "index.php").unwrap();
                assert!(text.contains("echo 1"));
            });
        }

        conn.drop_connection();
        let start = std::time::Instant::now();
        while reattached.load(Ordering::SeqCst) == 0 {
            assert!(start.elapsed() < Duration::from_secs(20), "no restart");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(starts.load(Ordering::SeqCst), 2);
        assert!(matches!(conn.status(), ConnectionStatus::Connected { .. }));
        let listing = conn.browse(&root).await.unwrap();
        assert!(listing.entries.iter().any(|e| e.name == "index.php"));

        conn.close().await;
        assert!(matches!(
            conn.status(),
            ConnectionStatus::Disconnected { .. }
        ));
        drop(dir);
    }
}
