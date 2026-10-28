use forge_core::ForgeError;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("{provider}: invalid API key")]
    InvalidApiKey { provider: String },
    #[error("{provider}: no API key configured (add one in Settings | Tools | AI Providers or set {env_var})")]
    MissingApiKey { provider: String, env_var: String },
    #[error("{provider}: cannot connect to {url}{}", hint.as_deref().map(|h| format!(" — {h}")).unwrap_or_default())]
    Connection {
        provider: String,
        url: String,
        hint: Option<String>,
    },
    #[error("{provider}: request timed out")]
    Timeout { provider: String },
    #[error("{provider}: rate limited, try again later{}", message.as_deref().map(|m| format!(" ({m})")).unwrap_or_default())]
    RateLimited {
        provider: String,
        message: Option<String>,
    },
    #[error("{provider}: HTTP {status}: {message}")]
    Http {
        provider: String,
        status: u16,
        message: String,
    },
    #[error("{provider}: {message}")]
    Api { provider: String, message: String },
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("request cancelled")]
    Cancelled,
    #[error("{provider} does not support {what}")]
    Unsupported { provider: String, what: String },
    #[error("AI provider {0} is disabled (enable it in Settings | Tools | AI Providers)")]
    ProviderDisabled(String),
    #[error("unknown AI provider: {0}")]
    UnknownProvider(String),
    #[error("no AI provider configured")]
    NoProvider,
    #[error("no model selected for {0}")]
    NoModel(String),
    #[error("AI config: {0}")]
    Config(String),
    #[error("system keychain: {0}")]
    Keychain(String),
}

impl From<AiError> for ForgeError {
    fn from(e: AiError) -> Self {
        ForgeError::Other(e.to_string())
    }
}

pub type AiResult<T> = std::result::Result<T, AiError>;
