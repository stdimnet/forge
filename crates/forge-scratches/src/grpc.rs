//! `GRPC` requests (feature `client`): unary calls like PHPStorm's,
//!
//! ```http
//! GRPC localhost:50051/shop.Users/GetUser
//!
//! { "id": 7 }
//! ```
//!
//! The message types come from the server's reflection service
//! (`grpc.reflection.v1.ServerReflection`, then `v1alpha`), or, when the server has none,
//! from `.proto` files the caller provides. The JSON body is encoded with the method's
//! input type (prost-reflect), sent over HTTP/2 (h2; TLS with rustls for `https://` /
//! `grpcs://` URLs) and the answer is decoded back to JSON. Streaming methods are not
//! supported.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use prost_reflect::prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, MethodDescriptor};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::http::HttpRequest;

/// Where a call goes: from the resolved request URL `http(s)://host:port/pkg.Service/Method`
/// (`grpc://` and `grpcs://` too).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// `pkg.Service`.
    pub service: String,
    pub method: String,
}

pub fn target(url: &str) -> Result<Target, String> {
    let (scheme, rest) = url.split_once("://").unwrap_or(("http", url));
    let tls = matches!(scheme, "https" | "grpcs");
    let (authority, path) = rest.split_once('/').ok_or_else(|| {
        format!("'{url}': expected host:port/package.Service/Method")
    })?;
    let (service, method) = path
        .trim_end_matches('/')
        .rsplit_once('/')
        .ok_or_else(|| format!("'{url}': expected host:port/package.Service/Method"))?;
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().map_err(|_| format!("Invalid port in '{url}'"))?),
        None => (authority.to_string(), if tls { 443 } else { 80 }),
    };
    if service.is_empty() || method.is_empty() {
        return Err(format!("'{url}': expected host:port/package.Service/Method"));
    }
    Ok(Target {
        host: host.trim_start_matches('[').trim_end_matches(']').to_string(),
        port,
        tls,
        service: service.to_string(),
        method: method.to_string(),
    })
}

// ---- Protobuf wire helpers (reflection messages) --------------------------------------

fn put_varint(buf: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
    buf.push(v as u8);
}

fn put_bytes_field(buf: &mut Vec<u8>, field: u32, data: &[u8]) {
    put_varint(buf, u64::from(field << 3 | 2));
    put_varint(buf, data.len() as u64);
    buf.extend_from_slice(data);
}

fn get_varint(buf: &mut &[u8]) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *buf.first()?;
        *buf = &buf[1..];
        v |= u64::from(b & 0x7f) << shift;
        if b < 0x80 {
            return Some(v);
        }
    }
    None
}

/// The fields of a message: `(number, wire type, value)` where length-delimited values
/// are their bytes and varints their value (as 8 little-endian bytes).
fn fields(mut buf: &[u8]) -> Vec<(u32, u8, Vec<u8>)> {
    let mut out = Vec::new();
    while !buf.is_empty() {
        let Some(key) = get_varint(&mut buf) else { break };
        let (num, wt) = ((key >> 3) as u32, (key & 7) as u8);
        match wt {
            0 => {
                let Some(v) = get_varint(&mut buf) else { break };
                out.push((num, wt, v.to_le_bytes().to_vec()));
            }
            1 => {
                if buf.len() < 8 {
                    break;
                }
                out.push((num, wt, buf[..8].to_vec()));
                buf = &buf[8..];
            }
            2 => {
                let Some(len) = get_varint(&mut buf) else { break };
                let len = len as usize;
                if buf.len() < len {
                    break;
                }
                out.push((num, wt, buf[..len].to_vec()));
                buf = &buf[len..];
            }
            5 => {
                if buf.len() < 4 {
                    break;
                }
                out.push((num, wt, buf[..4].to_vec()));
                buf = &buf[4..];
            }
            _ => break,
        }
    }
    out
}

