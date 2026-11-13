//! The Jupyter messaging protocol (v5.3): messages, HMAC-SHA256 signatures, the ZeroMQ wire
//! frames (`<IDS|MSG>`, signature, header, parent header, metadata, content, buffers), the
//! JSON form used over a Jupyter server's WebSocket, connection files and the kernel events
//! the notebook editor follows ([`KernelEvent`]).

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::Sha256;

use crate::output::{output_from_json, Output};

pub const PROTOCOL_VERSION: &str = "5.3";
pub const DELIMITER: &[u8] = b"<IDS|MSG>";

/// A message header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub msg_id: String,
    pub session: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub date: String,
    pub msg_type: String,
    #[serde(default)]
    pub version: String,
}

/// A message of any channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// ZeroMQ routing identities (before `<IDS|MSG>`).
    #[serde(skip)]
    pub identities: Vec<Vec<u8>>,
    pub header: Header,
    /// The request this message answers (an empty object when none).
    #[serde(default)]
    pub parent_header: Value,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub content: Value,
    /// Binary buffers (base64 in the WebSocket JSON form; usually none).
    #[serde(skip)]
    pub buffers: Vec<Vec<u8>>,
    /// WebSocket only: `shell`, `iopub`, `stdin` or `control`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

impl Message {
    /// A new request in `session`.
    pub fn new(session: &str, msg_id: &str, msg_type: &str, content: Value) -> Self {
        Message {
            identities: Vec::new(),
            header: Header {
                msg_id: msg_id.to_string(),
                session: session.to_string(),
                username: "forge".into(),
                date: now_iso8601(),
                msg_type: msg_type.to_string(),
                version: PROTOCOL_VERSION.into(),
            },
            parent_header: Value::Object(Map::new()),
            metadata: Value::Object(Map::new()),
            content,
            buffers: Vec::new(),
            channel: None,
        }
    }

    pub fn msg_type(&self) -> &str {
        &self.header.msg_type
    }

    /// `msg_id` of the request this message answers.
    pub fn parent_id(&self) -> Option<&str> {
        self.parent_header.get("msg_id").and_then(Value::as_str)
    }
}

/// `execute_request` content.
pub fn execute_content(code: &str, silent: bool, user_expressions: Option<Value>) -> Value {
    json!({
        "code": code,
        "silent": silent,
        "store_history": !silent,
        "user_expressions": user_expressions.unwrap_or_else(|| json!({})),
        "allow_stdin": false,
        "stop_on_error": true,
    })
}

/// Signs and checks messages with the connection's key (no key: no signatures).
#[derive(Clone)]
pub struct Signer {
    key: Vec<u8>,
}

type HmacSha256 = Hmac<Sha256>;

impl Signer {
    pub fn new(key: &str) -> Self {
        Signer {
            key: key.as_bytes().to_vec(),
        }
    }

    /// Hex HMAC-SHA256 over the four JSON parts (empty without a key).
    pub fn sign(&self, parts: &[&[u8]]) -> String {
        if self.key.is_empty() {
            return String::new();
        }
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC takes keys of any length");
        for p in parts {
            mac.update(p);
        }
        hex::encode(mac.finalize().into_bytes())
    }

    /// Constant-time check of a signature.
    pub fn verify(&self, signature: &[u8], parts: &[&[u8]]) -> bool {
        if self.key.is_empty() {
            return true;
        }
        let Ok(sig) = hex::decode(signature) else {
            return false;
        };
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC takes keys of any length");
        for p in parts {
            mac.update(p);
        }
        mac.verify_slice(&sig).is_ok()
    }
}

/// Protocol errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError(pub String);

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProtocolError {}

fn to_bytes(v: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(v).expect("JSON values serialize")
}

/// The ZeroMQ frames of a message.
pub fn encode(msg: &Message, signer: &Signer) -> Vec<Vec<u8>> {
    let header = to_bytes(&msg.header);
    let parent = to_bytes(&msg.parent_header);
    let metadata = to_bytes(&msg.metadata);
    let content = to_bytes(&msg.content);
    let sig = signer.sign(&[&header, &parent, &metadata, &content]);
    let mut frames = msg.identities.clone();
    frames.push(DELIMITER.to_vec());
    frames.push(sig.into_bytes());
    frames.push(header);
    frames.push(parent);
    frames.push(metadata);
    frames.push(content);
    frames.extend(msg.buffers.iter().cloned());
    frames
}

