//! The Docker API of an `ssh://user@host` daemon: one SSH session (`forge-remote`: ssh-agent,
//! `~/.ssh` keys and `~/.ssh/config`, `known_hosts`) and a forwarded Unix socket
//! (`direct-streamlocal`) to the daemon's socket per HTTP request.

use std::path::PathBuf;
use std::sync::Arc;

use forge_remote::session::{ConnectError, ConnectParams, SshSession};
use forge_remote::{store, AuthMethod, SshConfiguration};

use crate::http::BoxIo;
use crate::{Error, Result};

/// A lazily connected SSH session to a Docker host, shared by all requests of a client.
pub struct SshTunnel {
    params: ConnectParams,
    /// `user@host` for messages.
    destination: String,
    socket: String,
    session: tokio::sync::Mutex<Option<Arc<SshSession>>>,
}

impl std::fmt::Debug for SshTunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ssh://{}{}", self.destination, self.socket)
    }
}

/// Connection parameters for `ssh://[user@]host[:port]`, resolved like `ssh` does: aliases,
/// users, ports and identity files from `~/.ssh/config`, keys of the ssh-agent and the
/// default `~/.ssh` keys.
pub fn connect_params(user: Option<&str>, host: &str, port: Option<u16>) -> ConnectParams {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let config = SshConfiguration {
        id: String::new(),
        host: host.to_string(),
        port: port.unwrap_or(22),
        user: user.unwrap_or_default().to_string(),
        auth: AuthMethod::Agent,
        key_file: None,
        dont_save_secret: true,
        name: None,
        parse_config: true,
    };
    let mut p = ConnectParams::for_configuration(
        &config,
        &store::read_ssh_config(&home),
        &home,
        &store::local_user(),
    );
    p.known_hosts = store::known_hosts_path(&home);
    p
}

fn describe(e: ConnectError, destination: &str) -> Error {
    Error(match e {
        ConnectError::HostKey { .. } => format!(
            "{e}. Connect once with `ssh {destination}` (or Tools | SSH Configurations) to trust the host key."
        ),
        other => other.to_string(),
    })
}

impl SshTunnel {
    pub fn new(params: ConnectParams, socket: impl Into<String>) -> Self {
        let destination = if params.user.is_empty() {
            params.host.clone()
        } else {
            format!("{}@{}", params.user, params.host)
        };
        SshTunnel {
            params,
            destination,
            socket: socket.into(),
            session: tokio::sync::Mutex::new(None),
        }
    }

    async fn session(&self, fresh: bool) -> Result<Arc<SshSession>> {
        let mut guard = self.session.lock().await;
        if let Some(s) = guard.as_ref().filter(|s| !fresh && !s.is_closed()) {
            return Ok(s.clone());
        }
        let s = Arc::new(
            SshSession::connect(&self.params)
                .await
                .map_err(|e| describe(e, &self.destination))?,
        );
        *guard = Some(s.clone());
        Ok(s)
    }

    /// A new stream to the daemon's socket; reconnects once when the session died.
    pub async fn open(&self) -> Result<BoxIo> {
        let session = self.session(false).await?;
        match session.open_streamlocal(&self.socket).await {
            Ok(s) => Ok(Box::new(s)),
            Err(_) if session.is_closed() => {
                let session = self.session(true).await?;
                let s = session
                    .open_streamlocal(&self.socket)
                    .await
                    .map_err(|e| describe(e, &self.destination))?;
                Ok(Box::new(s))
            }
            Err(e) => Err(describe(e, &self.destination)),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::endpoint::Endpoint;
    use crate::engine::DockerClient;
    use crate::http::tests::{fake_server, ok_json};
    use crate::http::Transport;
    use russh::keys::ssh_key::private::Ed25519Keypair;
    use russh::keys::PrivateKey;
    use russh::server::{self, Auth, Msg, Session};
    use russh::Channel;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Default)]
    struct Seen {
        sessions: usize,
        sockets: Vec<String>,
    }

    /// An SSH host that forwards `direct-streamlocal` channels to `engine` (a fake Docker
    /// daemon socket) and accepts `docker` / `secret`.
    #[derive(Clone)]
    struct FakeHost {
        engine: String,
        seen: Arc<Mutex<Seen>>,
    }

