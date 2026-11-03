//! A small JSON REST client shared by the host implementations: authentication headers,
//! `Link`-header pagination and translating HTTP failures into friendly [`HostError`]s.

use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{HostError, HostResult};

/// How a token is sent.
#[derive(Debug, Clone)]
pub enum Auth {
    /// GitLab: `PRIVATE-TOKEN: <token>`.
    PrivateToken(String),
    /// Gitea/Forgejo: `Authorization: token <token>`.
    Token(String),
    /// GitHub: `Authorization: Bearer <token>`.
    Bearer(String),
    /// Azure DevOps: HTTP Basic with an empty user name and the token as password.
    Basic(String, String),
    None,
}

pub struct Http {
    client: reqwest::Client,
    /// API root without trailing slash, e.g. `https://gitlab.com/api/v4`.
    pub base: String,
    auth: Auth,
    /// Host name for messages ("GitLab").
    pub host: String,
    accept: &'static str,
}

/// One response page plus pagination info.
pub struct Paged<T> {
    pub items: Vec<T>,
    /// Absolute URL of the next page (from the `Link` header).
    pub next_url: Option<String>,
    pub next_page: Option<u32>,
    pub total: Option<u64>,
}

/// Pages fetched when listing an account's repositories.
pub const REPO_PAGES: usize = 5;

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("HTTP client")
}