/// A message from its ZeroMQ frames; the signature must match.
pub fn decode(frames: &[Vec<u8>], signer: &Signer) -> Result<Message, ProtocolError> {
    let d = frames
        .iter()
        .position(|f| f == DELIMITER)
        .ok_or_else(|| ProtocolError("message without <IDS|MSG> delimiter".into()))?;
    if frames.len() < d + 6 {
        return Err(ProtocolError("message with missing parts".into()));
    }
    let [sig, header, parent, metadata, content] = [
        &frames[d + 1],
        &frames[d + 2],
        &frames[d + 3],
        &frames[d + 4],
        &frames[d + 5],
    ];
    if !signer.verify(sig, &[header, parent, metadata, content]) {
        return Err(ProtocolError("invalid message signature".into()));
    }
    let json = |b: &[u8]| -> Result<Value, ProtocolError> {
        serde_json::from_slice(b).map_err(|e| ProtocolError(format!("invalid message JSON: {e}")))
    };
    let header: Header = serde_json::from_slice(header)
        .map_err(|e| ProtocolError(format!("invalid message header: {e}")))?;
    Ok(Message {
        identities: frames[..d].to_vec(),
        header,
        parent_header: json(parent)?,
        metadata: json(metadata)?,
        content: json(content)?,
        buffers: frames[d + 6..].to_vec(),
        channel: None,
    })
}

/// A kernel connection file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub ip: String,
    pub transport: String,
    pub shell_port: u16,
    pub iopub_port: u16,
    pub stdin_port: u16,
    pub control_port: u16,
    pub hb_port: u16,
    pub key: String,
    pub signature_scheme: String,
    #[serde(default)]
    pub kernel_name: String,
}

impl ConnectionInfo {
    /// A connection on 127.0.0.1 with the given ports (shell, iopub, stdin, control, hb).
    pub fn local(ports: [u16; 5], key: &str) -> Self {
        ConnectionInfo {
            ip: "127.0.0.1".into(),
            transport: "tcp".into(),
            shell_port: ports[0],
            iopub_port: ports[1],
            stdin_port: ports[2],
            control_port: ports[3],
            hb_port: ports[4],
            key: key.into(),
            signature_scheme: "hmac-sha256".into(),
            kernel_name: String::new(),
        }
    }

    pub fn endpoint(&self, port: u16) -> String {
        format!("{}://{}:{}", self.transport, self.ip, port)
    }
}

/// What the kernel's messages mean for the notebook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum KernelEvent {
    /// `starting`, `busy`, `idle`, `restarting`, `dead`; `parent` is the request.
    Status {
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
    },
    ExecuteInput {
        parent: String,
        execution_count: Option<u64>,
    },
    Output {
        parent: String,
        output: Output,
        /// `display_id` of a display that can be updated later.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_id: Option<String>,
    },
    /// `update_display_data`: replace every output with this display id.
    UpdateDisplay {
        display_id: String,
        output: Output,
    },
    ClearOutput {
        parent: String,
        wait: bool,
    },
    /// `execute_reply`: `ok`, `error` or `aborted`.
    ExecuteReply {
        parent: String,
        status: String,
        execution_count: Option<u64>,
    },
}

/// The notebook-relevant meaning of a message from iopub or shell.
pub fn event_of(msg: &Message) -> Option<KernelEvent> {
    let parent = msg.parent_id().unwrap_or("").to_string();
    let c = &msg.content;
    let display_id = || {
        c.pointer("/transient/display_id")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let output = || {
        let mut v = c.clone();
        if let Value::Object(m) = &mut v {
            m.insert("output_type".into(), Value::from(msg.msg_type()));
            m.remove("transient");
        }
        output_from_json(&v)
    };
    Some(match msg.msg_type() {
        "status" => KernelEvent::Status {
            state: c
                .get("execution_state")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            parent: msg.parent_id().map(str::to_string),
        },
        "execute_input" => KernelEvent::ExecuteInput {
            parent,
            execution_count: c.get("execution_count").and_then(Value::as_u64),
        },
        "stream" | "execute_result" | "display_data" | "error" => KernelEvent::Output {
            parent,
            output: output(),
            display_id: display_id(),
        },
        "update_display_data" => {
            let mut v = c.clone();
            if let Value::Object(m) = &mut v {
                m.insert("output_type".into(), Value::from("display_data"));
            }
            KernelEvent::UpdateDisplay {
                display_id: display_id()?,
                output: output_from_json(&v),
            }
        }
        "clear_output" => KernelEvent::ClearOutput {
            parent,
            wait: c.get("wait").and_then(Value::as_bool).unwrap_or(false),
        },
        "execute_reply" => KernelEvent::ExecuteReply {
            parent,
            status: c
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("ok")
                .to_string(),
            execution_count: c.get("execution_count").and_then(Value::as_u64),
        },
        _ => return None,
    })
}

/// UTC now as `2024-05-01T12:00:00.123456Z`.
pub fn now_iso8601() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    iso8601(now.as_secs(), now.subsec_micros())
}

fn iso8601(secs: u64, micros: u32) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{micros:06}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// A Python expression whose `text/plain` representation is the JSON of `expr`'s value
/// (IPython prints objects with their `__repr__`, so the reply needs no unquoting).
pub fn json_expression(expr: &str) -> String {
    format!("__forge_jupyter.as_json({expr})")
}

