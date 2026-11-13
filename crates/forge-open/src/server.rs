//! The Browser Companion's local HTTP API, modelled on JetBrains' built-in server.
//!
//! It binds to `127.0.0.1` only and serves:
//!
//! - `GET  /api/forge/ping` — name and version; with a valid token also the open projects.
//! - `POST /api/forge/pair` — `{ code, name }` → `{ clientId, token }`.
//! - `POST /api/forge/open` — `{ file, line?, column?, project?, symbol? }`.
//! - `POST /api/forge/open-symbol` — `{ name, kind?: "class" | "component" }`.
//! - `GET  /api/forge/live-edit?since=<seq>` — Live Edit events after `seq`
//!   (`{ enabled, seq, events }`, see [`crate::live_edit`]).
//!
//! Every request must name a loopback `Host` (defeats DNS rebinding) and, if it carries an
//! `Origin`, come from a browser extension (web pages are refused). Opening needs the
//! `Authorization: Bearer <token>` the extension received when pairing.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::http::{read_request, write_response, Request, Response};
use crate::live_edit::LiveEditHub;
use crate::pairing::{now_secs, ClientInfo, PairError, Pairing};
use crate::request::{OpenRequest, SymbolRequest};

/// JetBrains IDEs listen on 63342; Forge uses the same range so tools that probe it find us.
pub const DEFAULT_PORT: u16 = 63342;
/// Ports tried after the configured one when it is taken (e.g. by PHPStorm).
pub const PORT_ATTEMPTS: u16 = 10;
/// Version of this HTTP API; bumped on incompatible changes.
pub const API_VERSION: u32 = 1;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Extension origins allowed to call the API.
const EXTENSION_SCHEMES: &[&str] = &[
    "chrome-extension://",
    "moz-extension://",
    "safari-web-extension://",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub name: String,
    pub version: String,
    pub projects: Vec<ProjectInfo>,
}

/// What happened to an open request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenStatus {
    /// Opened in the editor.
    Opened,
    /// Forge asks the user first (other project, or outside every project).
    NeedsConfirmation,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOutcome {
    pub status: OpenStatus,
    /// Absolute path that is being opened.
    pub file: String,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub status: u16,
    pub message: String,
}

impl HostError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

/// The IDE side: implemented by the app to show files and answer `ping`.
pub trait OpenHost: Send + Sync + 'static {
    fn info(&self) -> HostInfo;
    fn open(&self, request: OpenRequest, client: &ClientInfo) -> Result<OpenOutcome, HostError>;
    fn open_symbol(
        &self,
        request: SymbolRequest,
        client: &ClientInfo,
    ) -> Result<OpenOutcome, HostError>;
    /// Live Edit events, when Live Edit is on.
    fn live_edit(&self) -> Option<Arc<LiveEditHub>> {
        None
    }
}

#[derive(Deserialize)]
struct PairBody {
    code: String,
    #[serde(default)]
    name: String,
}

/// Request handling without sockets, so it is easy to test.
pub struct Router {
    host: Arc<dyn OpenHost>,
    pairing: Arc<Mutex<Pairing>>,
    port: u16,
}

fn host_is_loopback(host: &str, port: u16) -> bool {
    let allowed = [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ];
    allowed.iter().any(|a| a.eq_ignore_ascii_case(host))
}

fn origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(o) => EXTENSION_SCHEMES.iter().any(|s| o.starts_with(s)),
    }
}

fn bearer(req: &Request) -> Option<&str> {
    let v = req.header("authorization")?;
    let (scheme, token) = v.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
}

fn parse_json<T: for<'de> Deserialize<'de>>(req: &Request) -> Result<T, Response> {
    let is_json = req
        .header("content-type")
        .is_some_and(|c| c.starts_with("application/json"));
    if !is_json {
        return Err(Response::error(415, "expected application/json"));
    }
    serde_json::from_slice(&req.body)
        .map_err(|e| Response::error(400, &format!("invalid request: {e}")))
}

