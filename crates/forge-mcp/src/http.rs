//! A small HTTP/1.1 subset for the loopback MCP endpoint and its clients (the stdio proxy and
//! the tests): one request per connection, `Content-Length` bodies, hard size limits.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_HEAD_BYTES: usize = 16 * 1024;
/// Write tools carry whole files.
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    /// Lowercase names.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query
            .split('&')
            .find_map(|p| p.strip_prefix(name)?.strip_prefix('='))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    Malformed,
    HeadTooLarge,
    BodyTooLarge,
    Unsupported,
    Closed,
}

impl HttpError {
    pub fn status(&self) -> u16 {
        match self {
            HttpError::HeadTooLarge => 431,
            HttpError::BodyTooLarge => 413,
            HttpError::Unsupported => 411,
            HttpError::Malformed | HttpError::Closed => 400,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            HttpError::Malformed => "malformed request",
            HttpError::HeadTooLarge => "request headers too large",
            HttpError::BodyTooLarge => "request body too large",
            HttpError::Unsupported => "chunked bodies are not supported",
            HttpError::Closed => "connection closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        411 => "Length Required",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    }
}

impl Response {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        Self::new(status)
            .with_header("Content-Type", "application/json")
            .with_body(value.to_string().into_bytes())
    }

    /// A plain error (not JSON-RPC): `{ "error": message }`.
    pub fn error(status: u16, message: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": message }))
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
            self.status,
            reason(self.status),
            self.body.len()
        );
        for (k, v) in &self.headers {
            out.push_str(&format!("{k}: {v}\r\n"));
        }
        out.push_str("\r\n");
        let mut bytes = out.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

type Head = (String, Vec<(String, String)>);

/// Split the head into its first line and lowercase-named headers.
fn parse_head(head: &[u8]) -> Result<Head, HttpError> {
    let text = std::str::from_utf8(head).map_err(|_| HttpError::Malformed)?;
    let mut lines = text.split("\r\n");
    let first = lines.next().ok_or(HttpError::Malformed)?.to_string();
    let mut headers = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (k, v) = line.split_once(':').ok_or(HttpError::Malformed)?;
        if k.is_empty() || k.contains(' ') {
            return Err(HttpError::Malformed);
        }
        headers.push((k.to_ascii_lowercase(), v.trim().to_string()));
    }
    Ok((first, headers))
}

/// Read a head and a `Content-Length` body (requests and responses alike).
async fn read_message<R: AsyncRead + Unpin>(
    stream: &mut R,
) -> Result<(String, Vec<(String, String)>, Vec<u8>), HttpError> {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(end) = find_head_end(&buf) {
            break end;
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Err(HttpError::HeadTooLarge);
        }
        let n = stream.read(&mut chunk).await.map_err(|_| HttpError::Closed)?;
        if n == 0 {
            return Err(HttpError::Closed);
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    if head_end > MAX_HEAD_BYTES {
        return Err(HttpError::HeadTooLarge);
    }
    let (first, headers) = parse_head(&buf[..head_end])?;
    let header = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    if header("transfer-encoding").is_some() {
        return Err(HttpError::Unsupported);
    }
    let length = match header("content-length") {
        Some(v) => v.parse::<usize>().map_err(|_| HttpError::Malformed)?,
        None => 0,
    };
    if length > MAX_BODY_BYTES {
        return Err(HttpError::BodyTooLarge);
    }
    let mut body = buf[head_end + 4..].to_vec();
    body.truncate(length);
    while body.len() < length {
        let n = stream.read(&mut chunk).await.map_err(|_| HttpError::Closed)?;
        if n == 0 {
            return Err(HttpError::Closed);
        }
        let take = n.min(length - body.len());
        body.extend_from_slice(&chunk[..take]);
    }
    Ok((first, headers, body))
}

/// Read one request. The caller applies a timeout.
pub async fn read_request<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Request, HttpError> {
    let (first, headers, body) = read_message(stream).await?;
    let mut parts = first.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HttpError::Malformed);
    };
    if !version.starts_with("HTTP/1.")
        || method.is_empty()
        || !method.bytes().all(|b| b.is_ascii_uppercase())
        || !target.starts_with('/')
    {
        return Err(HttpError::Malformed);
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    Ok(Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers,
        body,
    })
}

/// Read one response (the client side).
pub async fn read_response<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Response, HttpError> {
    let (first, headers, body) = read_message(stream).await?;
    let status = first
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|_| first.starts_with("HTTP/1."))
        .ok_or(HttpError::Malformed)?;
    Ok(Response {
        status,
        headers,
        body,
    })
}

pub async fn write_response<W: AsyncWrite + Unpin>(
    stream: &mut W,
    response: &Response,
) -> std::io::Result<()> {
    stream.write_all(&response.to_bytes()).await?;
    stream.flush().await
}

/// Send one request to `127.0.0.1:port` and read the answer.
pub async fn send(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<Response> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    read_response(&mut stream)
        .await
        .map_err(|e| std::io::Error::other(e.message()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_requests_and_responses() {
        let mut raw: &[u8] = b"POST /mcp?sessionId=ab HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: 2\r\n\r\n{}";
        let req = read_request(&mut raw).await.unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/mcp");
        assert_eq!(req.query_param("sessionId"), Some("ab"));
        assert_eq!(req.body, b"{}");
        let resp = Response::json(200, &serde_json::json!({ "a": 1 }))
            .with_header("Mcp-Session-Id", "s1")
            .to_bytes();
        let back = read_response(&mut resp.as_slice()).await.unwrap();
        assert_eq!(back.status, 200);
        assert_eq!(back.header("mcp-session-id"), Some("s1"));
        assert_eq!(back.body, b"{\"a\":1}");
    }

    #[tokio::test]
    async fn enforces_limits() {
        let big = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY_BYTES + 1);
        assert_eq!(
            read_request(&mut big.as_bytes()).await,
            Err(HttpError::BodyTooLarge)
        );
        let mut chunked: &[u8] = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert_eq!(read_request(&mut chunked).await, Err(HttpError::Unsupported));
        let mut bad: &[u8] = b"GET http://evil/ HTTP/1.1\r\n\r\n";
        assert_eq!(read_request(&mut bad).await, Err(HttpError::Malformed));
    }
}
