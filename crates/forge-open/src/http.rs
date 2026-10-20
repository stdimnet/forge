//! A deliberately tiny HTTP/1.1 subset for the local Browser Companion API: one request
//! per connection, `Content-Length` bodies only, small hard limits.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Request line plus headers.
pub const MAX_HEAD_BYTES: usize = 8 * 1024;
pub const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// Path without the query string.
    pub path: String,
    pub query: String,
    /// Header names are lowercase.
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
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    #[error("malformed request")]
    Malformed,
    #[error("request headers too large")]
    HeadTooLarge,
    #[error("request body too large")]
    BodyTooLarge,
    #[error("chunked bodies are not supported")]
    Unsupported,
    #[error("connection closed")]
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        Self {
            status,
            body: value.to_string(),
        }
    }

    pub fn error(status: u16, message: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": message }))
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            411 => "Length Required",
            413 => "Payload Too Large",
            415 => "Unsupported Media Type",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            _ => "Error",
        };
        let mut out = format!(
            "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
            self.status,
            self.body.len()
        );
        if !self.body.is_empty() {
            out.push_str("Content-Type: application/json; charset=utf-8\r\n");
        }
        out.push_str("\r\n");
        out.push_str(&self.body);
        out.into_bytes()
    }
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Method, request target and lowercase-named headers.
pub type Head = (String, String, Vec<(String, String)>);

/// Parse the request line and headers (everything before the blank line).
pub fn parse_head(head: &[u8]) -> Result<Head, HttpError> {
    let text = std::str::from_utf8(head).map_err(|_| HttpError::Malformed)?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next().ok_or(HttpError::Malformed)?;
    let mut parts = request_line.split(' ');
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
    let mut headers = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (k, v) = line.split_once(':').ok_or(HttpError::Malformed)?;
        if k.is_empty() || k.contains(' ') {
            return Err(HttpError::Malformed);
        }
        headers.push((k.to_ascii_lowercase(), v.trim().to_string()));
    }
    Ok((method.to_string(), target.to_string(), headers))
}

/// Read one request. The caller applies a timeout.
pub async fn read_request<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Request, HttpError> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    let head_end = loop {
        if let Some(end) = find_head_end(&buf) {
            break end;
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Err(HttpError::HeadTooLarge);
        }
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|_| HttpError::Closed)?;
        if n == 0 {
            return Err(HttpError::Closed);
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    if head_end > MAX_HEAD_BYTES {
        return Err(HttpError::HeadTooLarge);
    }
    let (method, target, headers) = parse_head(&buf[..head_end])?;
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
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
    if body.len() > length {
        body.truncate(length);
    }
    while body.len() < length {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|_| HttpError::Closed)?;
        if n == 0 {
            return Err(HttpError::Closed);
        }
        let take = n.min(length - body.len());
        body.extend_from_slice(&chunk[..take]);
    }
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    Ok(Request {
        method,
        path: path.to_string(),
        query: query.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn read(raw: &[u8]) -> Result<Request, HttpError> {
        let mut r = raw;
        read_request(&mut r).await
    }

    #[tokio::test]
    async fn reads_a_post_with_body() {
        let req = read(
            b"POST /api/forge/open?x=1 HTTP/1.1\r\nHost: 127.0.0.1:63342\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"file\":\"/a\"}",
        )
        .await
        .unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/api/forge/open");
        assert_eq!(req.query, "x=1");
        assert_eq!(req.header("host"), Some("127.0.0.1:63342"));
        assert_eq!(req.body, b"{\"file\":\"/a\"}");
    }

    #[tokio::test]
    async fn enforces_limits() {
        let big_head = format!(
            "GET / HTTP/1.1\r\nX: {}\r\n\r\n",
            "a".repeat(MAX_HEAD_BYTES)
        );
        assert_eq!(
            read(big_head.as_bytes()).await,
            Err(HttpError::HeadTooLarge)
        );
        let big_body = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        assert_eq!(
            read(big_body.as_bytes()).await,
            Err(HttpError::BodyTooLarge)
        );
        assert_eq!(
            read(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n").await,
            Err(HttpError::Unsupported)
        );
        assert_eq!(
            read(b"POST / HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc").await,
            Err(HttpError::Closed)
        );
    }

    #[tokio::test]
    async fn rejects_malformed_heads() {
        for raw in [
            &b"GARBAGE\r\n\r\n"[..],
            b"GET http://evil/ HTTP/1.1\r\n\r\n",
            b"get / HTTP/1.1\r\n\r\n",
            b"GET / HTTP/1.1\r\nBad Header\r\n\r\n",
            b"GET / SPDY/3\r\n\r\n",
        ] {
            assert_eq!(read(raw).await, Err(HttpError::Malformed), "{raw:?}");
        }
    }

    #[test]
    fn serializes_responses() {
        let bytes = Response::error(403, "nope").to_bytes();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("HTTP/1.1 403 Forbidden\r\n"));
        assert!(text.contains("Content-Length: 16\r\n"));
        assert!(text.ends_with("\r\n\r\n{\"error\":\"nope\"}"));
    }
}
