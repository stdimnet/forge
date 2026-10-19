//! Anthropic Messages API (`POST /v1/messages`).

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::error::{AiError, AiResult};
use crate::provider::{DeltaStream, Endpoint, Provider, DEFAULT_MAX_TOKENS};
use crate::sse::sse_events;
use crate::types::{Completion, CompletionRequest, ContentPart, Delta, ModelInfo, Usage};

pub const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    ep: Endpoint,
}

impl AnthropicProvider {
    pub fn new(ep: Endpoint) -> Self {
        Self { ep }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> AiResult<reqwest::RequestBuilder> {
        let key = self.ep.require_key()?;
        let rb = self
            .ep
            .client
            .request(method, self.ep.url(path))
            .header("x-api-key", key)
            .header("anthropic-version", ANTHROPIC_VERSION);
        Ok(self.ep.with_headers(rb))
    }

    fn body(&self, req: &CompletionRequest, stream: bool) -> AiResult<Value> {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                let content: Vec<Value> = m
                    .content
                    .iter()
                    .map(|p| match p {
                        ContentPart::Text { text } => json!({ "type": "text", "text": text }),
                    })
                    .collect();
                json!({ "role": m.role.as_str(), "content": content })
            })
            .collect();
        let mut body = json!({
            "model": self.ep.model_for(req)?,
            "max_tokens": req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            "messages": messages,
        });
        if let Some(system) = req.system.as_deref().filter(|s| !s.is_empty()) {
            body["system"] = json!(system);
        }
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        if !req.stop.is_empty() {
            body["stop_sequences"] = json!(req.stop);
        }
        if stream {
            body["stream"] = json!(true);
        }
        Ok(body)
    }

    fn api_error(&self, v: &Value) -> AiError {
        let err = v.get("error").unwrap_or(v);
        let kind = err.get("type").and_then(Value::as_str).unwrap_or("error");
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        match kind {
            "authentication_error" => AiError::InvalidApiKey {
                provider: self.ep.name.clone(),
            },
            "rate_limit_error" => AiError::RateLimited {
                provider: self.ep.name.clone(),
                message: Some(message.into()),
            },
            "overloaded_error" => AiError::Api {
                provider: self.ep.name.clone(),
                message: format!("overloaded, try again later ({message})"),
            },
            _ => AiError::Api {
                provider: self.ep.name.clone(),
                message: message.into(),
            },
        }
    }
}

fn usage(v: &Value) -> Option<Usage> {
    let u = v.get("usage")?;
    Some(Usage {
        input_tokens: u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
        output_tokens: u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
    })
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.ep.name
    }

    fn default_model(&self) -> Option<&str> {
        self.ep.default_model.as_deref()
    }

    async fn list_models(&self) -> AiResult<Vec<ModelInfo>> {
        let rb = self
            .request(reqwest::Method::GET, "models")?
            .query(&[("limit", "1000")]);
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
        Ok(data
            .iter()
            .filter_map(|m| {
                Some(ModelInfo {
                    id: m.get("id")?.as_str()?.into(),
                    name: m.get("display_name").and_then(Value::as_str).map(Into::into),
                })
            })
            .collect())
    }

    async fn complete(&self, request: CompletionRequest) -> AiResult<Completion> {
        let body = self.body(&request, false)?;
        let rb = self.request(reqwest::Method::POST, "messages")?.json(&body);
        let v: Value = self
            .ep
            .send(rb)
            .await?
            .json()
            .await
            .map_err(|e| AiError::InvalidResponse(e.to_string()))?;
        if v.get("type").and_then(Value::as_str) == Some("error") {
            return Err(self.api_error(&v));
        }
        let text = v
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<String>()
            })
            .ok_or_else(|| AiError::InvalidResponse("missing `content`".into()))?;
        Ok(Completion {
            text,
            model: v
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            stop_reason: v.get("stop_reason").and_then(Value::as_str).map(Into::into),
            usage: usage(&v),
        })
    }

    async fn stream(&self, request: CompletionRequest) -> AiResult<DeltaStream> {
        let body = self.body(&request, true)?;
        let rb = self
            .request(reqwest::Method::POST, "messages")?
            .header("accept", "text/event-stream")
            .json(&body);
        let resp = self.ep.send(rb).await?;
        let ep = self.ep.clone();
        let this = AnthropicProvider { ep: ep.clone() };
        Ok(Box::pin(async_stream::stream! {
            let events = sse_events(resp.bytes_stream());
            let mut events = std::pin::pin!(events);
            let mut input_tokens = 0u32;
            let mut stop_reason: Option<String> = None;
            let mut output_tokens = 0u32;
            while let Some(ev) = events.next().await {
                let ev = match ev {
                    Ok(ev) => ev,
                    Err(e) => { yield Err(ep.transport_error(e)); return; }
                };
                if ev.data.is_empty() { continue; }
                let v: Value = match serde_json::from_str(&ev.data) {
                    Ok(v) => v,
                    Err(e) => { yield Err(AiError::InvalidResponse(format!("{e}: {}", ev.data))); return; }
                };
                let kind = if ev.event.is_empty() {
                    v.get("type").and_then(Value::as_str).unwrap_or_default().to_string()
                } else {
                    ev.event.clone()
                };
                match kind.as_str() {
                    "message_start" => {
                        if let Some(u) = v.get("message").and_then(usage) {
                            input_tokens = u.input_tokens;
                        }
                    }
                    "content_block_delta" => {
                        let d = &v["delta"];
                        if d.get("type").and_then(Value::as_str) == Some("text_delta") {
                            if let Some(t) = d.get("text").and_then(Value::as_str) {
                                yield Ok(Delta::Text { text: t.to_string() });
                            }
                        }
                    }
                    "message_delta" => {
                        if let Some(r) = v["delta"].get("stop_reason").and_then(Value::as_str) {
                            stop_reason = Some(r.into());
                        }
                        if let Some(u) = usage(&v) {
                            output_tokens = u.output_tokens;
                        }
                    }
                    "message_stop" => {
                        yield Ok(Delta::Finish {
                            stop_reason: stop_reason.take(),
                            usage: Some(Usage { input_tokens, output_tokens }),
                        });
                        return;
                    }
                    "error" => { yield Err(this.api_error(&v)); return; }
                    _ => {} // ping, content_block_start/stop
                }
            }
            yield Err(AiError::InvalidResponse("stream ended before message_stop".into()));
        }))
    }
}