    impl server::Handler for FakeHost {
        type Error = russh::Error;

        async fn auth_password(
            &mut self,
            user: &str,
            password: &str,
        ) -> std::result::Result<Auth, Self::Error> {
            Ok(if user == "docker" && password == "secret" {
                Auth::Accept
            } else {
                Auth::reject()
            })
        }

        async fn channel_open_direct_streamlocal(
            &mut self,
            channel: Channel<Msg>,
            socket_path: &str,
            reply: server::ChannelOpenHandle,
            _session: &mut Session,
        ) -> std::result::Result<(), Self::Error> {
            self.seen
                .lock()
                .unwrap()
                .sockets
                .push(socket_path.to_string());
            reply.accept().await;
            let engine = self.engine.clone();
            tokio::spawn(async move {
                let mut remote = channel.into_stream();
                let Ok(mut local) = tokio::net::UnixStream::connect(&engine).await else {
                    return;
                };
                let _ = tokio::io::copy_bidirectional(&mut remote, &mut local).await;
            });
            Ok(())
        }
    }

    async fn start_host(engine: String) -> (u16, Arc<Mutex<Seen>>, PrivateKey) {
        let key = PrivateKey::from(Ed25519Keypair::from_seed(&[9u8; 32]));
        let config = Arc::new(server::Config {
            keys: vec![key.clone()],
            auth_rejection_time: Duration::from_millis(10),
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Arc<Mutex<Seen>> = Arc::default();
        let host = FakeHost {
            engine,
            seen: seen.clone(),
        };
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                host.seen.lock().unwrap().sessions += 1;
                let (config, handler) = (config.clone(), host.clone());
                tokio::spawn(async move {
                    if let Ok(running) = server::run_stream(config, socket, handler).await {
                        let _ = running.await;
                    }
                });
            }
        });
        (port, seen, key)
    }

    fn params(port: u16, known_hosts: PathBuf, accept: Option<String>) -> ConnectParams {
        ConnectParams {
            host: "127.0.0.1".into(),
            port,
            user: "docker".into(),
            auth: AuthMethod::Password,
            secret: Some("secret".into()),
            known_hosts,
            accept_fingerprint: accept,
            timeout: Duration::from_secs(10),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn docker_api_through_an_ssh_tunnel() {
        let (_engine_dir, engine) = fake_server(|head, _| {
            if head.starts_with("GET /version") {
                ok_json(r#"{"Version":"27.3.1","ApiVersion":"1.47","Os":"linux","Arch":"amd64"}"#)
            } else {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
            }
        })
        .await;
        let (port, seen, key) = start_host(engine).await;
        let dir = tempfile::tempdir().unwrap();
        let known_hosts = dir.path().join("known_hosts");

        // An unknown host key is refused with its fingerprint and a hint.
        let tunnel = SshTunnel::new(
            params(port, known_hosts.clone(), None),
            "/var/run/docker.sock",
        );
        let e = tunnel.open().await.err().unwrap();
        assert!(e.0.contains("cannot be established"), "{e}");
        assert!(e.0.contains("ssh docker@127.0.0.1"), "{e}");

        let blob = key.public_key().to_openssh().unwrap();
        let blob = blob.split_whitespace().nth(1).unwrap();
        let fp = forge_remote::known_hosts::fingerprint(blob).unwrap();
        let tunnel = SshTunnel::new(params(port, known_hosts, Some(fp)), "/var/run/docker.sock");
        let client = DockerClient::with_transport(
            Transport::Ssh(Arc::new(tunnel)),
            Endpoint::Ssh {
                user: Some("docker".into()),
                host: "127.0.0.1".into(),
                port: Some(port),
                socket: "/var/run/docker.sock".into(),
            },
        );
        assert_eq!(client.version().await.unwrap().version, "27.3.1");
        assert_eq!(client.version().await.unwrap().api_version, "1.47");
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen.sockets,
            ["/var/run/docker.sock", "/var/run/docker.sock"]
        );
        assert_eq!(seen.sessions, 2, "the refused one and one shared session");
    }
}
