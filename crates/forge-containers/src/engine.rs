//! Docker Engine API client (no CLI): snapshot of containers, images, volumes and networks,
//! container lifecycle, inspect, log streaming, image pulls with progress and the event
//! stream that tells the tree to refresh.

use serde_json::Value;

use crate::docker::{parse_version, EngineVersion, RawSnapshot};
use crate::endpoint::{Endpoint, TlsFiles};
use crate::http::{send, tls_config, Request, Response, TlsOptions, Transport};
use crate::logs::{docker_logs_query, LogChunk, LogDecoder};
use crate::pull::{pull_query, PullProgress, PullState};
use crate::{err, Error, Result};

#[derive(Debug, Clone)]
pub struct DockerClient {
    transport: Transport,
    endpoint: Endpoint,
}

/// Container lifecycle operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerOp {
    Start,
    Stop,
    Restart,
    Kill,
    Pause,
    Unpause,
}

impl ContainerOp {
    fn path(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Kill => "kill",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
        }
    }
}

/// What to inspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Container,
    Image,
    Volume,
    Network,
}

fn read_pem(path: &Option<String>, what: &str) -> Result<Option<String>> {
    match path {
        None => Ok(None),
        Some(p) => std::fs::read_to_string(p)
            .map(Some)
            .map_err(|e| Error(format!("Cannot read the {what} {p}: {e}"))),
    }
}

fn tls_from_files(files: &TlsFiles) -> Result<TlsOptions> {
    let exists = |p: &Option<String>| {
        p.as_ref()
            .filter(|p| std::path::Path::new(p).exists())
            .cloned()
    };
    Ok(TlsOptions {
        ca_pem: if files.verify {
            read_pem(&exists(&files.ca), "CA certificate")?
        } else {
            None
        },
        client_cert_pem: read_pem(&exists(&files.cert), "client certificate")?,
        client_key_pem: read_pem(&exists(&files.key), "client key")?,
        insecure: !files.verify,
    })
}

/// Base64url (`X-Registry-Auth`) of registry credentials.
pub fn registry_auth(server: &str, username: &str, password: &str) -> String {
    use base64::Engine;
    let json =
        serde_json::json!({ "username": username, "password": password, "serveraddress": server });
    base64::engine::general_purpose::URL_SAFE.encode(json.to_string())
}

impl DockerClient {
    pub fn new(endpoint: &Endpoint) -> Result<Self> {
        let transport = match endpoint {
            Endpoint::Unix { path } => Transport::Unix(path.clone()),
            Endpoint::NamedPipe { path } => Transport::NamedPipe(path.clone()),
            Endpoint::Tcp { host, port, tls } => Transport::Tcp {
                host: host.clone(),
                port: *port,
                tls: match tls {
                    Some(files) => Some(tls_config(&tls_from_files(files)?)?),
                    None => None,
                },
                server_name: None,
            },
        };
        Ok(DockerClient {
            transport,
            endpoint: endpoint.clone(),
        })
    }

