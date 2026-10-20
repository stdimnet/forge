---
title: forge-ai
description: "Forge IDE AI provider layer: Anthropic and OpenAI-compatible chat models."
order: 20
tags: [crate, rust]
---

# forge-ai

`crates/forge-ai` — Forge IDE AI provider layer: Anthropic and OpenAI-compatible chat models.

## Purpose

Forge AI provider layer.

A `Provider` turns a provider-neutral `CompletionRequest` into a one-shot
`Completion` or a stream of `Delta`s. Two wire protocols are implemented:
the Anthropic Messages API and OpenAI-compatible chat completions (OpenAI, OpenRouter,
LM Studio, Ollama, llama.cpp, custom servers). `AiService` stores provider configs in
`ai.toml` and API keys in the OS keychain.

## Modules

| Module | Summary |
| --- | --- |
| `anthropic.rs` | Anthropic Messages API (`POST /v1/messages`). |
| `config.rs` | Provider configuration, persisted as TOML in `<config dir>/forge/ai.toml`. API keys are never written here; see `crate::secrets`. |
| `error.rs` |  |
| `openai.rs` | OpenAI-compatible chat completions (`POST {base}/chat/completions`): OpenAI, OpenRouter, LM Studio, Ollama, llama.cpp server and any other compatible endpoint. |
| `provider.rs` | The `Provider` trait plus HTTP helpers shared by the implementations. |
| `secrets.rs` | API key storage: the OS keychain (service `forge-ai`, account = provider id) with an environment-variable fallback per provider kind. |
| `service.rs` | `AiService`: owns the provider config + key store and builds providers on demand. This is what the app's IPC layer talks to. |
| `sse.rs` | Minimal Server-Sent Events parser (WHATWG event-stream format). |
| `types.rs` | Provider-neutral request/response types. |

## Main public items

```rust
pub use config::{…};
pub use error::{AiError, AiResult};
pub use provider::{cancellable, collect_text, DeltaStream, Endpoint, Provider};
pub use secrets::{KeySource, KeyringStore, MemoryStore, SecretStore};
pub use service::{AiService, ProviderInfo, ProvidersState, TestResult};
pub use tokio_util::sync::CancellationToken;
pub use types::{…};
```

## Used by

- IPC: `ai_complete`, `ai_stream`, `ai_cancel`, `ai_providers_list`, `ai_provider_save`, `ai_provider_delete`, `ai_provider_test`, `ai_models`, `ai_set_default` (`app/src-tauri/src/commands/ai.rs`)
- UI: `app/ui/src/features/ai`, `app/ui/src/features/git (AI commit messages)`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md)
- External: `async-stream`, `async-trait`, `bytes`, `dirs`, `futures-util`, `keyring`, `reqwest`, `serde`, `serde_json`, `thiserror`, `tokio`, `tokio-util`, `toml`, `tracing`

## Tests

10 source files, about 1862 lines; 10 unit tests and 16 integration tests in `tests/` (`providers.rs`).

```sh
cargo test -p forge-ai
```
