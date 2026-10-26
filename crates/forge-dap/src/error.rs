use forge_core::ForgeError;
use forge_debug::DebugError;

/// Errors of the DAP layer.
#[derive(Debug, thiserror::Error)]
pub enum DapError {
    #[error("debug adapter io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed DAP message: {0}")]
    Protocol(String),
    /// An error response of the adapter (`success: false`).
    #[error("{message}")]
    Adapter { command: String, message: String },
    #[error("the debug adapter connection is closed")]
    Closed,
    #[error("the debug adapter did not answer '{0}' in time")]
    Timeout(String),
    /// The adapter does not offer a feature (missing capability).
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

pub type DapResult<T> = std::result::Result<T, DapError>;

impl From<serde_json::Error> for DapError {
    fn from(e: serde_json::Error) -> Self {
        Self::Protocol(e.to_string())
    }
}

impl From<DapError> for ForgeError {
    fn from(e: DapError) -> Self {
        ForgeError::Other(e.to_string())
    }
}

impl From<DebugError> for DapError {
    fn from(e: DebugError) -> Self {
        Self::Other(e.to_string())
    }
}
