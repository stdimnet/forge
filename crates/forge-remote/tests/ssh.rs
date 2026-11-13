//! End to end over real SSH: an in-process russh server on 127.0.0.1 plays the remote host
//! (it answers the probe, stores the uploaded agent and runs the agent in-process for the
//! start command). Covers the fingerprint confirmation, password auth, the agent upload,
//! a remote project, reconnecting after the server dropped the connection, port forwarding
//! both ways (databases, Xdebug) and jump hosts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::{Backend, RemoteBackend, RemoteHost};
use forge_remote::agent::{version_line, AgentBinaries, AGENT_VERSION};
use forge_remote::connection::{ConnectionStatus, RemoteConnection};
use forge_remote::deploy;
use forge_remote::session::{ConnectError, ConnectParams};
use forge_remote::{AuthMethod, HostKeyStatus};
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId, ChannelMsg};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

#[derive(Default)]
struct HostState {
    /// SHA-256 of the uploaded agent, once uploaded.
    installed: Option<String>,
    uploads: usize,
    starts: usize,
    handles: Vec<server::Handle>,
    /// `direct-tcpip` channels opened (tunnels, jump connections).
    direct: Vec<String>,
    /// Remote listeners (`tcpip-forward`) by port.
    listeners: HashMap<u32, tokio::task::JoinHandle<()>>,
}

#[derive(Clone)]
struct FakeHost {
    state: Arc<Mutex<HostState>>,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl server::Handler for FakeHost {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "deploy" && password == "secret" {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.state.lock().unwrap().handles.push(session.handle());
        self.channels.lock().unwrap().insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        id: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).into_owned();
        let Some(channel) = self.channels.lock().unwrap().remove(&id) else {
            return Ok(());
        };
        session.channel_success(id)?;
        let state = self.state.clone();
        tokio::spawn(async move { run_command(command, channel, state).await });
        Ok(())
    }

    /// `ssh -L` / `ssh -J`: connect to the target from "the host" and pipe.
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let target = format!("{host_to_connect}:{port_to_connect}");
        self.state.lock().unwrap().direct.push(target.clone());
        match tokio::net::TcpStream::connect(&target).await {
            Ok(mut socket) => {
                reply.accept().await;
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                });
            }
            Err(_) => reply.reject(russh::ChannelOpenFailure::ConnectFailed).await,
        }
        Ok(())
    }

    /// `ssh -R`: listen on the "host" and open a forwarded channel per connection.
    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let Ok(listener) = tokio::net::TcpListener::bind((address, *port as u16)).await else {
            return Ok(false);
        };
        let bound = listener.local_addr().unwrap().port() as u32;
        *port = bound;
        let handle = session.handle();
        let address = address.to_string();
        let task = tokio::spawn(async move {
            while let Ok((mut socket, peer)) = listener.accept().await {
                let Ok(channel) = handle
                    .channel_open_forwarded_tcpip(
                        address.clone(),
                        bound,
                        peer.ip().to_string(),
                        peer.port() as u32,
                    )
                    .await
                else {
                    continue;
                };
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                });
            }
        });
        self.state.lock().unwrap().listeners.insert(bound, task);
        Ok(true)
    }

    async fn cancel_tcpip_forward(
        &mut self,
        _address: &str,
        port: u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        if let Some(task) = self.state.lock().unwrap().listeners.remove(&port) {
            task.abort();
        }
        Ok(true)
    }
}

