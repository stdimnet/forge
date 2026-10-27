//! AI IPC: completions/streaming (shared contract in docs/ARCHITECTURE.md) and provider
//! management for Settings | Tools | AI Providers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use forge_ai::{
    cancellable, AiConfig, AiFeature, AiService, CancellationToken, CompletionRequest, Delta,
    KeyringStore, Message, ModelInfo, ProviderConfig, ProvidersState, Role, TestResult,
};
use forge_core::Result;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

/// Managed state: the provider service plus cancellation tokens of running streams.
pub struct AiState {
    pub(crate) service: AiService,
    pub(crate) streams: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl AiState {
    pub fn load() -> Self {
        let path = AiConfig::default_path();
        let service = AiService::new(path.clone(), Arc::new(KeyringStore)).unwrap_or_else(|e| {
            tracing::warn!("{e}; AI provider settings will not be saved");
            AiService::new(None, Arc::new(KeyringStore)).expect("in-memory AI config")
        });
        Self {
            service,
            streams: Arc::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct IpcMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequest {
    pub system: Option<String>,
    pub messages: Vec<IpcMessage>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stop: Vec<String>,
    /// Which feature asks (picks the per-feature model when provider/model are not given).
    pub feature: Option<AiFeature>,
}

impl AiRequest {
    fn into_parts(self) -> (Option<String>, Option<AiFeature>, CompletionRequest) {
        let req = CompletionRequest {
            model: self.model,
            system: self.system,
            messages: self
                .messages
                .into_iter()
                .map(|m| Message::text(m.role, m.content))
                .collect(),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            stop: self.stop,
        };
        (self.provider, self.feature, req)
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct StreamEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub(crate) fn stream_event_name(stream_id: &str) -> String {
    format!("ai://stream/{stream_id}")
}

#[tauri::command]
pub async fn ai_complete(request: AiRequest, state: State<'_, AiState>) -> Result<String> {
    let (provider, feature, mut req) = request.into_parts();
    let (p, model) = state
        .service
        .resolve(provider.as_deref(), req.model.as_deref(), feature)?;
    req.model = model;
    Ok(p.complete(req).await?.text)
}

/// Start a streamed completion; deltas arrive as `ai://stream/<streamId>` events. The UI
/// must subscribe before invoking. Setup errors are reported both as the command's error
/// and as a final `{ error }` event.
#[tauri::command]
pub async fn ai_stream(
    request: AiRequest,
    stream_id: String,
    app: AppHandle,
    state: State<'_, AiState>,
) -> Result<()> {
    let event = stream_event_name(&stream_id);
    let emit = move |payload: StreamEvent| {
        if let Err(e) = app.emit(&event, payload) {
            tracing::warn!("ai stream emit failed: {e}");
        }
    };
    let (provider, feature, mut req) = request.into_parts();
    let started = state
        .service
        .resolve(provider.as_deref(), req.model.as_deref(), feature);
    let p = match started {
        Ok((p, model)) => {
            req.model = model;
            p
        }
        Err(e) => {
            emit(StreamEvent {
                error: Some(e.to_string()),
                ..Default::default()
            });
            return Err(e.into());
        }
    };
    start_stream(p, req, stream_id, emit, &state).await
}

/// Stream `req` from `p`, emitting deltas with `emit` until it ends or `ai_cancel` stops it.
/// Setup errors are reported both as the error and as a final `{ error }` event.
pub(crate) async fn start_stream(
    p: Box<dyn forge_ai::Provider>,
    req: CompletionRequest,
    stream_id: String,
    emit: impl Fn(StreamEvent) + Send + 'static,
    state: &AiState,
) -> Result<()> {
    let token = CancellationToken::new();
    state
        .streams
        .lock()
        .expect("streams lock")
        .insert(stream_id.clone(), token.clone());
    let stream = match p.stream(req).await {
        Ok(s) => cancellable(s, token),
        Err(e) => {
            state
                .streams
                .lock()
                .expect("streams lock")
                .remove(&stream_id);
            emit(StreamEvent {
                error: Some(e.to_string()),
                ..Default::default()
            });
            return Err(e.into());
        }
    };

    let streams = state.streams.clone();
    tauri::async_runtime::spawn(async move {
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            match item {
                Ok(Delta::Text { text }) => emit(StreamEvent {
                    delta: Some(text),
                    ..Default::default()
                }),
                Ok(Delta::Finish { .. }) | Err(forge_ai::AiError::Cancelled) => {
                    emit(StreamEvent {
                        done: Some(true),
                        ..Default::default()
                    });
                    break;
                }
                Err(e) => {
                    emit(StreamEvent {
                        error: Some(e.to_string()),
                        ..Default::default()
                    });
                    break;
                }
            }
        }
        streams.lock().expect("streams lock").remove(&stream_id);
    });
    Ok(())
}

#[tauri::command]
pub fn ai_cancel(stream_id: String, state: State<'_, AiState>) -> Result<()> {
    if let Some(token) = state
        .streams
        .lock()
        .expect("streams lock")
        .remove(&stream_id)
    {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn ai_providers_list(state: State<'_, AiState>) -> Result<ProvidersState> {
    Ok(state.service.state())
}

#[tauri::command]
pub async fn ai_provider_save(
    config: ProviderConfig,
    api_key: Option<String>,
    state: State<'_, AiState>,
) -> Result<ProviderConfig> {
    Ok(state.service.save_provider(config, api_key.as_deref())?)
}

#[tauri::command]
pub async fn ai_provider_delete(id: String, state: State<'_, AiState>) -> Result<()> {
    Ok(state.service.delete_provider(&id)?)
}

#[tauri::command]
pub async fn ai_provider_test(id: String, state: State<'_, AiState>) -> Result<TestResult> {
    Ok(state.service.test(&id).await?)
}

#[tauri::command]
pub async fn ai_models(provider_id: String, state: State<'_, AiState>) -> Result<Vec<ModelInfo>> {
    Ok(state.service.models(&provider_id).await?)
}

/// Default provider (+ its default model), or a feature's model when `feature` is given.
#[tauri::command]
pub async fn ai_set_default(
    provider_id: String,
    model: Option<String>,
    feature: Option<AiFeature>,
    state: State<'_, AiState>,
) -> Result<()> {
    Ok(state
        .service
        .set_default(&provider_id, model.as_deref(), feature)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_matches_shared_ipc_contract() {
        let req: AiRequest = serde_json::from_value(serde_json::json!({
            "system": "Write a commit message",
            "messages": [{ "role": "user", "content": "diff --git ..." }],
            "provider": "anthropic",
            "maxTokens": 200,
            "feature": "commitMessages"
        }))
        .unwrap();
        let (provider, feature, req) = req.into_parts();
        assert_eq!(provider.as_deref(), Some("anthropic"));
        assert_eq!(feature, Some(AiFeature::CommitMessages));
        assert_eq!(req.max_tokens, Some(200));
        assert_eq!(req.messages[0].text_content(), "diff --git ...");
    }

    #[test]
    fn stream_events_omit_empty_fields() {
        let ev = StreamEvent {
            delta: Some("Hi".into()),
            ..Default::default()
        };
        assert_eq!(serde_json::to_string(&ev).unwrap(), r#"{"delta":"Hi"}"#);
        assert_eq!(stream_event_name("s1"), "ai://stream/s1");
    }
}
