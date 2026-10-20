//! Port forwarding between this machine and a remote project's host (SSH tunnels). Local
//! projects have none; `forge-remote` implements them over the project's SSH connection.

use futures_util::future::BoxFuture;

use forge_core::Result;

/// A forwarded port; the forwarding stops when it is dropped.
pub struct PortForward {
    /// The listening port: on this machine for [`Tunnels::forward_local`], on the remote
    /// host for [`Tunnels::forward_remote`].
    pub port: u16,
    /// Shown in the UI, e.g. `deploy@shop.example.com`.
    pub via: String,
    guard: Box<dyn Send + Sync>,
}

impl PortForward {
    pub fn new(port: u16, via: impl Into<String>, guard: impl Send + Sync + 'static) -> Self {
        Self {
            port,
            via: via.into(),
            guard: Box::new(guard),
        }
    }
}

impl std::fmt::Debug for PortForward {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = &self.guard;
        f.debug_struct("PortForward")
            .field("port", &self.port)
            .field("via", &self.via)
            .finish()
    }
}

/// Tunnels of a remote project's connection. They survive reconnects: each new local
/// connection opens a channel on the current session, and remote listeners are requested
/// again after a reconnect.
pub trait Tunnels: Send + Sync {
    /// Listen on `127.0.0.1:<free port>` here; every connection is forwarded to
    /// `host:port` as the remote host sees it (`ssh -L`). Used for databases.
    fn forward_local(&self, host: String, port: u16) -> BoxFuture<'static, Result<PortForward>>;

    /// Listen on the remote host's `127.0.0.1:remote_port`; every connection is forwarded
    /// to `127.0.0.1:local_port` here (`ssh -R`). Used for Xdebug.
    fn forward_remote(
        &self,
        remote_port: u16,
        local_port: u16,
    ) -> BoxFuture<'static, Result<PortForward>>;
}
