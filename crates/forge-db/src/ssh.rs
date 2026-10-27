//! The SSH tab of a data source: Forge opens an SSH session with `forge-remote` (saved SSH
//! configuration or ad-hoc host, user and authentication; `~/.ssh/config` aliases, jump
//! hosts and `known_hosts` checks included), listens on a free local port and forwards each
//! connection to the database host as the SSH host sees it (`ssh -L`).

use std::path::PathBuf;
use std::sync::Arc;

use forge_remote::session::{ConnectError, ConnectParams, HostKeyInfo, SshSession};
use forge_remote::{SshConfiguration, SshConfigurations};
use futures_util::future::BoxFuture;

use crate::config::SshTunnel;
use crate::error::{DbError, DbResult};
use crate::tunnel::TunnelEndpoint;

/// Opens the SSH tunnels of data sources.
pub trait SshConnector: Send + Sync {
    /// Connect with `tunnel` (and its `secret`) and forward a local port to `host:port`.
    /// `accept_host_key` is a fingerprint of an unknown or changed key the user confirmed.
    fn open(
        &self,
        tunnel: &SshTunnel,
        secret: Option<String>,
        accept_host_key: Option<String>,
        host: String,
        port: u16,
    ) -> BoxFuture<'static, DbResult<TunnelEndpoint>>;

    /// `deploy@bastion` for the tree and the status text.
    fn label(&self, tunnel: &SshTunnel) -> String;
}

/// The SSH configurations and OpenSSH files of the user (`~/.ssh`).
pub struct ForgeSsh {
    home: PathBuf,
    /// Saved configurations; `None` reads the SSH Configurations store each time.
    configurations: Option<SshConfigurations>,
    local_user: String,
}

impl Default for ForgeSsh {
    fn default() -> Self {
        Self {
            home: dirs::home_dir().unwrap_or_default(),
            configurations: None,
            local_user: forge_remote::store::local_user(),
        }
    }
}

impl ForgeSsh {
    /// For tests: another home directory (its `.ssh/known_hosts`) and fixed configurations.
    pub fn with(home: PathBuf, configurations: SshConfigurations) -> Self {
        Self {
            home,
            configurations: Some(configurations),
            local_user: "forge".into(),
        }
    }

    fn configuration(&self, id: &str) -> Option<SshConfiguration> {
        match &self.configurations {
            Some(all) => all.get(id).cloned(),
            None => forge_remote::store::load().get(id).cloned(),
        }
    }

    /// Connection parameters of a tunnel; a saved configuration brings its own secret.
    pub fn params(
        &self,
        tunnel: &SshTunnel,
        secret: Option<String>,
        accept_host_key: Option<String>,
    ) -> DbResult<ConnectParams> {
        let (config, secret) = match tunnel.configuration_id.as_deref().filter(|s| !s.is_empty()) {
            Some(id) => {
                let config = self.configuration(id).ok_or_else(|| {
                    DbError::Config(format!(
                        "SSH configuration {id} not found (Settings | Tools | SSH Configurations)"
                    ))
                })?;
                let stored = if self.configurations.is_some() {
                    secret
                } else {
                    forge_remote::store::secret(id).or(secret)
                };
                (config, stored)
            }
            None => {
                let config = SshConfiguration {
                    id: String::new(),
                    host: tunnel.host.trim().to_string(),
                    port: tunnel.port,
                    user: tunnel.user.trim().to_string(),
                    auth: tunnel.auth,
                    key_file: tunnel.key_file.clone(),
                    dont_save_secret: false,
                    name: None,
                    parse_config: true,
                };
                if config.host.is_empty() {
                    return Err(DbError::Config("SSH tunnel: host is required".into()));
                }
                (config, secret)
            }
        };
        let ssh_config = if self.configurations.is_some() {
            forge_remote::ssh_config::SshConfig::default()
        } else {
            forge_remote::store::read_ssh_config(&self.home)
        };
        let mut params =
            ConnectParams::for_configuration(&config, &ssh_config, &self.home, &self.local_user);
        params.secret = secret.filter(|s| !s.is_empty());
        params.accept_fingerprint = accept_host_key;
        Ok(params)
    }
}

/// An SSH failure as the data source dialog shows it; unknown host keys carry the key so
/// the user can trust it and test again.
pub fn ssh_error(e: ConnectError) -> DbError {
    match e {
        ConnectError::HostKey { host_key } => DbError::SshHostKey(Box::new(host_key)),
        other => DbError::Io(format!("SSH tunnel: {other}")),
    }
}

impl SshConnector for ForgeSsh {
    fn open(
        &self,
        tunnel: &SshTunnel,
        secret: Option<String>,
        accept_host_key: Option<String>,
        host: String,
        port: u16,
    ) -> BoxFuture<'static, DbResult<TunnelEndpoint>> {
        let params = self.params(tunnel, secret, accept_host_key);
        Box::pin(async move { forward(params?, host, port).await })
    }

    fn label(&self, tunnel: &SshTunnel) -> String {
        match tunnel.configuration_id.as_deref().filter(|s| !s.is_empty()) {
            Some(id) => self
                .configuration(id)
                .map(|c| c.name.clone().unwrap_or_else(|| c.label()))
                .unwrap_or_else(|| id.to_string()),
            None => tunnel.label(),
        }
    }
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Connect and listen on `127.0.0.1:<free port>`; every accepted connection is forwarded to
/// `host:port` through a `direct-tcpip` channel. Stops when the endpoint is dropped.
pub async fn forward(params: ConnectParams, host: String, port: u16) -> DbResult<TunnelEndpoint> {
    let session = Arc::new(SshSession::connect(&params).await.map_err(ssh_error)?);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| DbError::Io(format!("SSH tunnel: cannot open a local port: {e}")))?;
    let local = listener
        .local_addr()
        .map_err(|e| DbError::Io(e.to_string()))?
        .port();
    tracing::info!(
        "forwarding 127.0.0.1:{local} to {host}:{port} via {}@{}",
        params.user,
        params.host
    );
    let task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let session = session.clone();
            let host = host.clone();
            tokio::spawn(async move {
                let _ = socket.set_nodelay(true);
                match session.open_direct_tcpip(&host, port).await {
                    Ok(mut stream) => {
                        let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                    }
                    Err(e) => tracing::warn!("SSH tunnel to {host}:{port} failed: {e}"),
                }
            });
        }
    });
    Ok(TunnelEndpoint::new(local, AbortOnDrop(task)))
}

/// The host key of an [`DbError::SshHostKey`], for the "trust this host" prompt.
pub fn host_key(e: &DbError) -> Option<&HostKeyInfo> {
    match e {
        DbError::SshHostKey(k) => Some(k),
        _ => None,
    }
}