async fn run_command(command: String, channel: Channel<Msg>, state: Arc<Mutex<HostState>>) {
    if command == deploy::probe_command(AGENT_VERSION) {
        let mut out = "Welcome to the test host\nFORGE_OS=Linux\nFORGE_ARCH=x86_64\nFORGE_HOME=/home/deploy\n".to_string();
        if let Some(sha) = state.lock().unwrap().installed.clone() {
            out.push_str(&format!(
                "FORGE_AGENT={}\nFORGE_SHA={sha}\n",
                version_line()
            ));
        }
        let _ = channel.data_bytes(out.into_bytes()).await;
        let _ = channel.exit_status(0).await;
        let _ = channel.eof().await;
        let _ = channel.close().await;
    } else if command == deploy::upload_command(AGENT_VERSION) {
        let (mut read, write) = channel.split();
        let mut bytes = Vec::new();
        while let Some(msg) = read.wait().await {
            match msg {
                ChannelMsg::Data { data } => bytes.extend_from_slice(&data),
                ChannelMsg::Eof => break,
                _ => {}
            }
        }
        {
            let mut s = state.lock().unwrap();
            s.installed = Some(sha_hex(&bytes));
            s.uploads += 1;
        }
        let _ = write.exit_status(0).await;
        let _ = write.eof().await;
        let _ = write.close().await;
    } else if command == deploy::start_command(AGENT_VERSION) {
        state.lock().unwrap().starts += 1;
        let stream = channel.into_stream();
        let (read, mut write) = tokio::io::split(stream);
        write
            .write_all(b"Last login: never\nFORGE-AGENT-READY\n")
            .await
            .unwrap();
        let _ = forge_remote_agent::serve(read, write).await;
    } else {
        let _ = channel
            .extended_data_bytes(1, format!("unknown command: {command}\n").into_bytes())
            .await;
        let _ = channel.exit_status(127).await;
        let _ = channel.close().await;
    }
}

struct TestServer {
    port: u16,
    state: Arc<Mutex<HostState>>,
}

async fn start_server() -> TestServer {
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[7u8; 32]));
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let state: Arc<Mutex<HostState>> = Arc::default();
    let host = FakeHost {
        state: state.clone(),
        channels: Arc::default(),
    };
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let config = config.clone();
            let handler = FakeHost {
                state: host.state.clone(),
                channels: Arc::default(),
            };
            tokio::spawn(async move {
                if let Ok(running) = server::run_stream(config, socket, handler).await {
                    let _ = running.await;
                }
            });
        }
    });
    TestServer { port, state }
}

struct Fixture {
    _dir: tempfile::TempDir,
    known_hosts: PathBuf,
    binaries: AgentBinaries,
    agent_bytes: Vec<u8>,
    project: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let bins = dir.path().join("agents");
    std::fs::create_dir_all(&bins).unwrap();
    let agent_bytes = b"\x7fELF fake agent build".to_vec();
    std::fs::write(bins.join(deploy::bundled_name("x86_64")), &agent_bytes).unwrap();
    let project = dir.path().join("www/shop");
    std::fs::create_dir_all(project.join("app")).unwrap();
    std::fs::write(project.join("composer.json"), "{}").unwrap();
    std::fs::write(project.join("app/Order.php"), "<?php class Order {}").unwrap();
    Fixture {
        known_hosts: dir.path().join("ssh/known_hosts"),
        binaries: AgentBinaries::new(vec![bins]),
        agent_bytes,
        project: project.canonicalize().unwrap(),
        _dir: dir,
    }
}

fn params(f: &Fixture, port: u16, password: &str, accept: Option<String>) -> ConnectParams {
    ConnectParams {
        host: "127.0.0.1".into(),
        port,
        user: "deploy".into(),
        auth: AuthMethod::Password,
        secret: Some(password.into()),
        known_hosts: f.known_hosts.clone(),
        accept_fingerprint: accept,
        timeout: Duration::from_secs(10),
        ..ConnectParams::default()
    }
}

fn host(port: u16) -> RemoteHost {
    RemoteHost {
        host: "127.0.0.1".into(),
        port,
        user: "deploy".into(),
        config_id: None,
        kind: Default::default(),
    }
}

