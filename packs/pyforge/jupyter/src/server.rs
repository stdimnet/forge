//! Kernels of a running Jupyter server (JupyterLab, Notebook 7, `jupyter server`): started
//! and controlled with the REST API (`/api/kernels`), messages over the kernel's WebSocket
//! (`/api/kernels/<id>/channels`, JSON with a `channel` field). The token goes in the
//! `Authorization: token …` header.

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::protocol::Message;
use crate::transport::{disconnected, Channel, Incoming, TransportError};

pub use crate::serverurl::{split_server_url, ServerAddress};

/// A kernel spec the server offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelSpecInfo {
    pub name: String,
    pub display_name: String,
    pub language: String,
}

fn err(e: impl std::fmt::Display) -> TransportError {
    TransportError(e.to_string())
}

fn client() -> Result<reqwest::Client, TransportError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(err)
}

async fn call(
    method: reqwest::Method,
    server: &ServerAddress,
    path: &str,
    body: Option<Value>,
) -> Result<Value, TransportError> {
    let url = format!("{}{}", server.base, path);
    let mut req = client()?.request(method, &url);
    if !server.token.is_empty() {
        req = req.header("Authorization", format!("token {}", server.token));
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await.map_err(|e| {
        TransportError(format!(
            "cannot reach the Jupyter server at {}: {e}",
            server.base
        ))
    })?;
    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(TransportError(
            "the Jupyter server refused the token".into(),
        ));
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(TransportError(format!(
            "the Jupyter server answered {status}: {}",
            text.trim()
        )));
    }
    let text = resp.text().await.map_err(err)?;
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text)
        .map_err(|e| TransportError(format!("unexpected answer from the Jupyter server: {e}")))
}

/// The kernel specs of a server (also a connection test).
pub async fn kernel_specs(
    server: &ServerAddress,
) -> Result<(String, Vec<KernelSpecInfo>), TransportError> {
    let v = call(reqwest::Method::GET, server, "/api/kernelspecs", None).await?;
    let default = v
        .get("default")
        .and_then(Value::as_str)
        .unwrap_or("python3")
        .to_string();
    let mut specs: Vec<KernelSpecInfo> = v
        .get("kernelspecs")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(name, s)| KernelSpecInfo {
                    name: name.clone(),
                    display_name: s
                        .pointer("/spec/display_name")
                        .and_then(Value::as_str)
                        .unwrap_or(name)
                        .to_string(),
                    language: s
                        .pointer("/spec/language")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    specs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((default, specs))
}

/// A kernel started on a Jupyter server.
pub struct ServerKernel {
    server: ServerAddress,
    pub kernel_id: String,
    out: mpsc::UnboundedSender<String>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for ServerKernel {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl ServerKernel {
    /// Start a kernel (`kernel_name` empty = the server's default) and connect to it.
    pub async fn start(
        server: ServerAddress,
        kernel_name: &str,
        session: &str,
        tx: Incoming,
    ) -> Result<Self, TransportError> {
        let body = if kernel_name.is_empty() {
            json!({})
        } else {
            json!({ "name": kernel_name })
        };
        let created = call(reqwest::Method::POST, &server, "/api/kernels", Some(body)).await?;
        let kernel_id = created
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| TransportError("the Jupyter server did not start a kernel".into()))?
            .to_string();
        let ws_url = format!(
            "{}/api/kernels/{kernel_id}/channels?session_id={session}",
            server.base.replacen("http", "ws", 1)
        );
        let mut request = ws_url.into_client_request().map_err(err)?;
        if !server.token.is_empty() {
            let value = HeaderValue::from_str(&format!("token {}", server.token)).map_err(err)?;
            request.headers_mut().insert("Authorization", value);
        }
        let (ws, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| {
                TransportError(format!("cannot connect to the kernel's WebSocket: {e}"))
            })?;
        let (mut sink, mut stream) = ws.split();
        let (out, mut out_rx) = mpsc::unbounded_channel::<String>();
        let writer = tokio::spawn(async move {
            while let Some(text) = out_rx.recv().await {
                if sink.send(WsMessage::Text(text.into())).await.is_err() {
                    return;
                }
            }
        });
        let reader = tokio::spawn(async move {
            while let Some(frame) = stream.next().await {
                let text = match frame {
                    Ok(WsMessage::Text(t)) => t.to_string(),
                    Ok(WsMessage::Close(_)) | Err(_) => break,
                    Ok(_) => continue,
                };
                let Ok(msg) = serde_json::from_str::<Message>(&text) else {
                    continue;
                };
                let channel = msg
                    .channel
                    .as_deref()
                    .and_then(Channel::parse)
                    .unwrap_or(Channel::Iopub);
                if tx.send((channel, msg)).is_err() {
                    return;
                }
            }
            let _ = tx.send((Channel::Iopub, disconnected()));
        });
        Ok(ServerKernel {
            server,
            kernel_id,
            out,
            tasks: vec![writer, reader],
        })
    }

    pub fn send(&self, channel: Channel, msg: &Message) -> Result<(), TransportError> {
        let mut m = msg.clone();
        m.channel = Some(channel.name().into());
        let text = serde_json::to_string(&m).map_err(err)?;
        self.out
            .send(text)
            .map_err(|_| TransportError("the connection to the Jupyter server is closed".into()))
    }

    pub async fn interrupt(&self) -> Result<(), TransportError> {
        call(
            reqwest::Method::POST,
            &self.server,
            &format!("/api/kernels/{}/interrupt", self.kernel_id),
            None,
        )
        .await
        .map(|_| ())
    }

    pub async fn restart(&self) -> Result<(), TransportError> {
        call(
            reqwest::Method::POST,
            &self.server,
            &format!("/api/kernels/{}/restart", self.kernel_id),
            None,
        )
        .await
        .map(|_| ())
    }

    pub async fn shutdown(&self) -> Result<(), TransportError> {
        call(
            reqwest::Method::DELETE,
            &self.server,
            &format!("/api/kernels/{}", self.kernel_id),
            None,
        )
        .await
        .map(|_| ())
    }
}
