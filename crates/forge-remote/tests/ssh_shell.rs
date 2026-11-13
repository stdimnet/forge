//! SSH terminal sessions over real SSH: an in-process russh server on 127.0.0.1 plays a
//! host with a tiny line shell on a pseudo terminal. Covers the `pty-req` (terminal type and
//! size), the shell's output arriving as UTF-8 text (also when a character is split across
//! packets), input, window size changes, the exit status and closing a session.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::Terminal;
use forge_process::{PtyEvent, PtySink};
use forge_remote::session::ConnectParams;
use forge_remote::shell::{SshShell, TERM};
use forge_remote::AuthMethod;
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId};

#[derive(Default)]
struct HostState {
    /// `(term, cols, rows)` of the pty requests.
    ptys: Vec<(String, u32, u32)>,
    /// Window size changes.
    sizes: Vec<(u32, u32)>,
    shells: usize,
    /// Input bytes per channel, until a carriage return.
    lines: HashMap<ChannelId, Vec<u8>>,
    closed: usize,
}

#[derive(Clone)]
struct FakeHost {
    state: Arc<Mutex<HostState>>,
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
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.state
            .lock()
            .unwrap()
            .ptys
            .push((term.to_string(), col_width, row_height));
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.state.lock().unwrap().shells += 1;
        // A greeting with "ü" split across two packets, then the prompt.
        let umlaut = "ü".as_bytes();
        session.data(
            channel,
            [b"Welcome to shop-web-01 \xF0\x9F".as_slice()].concat(),
        )?;
        session.data(channel, [b"\x9A\x80 gr".as_slice(), &umlaut[..1]].concat())?;
        session.data(channel, [&umlaut[1..], b"n\r\n$ ".as_slice()].concat())?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.state
            .lock()
            .unwrap()
            .sizes
            .push((col_width, row_height));
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Echo like a terminal in cooked mode; run the line on Enter.
        session.data(channel, data.to_vec())?;
        let line = {
            let mut state = self.state.lock().unwrap();
            let buf = state.lines.entry(channel).or_default();
            buf.extend_from_slice(data);
            match buf.iter().position(|b| *b == b'\r') {
                Some(at) => {
                    let line = String::from_utf8_lossy(&buf[..at]).into_owned();
                    buf.drain(..=at);
                    Some(line)
                }
                None => None,
            }
        };
        match line.as_deref() {
            None => {}
            Some("exit") => {
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
            Some("fail") => {
                session.exit_status_request(channel, 3)?;
                session.close(channel)?;
            }
            Some(cmd) => {
                session.data(channel, format!("\nran {cmd}\r\n$ ").into_bytes())?;
            }
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.state.lock().unwrap().closed += 1;
        Ok(())
    }
}

struct TestServer {
    port: u16,
    state: Arc<Mutex<HostState>>,
    known_hosts: std::path::PathBuf,
    _dir: tempfile::TempDir,
    _rt: tokio::runtime::Runtime,
}

/// The server on its own runtime (the shell under test drives its own), with its host key
/// already in a temporary `known_hosts`.
fn start_server() -> TestServer {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[5u8; 32]));
    let public = key.public_key().to_openssh().unwrap();
    let state: Arc<Mutex<HostState>> = Arc::default();
    let host = FakeHost {
        state: state.clone(),
    };
    let port = rt.block_on(async move {
        let config = Arc::new(server::Config {
            keys: vec![key],
            auth_rejection_time: Duration::from_millis(10),
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let config = config.clone();
                let handler = host.clone();
                tokio::spawn(async move {
                    if let Ok(running) = server::run_stream(config, socket, handler).await {
                        let _ = running.await;
                    }
                });
            }
        });
        port
    });
    let dir = tempfile::tempdir().unwrap();
    let known_hosts = dir.path().join("known_hosts");
    let mut parts = public.split_whitespace();
    let (kind, blob) = (parts.next().unwrap(), parts.next().unwrap());
    std::fs::write(&known_hosts, format!("[127.0.0.1]:{port} {kind} {blob}\n")).unwrap();
    TestServer {
        port,
        state,
        known_hosts,
        _dir: dir,
        _rt: rt,
    }
}

