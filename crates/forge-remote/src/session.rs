//! SSH sessions with russh: host key verification against `known_hosts` (unknown and
//! changed keys need the user's confirmation of the fingerprint), password, key file and
//! ssh-agent authentication, command execution and streams, and forwarded Unix sockets (the
//! Docker API over SSH).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::{self, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelStream, Disconnect};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::config::AuthMethod;
use crate::known_hosts::{self, HostKeyStatus};

/// Everything needed to open a session.
#[derive(Debug, Clone, Default)]
pub struct ConnectParams {
    /// Host name or address to connect to (after `~/.ssh/config` resolution).
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
    /// Key for [`AuthMethod::KeyPair`].
    pub key_file: Option<PathBuf>,
    /// Password, or the passphrase of the key file.
    pub secret: Option<String>,
    /// Extra keys to try with [`AuthMethod::Agent`] (`IdentityFile`s, default keys).
    pub identity_files: Vec<PathBuf>,
    /// The `known_hosts` file to check and update.
    pub known_hosts: PathBuf,
    /// Fingerprint of an unknown or changed host key the user confirmed.
    pub accept_fingerprint: Option<String>,
    pub timeout: Duration,
}

impl ConnectParams {
    /// Parameters for a saved configuration: its host may be a `Host` alias of
    /// `~/.ssh/config` (host name, port, user and identity files come from there unless the
    /// configuration sets them), `~` in the key file is the home directory.
    pub fn for_configuration(
        config: &crate::config::SshConfiguration,
        ssh_config: &crate::ssh_config::SshConfig,
        home: &Path,
        local_user: &str,
    ) -> Self {
        let home_str = home.to_string_lossy();
        let resolved = ssh_config.resolve(config.host.trim(), &home_str, local_user);
        let port = match (config.port, resolved.port) {
            (22, Some(p)) => p,
            (p, _) => p,
        };
        let user = if config.user.trim().is_empty() {
            resolved
                .user
                .clone()
                .unwrap_or_else(|| local_user.to_string())
        } else {
            config.user.trim().to_string()
        };
        let expand = |p: &str| -> PathBuf {
            match p.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None => PathBuf::from(p),
            }
        };
        let mut identity_files: Vec<PathBuf> =
            resolved.identity_files.iter().map(|f| expand(f)).collect();
        if !resolved.identities_only {
            for f in default_identity_files(home) {
                if !identity_files.contains(&f) {
                    identity_files.push(f);
                }
            }
        }
        ConnectParams {
            host: resolved.host_name,
            port,
            user,
            auth: config.auth,
            key_file: config
                .key_file
                .as_deref()
                .filter(|k| !k.trim().is_empty())
                .map(|k| expand(k.trim())),
            secret: None,
            identity_files,
            known_hosts: home.join(".ssh").join("known_hosts"),
            accept_fingerprint: None,
            timeout: Duration::from_secs(15),
        }
    }
}

/// A host key that needs the user's decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyInfo {
    pub host: String,
    pub port: u16,
    /// Key algorithm, e.g. `ssh-ed25519`.
    pub key_type: String,
    /// `SHA256:…`
    pub fingerprint: String,
    #[serde(flatten)]
    pub status: HostKeyStatus,
}

