//! Data sources of a project on a remote host connect through its SSH connection: the
//! database host and port are as the remote host sees them (`localhost` is the remote
//! machine), reached through a local port forwarded with a `direct-tcpip` channel.

use std::sync::Arc;

use forge_core::SharedFiles;
use futures_util::future::BoxFuture;

use crate::error::DbResult;

/// A forwarded local port; the tunnel stays open while this lives.
pub struct TunnelEndpoint {
    /// Port on `127.0.0.1` here.
    pub port: u16,
    _guard: Box<dyn Send + Sync>,
}

impl TunnelEndpoint {
    pub fn new(port: u16, guard: impl Send + Sync + 'static) -> Self {
        Self {
            port,
            _guard: Box::new(guard),
        }
    }
}

/// Opens tunnels to databases as the project's host sees them.
pub trait DbTunnel: Send + Sync {
    fn open(&self, host: String, port: u16) -> BoxFuture<'static, DbResult<TunnelEndpoint>>;
    /// Where the tunnel goes, shown in the UI (`deploy@shop.example.com`).
    fn label(&self) -> String;
}

/// The open project as the database tools see it.
#[derive(Clone)]
pub struct ProjectContext {
    /// Absolute root on the project's machine.
    pub root: std::path::PathBuf,
    /// `.forge/dataSources.toml` and `.env` of the project.
    pub files: SharedFiles,
    /// Set for remote projects: project data sources connect through it.
    pub tunnel: Option<Arc<dyn DbTunnel>>,
}