fn params(server: &TestServer, password: &str) -> ConnectParams {
    ConnectParams {
        host: "127.0.0.1".into(),
        port: server.port,
        user: "deploy".into(),
        auth: AuthMethod::Password,
        secret: Some(password.into()),
        known_hosts: server.known_hosts.clone(),
        timeout: Duration::from_secs(10),
        ..ConnectParams::default()
    }
}

/// Everything the sink received: the output text and the exit code, once exited.
#[derive(Default)]
struct Received {
    text: String,
    exited: Option<Option<i32>>,
}

fn recorder() -> (PtySink, Arc<Mutex<Received>>) {
    let received: Arc<Mutex<Received>> = Arc::default();
    let r = received.clone();
    let sink: PtySink = Arc::new(move |e| {
        let mut r = r.lock().unwrap();
        match e {
            PtyEvent::Output { data } => r.text.push_str(&data),
            PtyEvent::Exited { code } => r.exited = Some(code),
        }
    });
    (sink, received)
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn shells_run_on_a_pseudo_terminal_over_ssh() {
    let server = start_server();
    let (sink, received) = recorder();
    let shell = SshShell::open(&params(&server, "secret"), 120, 32, sink).unwrap();
    assert_eq!(shell.shell(), "deploy@127.0.0.1");
    assert_eq!(shell.cwd(), std::path::PathBuf::from("~"));

    wait_for("the prompt", || {
        received.lock().unwrap().text.ends_with("$ ")
    });
    assert_eq!(
        received.lock().unwrap().text,
        "Welcome to shop-web-01 \u{1F680} grün\r\n$ "
    );
    {
        let state = server.state.lock().unwrap();
        assert_eq!(state.ptys, vec![(TERM.to_string(), 120, 32)]);
        assert_eq!(state.shells, 1);
    }

    shell.write(b"php artisan about\r").unwrap();
    wait_for("the command", || {
        received
            .lock()
            .unwrap()
            .text
            .contains("ran php artisan about")
    });
    shell.resize(200, 50).unwrap();
    wait_for("the window change", || {
        server.state.lock().unwrap().sizes == vec![(200, 50)]
    });

    shell.write(b"exit\r").unwrap();
    wait_for("the exit", || received.lock().unwrap().exited.is_some());
    assert_eq!(received.lock().unwrap().exited, Some(Some(0)));
    // The session is gone: further input fails instead of hanging.
    wait_for("the input to fail", || shell.write(b"x").is_err());
    shell.kill().unwrap();
}

#[test]
fn exit_codes_and_closing_tabs_end_the_session() {
    let server = start_server();
    let (sink, received) = recorder();
    let shell = SshShell::open(&params(&server, "secret"), 80, 24, sink).unwrap();
    wait_for("the prompt", || {
        received.lock().unwrap().text.ends_with("$ ")
    });
    shell.write(b"fail\r").unwrap();
    wait_for("the exit", || received.lock().unwrap().exited.is_some());
    assert_eq!(received.lock().unwrap().exited, Some(Some(3)));

    // Closing a tab closes the channel on the server.
    let (sink, received) = recorder();
    let shell = SshShell::open(&params(&server, "secret"), 80, 24, sink).unwrap();
    wait_for("the prompt", || {
        received.lock().unwrap().text.ends_with("$ ")
    });
    let closed = server.state.lock().unwrap().closed;
    shell.kill().unwrap();
    wait_for("the channel to close", || {
        server.state.lock().unwrap().closed > closed
    });
    drop(shell);
}

#[test]
fn wrong_passwords_are_reported() {
    let server = start_server();
    let (sink, _) = recorder();
    let err = SshShell::open(&params(&server, "wrong"), 80, 24, sink)
        .err()
        .expect("authentication must fail");
    assert!(err.to_string().contains("Authentication failed"), "{err}");
}