/// Why a connection failed, structured so the UI can react (fingerprint dialog, password
/// prompt).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConnectError {
    /// The host key is unknown, changed or revoked.
    #[serde(rename_all = "camelCase")]
    HostKey {
        host_key: HostKeyInfo,
    },
    /// The server rejected the credentials (or a key could not be read).
    Auth {
        message: String,
    },
    /// The host could not be reached.
    Network {
        message: String,
    },
    Other {
        message: String,
    },
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::HostKey { host_key } => match &host_key.status {
                HostKeyStatus::Changed { .. } => write!(
                    f,
                    "The host key of {} has changed ({}). Someone could be intercepting the connection.",
                    host_key.host, host_key.fingerprint
                ),
                HostKeyStatus::Revoked => {
                    write!(f, "The host key of {} has been revoked", host_key.host)
                }
                _ => write!(
                    f,
                    "The authenticity of host {} cannot be established ({} key fingerprint is {})",
                    host_key.host, host_key.key_type, host_key.fingerprint
                ),
            },
            ConnectError::Auth { message }
            | ConnectError::Network { message }
            | ConnectError::Other { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for ConnectError {}

impl From<ConnectError> for forge_core::ForgeError {
    fn from(e: ConnectError) -> Self {
        forge_core::ForgeError::Other(e.to_string())
    }
}

fn other(e: impl std::fmt::Display) -> ConnectError {
    ConnectError::Other {
        message: e.to_string(),
    }
}

struct Verifier {
    host: String,
    port: u16,
    known_hosts: PathBuf,
    accept: Option<String>,
    rejected: Arc<Mutex<Option<HostKeyInfo>>>,
}

/// Split an OpenSSH public key line into `(algorithm, base64 blob)`.
fn split_key(key: &keys::PublicKey) -> Option<(String, String)> {
    let line = key.to_openssh().ok()?;
    let mut parts = line.split_whitespace();
    Some((parts.next()?.to_string(), parts.next()?.to_string()))
}

/// Decide about a presented key; trusts it (and records it in `known_hosts`) when it is
/// known or the user confirmed its fingerprint.
pub fn verify_host_key(
    known_hosts_path: &Path,
    host: &str,
    port: u16,
    key_type: &str,
    key: &str,
    accept: Option<&str>,
) -> std::result::Result<(), HostKeyInfo> {
    let text = std::fs::read_to_string(known_hosts_path).unwrap_or_default();
    let status = known_hosts::check(&known_hosts::parse(&text), host, port, key_type, key);
    let fingerprint = known_hosts::fingerprint(key).unwrap_or_default();
    let info = HostKeyInfo {
        host: host.to_string(),
        port,
        key_type: key_type.to_string(),
        fingerprint: fingerprint.clone(),
        status: status.clone(),
    };
    match status {
        HostKeyStatus::Trusted => Ok(()),
        HostKeyStatus::Unknown | HostKeyStatus::Changed { .. }
            if accept == Some(fingerprint.as_str()) =>
        {
            let base = if matches!(status, HostKeyStatus::Changed { .. }) {
                known_hosts::remove_host_key(&text, host, port, key_type)
            } else {
                text
            };
            let updated =
                known_hosts::append(&base, &known_hosts::entry_line(host, port, key_type, key));
            if let Some(dir) = known_hosts_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(known_hosts_path, updated) {
                tracing::warn!("cannot update {}: {e}", known_hosts_path.display());
            }
            Ok(())
        }
        _ => Err(info),
    }
}

impl client::Handler for Verifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            // Host certificates are not supported: treat like an unknown key we cannot pin.
            return Ok(false);
        };
        let Some((key_type, blob)) = split_key(key) else {
            return Ok(false);
        };
        match verify_host_key(
            &self.known_hosts,
            &self.host,
            self.port,
            &key_type,
            &blob,
            self.accept.as_deref(),
        ) {
            Ok(()) => Ok(true),
            Err(info) => {
                *self.rejected.lock().expect("rejected lock") = Some(info);
                Ok(false)
            }
        }
    }
}

/// An authenticated SSH session.
pub struct SshSession {
    handle: Handle<Verifier>,
}

/// Default private keys tried with [`AuthMethod::Agent`], like `ssh` does.
pub fn default_identity_files(home: &Path) -> Vec<PathBuf> {
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|f| home.join(".ssh").join(f))
        .collect()
}

fn network(e: impl std::fmt::Display) -> ConnectError {
    ConnectError::Network {
        message: e.to_string(),
    }
}

impl SshSession {
    /// Connect, verify the host key and authenticate.
    pub async fn connect(params: &ConnectParams) -> std::result::Result<Self, ConnectError> {
        let addr = (params.host.as_str(), params.port);
        let stream = tokio::time::timeout(params.timeout, tokio::net::TcpStream::connect(addr))
            .await
            .map_err(|_| network(format!("Connection to {} timed out", params.host)))?
            .map_err(|e| {
                network(format!(
                    "Cannot connect to {}:{}: {e}",
                    params.host, params.port
                ))
            })?;
        let _ = stream.set_nodelay(true);
        Self::connect_stream(stream, params).await
    }

