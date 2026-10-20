use forge_core::ForgeError;

/// Errors from code hosts, worded for a notification balloon.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("{host}: the token was rejected. Log in again in Settings | Version Control | {host}")]
    Unauthorized { host: String },
    #[error("{host}: access denied{}", message.as_deref().map(|m| format!(" ({m})")).unwrap_or_default())]
    Forbidden {
        host: String,
        message: Option<String>,
    },
    #[error("{host}: {what} not found")]
    NotFound { host: String, what: String },
    #[error("{host}: API rate limit reached{}", reset_hint(*reset))]
    RateLimited { host: String, reset: Option<u64> },
    #[error("{host}: cannot connect to {url}")]
    Connection { host: String, url: String },
    #[error("{host}: the request timed out")]
    Timeout { host: String },
    #[error("{host}: {message}")]
    Api { host: String, message: String },
    #[error("{host} does not support {what}")]
    Unsupported { host: String, what: String },
    #[error("{host}: unexpected response: {message}")]
    InvalidResponse { host: String, message: String },
    #[error("no {host} account for {server}. Log in with a token in Settings | Version Control | {host}")]
    NoAccount { host: String, server: String },
    #[error("no GitLab, Forgejo/Gitea or GitHub remote found in this repository")]
    NoRemote,
    #[error("code host accounts: {0}")]
    Config(String),
    #[error("system keychain: {0}")]
    Keychain(String),
}

fn reset_hint(reset: Option<u64>) -> String {
    let Some(reset) = reset else {
        return ", try again later".into();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let minutes = reset.saturating_sub(now).div_ceil(60).max(1);
    format!(", try again in {minutes} min")
}

impl From<HostError> for ForgeError {
    fn from(e: HostError) -> Self {
        ForgeError::Other(e.to_string())
    }
}

pub type HostResult<T> = std::result::Result<T, HostError>;
