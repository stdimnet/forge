//! Sends the requests of `.http` scratches (feature `client`).

use std::time::{Duration, Instant};

use crate::http::{Header, HttpRequest, HttpResponse};

/// Largest body kept for the console; longer bodies are cut.
const MAX_BODY: usize = 2 * 1024 * 1024;

/// Send a resolved request (see [`crate::http::resolve`]). Errors are worded for the console.
pub async fn send(req: &HttpRequest, timeout: Duration) -> Result<HttpResponse, String> {
    let method = reqwest::Method::from_bytes(req.method.as_bytes())
        .map_err(|_| format!("Unknown method '{}'", req.method))?;
    let url =
        reqwest::Url::parse(&req.url).map_err(|e| format!("Invalid URL '{}': {e}", req.url))?;
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(concat!("Forge-HTTP-Client/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut builder = client.request(method, url);
    for h in &req.headers {
        builder = builder.header(h.name.as_str(), h.value.as_str());
    }
    if let Some(body) = &req.body {
        builder = builder.body(body.clone());
    }
    let started = Instant::now();
    let resp = builder.send().await.map_err(describe)?;
    let version = match resp.version() {
        reqwest::Version::HTTP_09 => "HTTP/0.9",
        reqwest::Version::HTTP_10 => "HTTP/1.0",
        reqwest::Version::HTTP_2 => "HTTP/2",
        reqwest::Version::HTTP_3 => "HTTP/3",
        _ => "HTTP/1.1",
    };
    let status = resp.status();
    let title_case = !version.starts_with("HTTP/2") && !version.starts_with("HTTP/3");
    let headers = resp
        .headers()
        .iter()
        .map(|(n, v)| Header {
            name: if title_case {
                title_case_header(n.as_str())
            } else {
                n.as_str().to_string()
            },
            value: String::from_utf8_lossy(v.as_bytes()).into_owned(),
        })
        .collect();
    let bytes = resp.bytes().await.map_err(describe)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let size = bytes.len() as u64;
    let body = match std::str::from_utf8(&bytes[..bytes.len().min(MAX_BODY)]) {
        Ok(text) if bytes.len() <= MAX_BODY => text.to_string(),
        Ok(text) => format!("{text}\n… (response cut at {MAX_BODY} bytes)"),
        Err(_) => format!("<binary response, {size} bytes>"),
    };
    Ok(HttpResponse {
        version: version.to_string(),
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        body,
        size,
        elapsed_ms,
    })
}

/// `content-type` → `Content-Type` (HTTP/1 servers send it like that; the client lowercases).
fn title_case_header(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn describe(e: reqwest::Error) -> String {
    use std::error::Error as _;
    let mut msg = if e.is_timeout() {
        "Request timed out".to_string()
    } else if e.is_connect() {
        "Connection failed".to_string()
    } else {
        "Request failed".to_string()
    };
    let mut source = e.source();
    let mut last = None;
    while let Some(s) = source {
        last = Some(s.to_string());
        source = s.source();
    }
    if let Some(cause) = last {
        msg.push_str(": ");
        msg.push_str(&cause);
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{parse, resolve};
    use std::collections::BTreeMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A one-shot local HTTP server that answers with `response` and returns the request.
    async fn server(response: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let mut got = Vec::new();
            loop {
                let n = sock.read(&mut buf).await.unwrap();
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if got.len() >= head_end + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            sock.write_all(response.as_bytes()).await.unwrap();
            sock.shutdown().await.ok();
            String::from_utf8_lossy(&got).to_string()
        });
        (format!("127.0.0.1:{}", addr.port()), handle)
    }

    #[tokio::test]
    async fn sends_a_request_to_a_local_server() {
        let (host, handle) = server(
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 9\r\nConnection: close\r\n\r\n{\"id\":42}",
        )
        .await;
        let file = parse(&format!(
            "@host = {host}\nPOST http://{{{{host}}}}/users\nContent-Type: application/json\nX-Trace: abc\n\n{{\"name\":\"Ada\"}}\n"
        ));
        let req = resolve(&file.requests[0], &file.variables, &BTreeMap::new());
        let resp = send(&req, Duration::from_secs(5)).await.unwrap();
        assert_eq!((resp.status, resp.reason.as_str()), (201, "Created"));
        assert_eq!(resp.version, "HTTP/1.1");
        assert_eq!(resp.body, "{\"id\":42}");
        assert_eq!(resp.size, 9);
        assert!(resp.headers.iter().any(|h| h.name == "Content-Type"));
        let seen = handle.await.unwrap();
        assert!(seen.starts_with("POST /users HTTP/1.1\r\n"), "{seen}");
        assert!(seen.to_ascii_lowercase().contains("x-trace: abc"));
        assert!(seen.ends_with("{\"name\":\"Ada\"}"));
    }

    #[tokio::test]
    async fn reports_connection_errors() {
        // Bind and drop to get a port nobody listens on.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let req = resolve(
            &parse(&format!("GET http://127.0.0.1:{port}/")).requests[0],
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        let err = send(&req, Duration::from_secs(5)).await.unwrap_err();
        assert!(err.starts_with("Connection failed"), "{err}");
        let bad = HttpRequest {
            url: "not a url".into(),
            ..req
        };
        assert!(send(&bad, Duration::from_secs(1))
            .await
            .unwrap_err()
            .starts_with("Invalid URL"));
    }

    #[test]
    fn title_cases_headers() {
        assert_eq!(title_case_header("content-type"), "Content-Type");
        assert_eq!(title_case_header("x-request-id"), "X-Request-Id");
    }
}