impl Http {
    pub fn new(
        client: reqwest::Client,
        base: impl Into<String>,
        auth: Auth,
        host: &str,
        accept: &'static str,
    ) -> Self {
        Self {
            client,
            base: base.into().trim_end_matches('/').to_string(),
            auth,
            host: host.to_string(),
            accept,
        }
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{}/{}", self.base, path.trim_start_matches('/'))
        }
    }

    pub fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let mut rb = self
            .client
            .request(method, self.url(path))
            .header(USER_AGENT, concat!("Forge-IDE/", env!("CARGO_PKG_VERSION")))
            .header(ACCEPT, self.accept);
        rb = match &self.auth {
            Auth::PrivateToken(t) => rb.header("PRIVATE-TOKEN", t),
            Auth::Token(t) => rb.header(AUTHORIZATION, format!("token {t}")),
            Auth::Bearer(t) => rb.header(AUTHORIZATION, format!("Bearer {t}")),
            Auth::Basic(u, p) => rb.basic_auth(u, Some(p)),
            Auth::None => rb,
        };
        rb
    }

    /// Send a request; non-2xx responses become errors. `what` names the resource for 404s.
    pub async fn send(&self, rb: reqwest::RequestBuilder, what: &str) -> HostResult<Response> {
        let res = rb.send().await.map_err(|e| self.transport_error(&e))?;
        // Azure DevOps answers a rejected token with 203 and its HTML sign-in page.
        let html = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/html"));
        if res.status() == StatusCode::NON_AUTHORITATIVE_INFORMATION && html {
            return Err(HostError::Unauthorized {
                host: self.host.clone(),
            });
        }
        if res.status().is_success() {
            Ok(res)
        } else {
            Err(self.status_error(res, what).await)
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str, what: &str) -> HostResult<T> {
        let res = self.send(self.request(Method::GET, path), what).await?;
        self.parse(res).await
    }

    /// GET returning `None` for 404 (optional resources such as a missing file).
    pub async fn get_optional<T: DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> HostResult<Option<T>> {
        match self.get(path, what).await {
            Ok(v) => Ok(Some(v)),
            Err(HostError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn get_text(&self, path: &str, accept: Option<&str>) -> HostResult<Option<String>> {
        let mut rb = self.request(Method::GET, path);
        if let Some(a) = accept {
            rb = rb.header(ACCEPT, a);
        }
        match self.send(rb, "file").await {
            Ok(res) => res
                .text()
                .await
                .map(Some)
                .map_err(|e| self.transport_error(&e)),
            Err(HostError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn send_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &Value,
        what: &str,
    ) -> HostResult<T> {
        let res = self
            .send(self.request(method, path).json(body), what)
            .await?;
        self.parse(res).await
    }

    /// Send a JSON body and ignore the response body.
    pub async fn send_unit(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        what: &str,
    ) -> HostResult<()> {
        let mut rb = self.request(method, path);
        if let Some(b) = body {
            rb = rb.json(b);
        }
        self.send(rb, what).await.map(drop)
    }

    pub async fn get_page<T: DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> HostResult<Paged<T>> {
        let res = self.send(self.request(Method::GET, path), what).await?;
        let headers = res.headers().clone();
        let items: Vec<T> = self.parse(res).await?;
        let next_url = next_link(&headers);
        let next_page = header_num(&headers, "x-next-page")
            .map(|n| n as u32)
            .or_else(|| next_url.as_deref().and_then(page_param));
        let total =
            header_num(&headers, "x-total").or_else(|| header_num(&headers, "x-total-count"));
        Ok(Paged {
            items,
            next_url,
            next_page,
            total,
        })
    }

    /// Follow `Link: rel="next"` up to `max_pages` pages.
    pub async fn get_all<T: DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
        max_pages: usize,
    ) -> HostResult<Vec<T>> {
        let mut out = Vec::new();
        let mut next = Some(path.to_string());
        let mut pages = 0;
        while let Some(p) = next.take() {
            let page = self.get_page::<T>(&p, what).await?;
            out.extend(page.items);
            pages += 1;
            if pages >= max_pages {
                break;
            }
            next = page.next_url;
        }
        Ok(out)
    }

    async fn parse<T: DeserializeOwned>(&self, res: Response) -> HostResult<T> {
        let bytes = res.bytes().await.map_err(|e| self.transport_error(&e))?;
        let bytes = if bytes.iter().all(u8::is_ascii_whitespace) {
            &b"null"[..]
        } else {
            &bytes[..]
        };
        serde_json::from_slice(bytes).map_err(|e| HostError::InvalidResponse {
            host: self.host.clone(),
            message: e.to_string(),
        })
    }

    fn transport_error(&self, e: &reqwest::Error) -> HostError {
        if e.is_timeout() {
            HostError::Timeout {
                host: self.host.clone(),
            }
        } else if e.is_decode() || e.is_body() {
            HostError::InvalidResponse {
                host: self.host.clone(),
                message: e.to_string(),
            }
        } else {
            HostError::Connection {
                host: self.host.clone(),
                url: self.base.clone(),
            }
        }
    }

    async fn status_error(&self, res: Response, what: &str) -> HostError {
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.text().await.unwrap_or_default();
        let message = api_message(&body);
        let host = self.host.clone();
        let remaining = header_num(&headers, "x-ratelimit-remaining")
            .or_else(|| header_num(&headers, "ratelimit-remaining"));
        let reset = header_num(&headers, "x-ratelimit-reset")
            .or_else(|| header_num(&headers, "ratelimit-reset"));
        match status {
            StatusCode::TOO_MANY_REQUESTS => HostError::RateLimited { host, reset },
            StatusCode::FORBIDDEN
                if remaining == Some(0)
                    || message
                        .as_deref()
                        .is_some_and(|m| m.to_lowercase().contains("rate limit")) =>
            {
                HostError::RateLimited { host, reset }
            }
            StatusCode::UNAUTHORIZED => HostError::Unauthorized { host },
            StatusCode::FORBIDDEN => HostError::Forbidden { host, message },
            StatusCode::NOT_FOUND => HostError::NotFound {
                host,
                what: what.to_string(),
            },
            _ => HostError::Api {
                host,
                message: message.unwrap_or_else(|| {
                    format!(
                        "HTTP {} {}",
                        status.as_u16(),
                        status.canonical_reason().unwrap_or_default()
                    )
                }),
            },
        }
    }
}

/// The human-readable part of an error body: `{"message": ...}`, `{"error": ...}`,
/// GitLab's `{"message": {"field": ["is invalid"]}}`, or GitHub's `errors[].message`.
pub fn api_message(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let mut parts = Vec::new();
    for key in ["message", "error", "error_description"] {
        match v.get(key) {
            Some(Value::String(s)) if !s.is_empty() => parts.push(s.clone()),
            Some(Value::Object(map)) => {
                for (k, v) in map {
                    let text = match v {
                        Value::Array(a) => a
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", "),
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    parts.push(format!("{k} {text}"));
                }
            }
            Some(Value::Array(a)) => {
                parts.extend(a.iter().filter_map(Value::as_str).map(str::to_string))
            }
            _ => {}
        }
        if !parts.is_empty() {
            break;
        }
    }
    if let Some(Value::Array(errors)) = v.get("errors") {
        for e in errors {
            if let Some(m) = e.get("message").and_then(Value::as_str) {
                parts.push(m.to_string());
            } else if let Some(s) = e.as_str() {
                parts.push(s.to_string());
            }
        }
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

fn header_num(h: &HeaderMap, name: &str) -> Option<u64> {
    h.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// URL of `rel="next"` in an RFC 8288 `Link` header.
pub fn next_link(h: &HeaderMap) -> Option<String> {
    let link = h.get("link")?.to_str().ok()?;
    link.split(',').find_map(|part| {
        let (url, params) = part.split_once(';')?;
        params
            .split(';')
            .any(|p| p.trim().replace(' ', "") == "rel=\"next\"")
            .then(|| {
                url.trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string()
            })
    })
}

fn page_param(url: &str) -> Option<u32> {
    let query = url.split_once('?')?.1;
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("page="))
        .and_then(|v| v.parse().ok())
}

/// Percent-encode a path segment or query value (RFC 3986 unreserved characters stay).
pub fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Encode a file path for use in a URL path, keeping the slashes.
pub fn enc_path(p: &str) -> String {
    p.split('/').map(enc).collect::<Vec<_>>().join("/")
}

/// Build `?a=b&c=d` from optional pairs, skipping empty values.
pub fn query(pairs: &[(&str, Option<String>)]) -> String {
    let parts: Vec<String> = pairs
        .iter()
        .filter_map(|(k, v)| {
            v.as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| format!("{k}={}", enc(v)))
        })
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn parses_link_header() {
        let mut h = HeaderMap::new();
        h.insert(
            "link",
            HeaderValue::from_static(
                r#"<https://x/api?page=3&per_page=2>; rel="next", <https://x/api?page=9>; rel="last""#,
            ),
        );
        assert_eq!(
            next_link(&h).as_deref(),
            Some("https://x/api?page=3&per_page=2")
        );
        assert_eq!(page_param("https://x/api?page=3&per_page=2"), Some(3));
    }

    #[test]
    fn extracts_api_messages() {
        assert_eq!(
            api_message(r#"{"message":"Bad credentials"}"#).as_deref(),
            Some("Bad credentials")
        );
        assert_eq!(
            api_message(r#"{"message":{"source_branch":["is invalid"]}}"#).as_deref(),
            Some("source_branch is invalid")
        );
        assert_eq!(
            api_message(r#"{"message":"Validation Failed","errors":[{"message":"A pull request already exists"}]}"#).as_deref(),
            Some("Validation Failed; A pull request already exists")
        );
        assert_eq!(api_message("<html>"), None);
    }

    #[test]
    fn encodes() {
        assert_eq!(enc("acme/shop"), "acme%2Fshop");
        assert_eq!(enc_path("app/My File.php"), "app/My%20File.php");
        assert_eq!(
            query(&[
                ("state", Some("opened".into())),
                ("search", None),
                ("q", Some("a b".into()))
            ]),
            "?state=opened&q=a%20b"
        );
    }
}
