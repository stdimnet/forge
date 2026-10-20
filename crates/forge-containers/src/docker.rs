//! Docker Engine API objects (`GET /containers/json`, `/images/json`, `/volumes`,
//! `/networks`, `/version`) as models for the tree. The raw JSON is parsed leniently: missing
//! fields get defaults so older and newer Engine versions both work.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Compose labels set on every container `docker compose` creates.
pub const LABEL_PROJECT: &str = "com.docker.compose.project";
pub const LABEL_SERVICE: &str = "com.docker.compose.service";
pub const LABEL_NUMBER: &str = "com.docker.compose.container-number";
pub const LABEL_WORKING_DIR: &str = "com.docker.compose.project.working_dir";
pub const LABEL_CONFIG_FILES: &str = "com.docker.compose.project.config_files";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Restarting,
    Removing,
    #[default]
    Exited,
    Dead,
}

impl ContainerState {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "created" => Self::Created,
            "running" => Self::Running,
            "paused" => Self::Paused,
            "restarting" => Self::Restarting,
            "removing" => Self::Removing,
            "dead" => Self::Dead,
            _ => Self::Exited,
        }
    }

    pub fn is_running(self) -> bool {
        matches!(self, Self::Running | Self::Restarting | Self::Paused)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    pub private_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_port: Option<u16>,
    pub protocol: String,
}

