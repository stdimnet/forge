//! JSON-RPC 2.0 messages as MCP uses them: requests, notifications and responses, single or
//! batched (batches were allowed by protocol version 2025-03-26 and are still accepted).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
/// MCP: `resources/read` of an unknown resource.
pub const RESOURCE_NOT_FOUND: i64 = -32002;

/// A JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(METHOD_NOT_FOUND, format!("Method not found: {method}"))
    }
}

/// One incoming message.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Expects a response with the same `id`.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// No `id`, no response.
    Notification { method: String, params: Value },
    /// A response to a request the server sent (Forge sends none; it is ignored).
    Response { id: Value },
}

impl Message {
    pub fn method(&self) -> Option<&str> {
        match self {
            Message::Request { method, .. } | Message::Notification { method, .. } => Some(method),
            Message::Response { .. } => None,
        }
    }

    pub fn is_request(&self) -> bool {
        matches!(self, Message::Request { .. })
    }
}

/// What a POST body contained.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Single(Message),
    Batch(Vec<Message>),
}

impl Incoming {
    pub fn messages(&self) -> Vec<&Message> {
        match self {
            Incoming::Single(m) => vec![m],
            Incoming::Batch(b) => b.iter().collect(),
        }
    }
}

fn valid_id(id: &Value) -> bool {
    matches!(id, Value::String(_) | Value::Number(_))
}

/// Parse one message object; `Err` carries the id (if any) for the error response.
pub fn parse_value(value: &Value) -> Result<Message, (Value, RpcError)> {
    let invalid = |id: Value, why: &str| Err((id, RpcError::new(INVALID_REQUEST, why.to_string())));
    let Some(obj) = value.as_object() else {
        return invalid(Value::Null, "a message must be an object");
    };
    let id = obj.get("id").cloned();
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid(id.unwrap_or(Value::Null), "jsonrpc must be \"2.0\"");
    }
    if let Some(id) = &id {
        if !valid_id(id) {
            return invalid(Value::Null, "id must be a string or a number");
        }
    }
    match obj.get("method") {
        Some(Value::String(method)) => {
            let params = obj.get("params").cloned().unwrap_or(Value::Null);
            if !matches!(params, Value::Null | Value::Object(_) | Value::Array(_)) {
                return invalid(id.unwrap_or(Value::Null), "params must be an object");
            }
            Ok(match id {
                Some(id) => Message::Request {
                    id,
                    method: method.clone(),
                    params,
                },
                None => Message::Notification {
                    method: method.clone(),
                    params,
                },
            })
        }
        Some(_) => invalid(id.unwrap_or(Value::Null), "method must be a string"),
        None if obj.contains_key("result") || obj.contains_key("error") => match id {
            Some(id) => Ok(Message::Response { id }),
            None => invalid(Value::Null, "a response needs an id"),
        },
        None => invalid(id.unwrap_or(Value::Null), "method is missing"),
    }
}

/// Parse a body: an object or a non-empty array of objects. A body that is not JSON yields a
/// parse error response.
pub fn parse(body: &[u8]) -> Result<Incoming, Value> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| error_response(Value::Null, &RpcError::new(PARSE_ERROR, e.to_string())))?;
    match value {
        Value::Array(items) if items.is_empty() => Err(error_response(
            Value::Null,
            &RpcError::new(INVALID_REQUEST, "empty batch"),
        )),
        Value::Array(items) => items
            .iter()
            .map(|v| parse_value(v).map_err(|(id, e)| error_response(id, &e)))
            .collect::<Result<Vec<_>, _>>()
            .map(Incoming::Batch),
        other => parse_value(&other)
            .map(Incoming::Single)
            .map_err(|(id, e)| error_response(id, &e)),
    }
}

pub fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn error_response(id: Value, error: &RpcError) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

/// A request the client sends (used by the in-process test client and the proxy's errors).
pub fn request(id: impl Into<Value>, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.into(), "method": method, "params": params })
}

pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requests_notifications_and_responses() {
        let r = parse(br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        assert_eq!(
            r,
            Incoming::Single(Message::Request {
                id: json!(1),
                method: "tools/list".into(),
                params: Value::Null
            })
        );
        let n = parse(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap();
        assert!(matches!(n, Incoming::Single(Message::Notification { .. })));
        let resp = parse(br#"{"jsonrpc":"2.0","id":"a","result":{}}"#).unwrap();
        assert_eq!(resp, Incoming::Single(Message::Response { id: json!("a") }));
        let batch = parse(
            br#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"x"}]"#,
        )
        .unwrap();
        assert_eq!(batch.messages().len(), 2);
    }

    #[test]
    fn reports_invalid_messages() {
        let err = parse(b"{not json").unwrap_err();
        assert_eq!(err["error"]["code"], PARSE_ERROR);
        let err = parse(br#"{"jsonrpc":"1.0","id":3,"method":"ping"}"#).unwrap_err();
        assert_eq!(err["error"]["code"], INVALID_REQUEST);
        assert_eq!(err["id"], 3);
        let err = parse(br#"{"jsonrpc":"2.0","id":{"x":1},"method":"ping"}"#).unwrap_err();
        assert_eq!(err["error"]["code"], INVALID_REQUEST);
        let err = parse(b"[]").unwrap_err();
        assert_eq!(err["error"]["code"], INVALID_REQUEST);
        let err = parse(br#"{"jsonrpc":"2.0","id":1,"method":"x","params":3}"#).unwrap_err();
        assert_eq!(err["error"]["code"], INVALID_REQUEST);
    }

    #[test]
    fn builds_responses() {
        let ok = result_response(json!(7), json!({ "a": 1 }));
        assert_eq!(ok, json!({ "jsonrpc": "2.0", "id": 7, "result": { "a": 1 } }));
        let err = error_response(json!("x"), &RpcError::method_not_found("nope"));
        assert_eq!(err["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(err["error"]["message"], "Method not found: nope");
        assert!(err["error"].get("data").is_none());
    }
}
