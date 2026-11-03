//! The loopback transports.
//!
//! - **Streamable HTTP** (`/mcp`): `POST` carries JSON-RPC messages and gets the responses as
//!   `application/json` (202 for notifications only); `initialize` returns the session in
//!   `Mcp-Session-Id`, which later requests must send; `DELETE` ends the session. Forge sends
//!   no server-initiated messages, so `GET` answers 405.
//! - **HTTP+SSE** (protocol 2024-11-05, for older clients): `GET /sse` opens an event stream
//!   whose `endpoint` event names `/messages?sessionId=…`; responses to `POST`s there arrive as
//!   `message` events.
//!
//! The server binds `127.0.0.1` only. Every request needs a loopback `Host` (against DNS
//! rebinding), no foreign `Origin`, and `Authorization: Bearer <token>`.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::engine::{random_hex, Engine, EngineError};
use crate::http::{read_request, write_response, Request, Response};
use crate::jsonrpc;
use crate::protocol::SUPPORTED_VERSIONS;

/// Default port (JetBrains IDEs use 64342 for their MCP server; Forge tries the same range).
pub const DEFAULT_PORT: u16 = 64342;
/// Ports tried after the configured one when it is taken.
pub const PORT_ATTEMPTS: u16 = 10;
pub const ENDPOINT: &str = "/mcp";
pub const SSE_ENDPOINT: &str = "/sse";
pub const MESSAGES_ENDPOINT: &str = "/messages";
pub const SESSION_HEADER: &str = "Mcp-Session-Id";
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const SSE_KEEPALIVE: Duration = Duration::from_secs(15);

/// Compare without an early exit, so timing does not reveal matching prefixes.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A new bearer token (256 bits).
pub fn new_token() -> String {
    random_hex(32)
}

fn host_is_loopback(host: &str, port: u16) -> bool {
    ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|h| host.eq_ignore_ascii_case(&format!("{h}:{port}")))
}

fn origin_allowed(origin: Option<&str>, port: u16) -> bool {
    match origin {
        None | Some("null") => origin.is_none(),
        Some(o) => ["http://127.0.0.1", "http://localhost", "http://[::1]"]
            .iter()
            .any(|h| o.eq_ignore_ascii_case(&format!("{h}:{port}"))),
    }
}

fn bearer(req: &Request) -> Option<&str> {
    let (scheme, token) = req.header("authorization")?.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
}

/// What to send back: a plain response or an event stream.
pub enum Reply {
    Plain(Response),
    EventStream {
        session: String,
        events: mpsc::UnboundedReceiver<String>,
    },
}

fn sse_event(event: &str, data: &str) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

/// Request handling without sockets, so it is easy to test.
pub struct Router {
    engine: Arc<Engine>,
    token: String,
    port: u16,
    /// Open SSE streams by session id.
    streams: Mutex<HashMap<String, mpsc::UnboundedSender<String>>>,
}

impl Router {
    pub fn new(engine: Arc<Engine>, token: String, port: u16) -> Self {
        Self {
            engine,
            token,
            port,
            streams: Mutex::new(HashMap::new()),
        }
    }

    fn streams(&self) -> std::sync::MutexGuard<'_, HashMap<String, mpsc::UnboundedSender<String>>> {
        self.streams.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The SSE stream of `session` closed.
    pub fn stream_closed(&self, session: &str) {
        self.streams().remove(session);
        self.engine.end_session(session);
    }