/// The JSON a `user_expressions` entry of an `execute_reply` evaluated to.
pub fn user_expression_json(reply: &Value, name: &str) -> Result<Value, ProtocolError> {
    let e = reply
        .pointer(&format!("/user_expressions/{name}"))
        .ok_or_else(|| ProtocolError("the kernel did not evaluate the expression".into()))?;
    if e.get("status").and_then(Value::as_str) == Some("error") {
        return Err(ProtocolError(format!(
            "{}: {}",
            e.get("ename").and_then(Value::as_str).unwrap_or("Error"),
            e.get("evalue").and_then(Value::as_str).unwrap_or("")
        )));
    }
    let text = e
        .pointer("/data/text~1plain")
        .and_then(Value::as_str)
        .ok_or_else(|| ProtocolError("the expression has no text value".into()))?;
    serde_json::from_str(text).map_err(|err| ProtocolError(format!("unexpected reply: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_like_jupyter_client() {
        // jupyter_client: hmac.new(b"key", digestmod=sha256), updated with the four parts.
        let s = Signer::new("key");
        let sig = s.sign(&[b"{}", b"{}", b"{}", b"{}"]);
        assert_eq!(sig.len(), 64);
        let mut mac = HmacSha256::new_from_slice(b"key").unwrap();
        mac.update(b"{}{}{}{}");
        assert_eq!(sig, hex::encode(mac.finalize().into_bytes()));
        assert!(s.verify(sig.as_bytes(), &[b"{}", b"{}", b"{}", b"{}"]));
        assert!(!s.verify(sig.as_bytes(), &[b"{}", b"{}", b"{}", b"{ }"]));
        assert!(!s.verify(b"zz", &[b"{}"]));
        assert_eq!(Signer::new("").sign(&[b"x"]), "");
    }

    #[test]
    fn encodes_and_decodes_frames() {
        let s = Signer::new("secret");
        let mut m = Message::new(
            "sess",
            "m1",
            "execute_request",
            execute_content("1+1", false, None),
        );
        m.identities = vec![b"peer".to_vec()];
        let frames = encode(&m, &s);
        assert_eq!(frames[0], b"peer");
        assert_eq!(frames[1], DELIMITER);
        let back = decode(&frames, &s).unwrap();
        assert_eq!(back, m);
        let mut bad = frames.clone();
        bad[6] = br#"{"code":"rm -rf"}"#.to_vec();
        assert!(decode(&bad, &s).unwrap_err().0.contains("signature"));
        assert!(decode(&frames, &Signer::new("other")).is_err());
        assert!(decode(&frames[..4], &s).is_err());
    }

    #[test]
    fn reads_kernel_events() {
        let mut m = Message::new(
            "s",
            "o1",
            "stream",
            json!({"name": "stdout", "text": "hi\n"}),
        );
        m.parent_header = json!({"msg_id": "req"});
        assert_eq!(
            event_of(&m),
            Some(KernelEvent::Output {
                parent: "req".into(),
                output: Output::Stream {
                    name: "stdout".into(),
                    text: "hi\n".into()
                },
                display_id: None
            })
        );
        let m = Message::new(
            "s",
            "o2",
            "display_data",
            json!({"data": {"text/plain": "a"}, "metadata": {}, "transient": {"display_id": "d1"}}),
        );
        assert!(
            matches!(event_of(&m), Some(KernelEvent::Output { display_id: Some(d), .. }) if d == "d1")
        );
        let m = Message::new(
            "s",
            "o3",
            "execute_reply",
            json!({"status": "error", "execution_count": 3}),
        );
        assert!(
            matches!(event_of(&m), Some(KernelEvent::ExecuteReply { status, execution_count: Some(3), .. }) if status == "error")
        );
        let ws: Message = serde_json::from_str(r#"{"header": {"msg_id": "x", "session": "s", "msg_type": "status"}, "parent_header": {}, "metadata": {}, "content": {"execution_state": "idle"}, "channel": "iopub"}"#).unwrap();
        assert_eq!(
            event_of(&ws),
            Some(KernelEvent::Status {
                state: "idle".into(),
                parent: None
            })
        );
    }

    #[test]
    fn formats_dates_and_reads_user_expressions() {
        assert_eq!(iso8601(0, 0), "1970-01-01T00:00:00.000000Z");
        assert_eq!(iso8601(1_709_210_096, 5), "2024-02-29T12:34:56.000005Z");
        let reply = json!({"user_expressions": {"v": {"status": "ok", "data": {"text/plain": "[{\"name\": \"df\"}]"}, "metadata": {}}}});
        assert_eq!(
            user_expression_json(&reply, "v").unwrap(),
            json!([{"name": "df"}])
        );
        let err = json!({"user_expressions": {"v": {"status": "error", "ename": "NameError", "evalue": "x"}}});
        assert_eq!(
            user_expression_json(&err, "v").unwrap_err().0,
            "NameError: x"
        );
    }
}