    /// A client on a given transport (tests).
    pub fn with_transport(transport: Transport, endpoint: Endpoint) -> Self {
        DockerClient {
            transport,
            endpoint,
        }
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    async fn request(&self, req: Request) -> Result<Response> {
        send(&self.transport, &req).await?.error_for_status().await
    }

    async fn get_json(&self, path: &str) -> Result<Value> {
        self.request(Request::new("GET", path)).await?.json().await
    }

    pub async fn version(&self) -> Result<EngineVersion> {
        Ok(parse_version(&self.get_json("/version").await?))
    }

    /// Everything the tree shows, fetched concurrently.
    pub async fn snapshot(&self) -> Result<RawSnapshot> {
        let (containers, images, volumes, networks, version) = tokio::join!(
            self.get_json("/containers/json?all=1"),
            self.get_json("/images/json"),
            self.get_json("/volumes"),
            self.get_json("/networks"),
            self.get_json("/version"),
        );
        Ok(RawSnapshot {
            containers: containers?,
            images: images?,
            volumes: volumes.unwrap_or(Value::Null),
            networks: networks.unwrap_or(Value::Null),
            version: version.unwrap_or(Value::Null),
        })
    }

    pub async fn container(&self, id: &str, op: ContainerOp) -> Result<()> {
        let resp = send(
            &self.transport,
            &Request::new("POST", format!("/containers/{id}/{}", op.path())),
        )
        .await?;
        // 304: already started / stopped.
        if resp.status == 304 {
            return Ok(());
        }
        resp.error_for_status().await?.bytes().await?;
        Ok(())
    }

    pub async fn remove_container(&self, id: &str, force: bool) -> Result<()> {
        self.request(Request::new(
            "DELETE",
            format!("/containers/{id}?force={force}&v=false"),
        ))
        .await?
        .bytes()
        .await?;
        Ok(())
    }

    pub async fn remove_image(&self, id: &str, force: bool) -> Result<()> {
        let id = crate::kube::encode_query(id);
        self.request(Request::new(
            "DELETE",
            format!("/images/{id}?force={force}"),
        ))
        .await?
        .bytes()
        .await?;
        Ok(())
    }

    pub async fn remove_volume(&self, name: &str) -> Result<()> {
        let name = crate::kube::encode_query(name);
        self.request(Request::new("DELETE", format!("/volumes/{name}")))
            .await?
            .bytes()
            .await?;
        Ok(())
    }

    pub async fn remove_network(&self, id: &str) -> Result<()> {
        self.request(Request::new("DELETE", format!("/networks/{id}")))
            .await?
            .bytes()
            .await?;
        Ok(())
    }

    pub async fn inspect(&self, kind: ObjectKind, id: &str) -> Result<Value> {
        let id = crate::kube::encode_query(id);
        let path = match kind {
            ObjectKind::Container => format!("/containers/{id}/json"),
            ObjectKind::Image => format!("/images/{id}/json"),
            ObjectKind::Volume => format!("/volumes/{id}"),
            ObjectKind::Network => format!("/networks/{id}"),
        };
        self.get_json(&path).await
    }

    /// Stream a container's logs into `sink` until the container stops (with `follow`) or
    /// the task is aborted.
    pub async fn logs(
        &self,
        id: &str,
        tail: u32,
        follow: bool,
        timestamps: bool,
        mut sink: impl FnMut(Vec<LogChunk>) + Send,
    ) -> Result<()> {
        let tty = self
            .inspect(ObjectKind::Container, id)
            .await
            .ok()
            .and_then(|v| v.pointer("/Config/Tty").and_then(Value::as_bool));
        let mut resp = self
            .request(Request::new(
                "GET",
                format!(
                    "/containers/{id}/logs?{}",
                    docker_logs_query(tail, follow, timestamps)
                ),
            ))
            .await?;
        let mut decoder = LogDecoder::new(tty);
        while let Some(bytes) = resp.chunk().await? {
            let chunks = decoder.push(&bytes);
            if !chunks.is_empty() {
                sink(chunks);
            }
        }
        let rest = decoder.finish();
        if !rest.is_empty() {
            sink(rest);
        }
        Ok(())
    }

    /// Pull an image; `sink` gets the console lines and the progress after each message.
    pub async fn pull(
        &self,
        image: &str,
        auth: Option<String>,
        mut sink: impl FnMut(Vec<String>, &PullState) + Send,
    ) -> Result<PullState> {
        let mut req = Request::new("POST", format!("/images/create?{}", pull_query(image)));
        if let Some(a) = auth {
            req = req.header("X-Registry-Auth", a);
        }
        let mut resp = self.request(req).await?;
        let mut progress = PullProgress::new(image);
        while let Some(bytes) = resp.chunk().await? {
            let lines = progress.push(&String::from_utf8_lossy(&bytes));
            sink(lines, progress.state());
        }
        let lines = progress.finish();
        sink(lines, progress.state());
        let state = progress.state().clone();
        match &state.error {
            Some(e) => err(e.clone()),
            None => Ok(state),
        }
    }

    /// Follow `/events` (container, image, volume and network changes) until the
    /// connection ends or the task is aborted.
    pub async fn events(&self, mut sink: impl FnMut(Value) + Send) -> Result<()> {
        let filters =
            crate::kube::encode_query(r#"{"type":["container","image","volume","network"]}"#);
        let mut resp = self
            .request(Request::new("GET", format!("/events?filters={filters}")))
            .await?;
        let mut partial = String::new();
        while let Some(bytes) = resp.chunk().await? {
            partial.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(nl) = partial.find('\n') {
                let line: String = partial.drain(..=nl).collect();
                if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                    sink(v);
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::http::tests::{chunked, fake_server, ok_json};
    use crate::logs::LogStream;
    use std::sync::{Arc, Mutex};

    fn frame(stream: u8, text: &str) -> Vec<u8> {
        let mut f = vec![stream, 0, 0, 0];
        f.extend_from_slice(&(text.len() as u32).to_be_bytes());
        f.extend_from_slice(text.as_bytes());
        f
    }

    async fn client(
        respond: impl Fn(&str, &[u8]) -> Vec<u8> + Send + Sync + 'static,
    ) -> (tempfile::TempDir, DockerClient) {
        let (dir, path) = fake_server(respond).await;
        let ep = Endpoint::Unix { path: path.clone() };
        (dir, DockerClient::new(&ep).unwrap())
    }

    #[tokio::test]
    async fn snapshot_and_lifecycle() {
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = seen.clone();
        let (_d, c) = client(move |head, _| {
            let line = head.lines().next().unwrap().to_string();
            log.lock().unwrap().push(line.clone());
            match line.split_whitespace().nth(1).unwrap() {
                "/containers/json?all=1" => ok_json(r#"[{"Id":"abc","Names":["/web"],"State":"running","Status":"Up 1 second"}]"#),
                "/images/json" => ok_json(r#"[{"Id":"sha256:1","RepoTags":["nginx:latest"],"Size":1}]"#),
                "/volumes" => ok_json(r#"{"Volumes":[]}"#),
                "/networks" => ok_json("[]"),
                "/version" => ok_json(r#"{"Version":"27.3.1","ApiVersion":"1.47","Os":"linux","Arch":"amd64"}"#),
                "/containers/abc/stop" => b"HTTP/1.1 304 Not Modified\r\n\r\n".to_vec(),
                "/containers/abc/start" => b"HTTP/1.1 204 No Content\r\n\r\n".to_vec(),
                "/containers/abc?force=true&v=false" => b"HTTP/1.1 204 No Content\r\n\r\n".to_vec(),
                _ => b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 19\r\n\r\n{\"message\":\"boom\"}\n".to_vec(),
            }
        })
        .await;
        let snap = c.snapshot().await.unwrap().parse();
        assert_eq!(snap.containers[0].name, "web");
        assert_eq!(snap.images[0].label(), "nginx:latest");
        assert_eq!(snap.version.unwrap().api_version, "1.47");
        c.container("abc", ContainerOp::Stop).await.unwrap();
        c.container("abc", ContainerOp::Start).await.unwrap();
        c.remove_container("abc", true).await.unwrap();
        let e = c.container("abc", ContainerOp::Kill).await.err().unwrap();
        assert_eq!(e.0, "boom");
        assert!(seen
            .lock()
            .unwrap()
            .contains(&"DELETE /containers/abc?force=true&v=false HTTP/1.1".to_string()));
    }

    #[tokio::test]
    async fn streams_logs() {
        let (_d, c) = client(|head, _| {
            if head.starts_with("GET /containers/abc/json") {
                ok_json(r#"{"Id":"abc","Config":{"Tty":false}}"#)
            } else {
                let mut a = frame(1, "Server running on [http://0.0.0.0:8000].\n");
                a.extend(frame(2, "PHP Warning:  oops\n"));
                chunked("200 OK", &[&a, &frame(1, "GET / 200\n")])
            }
        })
        .await;
        let out = Arc::new(Mutex::new(Vec::new()));
        let o = out.clone();
        c.logs("abc", 100, true, false, move |chunks| {
            o.lock().unwrap().extend(chunks)
        })
        .await
        .unwrap();
        let out = out.lock().unwrap();
        let stderr: Vec<_> = out
            .iter()
            .filter(|c| c.stream == LogStream::Stderr)
            .collect();
        assert_eq!(stderr[0].text, "PHP Warning:  oops\n");
        let all: String = out.iter().map(|c| c.text.as_str()).collect();
        assert!(all.ends_with("GET / 200\n"));
    }

    #[tokio::test]
    async fn pulls_with_progress() {
        let (_d, c) = client(|head, _| {
            assert!(head.contains("X-Registry-Auth: "));
            assert!(head.starts_with("POST /images/create?fromImage=redis&tag=7-alpine"));
            chunked(
                "200 OK",
                &[
                    b"{\"status\":\"Pulling fs layer\",\"id\":\"aaa\"}\n{\"status\":\"Downloading\",\"progressDetail\":{\"current\":5,\"total\":10},\"id\":\"aaa\"}\n",
                    b"{\"status\":\"Pull complete\",\"id\":\"aaa\"}\n{\"status\":\"Status: Downloaded newer image for redis:7-alpine\"}\n",
                ],
            )
        })
        .await;
        let mut percents = Vec::new();
        let state = c
            .pull(
                "redis:7-alpine",
                Some(registry_auth("docker.io", "u", "p")),
                |_, s| percents.push(s.percent),
            )
            .await
            .unwrap();
        assert!(state.done);
        assert!(percents.contains(&50));
        assert_eq!(*percents.last().unwrap(), 100);
    }

    #[tokio::test]
    async fn pull_errors() {
        let (_d, c) =
            client(|_, _| chunked("200 OK", &[b"{\"error\":\"manifest unknown\"}\n"])).await;
        assert_eq!(
            c.pull("nope:1", None, |_, _| {}).await.err().unwrap().0,
            "manifest unknown"
        );
    }

    #[tokio::test]
    async fn follows_events() {
        let (_d, c) = client(|head, _| {
            assert!(head.starts_with("GET /events?filters="));
            chunked(
                "200 OK",
                &[
                    b"{\"Type\":\"container\",\"Action\":\"start\"}\n{\"Type\":\"image\",",
                    b"\"Action\":\"pull\"}\n",
                ],
            )
        })
        .await;
        let mut events = Vec::new();
        c.events(|v| events.push(v["Action"].as_str().unwrap().to_string()))
            .await
            .unwrap();
        assert_eq!(events, ["start", "pull"]);
    }
}