async fn open(
    f: &Fixture,
    server: &TestServer,
    password: &str,
    accept: Option<String>,
) -> Result<Arc<RemoteConnection>, ConnectError> {
    RemoteConnection::open(
        host(server.port),
        params(f, server.port, password, accept),
        f.binaries.clone(),
        &|_| {},
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connect_confirm_fingerprint_upload_and_open_a_project() {
    let server = start_server().await;
    let f = fixture();

    // First contact: the host key is unknown and must be confirmed.
    let fingerprint = match open(&f, &server, "secret", None).await {
        Err(ConnectError::HostKey { host_key }) => {
            assert_eq!(host_key.status, HostKeyStatus::Unknown);
            assert_eq!(host_key.key_type, "ssh-ed25519");
            assert!(host_key.fingerprint.starts_with("SHA256:"));
            host_key.fingerprint
        }
        Err(e) => panic!("expected a host key question, got {e}"),
        Ok(_) => panic!("an unknown host key must not be accepted silently"),
    };
    assert!(!f.known_hosts.exists());

    let conn = open(&f, &server, "secret", Some(fingerprint.clone()))
        .await
        .unwrap();
    let known = std::fs::read_to_string(&f.known_hosts).unwrap();
    assert!(
        known.starts_with(&format!("[127.0.0.1]:{} ssh-ed25519 ", server.port)),
        "{known}"
    );
    {
        let s = server.state.lock().unwrap();
        assert_eq!(s.uploads, 1);
        assert_eq!(
            s.installed.as_deref(),
            Some(sha_hex(&f.agent_bytes).as_str())
        );
    }
    assert_eq!(conn.hello().unwrap().version, AGENT_VERSION);
    assert!(conn.ping().await.unwrap() < Duration::from_secs(5));

    // Remote folder chooser.
    let parent = f.project.parent().unwrap().to_string_lossy().into_owned();
    let listing = conn.browse(&parent).await.unwrap();
    assert!(listing
        .entries
        .iter()
        .any(|e| e.name == "shop" && e.project));

    // The project through the SSH connection.
    let root = f.project.to_string_lossy().into_owned();
    let c2 = conn.clone();
    let backend = tokio::task::spawn_blocking(move || {
        RemoteBackend::open(c2, host(server.port), &root).unwrap()
    })
    .await
    .unwrap();
    let names = tokio::task::spawn_blocking(move || {
        backend
            .list_dir("")
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect::<Vec<_>>()
    })
    .await
    .unwrap();
    assert_eq!(names, vec!["app", "composer.json"]);
    conn.close().await;

    // Second connection: known host, agent already installed.
    let again = open(&f, &server, "secret", None).await.unwrap();
    assert_eq!(server.state.lock().unwrap().uploads, 1);
    again.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_passwords_and_changed_host_keys_are_reported() {
    let server = start_server().await;
    let f = fixture();
    // Pretend we knew another key for this host.
    std::fs::create_dir_all(f.known_hosts.parent().unwrap()).unwrap();
    std::fs::write(
        &f.known_hosts,
        format!(
            "[127.0.0.1]:{} ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\n",
            server.port
        ),
    )
    .unwrap();
    let fingerprint = match open(&f, &server, "secret", None).await {
        Err(ConnectError::HostKey { host_key }) => {
            assert!(matches!(
                host_key.status,
                HostKeyStatus::Changed { line: 0, .. }
            ));
            host_key.fingerprint
        }
        other => panic!("expected a changed host key, got {:?}", other.err()),
    };
    // Accepting the new key replaces the old line.
    match open(&f, &server, "wrong", Some(fingerprint)).await {
        Err(ConnectError::Auth { message }) => assert!(message.contains("deploy@127.0.0.1")),
        other => panic!("expected an auth error, got {:?}", other.err()),
    }
    let known = std::fs::read_to_string(&f.known_hosts).unwrap();
    assert_eq!(known.lines().count(), 1);
    assert!(!known.contains("AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl"));
    // The new key is trusted now.
    let conn = open(&f, &server, "secret", None).await.unwrap();
    conn.close().await;
}

/// A TCP server on this machine answering every line with `<name>: <line>`.
async fn echo_server(name: &'static str) -> u16 {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (read, mut write) = socket.into_split();
                let mut lines = BufReader::new(read).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if write
                        .write_all(format!("{name}: {line}\n").as_bytes())
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
    });
    port
}

async fn ask(port: u16, line: &str) -> String {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    socket
        .write_all(format!("{line}\n").as_bytes())
        .await
        .unwrap();
    let mut answer = String::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        BufReader::new(socket).read_line(&mut answer),
    )
    .await
    .unwrap()
    .unwrap();
    answer.trim_end().to_string()
}

