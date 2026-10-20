//! A small HTTP/1.1 client for the Docker Engine API and the Kubernetes API server: one
//! connection per request over a Unix socket, a Windows named pipe or TCP (optionally TLS),
//! `Content-Length`, chunked and read-to-close bodies, and streaming bodies for logs, events
//! and pulls.

use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::{err, Error, Result};

pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

pub type BoxIo = Box<dyn Io>;

/// Where to connect.
#[derive(Clone)]
pub enum Transport {
    Unix(String),
    NamedPipe(String),
    Tcp {
        host: String,
        port: u16,
        tls: Option<Arc<rustls::ClientConfig>>,
        /// SNI / certificate name (defaults to `host`).
        server_name: Option<String>,
    },
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Transport::Unix(p) => write!(f, "unix://{p}"),
            Transport::NamedPipe(p) => write!(f, "npipe://{p}"),
            Transport::Tcp {
                host, port, tls, ..
            } => {
                write!(
                    f,
                    "{}://{host}:{port}",
                    if tls.is_some() { "https" } else { "tcp" }
                )
            }
        }
    }
}

impl Transport {
    /// `Host` header value.
    pub fn host_header(&self) -> String {
        match self {
            Transport::Tcp { host, port, .. } => {
                if host.contains(':') {
                    format!("[{host}]:{port}")
                } else {
                    format!("{host}:{port}")
                }
            }
            // The Engine ignores the host on sockets; Docker's own clients send this.
            _ => "docker".into(),
        }
    }