    /// Like [`connect`](Self::connect) over an existing stream (tests use in-memory pipes).
    pub async fn connect_stream<S>(
        stream: S,
        params: &ConnectParams,
    ) -> std::result::Result<Self, ConnectError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let config = Arc::new(client::Config {
            // Keepalives detect dead connections; the session layer reconnects.
            keepalive_interval: Some(Duration::from_secs(15)),
            keepalive_max: 3,
            inactivity_timeout: None,
            ..Default::default()
        });
        let rejected = Arc::new(Mutex::new(None));
        let verifier = Verifier {
            host: params.host.clone(),
            port: params.port,
            known_hosts: params.known_hosts.clone(),
            accept: params.accept_fingerprint.clone(),
            rejected: rejected.clone(),
        };
        let handle = match tokio::time::timeout(
            params.timeout,
            client::connect_stream(config, stream, verifier),
        )
        .await
        {
            Err(_) => return Err(network("The SSH handshake timed out")),
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                if let Some(info) = rejected.lock().expect("rejected lock").take() {
                    return Err(ConnectError::HostKey { host_key: info });
                }
                return Err(network(format!("SSH handshake failed: {e}")));
            }
        };
        let mut session = SshSession { handle };
        session.authenticate(params).await?;
        Ok(session)
    }

    async fn authenticate(
        &mut self,
        params: &ConnectParams,
    ) -> std::result::Result<(), ConnectError> {
        let user = params.user.clone();
        let denied = || ConnectError::Auth {
            message: format!("Authentication failed for {}@{}", params.user, params.host),
        };
        match params.auth {
            AuthMethod::Password => {
                let password = params.secret.clone().unwrap_or_default();
                let res = self
                    .handle
                    .authenticate_password(user, password)
                    .await
                    .map_err(other)?;
                if res.success() {
                    Ok(())
                } else {
                    Err(denied())
                }
            }
            AuthMethod::KeyPair => {
                let path = params.key_file.clone().ok_or_else(|| ConnectError::Auth {
                    message: "No private key file is set".into(),
                })?;
                if self.try_key(&user, &path, params.secret.as_deref()).await? {
                    Ok(())
                } else {
                    Err(denied())
                }
            }
            AuthMethod::Agent => {
                if self.try_agent(&user).await {
                    return Ok(());
                }
                for path in &params.identity_files {
                    if path.is_file() {
                        match self.try_key(&user, path, params.secret.as_deref()).await {
                            Ok(true) => return Ok(()),
                            Ok(false) => {}
                            // An encrypted key without passphrase: try the next one.
                            Err(e) => tracing::debug!("{}: {e}", path.display()),
                        }
                    }
                }
                Err(ConnectError::Auth {
                    message: format!(
                        "Authentication failed for {}@{}: no key of the ssh-agent or ~/.ssh was accepted",
                        params.user, params.host
                    ),
                })
            }
        }
    }

    async fn try_key(
        &mut self,
        user: &str,
        path: &Path,
        passphrase: Option<&str>,
    ) -> std::result::Result<bool, ConnectError> {
        let key = keys::load_secret_key(path, passphrase).map_err(|e| ConnectError::Auth {
            message: match e {
                keys::Error::KeyIsEncrypted => {
                    format!("The key {} needs a passphrase", path.display())
                }
                e => format!("Cannot read the key {}: {e}", path.display()),
            },
        })?;
        let hash = self
            .handle
            .best_supported_rsa_hash()
            .await
            .map_err(other)?
            .flatten();
        let res = self
            .handle
            .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
            .await
            .map_err(other)?;
        Ok(res.success())
    }

    #[cfg(unix)]
    async fn try_agent(&mut self, user: &str) -> bool {
        let Ok(mut agent) = keys::agent::client::AgentClient::connect_env().await else {
            return false;
        };
        let Ok(identities) = agent.request_identities().await else {
            return false;
        };
        let hash = self
            .handle
            .best_supported_rsa_hash()
            .await
            .ok()
            .flatten()
            .flatten();
        for id in identities {
            let keys::agent::AgentIdentity::PublicKey { key, .. } = id else {
                continue;
            };
            let res = self
                .handle
                .authenticate_publickey_with(user, key, hash, &mut agent)
                .await;
            if res.is_ok_and(|r| r.success()) {
                return true;
            }
        }
        false
    }

    #[cfg(not(unix))]
    async fn try_agent(&mut self, _user: &str) -> bool {
        false
    }

    /// Run `command`, feed it `stdin` and collect its output.
    pub async fn exec(
        &self,
        command: &str,
        stdin: Option<&[u8]>,
    ) -> std::result::Result<ExecOutput, ConnectError> {
        let mut channel = self.handle.channel_open_session().await.map_err(other)?;
        channel.exec(true, command).await.map_err(other)?;
        if let Some(data) = stdin {
            let mut writer = channel.make_writer();
            writer.write_all(data).await.map_err(other)?;
            writer.flush().await.map_err(other)?;
            drop(writer);
            channel.eof().await.map_err(other)?;
        }
        let mut out = ExecOutput::default();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => out.stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => out.stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => out.status = Some(exit_status),
                _ => {}
            }
        }
        Ok(out)
    }

    /// Run `command` and return its stdin/stdout as one stream (stderr is dropped).
    pub async fn exec_stream(
        &self,
        command: &str,
    ) -> std::result::Result<ChannelStream<client::Msg>, ConnectError> {
        let channel = self.handle.channel_open_session().await.map_err(other)?;
        channel.exec(true, command).await.map_err(other)?;
        Ok(channel.into_stream())
    }

    /// A stream to a Unix socket on the host (`direct-streamlocal@openssh.com`, like
    /// `ssh -L local:/var/run/docker.sock`).
    pub async fn open_streamlocal(
        &self,
        socket_path: &str,
    ) -> std::result::Result<ChannelStream<client::Msg>, ConnectError> {
        let channel = self
            .handle
            .channel_open_direct_streamlocal(socket_path)
            .await
            .map_err(|e| ConnectError::Other {
                message: format!("Cannot open {socket_path} on the host: {e}"),
            })?;
        Ok(channel.into_stream())
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    pub async fn disconnect(&self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
    }
}

