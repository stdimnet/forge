use std::path::PathBuf;

/// Errors surfaced by the core. Serializable so they can cross the IPC boundary as strings.
#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    #[error("path is outside of the workspace: {0}")]
    OutsideWorkspace(PathBuf),
    #[error("not found: {0}")]
    NotFound(PathBuf),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

impl ForgeError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl serde::Serialize for ForgeError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ForgeError>;