fn string_fields(buf: &[u8], field: u32) -> Vec<String> {
    fields(buf)
        .into_iter()
        .filter(|(n, wt, _)| *n == field && *wt == 2)
        .map(|(_, _, v)| String::from_utf8_lossy(&v).into_owned())
        .collect()
}

/// gRPC message framing: compression flag, big-endian length, message.
fn frame(message: &[u8]) -> Bytes {
    let mut b = BytesMut::with_capacity(message.len() + 5);
    b.put_u8(0);
    b.put_u32(message.len() as u32);
    b.extend_from_slice(message);
    b.freeze()
}

fn unframe(mut data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    while data.len() >= 5 {
        if data[0] != 0 {
            return Err("The server sent a compressed message, which Forge cannot read".into());
        }
        let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
        if data.len() < 5 + len {
            return Err("Truncated gRPC message".into());
        }
        out.push(data[5..5 + len].to_vec());
        data = &data[5 + len..];
    }
    Ok(out)
}

// ---- Transport ---------------------------------------------------------------------------

/// The answer of one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResponse {
    pub http_status: u16,
    pub headers: Vec<(String, String)>,
    pub messages: Vec<Vec<u8>>,
    /// `grpc-status` (0 = OK) and `grpc-message`, from the trailers (or headers).
    pub status: Option<u32>,
    pub message: String,
}

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

async fn connect(t: &Target) -> Result<Box<dyn Io>, String> {
    let tcp = TcpStream::connect((t.host.as_str(), t.port))
        .await
        .map_err(|e| format!("Connection failed: {e}"))?;
    if !t.tls {
        return Ok(Box::new(tcp));
    }
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec()];
    let name = rustls::pki_types::ServerName::try_from(t.host.clone())
        .map_err(|e| format!("Invalid host '{}': {e}", t.host))?;
    let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .map_err(|e| format!("TLS handshake failed: {e}"))?;
    Ok(Box::new(tls))
}

/// One unary call: `path` is `/pkg.Service/Method`, `body` the encoded request message.
pub async fn call_raw(
    t: &Target,
    path: &str,
    metadata: &[(String, String)],
    body: &[u8],
) -> Result<RawResponse, String> {
    let io = connect(t).await?;
    let (client, conn) = h2::client::handshake(io)
        .await
        .map_err(|e| format!("HTTP/2 handshake failed: {e}"))?;
    let conn_task = tokio::spawn(async move {
        let _ = conn.await;
    });
    let mut client = client.ready().await.map_err(|e| e.to_string())?;
    let scheme = if t.tls { "https" } else { "http" };
    let mut builder = http::Request::builder()
        .method("POST")
        .uri(format!("{scheme}://{}:{}{path}", t.host, t.port))
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .header("user-agent", concat!("forge-http-client/", env!("CARGO_PKG_VERSION")));
    for (k, v) in metadata {
        let k = k.to_ascii_lowercase();
        if matches!(k.as_str(), "content-type" | "te" | "host" | "content-length") {
            continue;
        }
        builder = builder.header(k, v);
    }
    let request = builder.body(()).map_err(|e| e.to_string())?;
    let (response, mut send) = client
        .send_request(request, false)
        .map_err(|e| format!("Cannot send the request: {e}"))?;
    send.send_data(frame(body), true)
        .map_err(|e| format!("Cannot send the request: {e}"))?;
    let response = response.await.map_err(|e| format!("The call failed: {e}"))?;
    let http_status = response.status().as_u16();
    let header = |h: &http::HeaderMap, k: &str| h.get(k).and_then(|v| v.to_str().ok()).map(str::to_string);
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();
    let mut status = header(response.headers(), "grpc-status").and_then(|s| s.parse().ok());
    let mut message = header(response.headers(), "grpc-message").unwrap_or_default();
    let mut body = response.into_body();
    let mut data = Vec::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(|e| format!("Cannot read the response: {e}"))?;
        let _ = body.flow_control().release_capacity(chunk.len());
        data.extend_from_slice(chunk.chunk());
    }
    if let Ok(Some(trailers)) = body.trailers().await {
        if let Some(s) = header(&trailers, "grpc-status").and_then(|s| s.parse().ok()) {
            status = Some(s);
        }
        if let Some(m) = header(&trailers, "grpc-message") {
            message = m;
        }
    }
    conn_task.abort();
    Ok(RawResponse {
        http_status,
        headers,
        messages: unframe(&data)?,
        status,
        message: percent_decode(&message),
    })
}