/// Output of [`SshSession::exec`].
#[derive(Debug, Clone, Default)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: Option<u32>,
}

impl ExecOutput {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

    #[test]
    fn unknown_keys_need_a_confirmed_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ssh/known_hosts");
        let fp = known_hosts::fingerprint(KEY).unwrap();

        let err =
            verify_host_key(&path, "shop.example.com", 22, "ssh-ed25519", KEY, None).unwrap_err();
        assert_eq!(err.status, HostKeyStatus::Unknown);
        assert_eq!(err.fingerprint, fp);
        // A different confirmed fingerprint does not count.
        assert!(verify_host_key(
            &path,
            "shop.example.com",
            22,
            "ssh-ed25519",
            KEY,
            Some("SHA256:x")
        )
        .is_err());

        verify_host_key(&path, "shop.example.com", 22, "ssh-ed25519", KEY, Some(&fp)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, format!("shop.example.com ssh-ed25519 {KEY}\n"));
        // Known now: no confirmation needed.
        verify_host_key(&path, "shop.example.com", 22, "ssh-ed25519", KEY, None).unwrap();
    }

    #[test]
    fn changed_keys_are_replaced_only_after_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let old = "AAAAC3NzaC1lZDI1NTE5AAAAIBH1bD1mVrOR8vT3+3tW9I+gp5+rtZ5Xj5wIWmhB9i1y";
        std::fs::write(
            &path,
            format!("other ssh-rsa AAAA\n[shop]:2222 ssh-ed25519 {old}\n"),
        )
        .unwrap();
        let err = verify_host_key(&path, "shop", 2222, "ssh-ed25519", KEY, None).unwrap_err();
        assert!(matches!(err.status, HostKeyStatus::Changed { line: 1, .. }));
        let fp = known_hosts::fingerprint(KEY).unwrap();
        verify_host_key(&path, "shop", 2222, "ssh-ed25519", KEY, Some(&fp)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("other ssh-rsa AAAA\n[shop]:2222 ssh-ed25519 {KEY}\n")
        );
    }

    #[test]
    fn configurations_resolve_ssh_config_aliases() {
        let ssh = crate::ssh_config::SshConfig::parse(
            "Host shop\n  HostName shop.example.com\n  Port 2222\n  User deploy\n  IdentityFile ~/.ssh/id_shop\n",
        );
        let config = crate::config::SshConfiguration {
            id: "c".into(),
            host: "shop".into(),
            port: 22,
            user: String::new(),
            auth: AuthMethod::KeyPair,
            key_file: Some("~/.ssh/deploy_key".into()),
            dont_save_secret: false,
            name: None,
        };
        let home = Path::new("/home/me");
        let p = ConnectParams::for_configuration(&config, &ssh, home, "me");
        assert_eq!(p.host, "shop.example.com");
        assert_eq!(p.port, 2222);
        assert_eq!(p.user, "deploy");
        assert_eq!(p.key_file, Some(PathBuf::from("/home/me/.ssh/deploy_key")));
        assert_eq!(p.identity_files[0], PathBuf::from("/home/me/.ssh/id_shop"));
        assert!(p
            .identity_files
            .contains(&PathBuf::from("/home/me/.ssh/id_ed25519")));
        assert_eq!(p.known_hosts, PathBuf::from("/home/me/.ssh/known_hosts"));

        // Explicit values win over the ssh config.
        let explicit = crate::config::SshConfiguration {
            port: 2200,
            user: "root".into(),
            ..config
        };
        let p = ConnectParams::for_configuration(&explicit, &ssh, home, "me");
        assert_eq!((p.port, p.user.as_str()), (2200, "root"));
    }

    #[test]
    fn errors_serialize_for_the_ui() {
        let e = ConnectError::HostKey {
            host_key: HostKeyInfo {
                host: "h".into(),
                port: 22,
                key_type: "ssh-ed25519".into(),
                fingerprint: "SHA256:abc".into(),
                status: HostKeyStatus::Unknown,
            },
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "hostKey");
        assert_eq!(json["hostKey"]["status"], "unknown");
        assert_eq!(json["hostKey"]["fingerprint"], "SHA256:abc");
        assert!(e.to_string().contains("cannot be established"));
    }
}