fn outcome_response(result: Result<OpenOutcome, HostError>) -> Response {
    match result {
        Ok(outcome) => Response::json(200, &json!(outcome)),
        Err(e) => Response::error(e.status, &e.message),
    }
}

impl Router {
    pub fn new(host: Arc<dyn OpenHost>, pairing: Arc<Mutex<Pairing>>, port: u16) -> Self {
        Self {
            host,
            pairing,
            port,
        }
    }

    fn pairing(&self) -> std::sync::MutexGuard<'_, Pairing> {
        self.pairing.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn client(&self, req: &Request) -> Option<ClientInfo> {
        let token = bearer(req)?;
        self.pairing()
            .verify(token, req.header("origin"), now_secs())
    }

    pub fn handle(&self, req: &Request) -> Response {
        if !req
            .header("host")
            .is_some_and(|h| host_is_loopback(h, self.port))
        {
            return Response::error(403, "invalid host");
        }
        let origin = req.header("origin");
        if !origin_allowed(origin) {
            return Response::error(403, "requests from web pages are not allowed");
        }
        if req.method == "OPTIONS" {
            // No CORS headers: web pages never get a successful preflight.
            return Response::empty(204);
        }

        match (req.method.as_str(), req.path.as_str()) {
            ("GET", "/api/forge/ping") => {
                let info = self.host.info();
                let mut body = json!({
                    "name": info.name,
                    "version": info.version,
                    "api": API_VERSION,
                    "paired": false,
                });
                if let Some(client) = self.client(req) {
                    body["paired"] = json!(true);
                    body["client"] = json!(client.name);
                    body["projects"] = json!(info.projects);
                }
                Response::json(200, &body)
            }
            ("POST", "/api/forge/pair") => {
                let body: PairBody = match parse_json(req) {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                let result = self
                    .pairing()
                    .pair(&body.code, &body.name, origin, now_secs());
                match result {
                    Ok(paired) => Response::json(200, &json!(paired)),
                    Err(e @ PairError::TooManyAttempts) => Response::error(429, &e.to_string()),
                    Err(e @ PairError::NotPairing) => Response::error(409, &e.to_string()),
                    Err(e) => Response::error(403, &e.to_string()),
                }
            }
            ("POST", "/api/forge/open") => {
                let Some(client) = self.client(req) else {
                    return Response::error(401, "not paired");
                };
                let request = match parse_json::<OpenRequest>(req) {
                    Ok(r) => r,
                    Err(r) => return r,
                };
                match request.validate() {
                    Ok(r) => outcome_response(self.host.open(r, &client)),
                    Err(e) => Response::error(400, &e.to_string()),
                }
            }
            ("POST", "/api/forge/open-symbol") => {
                let Some(client) = self.client(req) else {
                    return Response::error(401, "not paired");
                };
                let request = match parse_json::<SymbolRequest>(req) {
                    Ok(r) => r,
                    Err(r) => return r,
                };
                match request.validate() {
                    Ok(r) => outcome_response(self.host.open_symbol(r, &client)),
                    Err(e) => Response::error(400, &e.to_string()),
                }
            }
            ("GET", "/api/forge/live-edit") => {
                if self.client(req).is_none() {
                    return Response::error(401, "not paired");
                }
                let Some(hub) = self.host.live_edit() else {
                    return Response::json(
                        200,
                        &json!({ "enabled": false, "seq": 0, "events": [] }),
                    );
                };
                let since = req
                    .query
                    .split('&')
                    .find_map(|p| p.strip_prefix("since="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let poll = hub.since(since);
                Response::json(
                    200,
                    &json!({ "enabled": true, "seq": poll.seq, "events": poll.events }),
                )
            }
            (_, "/api/forge/ping" | "/api/forge/pair" | "/api/forge/open")
            | (_, "/api/forge/open-symbol") => Response::error(405, "method not allowed"),
            _ => Response::error(404, "not found"),
        }
    }
}

/// A running server; dropping it does not stop it, call [`CompanionServer::stop`].
pub struct CompanionServer {
    pub port: u16,
    task: JoinHandle<()>,
}

impl CompanionServer {
    /// Bind `127.0.0.1:port` (or the next free port of the range) and serve in the
    /// background on the current tokio runtime.
    pub async fn start(
        port: u16,
        host: Arc<dyn OpenHost>,
        pairing: Arc<Mutex<Pairing>>,
    ) -> std::io::Result<Self> {
        let mut last_err = None;
        for p in port..port.saturating_add(PORT_ATTEMPTS) {
            match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, p))).await {
                Ok(listener) => {
                    let port = listener.local_addr()?.port();
                    let router = Arc::new(Router::new(host, pairing, port));
                    let task = tokio::spawn(serve(listener, router));
                    return Ok(Self { port, task });
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| std::io::Error::other("no port to bind")))
    }

    pub fn stop(self) {
        self.task.abort();
    }
}

async fn serve(listener: TcpListener, router: Arc<Router>) {
    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("browser companion: accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        if !peer.ip().is_loopback() {
            continue;
        }
        let router = router.clone();
        tokio::spawn(async move {
            let response = match tokio::time::timeout(READ_TIMEOUT, read_request(&mut stream)).await
            {
                Ok(Ok(req)) => router.handle(&req),
                Ok(Err(e)) => Response::error(e.status(), &e.to_string()),
                Err(_) => return,
            };
            let _ = write_response(&mut stream, &response).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct FakeHost {
        opened: Mutex<Vec<OpenRequest>>,
    }

    impl OpenHost for FakeHost {
        fn info(&self) -> HostInfo {
            HostInfo {
                name: "Forge".into(),
                version: "0.1.0".into(),
                projects: vec![ProjectInfo {
                    name: "shop".into(),
                    root: "/home/me/shop".into(),
                }],
            }
        }

        fn open(&self, request: OpenRequest, _: &ClientInfo) -> Result<OpenOutcome, HostError> {
            let file = request.file.clone();
            let line = request.line;
            self.opened.lock().unwrap().push(request);
            Ok(OpenOutcome {
                status: OpenStatus::Opened,
                file,
                line,
            })
        }

        fn open_symbol(
            &self,
            request: SymbolRequest,
            _: &ClientInfo,
        ) -> Result<OpenOutcome, HostError> {
            Err(HostError::new(404, format!("{} not found", request.name)))
        }
    }

    const ORIGIN: &str = "chrome-extension://abcdef";

    fn setup() -> (Router, Arc<FakeHost>, String) {
        let host = Arc::new(FakeHost {
            opened: Mutex::new(vec![]),
        });
        let pairing = Arc::new(Mutex::new(Pairing::in_memory()));
        let code = pairing.lock().unwrap().start(now_secs()).code;
        let token = pairing
            .lock()
            .unwrap()
            .pair(&code, "Chrome", Some(ORIGIN), now_secs())
            .unwrap()
            .token;
        (Router::new(host.clone(), pairing, 63342), host, token)
    }

    fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
        let mut h: Vec<(String, String)> = vec![("host".into(), "127.0.0.1:63342".into())];
        for (k, v) in headers {
            h.retain(|(name, _)| name != k);
            h.push((k.to_string(), v.to_string()));
        }
        Request {
            method: method.into(),
            path: path.into(),
            query: String::new(),
            headers: h,
            body: body.as_bytes().to_vec(),
        }
    }

    fn body(r: &Response) -> serde_json::Value {
        serde_json::from_str(&r.body).unwrap()
    }

    struct LiveHost(Arc<LiveEditHub>);

    impl OpenHost for LiveHost {
        fn info(&self) -> HostInfo {
            HostInfo {
                name: "Forge".into(),
                version: "0".into(),
                projects: vec![],
            }
        }
        fn open(&self, _: OpenRequest, _: &ClientInfo) -> Result<OpenOutcome, HostError> {
            Err(HostError::new(500, "no"))
        }
        fn open_symbol(&self, _: SymbolRequest, _: &ClientInfo) -> Result<OpenOutcome, HostError> {
            Err(HostError::new(500, "no"))
        }
        fn live_edit(&self) -> Option<Arc<LiveEditHub>> {
            Some(self.0.clone())
        }
    }

    #[test]
    fn serves_live_edit_events_to_paired_browsers() {
        let hub = Arc::new(LiveEditHub::default());
        let pairing = Arc::new(Mutex::new(Pairing::in_memory()));
        let code = pairing.lock().unwrap().start(now_secs()).code;
        let token = pairing
            .lock()
            .unwrap()
            .pair(&code, "Chrome", Some(ORIGIN), now_secs())
            .unwrap()
            .token;
        let router = Router::new(Arc::new(LiveHost(hub.clone())), pairing, 63342);
        let auth = format!("Bearer {token}");
        let get = |since: &str, auth: &str| {
            let mut r = request(
                "GET",
                "/api/forge/live-edit",
                &[("authorization", auth), ("origin", ORIGIN)],
                "",
            );
            r.query = format!("since={since}");
            router.handle(&r)
        };
        assert_eq!(get("0", "Bearer nope").status, 401);
        assert_eq!(body(&get("0", &auth))["seq"], 0);
        hub.push("public/app.css", Some("h1{}".into()));
        let r = body(&get("0", &auth));
        assert_eq!(
            (r["seq"].as_u64(), r["events"].as_array().unwrap().len()),
            (Some(1), 0)
        );
        hub.push("index.html", None);
        let r = body(&get("1", &auth));
        assert_eq!(r["events"][0]["kind"], "reload");
        assert_eq!(r["enabled"], true);
        // Hosts without Live Edit answer that it is off.
        let (plain, _, token) = setup();
        let mut r = request(
            "GET",
            "/api/forge/live-edit",
            &[
                ("authorization", &format!("Bearer {token}")),
                ("origin", ORIGIN),
            ],
            "",
        );
        r.query = "since=0".into();
        assert_eq!(body(&plain.handle(&r))["enabled"], false);
    }

    #[test]
    fn ping_hides_projects_until_paired() {
        let (router, _, token) = setup();
        let r = router.handle(&request("GET", "/api/forge/ping", &[], ""));
        assert_eq!(r.status, 200);
        assert_eq!(body(&r)["name"], "Forge");
        assert_eq!(body(&r)["paired"], false);
        assert!(body(&r).get("projects").is_none());

        let auth = format!("Bearer {token}");
        let r = router.handle(&request(
            "GET",
            "/api/forge/ping",
            &[("authorization", &auth), ("origin", ORIGIN)],
            "",
        ));
        assert_eq!(body(&r)["paired"], true);
        assert_eq!(body(&r)["projects"][0]["root"], "/home/me/shop");
    }

    #[test]
    fn rejects_web_pages_and_foreign_hosts() {
        let (router, host, token) = setup();
        let auth = format!("Bearer {token}");
        let open = r#"{"file":"/home/me/shop/a.ts"}"#;
        let json = ("content-type", "application/json");

        let from_page = router.handle(&request(
            "POST",
            "/api/forge/open",
            &[
                ("origin", "https://evil.example"),
                ("authorization", &auth),
                json,
            ],
            open,
        ));
        assert_eq!(from_page.status, 403);

        let rebinding = router.handle(&request(
            "POST",
            "/api/forge/open",
            &[
                ("host", "evil.example:63342"),
                ("authorization", &auth),
                json,
            ],
            open,
        ));
        assert_eq!(rebinding.status, 403);

        let unpaired = router.handle(&request("POST", "/api/forge/open", &[json], open));
        assert_eq!(unpaired.status, 401);

        let wrong_token = router.handle(&request(
            "POST",
            "/api/forge/open",
            &[("authorization", "Bearer 1234"), json],
            open,
        ));
        assert_eq!(wrong_token.status, 401);

        let other_extension = router.handle(&request(
            "POST",
            "/api/forge/open",
            &[
                ("origin", "chrome-extension://other"),
                ("authorization", &auth),
                json,
            ],
            open,
        ));
        assert_eq!(other_extension.status, 401);

        let not_json = router.handle(&request(
            "POST",
            "/api/forge/open",
            &[("authorization", &auth), ("content-type", "text/plain")],
            open,
        ));
        assert_eq!(not_json.status, 415);

        assert!(host.opened.lock().unwrap().is_empty());
    }

    #[test]
    fn opens_validated_requests() {
        let (router, host, token) = setup();
        let auth = format!("Bearer {token}");
        let headers = [
            ("authorization", auth.as_str()),
            ("origin", ORIGIN),
            ("content-type", "application/json"),
        ];
        let r = router.handle(&request(
            "POST",
            "/api/forge/open",
            &headers,
            r#"{"file":"file:///home/me/shop/src/App.tsx","line":12,"column":0}"#,
        ));
        assert_eq!(r.status, 200, "{}", r.body);
        assert_eq!(body(&r)["status"], "opened");
        let opened = host.opened.lock().unwrap().clone();
        assert_eq!(opened[0].file, "/home/me/shop/src/App.tsx");
        assert_eq!((opened[0].line, opened[0].column), (Some(12), None));

        let bad = router.handle(&request(
            "POST",
            "/api/forge/open",
            &headers,
            r#"{"file":""}"#,
        ));
        assert_eq!(bad.status, 400);

        let missing = router.handle(&request(
            "POST",
            "/api/forge/open-symbol",
            &headers,
            r#"{"name":"UserCardComponent","kind":"component"}"#,
        ));
        assert_eq!(missing.status, 404);
        assert_eq!(body(&missing)["error"], "UserCardComponent not found");
    }

    #[test]
    fn pairing_over_http() {
        let (router, _, _) = setup();
        let json = ("content-type", "application/json");
        let r = router.handle(&request(
            "POST",
            "/api/forge/pair",
            &[json],
            r#"{"code":"123456"}"#,
        ));
        assert_eq!(r.status, 409, "no pairing in progress");

        let code = router.pairing().start(now_secs()).code;
        let r = router.handle(&request(
            "POST",
            "/api/forge/pair",
            &[json, ("origin", ORIGIN)],
            &format!(r#"{{"code":"{code}","name":"Firefox"}}"#),
        ));
        assert_eq!(r.status, 200);
        assert_eq!(body(&r)["token"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn unknown_routes_and_methods() {
        let (router, _, _) = setup();
        assert_eq!(router.handle(&request("GET", "/", &[], "")).status, 404);
        assert_eq!(
            router
                .handle(&request("GET", "/api/forge/open", &[], ""))
                .status,
            405
        );
        assert_eq!(
            router
                .handle(&request("OPTIONS", "/api/forge/open", &[], ""))
                .status,
            204
        );
    }

    #[tokio::test]
    async fn serves_on_loopback() {
        let (router, _, _) = setup();
        let host = router.host.clone();
        let pairing = router.pairing.clone();
        // Port 0 lets the OS pick a free port.
        let server = CompanionServer::start(0, host, pairing).await.unwrap();
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", server.port))
            .await
            .unwrap();
        let req = format!(
            "GET /api/forge/ping HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
            server.port
        );
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut out = String::new();
        stream.read_to_string(&mut out).await.unwrap();
        assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
        assert!(out.contains(r#""name":"Forge""#));
        server.stop();
    }
}