impl Port {
    /// `0.0.0.0:8000->8000/tcp` or `3306/tcp`, like `docker ps`.
    pub fn label(&self) -> String {
        match self.public_port {
            Some(p) => format!(
                "{}:{p}->{}/{}",
                self.ip.as_deref().unwrap_or("0.0.0.0"),
                self.private_port,
                self.protocol
            ),
            None => format!("{}/{}", self.private_port, self.protocol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mount {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub source: String,
    pub destination: String,
    pub read_write: bool,
}

/// Which compose service a container belongs to (from its labels).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeRef {
    pub project: String,
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub config_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub id: String,
    /// Name without the leading slash.
    pub name: String,
    pub image: String,
    pub image_id: String,
    pub command: String,
    /// Unix seconds.
    pub created: i64,
    pub state: ContainerState,
    /// `Up 2 hours`, `Exited (0) 3 minutes ago`.
    pub status: String,
    pub ports: Vec<Port>,
    pub labels: BTreeMap<String, String>,
    /// Network name → IP address.
    pub networks: BTreeMap<String, String>,
    pub mounts: Vec<Mount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose: Option<ComposeRef>,
}

impl Container {
    pub fn short_id(&self) -> &str {
        short_id(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub id: String,
    /// `mysql:8.4`; empty for dangling images.
    pub repo_tags: Vec<String>,
    pub repo_digests: Vec<String>,
    pub created: i64,
    pub size: i64,
    /// Containers using the image (-1 when the Engine did not count them).
    pub containers: i64,
}

impl Image {
    /// `mysql:8.4`, or `<none>:<none>` for dangling images.
    pub fn label(&self) -> String {
        self.repo_tags
            .iter()
            .find(|t| t.as_str() != "<none>:<none>")
            .cloned()
            .unwrap_or_else(|| "<none>:<none>".into())
    }

    pub fn dangling(&self) -> bool {
        self.repo_tags.iter().all(|t| t == "<none>:<none>")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
    pub labels: BTreeMap<String, String>,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub internal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subnet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    pub labels: BTreeMap<String, String>,
}

/// `GET /version`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineVersion {
    pub version: String,
    pub api_version: String,
    pub os: String,
    pub arch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

/// Everything the tree shows of one Docker daemon.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerSnapshot {
    pub containers: Vec<Container>,
    pub images: Vec<Image>,
    pub volumes: Vec<Volume>,
    pub networks: Vec<Network>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<EngineVersion>,
}

/// The raw Engine responses a [`DockerSnapshot`] is built from (also what the browser mock
/// keeps as its fake daemon state).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSnapshot {
    #[serde(default)]
    pub containers: Value,
    #[serde(default)]
    pub images: Value,
    #[serde(default)]
    pub volumes: Value,
    #[serde(default)]
    pub networks: Value,
    #[serde(default)]
    pub version: Value,
}

impl RawSnapshot {
    pub fn parse(&self) -> DockerSnapshot {
        DockerSnapshot {
            containers: parse_containers(&self.containers),
            images: parse_images(&self.images),
            volumes: parse_volumes(&self.volumes),
            networks: parse_networks(&self.networks),
            version: (!self.version.is_null()).then(|| parse_version(&self.version)),
        }
    }
}

pub fn short_id(id: &str) -> &str {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    &id[..id.len().min(12)]
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn i(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or_default()
}

fn labels(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The compose service of a container, from its labels.
pub fn compose_ref(labels: &BTreeMap<String, String>) -> Option<ComposeRef> {
    let project = labels.get(LABEL_PROJECT)?.clone();
    let service = labels.get(LABEL_SERVICE)?.clone();
    Some(ComposeRef {
        project,
        service,
        number: labels.get(LABEL_NUMBER).and_then(|n| n.parse().ok()),
        working_dir: labels.get(LABEL_WORKING_DIR).cloned(),
        config_files: labels
            .get(LABEL_CONFIG_FILES)
            .map(|f| {
                f.split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub fn parse_container(c: &Value) -> Container {
    let labels = labels(c.get("Labels"));
    let name = strings(c.get("Names"))
        .first()
        .map(|n| n.trim_start_matches('/').to_string())
        .or_else(|| {
            // Inspect responses use `Name`.
            c.get("Name")
                .and_then(Value::as_str)
                .map(|n| n.trim_start_matches('/').to_string())
        })
        .unwrap_or_else(|| short_id(&s(c, "Id")).to_string());
    let ports = c
        .get("Ports")
        .and_then(Value::as_array)
        .map(|ports| {
            let mut out: Vec<Port> = ports
                .iter()
                .map(|p| Port {
                    ip: p.get("IP").and_then(Value::as_str).map(str::to_string),
                    private_port: i(p, "PrivatePort") as u16,
                    public_port: p
                        .get("PublicPort")
                        .and_then(Value::as_u64)
                        .map(|n| n as u16),
                    protocol: p
                        .get("Type")
                        .and_then(Value::as_str)
                        .unwrap_or("tcp")
                        .to_string(),
                })
                .collect();
            // Docker lists IPv4 and IPv6 bindings separately; show each port once.
            out.sort_by(|a, b| {
                (a.private_port, a.public_port, &a.protocol).cmp(&(
                    b.private_port,
                    b.public_port,
                    &b.protocol,
                ))
            });
            out.dedup_by(|a, b| {
                a.private_port == b.private_port
                    && a.public_port == b.public_port
                    && a.protocol == b.protocol
            });
            out
        })
        .unwrap_or_default();
    let networks = c
        .pointer("/NetworkSettings/Networks")
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), s(v, "IPAddress")))
                .collect()
        })
        .unwrap_or_default();
    let mounts = c
        .get("Mounts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|m| Mount {
                    kind: s(m, "Type"),
                    name: m.get("Name").and_then(Value::as_str).map(str::to_string),
                    source: s(m, "Source"),
                    destination: s(m, "Destination"),
                    read_write: m.get("RW").and_then(Value::as_bool).unwrap_or(true),
                })
                .collect()
        })
        .unwrap_or_default();
    let state = match c.get("State") {
        Some(Value::String(st)) => ContainerState::parse(st),
        Some(Value::Object(_)) => ContainerState::parse(
            c.pointer("/State/Status")
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        _ => ContainerState::Exited,
    };
    Container {
        id: s(c, "Id"),
        name,
        image: s(c, "Image"),
        image_id: s(c, "ImageID"),
        command: s(c, "Command"),
        created: i(c, "Created"),
        state,
        status: s(c, "Status"),
        ports,
        compose: compose_ref(&labels),
        labels,
        networks,
        mounts,
    }
}

/// `GET /containers/json?all=1`, sorted by name.
pub fn parse_containers(v: &Value) -> Vec<Container> {
    let mut out: Vec<Container> = v
        .as_array()
        .map(|a| a.iter().map(parse_container).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `GET /images/json`, sorted by tag.
pub fn parse_images(v: &Value) -> Vec<Image> {
    let mut out: Vec<Image> = v
        .as_array()
        .map(|a| {
            a.iter()
                .map(|im| Image {
                    id: s(im, "Id"),
                    repo_tags: strings(im.get("RepoTags")),
                    repo_digests: strings(im.get("RepoDigests")),
                    created: i(im, "Created"),
                    size: i(im, "Size"),
                    containers: im.get("Containers").and_then(Value::as_i64).unwrap_or(-1),
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|a| (a.dangling(), a.label()));
    out
}

/// `GET /volumes` (`{ Volumes: [...] }`), sorted by name.
pub fn parse_volumes(v: &Value) -> Vec<Volume> {
    let list = v.get("Volumes").unwrap_or(v);
    let mut out: Vec<Volume> = list
        .as_array()
        .map(|a| {
            a.iter()
                .map(|vol| Volume {
                    name: s(vol, "Name"),
                    driver: s(vol, "Driver"),
                    mountpoint: s(vol, "Mountpoint"),
                    created: vol
                        .get("CreatedAt")
                        .and_then(Value::as_str)
                        .and_then(crate::time::parse_rfc3339),
                    labels: labels(vol.get("Labels")),
                    scope: s(vol, "Scope"),
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `GET /networks`, sorted by name.
pub fn parse_networks(v: &Value) -> Vec<Network> {
    let mut out: Vec<Network> = v
        .as_array()
        .map(|a| {
            a.iter()
                .map(|n| {
                    let config = n.pointer("/IPAM/Config/0");
                    Network {
                        id: s(n, "Id"),
                        name: s(n, "Name"),
                        driver: s(n, "Driver"),
                        scope: s(n, "Scope"),
                        internal: n.get("Internal").and_then(Value::as_bool).unwrap_or(false),
                        subnet: config
                            .and_then(|c| c.get("Subnet"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        gateway: config
                            .and_then(|c| c.get("Gateway"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        labels: labels(n.get("Labels")),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `GET /version`.
pub fn parse_version(v: &Value) -> EngineVersion {
    EngineVersion {
        version: s(v, "Version"),
        api_version: s(v, "ApiVersion"),
        os: s(v, "Os"),
        arch: s(v, "Arch"),
        platform: v
            .pointer("/Platform/Name")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn fixture() -> Value {
        json!([
            {
                "Id": "4f2a9c1be8d3a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123",
                "Names": ["/acme-shop-app-1"],
                "Image": "acme-shop-app",
                "ImageID": "sha256:9b1c",
                "Command": "docker-php-entrypoint php artisan serve --host=0.0.0.0",
                "Created": 1_792_480_000,
                "Ports": [
                    { "IP": "0.0.0.0", "PrivatePort": 8000, "PublicPort": 8000, "Type": "tcp" },
                    { "IP": "::", "PrivatePort": 8000, "PublicPort": 8000, "Type": "tcp" },
                    { "PrivatePort": 9000, "Type": "tcp" }
                ],
                "Labels": {
                    "com.docker.compose.project": "acme-shop",
                    "com.docker.compose.service": "app",
                    "com.docker.compose.container-number": "1",
                    "com.docker.compose.project.working_dir": "/home/dev/Projects/acme-shop",
                    "com.docker.compose.project.config_files": "/home/dev/Projects/acme-shop/docker-compose.yml"
                },
                "State": "running",
                "Status": "Up 2 hours",
                "NetworkSettings": { "Networks": { "acme-shop_default": { "IPAddress": "172.18.0.3" } } },
                "Mounts": [
                    { "Type": "bind", "Source": "/home/dev/Projects/acme-shop", "Destination": "/var/www/html", "RW": true }
                ]
            },
            {
                "Id": "0a1b2c3d4e5f",
                "Names": ["/adminer"],
                "Image": "adminer:latest",
                "State": "exited",
                "Status": "Exited (0) 3 days ago",
                "Labels": {}
            }
        ])
    }

    #[test]
    fn parses_containers() {
        let list = parse_containers(&fixture());
        assert_eq!(list.len(), 2);
        let app = list.iter().find(|c| c.name == "acme-shop-app-1").unwrap();
        assert_eq!(app.short_id(), "4f2a9c1be8d3");
        assert_eq!(app.state, ContainerState::Running);
        assert_eq!(app.ports.len(), 2, "IPv6 duplicate is dropped");
        assert_eq!(app.ports[0].label(), "0.0.0.0:8000->8000/tcp");
        assert_eq!(app.ports[1].label(), "9000/tcp");
        let compose = app.compose.as_ref().unwrap();
        assert_eq!(
            (compose.project.as_str(), compose.service.as_str()),
            ("acme-shop", "app")
        );
        assert_eq!(compose.number, Some(1));
        assert_eq!(compose.config_files.len(), 1);
        assert_eq!(app.networks["acme-shop_default"], "172.18.0.3");
        let adminer = list.iter().find(|c| c.name == "adminer").unwrap();
        assert_eq!(adminer.state, ContainerState::Exited);
        assert!(adminer.compose.is_none());
    }

    #[test]
    fn parses_inspect_state_object() {
        let c = parse_container(
            &json!({ "Id": "abc", "Name": "/web", "State": { "Status": "paused" } }),
        );
        assert_eq!(c.name, "web");
        assert_eq!(c.state, ContainerState::Paused);
    }

    #[test]
    fn parses_images_volumes_networks() {
        let images = parse_images(&json!([
            { "Id": "sha256:aaa", "RepoTags": ["<none>:<none>"], "Size": 10, "Created": 1 },
            { "Id": "sha256:bbb", "RepoTags": ["mysql:8.4"], "Size": 612_000_000, "Created": 2, "Containers": 1 }
        ]));
        assert_eq!(images[0].label(), "mysql:8.4");
        assert!(images[1].dangling());
        let volumes = parse_volumes(&json!({ "Volumes": [
            { "Name": "acme-shop_db-data", "Driver": "local", "Mountpoint": "/var/lib/docker/volumes/acme-shop_db-data/_data", "CreatedAt": "2026-10-18T08:00:00Z", "Scope": "local" }
        ]}));
        assert_eq!(volumes[0].name, "acme-shop_db-data");
        assert!(volumes[0].created.is_some());
        let networks = parse_networks(&json!([
            { "Id": "n1", "Name": "bridge", "Driver": "bridge", "Scope": "local", "IPAM": { "Config": [{ "Subnet": "172.17.0.0/16", "Gateway": "172.17.0.1" }] } }
        ]));
        assert_eq!(networks[0].subnet.as_deref(), Some("172.17.0.0/16"));
        let v = parse_version(
            &json!({ "Version": "27.3.1", "ApiVersion": "1.47", "Os": "linux", "Arch": "amd64", "Platform": { "Name": "Docker Engine - Community" } }),
        );
        assert_eq!(v.platform.as_deref(), Some("Docker Engine - Community"));
    }
}
