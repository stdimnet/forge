//! OpenAI-compatible chat completions (`POST {base}/chat/completions`): OpenAI, OpenRouter,
//! LM Studio, Ollama, llama.cpp server and any other compatible endpoint.

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::error::{AiError, AiResult};
use crate::provider::{error_message, DeltaStream, Endpoint, Provider};
use crate::sse::sse_events;
use crate::types::{Completion, CompletionRequest, Delta, ModelInfo, Usage};

pub struct OpenAiCompatProvider {
    ep: Endpoint,
}

impl OpenAiCompatProvider {
    pub fn new(ep: Endpoint) -> Self {
        Self { ep }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> AiResult<reqwest::RequestBuilder> {
        let mut rb = self.ep.client.request(method, self.ep.url(path));
        match self.ep.api_key.as_deref() {
            Some(key) => rb = rb.bearer_auth(key),
            None if self.ep.kind.requires_api_key() => {
                self.ep.require_key()?;
            }
            None => {}
        }
        Ok(self.ep.with_headers(rb))
    }

    fn body(&self, req: &CompletionRequest, stream: bool) -> AiResult<Value> {
        let mut messages = Vec::with_capacity(req.messages.len() + 1);
        if let Some(system) = req.system.as_deref().filter(|s| !s.is_empty()) {
            messages.push(json!({ "role": "system", "content": system }));
        }
        for m in &req.messages {
            messages.push(json!({ "role": m.role.as_str(), "content": m.text_content() }));
        }
        let mut body = json!({
            "model": self.ep.model_for(req)?,
            "messages": messages,
        });
        if let Some(n) = req.max_tokens {
            body["max_tokens"] = json!(n);
        }
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        if !req.stop.is_empty() {
            body["stop"] = json!(req.stop);
        }
        if stream {
            body["stream"] = json!(true);
        }
        Ok(body)
    }

    fn api_error(&self, v: &Value) -> AiError {
        AiError::Api {
            provider: self.ep.name.clone(),
            message: error_message(&v.to_string()).unwrap_or_else(|| v.to_string()),
        }
    }
}

fn usage(v: &Value) -> Option<Usage> {
    let u = v.get("usage").filter(|u| u.is_object())?;
    Some(Usage {
        input_tokens: u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
        output_tokens: u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
    })
}

#[async_trait]
impl Provider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        &self.ep.name
    }

    fn default_model(&self) -> Option<&str> {
        self.ep.default_model.as_deref()
    }

    async fn list_models(&self) -> AiResult<Vec<ModelInfo>> {
        let rb = self.request(reqwest::Method::GET, "models")?;
        let v: Value = self
            .ep
            .send(rb)
            .await?
            .json()
            .await
            .map_err(|e| AiError::InvalidResponse(e.to_string()))?;
        let data = v
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| AiError::InvalidResponse("missing `data` in model list".into()))?;
        let mut models: Vec<ModelInfo> = data
            .iter()
            .filter_map(|m| {
                Some(ModelInfo {
                    id: m.get("id")?.as_str()?.into(),
                    name: m.get("name").and_then(Value::as_str).map(Into::into),
                })
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    async fn complete(&self, request: CompletionRequest) -> AiResult<Completion> {
        let body = self.body(&request, false)?;
        let rb = self
            .request(reqwest::Method::POST, "chat/completions")?
            .json(&body);
        let v: Value = self
            .ep
            .send(rb)
            .await?
            .json()
            .await
            .map_err(|e| AiError::InvalidResponse(e.to_string()))?;
        if v.get("error").is_some() {
            return Err(self.api_error(&v));
        }
        let choice = v
            .get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| AiError::InvalidResponse("missing `choices`".into()))?;
        Ok(Completion {
            text: choice["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .into(),
            model: v
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            stop_reason: choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .map(Into::into),
            usage: usage(&v),
        })
    }

    async fn stream(&self, request: CompletionRequest) -> AiResult<DeltaStream> {
        let body = self.body(&request, true)?;
        let rb = self
            .request(reqwest::Method::POST, "chat/completions")?
            .header("accept", "text/event-stream")
            .json(&body);
        let resp = self.ep.send(rb).await?;
        let this = OpenAiCompatProvider {
            ep: self.ep.clone(),
        };
        Ok(Box::pin(async_stream::stream! {
            let events = sse_events(resp.bytes_stream());
            let mut events = std::pin::pin!(events);
            let mut stop_reason: Option<String> = None;
            let mut last_usage: Option<Usage> = None;
            while let Some(ev) = events.next().await {
                let ev = match ev {
                    Ok(ev) => ev,
                    Err(e) => { yield Err(this.ep.transport_error(e)); return; }
                };
                let data = ev.data.trim();
                if data.is_empty() { continue; }
                if data == "[DONE]" {
                    yield Ok(Delta::Finish { stop_reason: stop_reason.take(), usage: last_usage });
                    return;
                }
                let v: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(e) => { yield Err(AiError::InvalidResponse(format!("{e}: {data}"))); return; }
                };
                if v.get("error").is_some() || ev.event == "error" {
                    yield Err(this.api_error(&v));
                    return;
                }
                if let Some(u) = usage(&v) {
                    last_usage = Some(u);
                }
                if let Some(choice) = v.get("choices").and_then(|c| c.get(0)) {
                    if let Some(t) = choice["delta"].get("content").and_then(Value::as_str) {
                        if !t.is_empty() {
                            yield Ok(Delta::Text { text: t.to_string() });
                        }
                    }
                    if let Some(r) = choice.get("finish_reason").and_then(Value::as_str) {
                        stop_reason = Some(r.into());
                    }
                }
            }
            // Some servers close without `[DONE]`; treat a seen finish_reason as success.
            if stop_reason.is_some() {
                yield Ok(Delta::Finish { stop_reason, usage: last_usage });
            } else {
                yield Err(AiError::InvalidResponse("stream ended unexpectedly".into()));
            }
        }))
    }
}
