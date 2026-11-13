//! The `Provider` trait plus HTTP helpers shared by the implementations.

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;

use crate::config::ProviderKind;
use crate::error::{AiError, AiResult};
use crate::types::{Completion, CompletionRequest, Delta, FimRequest, ModelInfo};

pub type DeltaStream = Pin<Box<dyn Stream<Item = AiResult<Delta>> + Send>>;

pub const DEFAULT_MAX_TOKENS: u32 = 4096;

#[async_trait]
pub trait Provider: Send + Sync {
    /// Display name used in error messages.
    fn name(&self) -> &str;

    /// Model used when the request names none.
    fn default_model(&self) -> Option<&str>;

    async fn list_models(&self) -> AiResult<Vec<ModelInfo>>;

    /// One-shot completion. Dropping the future aborts the HTTP request.
    async fn complete(&self, request: CompletionRequest) -> AiResult<Completion>;

    /// Streamed completion. Dropping the stream aborts the HTTP request; to cancel from
    /// elsewhere wrap it with [`cancellable`].
    async fn stream(&self, request: CompletionRequest) -> AiResult<DeltaStream>;

    /// Whether [`Provider::fim`] is implemented (legacy `completions` endpoint with `suffix`).
    fn supports_fim(&self) -> bool {
        false
    }

    /// Fill-in-the-middle completion for inline code completion.
    async fn fim(&self, request: FimRequest) -> AiResult<Completion> {
        let _ = request;
        Err(AiError::Unsupported {
            provider: self.name().to_string(),
            what: "fill-in-the-middle completion".into(),
        })
    }
}

/// Ends `stream` with `Err(AiError::Cancelled)` as soon as `token` is cancelled.
pub fn cancellable(stream: DeltaStream, token: CancellationToken) -> DeltaStream {
    Box::pin(async_stream::stream! {
        let mut stream = stream;
        loop {
            tokio::select! {
                biased;
                _ = token.cancelled() => {
                    yield Err(AiError::Cancelled);
                    return;
                }
                item = stream.next() => match item {
                    Some(item) => yield item,
                    None => return,
                }
            }
        }
    })
}

/// Collect a stream into a single string (used by tests and simple callers).
pub async fn collect_text(mut stream: DeltaStream) -> AiResult<String> {
    let mut out = String::new();
    while let Some(d) = stream.next().await {
        if let Delta::Text { text } = d? {
            out.push_str(&text);
        }
    }
    Ok(out)
}

/// Shared HTTP client settings.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("http client")
}

/// Common pieces of a provider instance.
#[derive(Clone)]
pub struct Endpoint {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
    pub headers: Vec<(String, String)>,
    pub default_model: Option<String>,
    pub client: reqwest::Client,
}

impl Endpoint {
    pub fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    pub fn model_for(&self, req: &CompletionRequest) -> AiResult<String> {
        req.model
            .clone()
            .filter(|m| !m.is_empty())
            .or_else(|| self.default_model.clone())
            .ok_or_else(|| AiError::NoModel(self.name.clone()))
    }

    pub fn require_key(&self) -> AiResult<&str> {
        match self.api_key.as_deref() {
            Some(k) => Ok(k),
            None => Err(AiError::MissingApiKey {
                provider: self.name.clone(),
                env_var: self.kind.env_var().unwrap_or("an API key").into(),
            }),
        }
    }

    pub fn with_headers(&self, mut rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (k, v) in &self.headers {
            rb = rb.header(k, v);
        }
        rb
    }

    /// Send and map transport errors and non-2xx statuses to [`AiError`].
    pub async fn send(&self, rb: reqwest::RequestBuilder) -> AiResult<reqwest::Response> {
        let resp = rb.send().await.map_err(|e| self.transport_error(e))?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Err(self.status_error(status, &body))
    }

    pub fn transport_error(&self, e: reqwest::Error) -> AiError {
        if e.is_timeout() {
            return AiError::Timeout {
                provider: self.name.clone(),
            };
        }
        if e.is_connect() {
            return AiError::Connection {
                provider: self.name.clone(),
                url: self.base_url.clone(),
                hint: self.kind.connection_hint().map(Into::into),
            };
        }
        AiError::Api {
            provider: self.name.clone(),
            message: e.to_string(),
        }
    }

    pub fn status_error(&self, status: u16, body: &str) -> AiError {
        let message = error_message(body).unwrap_or_else(|| {
            let t = body.trim();
            if t.is_empty() {
                "request failed".to_string()
            } else {
                t.chars().take(300).collect()
            }
        });
        let provider = self.name.clone();
        match status {
            401 | 403 if self.api_key.is_some() || self.kind.requires_api_key() => {
                AiError::InvalidApiKey { provider }
            }
            429 => AiError::RateLimited {
                provider,
                message: Some(message),
            },
            _ => AiError::Http {
                provider,
                status,
                message,
            },
        }
    }
}

/// Extract `error.message` (Anthropic + OpenAI) or a string `error` (Ollama, llama.cpp).
pub fn error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let err = v.get("error")?;
    err.get("message")
        .and_then(|m| m.as_str())
        .or_else(|| err.as_str())
        .map(Into::into)
}
