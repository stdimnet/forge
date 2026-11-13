//! Language server installs on the host (`Call::LangServerInstall`, `Call::LangServerUpdates`)
//! driven through `RemoteBackend` over an in-memory pipe. Downloads go to a local fake
//! server; the install folders are redirected to a temporary data directory (this test
//! binary runs in its own process, so changing the environment is safe here).

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use forge_backend::rpc::RpcClient;
use forge_backend::{Backend, InstallRequest, InstallTarget, RemoteBackend, RemoteHost};
use forge_langservers::install::InstallOutcome;
use forge_langservers::managed::Event;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static DATA: OnceLock<tempfile::TempDir> = OnceLock::new();

/// Keep the installs out of the real data directory; returns the data directory in use.
fn isolate() -> &'static std::path::Path {
    DATA.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", dir.path());
        for k in ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
            std::env::remove_var(k);
        }
        dir
    })
    .path()
}

/// A local HTTP server: `/missing/…` answers 404, `/hang/…` never answers.
async fn fake_registry() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                if head.contains(" /hang") {
                    tokio::time::sleep(Duration::from_secs(120)).await;
                    return;
                }
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            });
        }
    });
    port
}

fn remote(dir: &tempfile::TempDir) -> Arc<dyn Backend> {
    let (client_end, agent_end) = tokio::io::duplex(1 << 16);
    let (agent_read, agent_write) = tokio::io::split(agent_end);
    tokio::spawn(forge_remote_agent::serve(agent_read, agent_write));
    let (read, write) = tokio::io::split(client_end);
    let client = RpcClient::spawn(read, write);
    let host = RemoteHost {
        host: "shop.example.com".into(),
        port: 22,
        user: "deploy".into(),
        config_id: None,
        kind: Default::default(),
    };
    Arc::new(RemoteBackend::open(Arc::new(client), host, &dir.path().to_string_lossy()).unwrap())
}

fn request(base: &str) -> InstallRequest {
    InstallRequest {
        target: InstallTarget::Package {
            spec: "forge-test-server".into(),
        },
        registry: Some(base.into()),
        node_mirror: Some(base.into()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn installs_report_progress_and_errors_from_the_host() {
    isolate();
    let port = fake_registry().await;
    let dir = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::<Event>::new()));
    let seen = events.clone();
    let result = tokio::task::spawn_blocking(move || {
        let b = remote(&dir);
        let r = b.install_langserver(
            request(&format!("http://127.0.0.1:{port}/missing")),
            Arc::new(tokio::sync::Notify::new()),
            Arc::new(move |e| seen.lock().unwrap().push(e)),
        );
        // Asking for updates needs no project, and nothing was installed yet.
        let updates = b
            .langserver_updates(Some(format!("http://127.0.0.1:{port}/missing")), None, true)
            .unwrap();
        (r, updates)
    })
    .await
    .unwrap();
    let (r, updates) = result;
    let err = r.unwrap_err().to_string();
    assert!(err.contains("/missing"), "{err}");
    let events = events.lock().unwrap();
    // "Using Node.js at …" or "No Node.js 18 or newer found …" streamed from the host.
    assert!(
        events.iter().any(|e| matches!(e, Event::Step { .. })),
        "{events:?}"
    );
    assert_eq!(updates, Some(Vec::new()));
    // The host's Forge folder (here: the redirected data directory) was used.
    assert!(isolate().join("forge").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_stops_the_install_on_the_host() {
    isolate();
    let port = fake_registry().await;
    let dir = tempfile::tempdir().unwrap();
    let cancel = Arc::new(tokio::sync::Notify::new());
    let trigger = cancel.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let b = remote(&dir);
        // The first step means the agent is waiting for the (hanging) download.
        let sink = Arc::new(move |e: Event| {
            if matches!(e, Event::Step { .. }) {
                trigger.notify_one();
            }
        });
        b.install_langserver(
            request(&format!("http://127.0.0.1:{port}/hang")),
            cancel,
            sink,
        )
    })
    .await
    .unwrap();
    assert_eq!(outcome.unwrap(), InstallOutcome::Cancelled);
}
