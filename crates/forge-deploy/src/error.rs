//! The error of every deployment operation: a message for the user.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployError(pub String);

pub type Result<T> = std::result::Result<T, DeployError>;

impl DeployError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// `"<context>: <error>"`.
    pub fn context(context: impl fmt::Display, error: impl fmt::Display) -> Self {
        Self(format!("{context}: {error}"))
    }
}

impl fmt::Display for DeployError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DeployError {}

impl From<std::io::Error> for DeployError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}