    pub async fn handle(&self, req: &Request) -> Reply {
        Reply::Plain(match self.check(req) {
            Err(r) => r,
            Ok(()) => match (req.method.as_str(), req.path.as_str()) {
                ("POST", ENDPOINT) => self.post(req).await,
                ("DELETE", ENDPOINT) => match req.header("mcp-session-id") {
                    Some(id) if self.engine.end_session(id) => Response::new(200),
                    Some(_) => Response::error(404, "session not found"),
                    None => Response::error(400, "Mcp-Session-Id header is missing"),
                },
                ("GET", ENDPOINT) => {
                    Response::error(405, "this server sends no server-initiated messages")
                        .with_header("Allow", "POST, DELETE")
                }
                ("GET", SSE_ENDPOINT) => {
                    let session = random_hex(16);
                    let (tx, rx) = mpsc::unbounded_channel();
                    let _ = tx.send(sse_event(
                        "endpoint",
                        &format!("{MESSAGES_ENDPOINT}?sessionId={session}"),
                    ));
                    self.streams().insert(session.clone(), tx);
                    return Reply::EventStream {
                        session,
                        events: rx,
                    };
                }
                ("POST", MESSAGES_ENDPOINT) => self.post_sse(req).await,
                (_, ENDPOINT | SSE_ENDPOINT | MESSAGES_ENDPOINT) => {
                    Response::error(405, "method not allowed")
                }
                _ => Response::error(404, "not found"),
            },
        })
    }

    fn check(&self, req: &Request) -> Result<(), Response> {
        if !req
            .header("host")
            .is_some_and(|h| host_is_loopback(h, self.port))
        {
            return Err(Response::error(403, "invalid host"));
        }
        if !origin_allowed(req.header("origin"), self.port) {
            return Err(Response::error(
                403,
                "requests from web pages are not allowed",
            ));
        }
        let authorized =
            bearer(req).is_some_and(|t| constant_time_eq(t.as_bytes(), self.token.as_bytes()));
        if !authorized {
            return Err(Response::error(401, "missing or invalid bearer token")
                .with_header("WWW-Authenticate", "Bearer realm=\"forge\""));
        }
        if let Some(v) = req.header("mcp-protocol-version") {
            if !SUPPORTED_VERSIONS.contains(&v) {
                return Err(Response::error(
                    400,
                    &format!("unsupported protocol version {v}"),
                ));
            }
        }
        if let Some(ct) = req.header("content-type") {
            if req.method == "POST" && !ct.to_ascii_lowercase().starts_with("application/json") {
                return Err(Response::error(415, "expected application/json"));
            }
        }
        Ok(())
    }

    async fn post(&self, req: &Request) -> Response {
        let incoming = match jsonrpc::parse(&req.body) {
            Ok(i) => i,
            Err(body) => return Response::json(400, &body),
        };
        let session = req.header("mcp-session-id");
        match self.engine.handle(session, None, "http", incoming).await {
            Err(EngineError::MissingSession) => Response::error(
                400,
                "Mcp-Session-Id header is missing; send initialize first",
            ),
            Err(EngineError::UnknownSession) => Response::error(404, "session not found"),
            Ok(outcome) => {
                let mut resp = match outcome.body() {
                    Some(body) => Response::json(200, &body),
                    None => Response::new(202),
                };
                if let Some(id) = &outcome.created_session {
                    resp = resp.with_header(SESSION_HEADER, id);
                }
                resp
            }
        }
    }

    async fn post_sse(&self, req: &Request) -> Response {
        let Some(session) = req.query_param("sessionId").map(str::to_string) else {
            return Response::error(400, "sessionId is missing");
        };
        let Some(tx) = self.streams().get(&session).cloned() else {
            return Response::error(404, "session not found");
        };
        let incoming = match jsonrpc::parse(&req.body) {
            Ok(i) => i,
            Err(body) => {
                let _ = tx.send(sse_event("message", &body.to_string()));
                return Response::new(202);
            }
        };
        let known = self
            .engine
            .has_session(&session)
            .then_some(session.as_str());
        match self
            .engine
            .handle(known, Some(&session), "sse", incoming)
            .await
        {
            Err(_) => Response::error(400, "send initialize first"),
            Ok(outcome) => {
                if let Some(body) = outcome.body() {
                    let _ = tx.send(sse_event("message", &body.to_string()));
                }
                Response::new(202)
            }
        }
    }
}