    pub async fn connect(&self) -> Result<BoxIo> {
        match self {
            #[cfg(unix)]
            Transport::Unix(path) => {
                let s = tokio::net::UnixStream::connect(path)
                    .await
                    .map_err(|e| Error(format!("Cannot connect to {path}: {e}")))?;
                Ok(Box::new(s))
            }
            #[cfg(not(unix))]
            Transport::Unix(path) => err(format!("Unix sockets are not available here ({path})")),
            #[cfg(windows)]
            Transport::NamedPipe(path) => {
                use tokio::net::windows::named_pipe::ClientOptions;
                let mut tries = 0;
                loop {
                    match ClientOptions::new().open(path) {
                        Ok(p) => return Ok(Box::new(p)),
                        // ERROR_PIPE_BUSY: wait for a free instance.
                        Err(e) if e.raw_os_error() == Some(231) && tries < 20 => {
                            tries += 1;
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                        Err(e) => return err(format!("Cannot connect to {path}: {e}")),
                    }
                }
            }
            #[cfg(not(windows))]
            Transport::NamedPipe(path) => err(format!(
                "Named pipes are only available on Windows ({path})"
            )),
            Transport::Tcp {
                host,
                port,
                tls,
                server_name,
            } => {
                let tcp = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    tokio::net::TcpStream::connect((host.as_str(), *port)),
                )
                .await
                .map_err(|_| Error(format!("Connecting to {host}:{port} timed out")))?
                .map_err(|e| Error(format!("Cannot connect to {host}:{port}: {e}")))?;
                tcp.set_nodelay(true).ok();
                match tls {
                    None => Ok(Box::new(tcp)),
                    Some(config) => {
                        let name = server_name.as_deref().unwrap_or(host);
                        let name = ServerName::try_from(name.to_string())
                            .map_err(|e| Error(format!("Invalid server name `{name}`: {e}")))?;
                        let s = tokio_rustls::TlsConnector::from(config.clone())
                            .connect(name, tcp)
                            .await
                            .map_err(|e| {
                                Error(format!("TLS handshake with {host}:{port} failed: {e}"))
                            })?;
                        Ok(Box::new(s))
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub method: String,
    /// Path with query (`/v1.43/containers/json?all=1`).
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl Request {
    pub fn new(method: &str, path: impl Into<String>) -> Self {
        Request {
            method: method.into(),
            path: path.into(),
            ..Default::default()
        }
    }

    pub fn header(mut self, k: &str, v: impl Into<String>) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }

    pub fn json(mut self, body: &serde_json::Value) -> Self {
        self.body = Some(body.to_string().into_bytes());
        self.header("Content-Type", "application/json")
    }
}

enum BodyMode {
    Length(u64),
    Chunked { left: u64, done: bool },
    Close,
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    reader: BufReader<BoxIo>,
    mode: BodyMode,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The next piece of the body; `None` at the end.
    pub async fn chunk(&mut self) -> Result<Option<Vec<u8>>> {
        let mut buf = vec![0u8; 16 * 1024];
        match &mut self.mode {
            BodyMode::Length(left) => {
                if *left == 0 {
                    return Ok(None);
                }
                let want = (*left).min(buf.len() as u64) as usize;
                let n = self.reader.read(&mut buf[..want]).await?;
                if n == 0 {
                    return err("Connection closed in the middle of a response");
                }
                *left -= n as u64;
                buf.truncate(n);
                Ok(Some(buf))
            }
            BodyMode::Close => {
                let n = self.reader.read(&mut buf).await?;
                if n == 0 {
                    return Ok(None);
                }
                buf.truncate(n);
                Ok(Some(buf))
            }
            BodyMode::Chunked { left, done } => {
                if *done {
                    return Ok(None);
                }
                if *left == 0 {
                    let mut line = String::new();
                    if self.reader.read_line(&mut line).await? == 0 {
                        // Servers that stop streaming may just close.
                        *done = true;
                        return Ok(None);
                    }
                    let size = line.trim().split(';').next().unwrap_or("");
                    let size = u64::from_str_radix(size, 16)
                        .map_err(|_| Error(format!("Bad chunk size `{}`", line.trim())))?;
                    if size == 0 {
                        // Trailers until the empty line.
                        loop {
                            let mut t = String::new();
                            if self.reader.read_line(&mut t).await? == 0 || t.trim().is_empty() {
                                break;
                            }
                        }
                        *done = true;
                        return Ok(None);
                    }
                    *left = size;
                }
                let want = (*left).min(buf.len() as u64) as usize;
                let n = self.reader.read(&mut buf[..want]).await?;
                if n == 0 {
                    return err("Connection closed in the middle of a chunk");
                }
                *left -= n as u64;
                if *left == 0 {
                    let mut crlf = String::new();
                    self.reader.read_line(&mut crlf).await?;
                }
                buf.truncate(n);
                Ok(Some(buf))
            }
        }
    }

    /// The whole body.
    pub async fn bytes(mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        while let Some(c) = self.chunk().await? {
            out.extend_from_slice(&c);
        }
        Ok(out)
    }

    pub async fn json(self) -> Result<serde_json::Value> {
        let body = self.bytes().await?;
        if body.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        Ok(serde_json::from_slice(&body)?)
    }

    /// Turn 4xx/5xx into an error with the server's message (`{"message": …}`).
    pub async fn error_for_status(self) -> Result<Response> {
        if self.status < 400 {
            return Ok(self);
        }
        let status = self.status;
        let body = self.bytes().await.unwrap_or_default();
        let text = String::from_utf8_lossy(&body);
        let message = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("message")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| text.trim().to_string());
        err(match status {
            401 => format!("Unauthorized: {message}"),
            403 => format!("Forbidden: {message}"),
            404 => format!("Not found: {message}"),
            _ if message.is_empty() => format!("HTTP {status}"),
            _ => message,
        })
    }
}

/// Send a request over a fresh connection and read the response head.
pub async fn send(transport: &Transport, req: &Request) -> Result<Response> {
    let io = transport.connect().await?;
    send_on(io, &transport.host_header(), req).await
}

pub async fn send_on(mut io: BoxIo, host: &str, req: &Request) -> Result<Response> {
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Forge\r\nConnection: close\r\n",
        req.method, req.path
    );
    for (k, v) in &req.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    let body = req.body.as_deref().unwrap_or_default();
    if req.body.is_some() || matches!(req.method.as_str(), "POST" | "PUT" | "PATCH") {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    io.write_all(head.as_bytes()).await?;
    if !body.is_empty() {
        io.write_all(body).await?;
    }
    io.flush().await?;

    let mut reader = BufReader::new(io);
    let mut status_line = String::new();
    loop {
        status_line.clear();
        if reader.read_line(&mut status_line).await? == 0 {
            return err("The server closed the connection without answering");
        }
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error(format!("Not an HTTP response: `{}`", status_line.trim())))?;
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await? == 0 {
                break;
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        // Skip interim responses (100 Continue).
        if (100..200).contains(&status) && status != 101 {
            continue;
        }
        let get = |name: &str| {
            headers
                .iter()
                .find(|(k, _): &&(String, String)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        };
        let mode = if req.method == "HEAD" || status == 204 || status == 304 {
            BodyMode::Length(0)
        } else if get("Transfer-Encoding")
            .is_some_and(|t| t.to_ascii_lowercase().contains("chunked"))
        {
            BodyMode::Chunked {
                left: 0,
                done: false,
            }
        } else if let Some(n) = get("Content-Length").and_then(|n| n.parse().ok()) {
            BodyMode::Length(n)
        } else {
            BodyMode::Close
        };
        return Ok(Response {
            status,
            headers,
            reader,
            mode,
        });
    }
}

// ---- TLS -------------------------------------------------------------------------------

/// Certificates and key for a TLS connection, as PEM text.
#[derive(Debug, Clone, Default)]
pub struct TlsOptions {
    pub ca_pem: Option<String>,
    pub client_cert_pem: Option<String>,
    pub client_key_pem: Option<String>,
    /// Skip certificate verification (`insecure-skip-tls-verify`).
    pub insecure: bool,
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

pub fn tls_config(opts: &TlsOptions) -> Result<Arc<rustls::ClientConfig>> {
    let builder = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| Error(format!("TLS setup failed: {e}")))?;
    let builder = if opts.insecure {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify(provider())))
    } else {
        let mut roots = rustls::RootCertStore::empty();
        match &opts.ca_pem {
            Some(pem) => {
                let mut n = 0;
                for cert in CertificateDer::pem_slice_iter(pem.as_bytes()) {
                    let cert = cert.map_err(|e| Error(format!("Invalid CA certificate: {e:?}")))?;
                    roots
                        .add(cert)
                        .map_err(|e| Error(format!("Invalid CA certificate: {e}")))?;
                    n += 1;
                }
                if n == 0 {
                    return err("The CA file contains no certificate");
                }
            }
            None => roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        }
        builder.with_root_certificates(roots)
    };
    let config = match (&opts.client_cert_pem, &opts.client_key_pem) {
        (Some(cert), Some(key)) => {
            let certs: Vec<CertificateDer<'static>> =
                CertificateDer::pem_slice_iter(cert.as_bytes())
                    .collect::<std::result::Result<_, _>>()
                    .map_err(|e| Error(format!("Invalid client certificate: {e:?}")))?;
            let key = PrivateKeyDer::from_pem_slice(key.as_bytes())
                .map_err(|e| Error(format!("Invalid client key: {e:?}")))?;
            builder
                .with_client_auth_cert(certs, key)
                .map_err(|e| Error(format!("Invalid client certificate: {e}")))?
        }
        _ => builder.with_no_client_auth(),
    };
    Ok(Arc::new(config))
}

#[derive(Debug)]
struct NoVerify(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Split an `https://host:port/prefix` URL into host, port and path prefix.
pub fn split_url(url: &str) -> Result<(bool, String, u16, String)> {
    let (https, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return err(format!("Unsupported server URL `{url}`"));
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].trim_end_matches('/').to_string()),
        None => (rest, String::new()),
    };
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        let end = stripped
            .find(']')
            .ok_or_else(|| Error(format!("Invalid server URL `{url}`")))?;
        let host = &stripped[..end];
        let port = stripped[end + 1..]
            .strip_prefix(':')
            .and_then(|p| p.parse().ok());
        (host.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (
                h.to_string(),
                Some(
                    p.parse()
                        .map_err(|_| Error(format!("Invalid port in `{url}`")))?,
                ),
            ),
            None => (authority.to_string(), None),
        }
    };
    Ok((
        https,
        host,
        port.unwrap_or(if https { 443 } else { 80 }),
        path,
    ))
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixListener;

    /// A fake HTTP server on a temp Unix socket answering every connection with the
    /// response `respond` builds from the request head and body.
    pub(crate) async fn fake_server(
        respond: impl Fn(&str, &[u8]) -> Vec<u8> + Send + Sync + 'static,
    ) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("docker.sock")
            .to_string_lossy()
            .into_owned();
        let listener = UnixListener::bind(&path).unwrap();
        let respond = Arc::new(respond);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let respond = respond.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stream);
                    let mut head = String::new();
                    let mut len = 0usize;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                            return;
                        }
                        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                            len = v.trim().parse().unwrap_or(0);
                        }
                        let end = line == "\r\n";
                        head.push_str(&line);
                        if end {
                            break;
                        }
                    }
                    let mut body = vec![0u8; len];
                    reader.read_exact(&mut body).await.unwrap();
                    let out = respond(&head, &body);
                    let mut stream = reader.into_inner();
                    for piece in out.chunks(7) {
                        stream.write_all(piece).await.unwrap();
                        stream.flush().await.unwrap();
                    }
                    stream.shutdown().await.ok();
                });
            }
        });
        (dir, path)
    }

    pub(crate) fn ok_json(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    pub(crate) fn chunked(status: &str, parts: &[&[u8]]) -> Vec<u8> {
        let mut out =
            format!("HTTP/1.1 {status}\r\nTransfer-Encoding: chunked\r\n\r\n").into_bytes();
        for p in parts {
            out.extend(format!("{:x}\r\n", p.len()).into_bytes());
            out.extend_from_slice(p);
            out.extend(b"\r\n");
        }
        out.extend(b"0\r\n\r\n");
        out
    }

    #[tokio::test]
    async fn content_length_and_chunked_bodies() {
        let (_dir, path) = fake_server(|head, body| {
            if head.starts_with("GET /length") {
                ok_json("{\"a\":1}")
            } else if head.starts_with("POST /echo") {
                ok_json(std::str::from_utf8(body).unwrap())
            } else if head.starts_with("GET /chunked") {
                chunked("200 OK", &[b"hello ", b"chunked ", b"world"])
            } else if head.starts_with("GET /close") {
                b"HTTP/1.1 200 OK\r\n\r\nuntil close".to_vec()
            } else {
                let body = "{\"message\":\"no such container\"}";
                format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .into_bytes()
            }
        })
        .await;
        let t = Transport::Unix(path);
        let v = send(&t, &Request::new("GET", "/length"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(v["a"], 1);
        let v = send(
            &t,
            &Request::new("POST", "/echo").json(&serde_json::json!({ "x": "y" })),
        )
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(v["x"], "y");
        let body = send(&t, &Request::new("GET", "/chunked"))
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(body, b"hello chunked world");
        let body = send(&t, &Request::new("GET", "/close"))
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(body, b"until close");
        let e = send(&t, &Request::new("GET", "/missing"))
            .await
            .unwrap()
            .error_for_status()
            .await
            .err()
            .unwrap();
        assert_eq!(e.0, "Not found: no such container");
    }

    #[tokio::test]
    async fn connect_errors_are_readable() {
        let e = send(
            &Transport::Unix("/nonexistent/forge.sock".into()),
            &Request::new("GET", "/_ping"),
        )
        .await
        .err()
        .unwrap();
        assert!(
            e.0.starts_with("Cannot connect to /nonexistent/forge.sock"),
            "{e}"
        );
    }

    #[test]
    fn urls_and_tls() {
        assert_eq!(
            split_url("https://127.0.0.1:6443").unwrap(),
            (true, "127.0.0.1".into(), 6443, String::new())
        );
        assert_eq!(
            split_url("https://k8s.example.com/prefix/").unwrap(),
            (true, "k8s.example.com".into(), 443, "/prefix".into())
        );
        assert_eq!(split_url("https://[::1]:6443").unwrap().1, "::1");
        assert!(split_url("ftp://x").is_err());
        assert!(tls_config(&TlsOptions::default()).is_ok());
        assert!(tls_config(&TlsOptions {
            insecure: true,
            ..Default::default()
        })
        .is_ok());
        assert!(tls_config(&TlsOptions {
            ca_pem: Some("garbage".into()),
            ..Default::default()
        })
        .is_err());
    }
}
