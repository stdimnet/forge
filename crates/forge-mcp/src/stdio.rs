//! The stdio proxy (`forge --mcp-stdio`) for clients that only start stdio servers (Claude
//! Desktop, Codex): newline-delimited JSON-RPC from stdin is posted to the running Forge's
//! Streamable HTTP endpoint and the answers are written to stdout. The running instance is
//! found through the discovery file the IDE writes when its server starts; the token comes
//! from `FORGE_MCP_TOKEN` or the OS keychain.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::http;
use crate::server::{transport_error, ENDPOINT, SESSION_HEADER};

pub const TOKEN_ENV: &str = "FORGE_MCP_TOKEN";

/// What the running Forge publishes about its server (no secrets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub port: u16,
    pub pid: u32,
    pub version: String,
}

/// `<config dir>/forge/mcp-server.json`.
pub fn discovery_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("forge").join("mcp-server.json"))
}

pub fn write_discovery(path: &Path, discovery: &Discovery) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(discovery).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

pub fn read_discovery(path: &Path) -> Option<Discovery> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Remove the file if it still describes this process's server.
pub fn remove_discovery(path: &Path, pid: u32) {
    if read_discovery(path).is_some_and(|d| d.pid == pid) {
        let _ = std::fs::remove_file(path);
    }
}

/// Where the proxy sends messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub port: u16,
    pub token: String,
}

/// Ids of the requests in a line (to answer them when Forge cannot be reached).
fn request_ids(line: &str) -> Vec<Value> {
    let ids = |v: &Value| {
        v.get("method")
            .and(v.get("id"))
            .filter(|id| !id.is_null())
            .cloned()
    };
    match serde_json::from_str::<Value>(line) {
        Ok(Value::Array(items)) => items.iter().filter_map(ids).collect(),
        Ok(v) => ids(&v).into_iter().collect(),
        Err(_) => Vec::new(),
    }
}

fn is_initialize(line: &str) -> bool {
    let is_init = |v: &Value| v["method"] == "initialize";
    match serde_json::from_str::<Value>(line) {
        Ok(Value::Array(items)) => items.iter().any(is_init),
        Ok(v) => is_init(&v),
        Err(_) => false,
    }
}

async fn write_line<W: AsyncWrite + Unpin>(out: &mut W, text: &str) -> std::io::Result<()> {
    out.write_all(text.trim_end().as_bytes()).await?;
    out.write_all(b"\n").await?;
    out.flush().await
}

/// Forward until stdin ends, then end the session. `resolve` finds the running Forge; it is
/// asked again after a failure, so the proxy recovers when Forge starts later.
pub async fn run_proxy<R, W, F>(input: R, mut output: W, resolve: F) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
    F: Fn() -> Result<Target, String>,
{
    let mut lines = input.lines();
    let mut target: Option<Target> = None;
    let mut session: Option<String> = None;
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let t = match target.clone().map(Ok).unwrap_or_else(&resolve) {
            Ok(t) => t,
            Err(e) => {
                for id in request_ids(&line) {
                    write_line(&mut output, &transport_error(id, &e).to_string()).await?;
                }
                continue;
            }
        };
        let auth = format!("Bearer {}", t.token);
        let mut headers = vec![
            ("Authorization", auth.as_str()),
            ("Content-Type", "application/json"),
            ("Accept", "application/json, text/event-stream"),
        ];
        let initializes = is_initialize(&line);
        if let (Some(s), false) = (&session, initializes) {
            headers.push((SESSION_HEADER, s.as_str()));
        }
        match http::send(t.port, "POST", ENDPOINT, &headers, line.as_bytes()).await {
            Ok(resp) => {
                target = Some(t);
                if let Some(id) = resp.header(SESSION_HEADER) {
                    session = Some(id.to_string());
                }
                match resp.status {
                    200 => write_line(&mut output, &String::from_utf8_lossy(&resp.body)).await?,
                    202 => {}
                    status => {
                        let message = serde_json::from_slice::<Value>(&resp.body)
                            .ok()
                            .and_then(|v| v["error"].as_str().map(str::to_string))
                            .unwrap_or_else(|| format!("Forge answered HTTP {status}"));
                        for id in request_ids(&line) {
                            write_line(&mut output, &transport_error(id, &message).to_string())
                                .await?;
                        }
                    }
                }
            }
            Err(e) => {
                target = None;
                let message = format!("Cannot reach Forge's MCP server on port {}: {e}. Is Forge running with the MCP server enabled?", t.port);
                for id in request_ids(&line) {
                    write_line(&mut output, &transport_error(id, &message).to_string()).await?;
                }
            }
        }
    }
    if let (Some(t), Some(s)) = (target, session) {
        let auth = format!("Bearer {}", t.token);
        let _ = http::send(
            t.port,
            "DELETE",
            ENDPOINT,
            &[
                ("Authorization", auth.as_str()),
                (SESSION_HEADER, s.as_str()),
            ],
            b"",
        )
        .await;
    }
    Ok(())
}

/// The target from the discovery file and a token (`FORGE_MCP_TOKEN` first).
pub fn resolve_target(
    discovery: Option<PathBuf>,
    keychain_token: impl Fn() -> Option<String>,
) -> Result<Target, String> {
    let path = discovery.ok_or("no configuration directory")?;
    let d = read_discovery(&path).ok_or(
        "Forge's MCP server is not running. Start Forge and enable Settings | Tools | MCP Server.",
    )?;
    let token = std::env::var(TOKEN_ENV)
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(keychain_token)
        .ok_or("no MCP token found: set FORGE_MCP_TOKEN or open Forge once with the MCP server enabled")?;
    Ok(Target {
        port: d.port,
        token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_request_ids() {
        assert_eq!(
            request_ids(r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#),
            vec![Value::from(4)]
        );
        assert!(
            request_ids(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_empty()
        );
        assert_eq!(
            request_ids(
                r#"[{"jsonrpc":"2.0","id":"a","method":"x"},{"jsonrpc":"2.0","method":"y"}]"#
            ),
            vec![Value::from("a")]
        );
    }

    #[test]
    fn discovery_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge/mcp-server.json");
        let d = Discovery {
            port: 64343,
            pid: 42,
            version: "0.1.0".into(),
        };
        write_discovery(&path, &d).unwrap();
        assert_eq!(read_discovery(&path), Some(d));
        let t = resolve_target(Some(path.clone()), || Some("tok".into())).unwrap();
        assert_eq!(t.port, 64343);
        remove_discovery(&path, 7);
        assert!(path.exists(), "another process's file stays");
        remove_discovery(&path, 42);
        assert!(!path.exists());
        assert!(resolve_target(Some(path), || Some("tok".into()))
            .unwrap_err()
            .contains("not running"));
    }

    #[tokio::test]
    async fn answers_requests_when_forge_is_unreachable() {
        let input: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
        let mut out = Vec::new();
        run_proxy(input, &mut out, || Err("Forge is not running".to_string()))
            .await
            .unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
        let v: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["id"], 1);
        assert_eq!(v["error"]["message"], "Forge is not running");
    }
}