fn percent_decode(s: &str) -> String {
    crate::oauth::parse_query(&format!("m={}", s.replace('+', "%2B")))
        .remove("m")
        .unwrap_or_default()
}

// ---- Descriptors -------------------------------------------------------------------------

const REFLECTION: &[&str] = &[
    "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo",
    "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo",
];

/// Ask the server's reflection service for the files defining `symbol` (and their
/// dependencies): encoded `FileDescriptorProto`s by file name.
async fn reflect_files(t: &Target, symbol: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut last_err = String::new();
    'versions: for path in REFLECTION {
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut wanted: Vec<(u32, String)> = vec![(4, symbol.to_string())];
        let mut asked: BTreeSet<String> = BTreeSet::new();
        while let Some((field, what)) = wanted.pop() {
            if !asked.insert(what.clone()) || asked.len() > 64 {
                continue;
            }
            let mut req = Vec::new();
            put_bytes_field(&mut req, 1, t.host.as_bytes());
            put_bytes_field(&mut req, field, what.as_bytes());
            let resp = call_raw(t, path, &[], &req).await?;
            if resp.status.is_some_and(|s| s != 0) || resp.messages.is_empty() {
                last_err = if resp.message.is_empty() {
                    format!("grpc-status {}", resp.status.unwrap_or(2))
                } else {
                    resp.message.clone()
                };
                continue 'versions;
            }
            for m in &resp.messages {
                for (num, _, v) in fields(m) {
                    match num {
                        4 => {
                            for file in fields(&v).into_iter().filter(|(n, _, _)| *n == 1) {
                                let name = string_fields(&file.2, 1).into_iter().next().unwrap_or_default();
                                for dep in string_fields(&file.2, 3) {
                                    if !files.contains_key(&dep) {
                                        wanted.push((3, dep));
                                    }
                                }
                                files.insert(name, file.2);
                            }
                        }
                        7 => {
                            let msg = string_fields(&v, 2).into_iter().next().unwrap_or_default();
                            return Err(format!("Server reflection: {msg}"));
                        }
                        _ => {}
                    }
                }
            }
        }
        return Ok(files);
    }
    Err(format!("The server has no reflection service ({last_err})"))
}

fn pool_from_files(files: &BTreeMap<String, Vec<u8>>) -> Result<DescriptorPool, String> {
    let mut set = Vec::new();
    for bytes in files.values() {
        put_bytes_field(&mut set, 1, bytes);
    }
    DescriptorPool::decode(set.as_slice()).map_err(|e| format!("Invalid descriptors from the server: {e}"))
}

/// In-memory `.proto` sources for protox (well-known types included).
struct Sources(BTreeMap<String, String>);

impl protox::file::FileResolver for Sources {
    fn open_file(&self, name: &str) -> Result<protox::file::File, protox::Error> {
        match self.0.get(name) {
            Some(text) => protox::file::File::from_source(name, text),
            None => Err(protox::Error::file_not_found(name)),
        }
    }
}

/// Compile `.proto` files (name relative to their import root → text).
pub fn pool_from_protos(protos: &BTreeMap<String, String>) -> Result<DescriptorPool, String> {
    let mut chain = protox::file::ChainFileResolver::new();
    chain.add(Sources(protos.clone()));
    chain.add(protox::file::GoogleFileResolver::new());
    let mut compiler = protox::Compiler::with_file_resolver(chain);
    compiler.include_imports(true);
    for name in protos.keys() {
        compiler
            .open_file(name)
            .map_err(|e| format!("Cannot compile {name}: {e}"))?;
    }
    Ok(compiler.descriptor_pool())
}

