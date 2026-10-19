//! Forge AI provider layer.
//!
//! A [`Provider`] turns a provider-neutral [`CompletionRequest`] into a one-shot
//! [`Completion`] or a stream of [`Delta`]s. Two wire protocols are implemented:
//! the Anthropic Messages API and OpenAI-compatible chat completions (OpenAI, OpenRouter,
//! LM Studio, Ollama, llama.cpp, custom servers). [`AiService`] stores provider configs in
//! `ai.toml` and API keys in the OS keychain.

pub mod anthropic;
pub mod config;
pub mod error;
pub mod openai;
pub mod provider;
pub mod secrets;
pub mod service;
pub mod sse;
pub mod types;

pub use config::{
    AiConfig, AiFeature, FeatureModels, ModelSelection, ProviderConfig, ProviderKind,
};
pub use error::{AiError, AiResult};
pub use provider::{cancellable, collect_text, DeltaStream, Endpoint, Provider};
pub use secrets::{KeySource, KeyringStore, MemoryStore, SecretStore};
pub use service::{AiService, ProviderInfo, ProvidersState, TestResult};
pub use tokio_util::sync::CancellationToken;
pub use types::{
    Completion, CompletionRequest, ContentPart, Delta, Message, ModelInfo, Role, Usage,
};