async fn trusted(f: &Fixture, server: &TestServer) -> Arc<RemoteConnection> {
    let fp = match open(f, server, "secret", None).await {
        Err(ConnectError::HostKey { host_key }) => host_key.fingerprint,
        Ok(conn) => return conn,
        Err(e) => panic!("{e}"),
    };
    open(f, server, "secret", Some(fp)).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_and_remote_port_forwarding() {
    let server = start_server().await;
    let f = fixture();
    let conn = trusted(&f, &server).await;

    // A database "on the host" (127.0.0.1 as the host sees it), reached through `ssh -L`.
    let db = echo_server("mysql").await;
    let forward = conn.forward_local("127.0.0.1".into(), db).await.unwrap();
    assert_ne!(forward.port, db);
    assert_eq!(forward.via, format!("deploy@127.0.0.1:{}", server.port));
    assert_eq!(ask(forward.port, "select 1").await, "mysql: select 1");
    assert_eq!(ask(forward.port, "again").await, "mysql: again");
    assert!(server
        .state
        .lock()
        .unwrap()
        .direct
        .contains(&format!("127.0.0.1:{db}")));
    drop(forward);

    // Xdebug on the host connects to its port 9003; the IDE listens here (`ssh -R`).
    let ide = echo_server("dbgp").await;
    let remote = conn.forward_remote(0, ide).await.unwrap();
    assert!(remote.port > 0);
    assert_eq!(ask(remote.port, "init").await, "dbgp: init");
    // Dropping the forward closes the remote listener.
    let port = remote.port;
    drop(remote);
    let start = Instant::now();
    while server
        .state
        .lock()
        .unwrap()
        .listeners
        .contains_key(&(port as u32))
    {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "listener not cancelled"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // The backend hands the same tunnels to the app's database and debugger code.
    let tunnels = forge_backend::remote::Connection::tunnels(conn.as_ref()).unwrap();
    let again = tunnels.forward_local("127.0.0.1".into(), db).await.unwrap();
    assert_eq!(ask(again.port, "x").await, "mysql: x");

    // A single stream to a port of the host (`kubectl port-forward` listening there).
    {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut stream = tunnels.open_tcp("127.0.0.1".into(), db).await.unwrap();
        stream.write_all(b"kubectl\n").await.unwrap();
        let mut answer = String::new();
        tokio::time::timeout(
            Duration::from_secs(10),
            BufReader::new(stream).read_line(&mut answer),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(answer.trim_end(), "mysql: kubectl");
    }
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connects_through_a_jump_host() {
    let target = start_server().await;
    let bastion = start_server().await;
    let f = fixture();
    let mut p = params(&f, target.port, "secret", None);
    p.route = forge_remote::session::Route::Jump(vec![params(&f, bastion.port, "secret", None)]);
    let host_key = |r: Result<Arc<RemoteConnection>, ConnectError>| match r {
        Err(ConnectError::HostKey { host_key }) => host_key,
        Err(e) => panic!("expected a host key question, got {e}"),
        Ok(_) => panic!("expected a host key question"),
    };
    let open_via = |p: ConnectParams| {
        let binaries = f.binaries.clone();
        async move { RemoteConnection::open(host(target.port), p, binaries, &|_| {}).await }
    };
    // The jump host is asked about first, then the target (same test key, other port).
    let first = host_key(open_via(p.clone()).await);
    assert_eq!(first.port, bastion.port);
    p.accept_fingerprint = Some(first.fingerprint.clone());
    let conn = open_via(p.clone()).await.unwrap();
    assert_eq!(conn.hello().unwrap().version, AGENT_VERSION);
    let known = std::fs::read_to_string(&f.known_hosts).unwrap();
    assert!(
        known.contains(&format!("[127.0.0.1]:{}", bastion.port)),
        "{known}"
    );
    assert!(
        known.contains(&format!("[127.0.0.1]:{}", target.port)),
        "{known}"
    );
    // The target was reached from the bastion, which started no agent itself.
    assert_eq!(
        bastion.state.lock().unwrap().direct,
        vec![format!("127.0.0.1:{}", target.port)]
    );
    assert_eq!(bastion.state.lock().unwrap().starts, 0);
    assert_eq!(target.state.lock().unwrap().starts, 1);

    let root = f.project.to_string_lossy().into_owned();
    let c2 = conn.clone();
    let port = target.port;
    let content = tokio::task::spawn_blocking(move || {
        RemoteBackend::open(c2, host(port), &root)
            .unwrap()
            .read_file("app/Order.php")
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(content, "<?php class Order {}");
    conn.close().await;

    // A jump host that cannot be reached is named in the error.
    let mut broken = params(&f, target.port, "secret", None);
    broken.route = forge_remote::session::Route::Jump(vec![params(&f, 1, "secret", None)]);
    match open_via(broken).await {
        Err(ConnectError::Network { message }) => {
            assert!(
                message.starts_with("Jump host deploy@127.0.0.1:1"),
                "{message}"
            )
        }
        other => panic!("expected a network error, got {:?}", other.err()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconnects_after_the_connection_dropped() {
    let server = start_server().await;
    let f = fixture();
    let fp = match open(&f, &server, "secret", None).await {
        Err(ConnectError::HostKey { host_key }) => host_key.fingerprint,
        _ => panic!("expected a host key question"),
    };
    let conn = open(&f, &server, "secret", Some(fp)).await.unwrap();
    let root = f.project.to_string_lossy().into_owned();
    let c2 = conn.clone();
    let port = server.port;
    let backend = Arc::new(
        tokio::task::spawn_blocking(move || RemoteBackend::open(c2, host(port), &root).unwrap())
            .await
            .unwrap(),
    );
    let reattached = Arc::new(Mutex::new(false));
    // Set by the callbacks: the status channel only keeps the latest state, so a fast
    // reconnect can skip `Reconnecting` there.
    let lost = Arc::new(Mutex::new(false));
    {
        let b = backend.clone();
        let flag = reattached.clone();
        conn.on_reconnect(move || {
            b.reattach().unwrap();
            *flag.lock().unwrap() = true;
        });
        let b = backend.clone();
        let lost = lost.clone();
        conn.on_lost(move || {
            b.connection_lost();
            *lost.lock().unwrap() = true;
        });
    }

    let mut status = conn.subscribe();
    // The host drops every session (e.g. a network change).
    let handles = std::mem::take(&mut server.state.lock().unwrap().handles);
    for h in handles {
        let _ = h
            .disconnect(russh::Disconnect::ByApplication, "bye".into(), "en".into())
            .await;
    }
    let start = Instant::now();
    loop {
        let s = status.borrow_and_update().clone();
        if *lost.lock().unwrap() && *reattached.lock().unwrap() {
            assert!(matches!(s, ConnectionStatus::Connected { .. }), "{s:?}");
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "no reconnect: {s:?}, lost: {}, reattached: {}",
            *lost.lock().unwrap(),
            *reattached.lock().unwrap()
        );
        let _ = tokio::time::timeout(Duration::from_millis(200), status.changed()).await;
    }
    assert!(server.state.lock().unwrap().starts >= 2);
    let b = backend.clone();
    let content = tokio::task::spawn_blocking(move || b.read_file("app/Order.php").unwrap())
        .await
        .unwrap();
    assert_eq!(content, "<?php class Order {}");
    conn.close().await;
}