fn method_in(pool: &DescriptorPool, t: &Target) -> Option<MethodDescriptor> {
    pool.get_service_by_name(&t.service)?
        .methods()
        .find(|m| m.name() == t.method)
}

// ---- Calls ---------------------------------------------------------------------------------

/// A finished call, for the console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcResponse {
    /// Where the types came from: `server reflection` or `.proto files`.
    pub source: String,
    pub status: u32,
    pub message: String,
    /// The response messages as pretty JSON.
    pub json: Vec<String>,
    pub headers: Vec<(String, String)>,
    pub elapsed_ms: u64,
}

/// gRPC status code names.
pub fn status_name(code: u32) -> &'static str {
    const NAMES: &[&str] = &[
        "OK", "CANCELLED", "UNKNOWN", "INVALID_ARGUMENT", "DEADLINE_EXCEEDED", "NOT_FOUND",
        "ALREADY_EXISTS", "PERMISSION_DENIED", "RESOURCE_EXHAUSTED", "FAILED_PRECONDITION",
        "ABORTED", "OUT_OF_RANGE", "UNIMPLEMENTED", "INTERNAL", "UNAVAILABLE", "DATA_LOSS",
        "UNAUTHENTICATED",
    ];
    NAMES.get(code as usize).copied().unwrap_or("UNKNOWN")
}

/// Run a resolved `GRPC` request. `protos` are `.proto` sources used when the server has
/// no reflection service (or always, when `prefer_protos`).
pub async fn call(
    req: &HttpRequest,
    protos: &BTreeMap<String, String>,
    timeout: Duration,
) -> Result<GrpcResponse, String> {
    let t = target(&req.url)?;
    tokio::time::timeout(timeout, call_inner(req, &t, protos))
        .await
        .map_err(|_| "The call timed out".to_string())?
}

async fn call_inner(
    req: &HttpRequest,
    t: &Target,
    protos: &BTreeMap<String, String>,
) -> Result<GrpcResponse, String> {
    let mut source = "server reflection";
    let method = match reflect_files(t, &t.service).await {
        Ok(files) => pool_from_files(&files).ok().and_then(|p| method_in(&p, t)),
        Err(e) if protos.is_empty() => return Err(format!("{e}. Add the service's .proto files in Settings | Tools | HTTP Client.")),
        Err(_) => None,
    };
    let method = match method {
        Some(m) => m,
        None => {
            source = ".proto files";
            let pool = pool_from_protos(protos)?;
            method_in(&pool, t).ok_or_else(|| format!("Method {}/{} not found", t.service, t.method))?
        }
    };
    if method.is_client_streaming() || method.is_server_streaming() {
        return Err(format!("{}/{} is a streaming method; only unary calls are supported", t.service, t.method));
    }
    let body = req.body.as_deref().unwrap_or("{}");
    let body = if body.trim().is_empty() { "{}" } else { body };
    let mut de = serde_json::Deserializer::from_str(body);
    let input = DynamicMessage::deserialize(method.input(), &mut de)
        .map_err(|e| format!("The request body does not match {}: {e}", method.input().full_name()))?;
    let metadata: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|h| (h.name.clone(), h.value.clone()))
        .collect();
    let started = Instant::now();
    let path = format!("/{}/{}", t.service, t.method);
    let raw = call_raw(t, &path, &metadata, &input.encode_to_vec()).await?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let mut json = Vec::new();
    for m in &raw.messages {
        let msg = DynamicMessage::decode(method.output(), m.as_slice())
            .map_err(|e| format!("Cannot decode the response: {e}"))?;
        let mut out = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b"  "));
        msg.serialize_with_options(&mut ser, &prost_reflect::SerializeOptions::new().skip_default_fields(false))
            .map_err(|e| e.to_string())?;
        json.push(String::from_utf8_lossy(&out).into_owned());
    }
    Ok(GrpcResponse {
        source: source.into(),
        status: raw.status.unwrap_or(if raw.http_status == 200 { 0 } else { 2 }),
        message: raw.message,
        json,
        headers: raw.headers,
        elapsed_ms,
    })
}