/// A running server; [`McpServer::stop`] closes it and every session.
pub struct McpServer {
    pub port: u16,
    task: JoinHandle<()>,
    shutdown: watch::Sender<bool>,
    engine: Arc<Engine>,
}

impl McpServer {
    /// Bind `127.0.0.1:port` (or one of the next [`PORT_ATTEMPTS`] - 1 ports) and serve in
    /// the background on the current tokio runtime. Port 0 picks a free port (tests).
    pub async fn start(port: u16, token: String, engine: Arc<Engine>) -> std::io::Result<Self> {
        let mut last_err = None;
        let attempts = if port == 0 { 1 } else { PORT_ATTEMPTS };
        for p in port..port.saturating_add(attempts) {
            match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, p))).await {
                Ok(listener) => {
                    let port = listener.local_addr()?.port();
                    let router = Arc::new(Router::new(engine.clone(), token, port));
                    let (shutdown, rx) = watch::channel(false);
                    let task = tokio::spawn(serve(listener, router, rx));
                    return Ok(Self {
                        port,
                        task,
                        shutdown,
                        engine,
                    });
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| std::io::Error::other("no port to bind")))
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}{ENDPOINT}", self.port)
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    pub fn stop(self) {
        let _ = self.shutdown.send(true);
        self.task.abort();
        self.engine.end_all();
    }
}

async fn serve(listener: TcpListener, router: Arc<Router>, shutdown: watch::Receiver<bool>) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("mcp server: accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        if !peer.ip().is_loopback() {
            continue;
        }
        tokio::spawn(connection(stream, router.clone(), shutdown.clone()));
    }
}

async fn connection(
    mut stream: TcpStream,
    router: Arc<Router>,
    mut shutdown: watch::Receiver<bool>,
) {
    let req = match tokio::time::timeout(READ_TIMEOUT, read_request(&mut stream)).await {
        Ok(Ok(req)) => req,
        Ok(Err(e)) => {
            let _ = write_response(&mut stream, &Response::error(e.status(), e.message())).await;
            return;
        }
        Err(_) => return,
    };
    match router.handle(&req).await {
        Reply::Plain(resp) => {
            let _ = write_response(&mut stream, &resp).await;
        }
        Reply::EventStream {
            session,
            mut events,
        } => {
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: keep-alive\r\n\r\n";
            if stream.write_all(head.as_bytes()).await.is_ok() {
                let mut keepalive = tokio::time::interval(SSE_KEEPALIVE);
                keepalive.tick().await;
                loop {
                    let chunk = tokio::select! {
                        e = events.recv() => match e { Some(e) => e, None => break },
                        _ = keepalive.tick() => ": keepalive\n\n".to_string(),
                        _ = shutdown.changed() => break,
                    };
                    if stream.write_all(chunk.as_bytes()).await.is_err()
                        || stream.flush().await.is_err()
                    {
                        break;
                    }
                }
            }
            router.stream_closed(&session);
        }
    }
}

/// `{ "error": … }` body for transports that must answer a request they cannot route.
pub fn transport_error(id: serde_json::Value, message: &str) -> serde_json::Value {
    jsonrpc::error_response(
        id,
        &jsonrpc::RpcError::new(jsonrpc::INTERNAL_ERROR, message),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_hosts_and_origins() {
        assert!(host_is_loopback("127.0.0.1:64342", 64342));
        assert!(host_is_loopback("localhost:64342", 64342));
        assert!(!host_is_loopback("evil.com:64342", 64342));
        assert!(!host_is_loopback("127.0.0.1:1", 64342));
        assert!(origin_allowed(None, 1));
        assert!(origin_allowed(Some("http://localhost:1"), 1));
        assert!(!origin_allowed(Some("https://evil.com"), 1));
        assert!(!origin_allowed(Some("null"), 1));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert_eq!(new_token().len(), 64);
    }
}
