use forge_core::ForgeError;

/// Errors of the debugger layer.
#[derive(Debug, thiserror::Error)]
pub enum DebugError {
    #[error("debugger io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed DBGp message: {0}")]
    Xml(String),
    #[error("DBGp protocol error: {0}")]
    Protocol(String),
    /// An error answer of the debug engine (`<error code="..."><message>`).
    #[error("{message} (DBGp error {code})")]
    Engine { code: u32, message: String },
    #[error("the debug connection is closed")]
    Closed,
    #[error("the debug engine did not answer in time")]
    Timeout,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

pub type DebugResult<T> = std::result::Result<T, DebugError>;

impl From<DebugError> for ForgeError {
    fn from(e: DebugError) -> Self {
        ForgeError::Other(e.to_string())
    }
}