/// Console text of a call.
pub fn render(resp: &GrpcResponse) -> String {
    let color = if resp.status == 0 { "\x1b[32m" } else { "\x1b[31m" };
    let mut out = format!("{color}{} {}\x1b[0m", resp.status, status_name(resp.status));
    if !resp.message.is_empty() {
        out.push_str(&format!(" {}", resp.message));
    }
    out.push('\n');
    for (k, v) in &resp.headers {
        if k != "content-type" {
            out.push_str(&format!("\x1b[36m{k}\x1b[0m: {v}\n"));
        }
    }
    out.push('\n');
    for j in &resp.json {
        out.push_str(j);
        out.push_str("\n\n");
    }
    out.push_str(&format!(
        "\x1b[90mStatus: {}; Time: {} ms; Types from {}\x1b[0m\n",
        status_name(resp.status),
        resp.elapsed_ms,
        resp.source
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::parse;

    const PROTO: &str = r#"syntax = "proto3";
package test;
import "google/protobuf/timestamp.proto";
service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
  rpc Fail (HelloRequest) returns (HelloReply);
}
message HelloRequest { string name = 1; int32 times = 2; }
message HelloReply { string message = 1; repeated string tags = 2; google.protobuf.Timestamp at = 3; }
"#;

    fn protos() -> BTreeMap<String, String> {
        [("test.proto".to_string(), PROTO.to_string())].into_iter().collect()
    }

    /// A gRPC server on h2: Greeter, and (with `reflection`) the v1 reflection service.
    async fn server(reflection: bool) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let pool = pool_from_protos(&protos()).unwrap();
        // Encoded FileDescriptorProtos by name, for reflection answers.
        let mut c = protox::Compiler::with_file_resolver({
            let mut chain = protox::file::ChainFileResolver::new();
            chain.add(Sources(protos()));
            chain.add(protox::file::GoogleFileResolver::new());
            chain
        });
        c.include_imports(true);
        c.open_file("test.proto").unwrap();
        let set = c.encode_file_descriptor_set();
        let files: Vec<Vec<u8>> = fields(&set).into_iter().map(|(_, _, v)| v).collect();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else { break };
                let (pool, files) = (pool.clone(), files.clone());
                tokio::spawn(async move {
                    let mut conn = h2::server::handshake(sock).await.unwrap();
                    while let Some(Ok((req, mut respond))) = conn.accept().await {
                        let path = req.uri().path().to_string();
                        let mut body = req.into_body();
                        let mut data = Vec::new();
                        while let Some(Ok(chunk)) = body.data().await {
                            let _ = body.flow_control().release_capacity(chunk.len());
                            data.extend_from_slice(&chunk);
                        }
                        let msgs = unframe(&data).unwrap();
                        let (status, reply): (u32, Option<Vec<u8>>) = match path.as_str() {
                            "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo" if reflection => {
                                let mut resp = Vec::new();
                                let mut fdr = Vec::new();
                                // file_containing_symbol (4) → test.proto first; by name (3) → that file.
                                let req = fields(&msgs[0]);
                                let by_name = req.iter().find(|(n, _, _)| *n == 3).map(|(_, _, v)| String::from_utf8_lossy(v).to_string());
                                for f in &files {
                                    let name = string_fields(f, 1).remove(0);
                                    let wanted = match &by_name {
                                        Some(n) => &name == n,
                                        None => name == "test.proto",
                                    };
                                    if wanted {
                                        put_bytes_field(&mut fdr, 1, f);
                                    }
                                }
                                put_bytes_field(&mut resp, 4, &fdr);
                                (0, Some(resp))
                            }
                            "/test.Greeter/SayHello" => {
                                let input = pool.get_message_by_name("test.HelloRequest").unwrap();
                                let msg = DynamicMessage::decode(input, msgs[0].as_slice()).unwrap();
                                let name = msg.get_field_by_name("name").unwrap().as_str().unwrap().to_string();
                                let mut out = DynamicMessage::new(pool.get_message_by_name("test.HelloReply").unwrap());
                                out.set_field_by_name("message", prost_reflect::Value::String(format!("Hello, {name}!")));
                                (0, Some(out.encode_to_vec()))
                            }
                            "/test.Greeter/Fail" => (5, None),
                            _ => (12, None),
                        };
                        let mut headers = http::Response::builder().status(200).header("content-type", "application/grpc");
                        if reply.is_none() {
                            // Trailers-only answer.
                            headers = headers.header("grpc-status", status.to_string()).header("grpc-message", "user%20not%20found");
                            let resp = headers.body(()).unwrap();
                            let _ = respond.send_response(resp, true);
                            continue;
                        }
                        let mut stream = respond.send_response(headers.body(()).unwrap(), false).unwrap();
                        stream.send_data(frame(&reply.unwrap()), false).unwrap();
                        let mut trailers = http::HeaderMap::new();
                        trailers.insert("grpc-status", status.to_string().parse().unwrap());
                        stream.send_trailers(trailers).unwrap();
                    }
                });
            }
        });
        port
    }

    #[test]
    fn parses_targets() {
        let t = target("http://localhost:50051/shop.v1.Users/GetUser").unwrap();
        assert_eq!((t.host.as_str(), t.port, t.tls, t.service.as_str(), t.method.as_str()), ("localhost", 50051, false, "shop.v1.Users", "GetUser"));
        assert!(target("grpcs://api.example.com/a.B/C").unwrap().tls);
        assert_eq!(target("grpcs://api.example.com/a.B/C").unwrap().port, 443);
        assert!(target("http://localhost:50051").is_err());
    }

    #[tokio::test]
    async fn unary_call_with_server_reflection() {
        let port = server(true).await;
        let req = crate::http::resolve(
            &parse(&format!("GRPC localhost:{port}/test.Greeter/SayHello\nx-api-key: k\n\n{{\"name\": \"Ada\"}}\n")).requests[0],
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        let resp = call(&req, &BTreeMap::new(), Duration::from_secs(10)).await.unwrap();
        assert_eq!(resp.status, 0);
        assert_eq!(resp.source, "server reflection");
        let v: serde_json::Value = serde_json::from_str(&resp.json[0]).unwrap();
        assert_eq!(v["message"], "Hello, Ada!");
        let text = render(&resp);
        assert!(text.contains("0 OK"), "{text}");
    }

    #[tokio::test]
    async fn unary_call_with_proto_files_and_errors() {
        let port = server(false).await;
        let req = |m: &str, body: &str| {
            crate::http::resolve(
                &parse(&format!("GRPC localhost:{port}/test.Greeter/{m}\n\n{body}\n")).requests[0],
                &BTreeMap::new(),
                &BTreeMap::new(),
            )
        };
        let resp = call(&req("SayHello", "{\"name\": \"Bob\", \"times\": 2}"), &protos(), Duration::from_secs(10)).await.unwrap();
        assert_eq!(resp.source, ".proto files");
        assert!(resp.json[0].contains("Hello, Bob!"));
        let failed = call(&req("Fail", "{}"), &protos(), Duration::from_secs(10)).await.unwrap();
        assert_eq!((failed.status, failed.message.as_str()), (5, "user not found"));
        assert!(render(&failed).contains("NOT_FOUND"));
        let bad = call(&req("SayHello", "{\"nope\": 1}"), &protos(), Duration::from_secs(10)).await.unwrap_err();
        assert!(bad.starts_with("The request body does not match test.HelloRequest"), "{bad}");
        let none = call(&req("SayHello", "{}"), &BTreeMap::new(), Duration::from_secs(10)).await.unwrap_err();
        assert!(none.contains("no reflection service"), "{none}");
        let missing = call(&req("Nope", "{}"), &protos(), Duration::from_secs(10)).await.unwrap_err();
        assert_eq!(missing, "Method test.Greeter/Nope not found");
    }
}
