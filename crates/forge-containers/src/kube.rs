//! Kubernetes: kubeconfig files (clusters, users, contexts) and the API objects the tree
//! shows (namespaces, pods, deployments, services).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use yaml_rust2::{Yaml, YamlLoader};

use crate::time::{parse_rfc3339, short_age};
use crate::{err, Result};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeCluster {
    pub name: String,
    pub server: String,
    /// PEM, decoded from `certificate-authority-data`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_file: Option<String>,
    #[serde(default)]
    pub insecure_skip_tls_verify: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_server_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
}

/// Credentials of a kubeconfig user. Never serialized to the UI with secrets: see
/// [`KubeUser::redacted`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeUser {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert_data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key_data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec: Option<ExecConfig>,
}

impl KubeUser {
    /// How the user authenticates, for the Dashboard (`Bearer token`, `Client certificate`).
    pub fn auth_kind(&self) -> &'static str {
        if self.exec.is_some() {
            "Exec plugin"
        } else if self.client_cert_data.is_some() || self.client_cert_file.is_some() {
            "Client certificate"
        } else if self.token.is_some() || self.token_file.is_some() {
            "Bearer token"
        } else if self.username.is_some() {
            "Basic auth"
        } else {
            "None"
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeContext {
    pub name: String,
    pub cluster: String,
    pub user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_context: Option<String>,
    pub contexts: Vec<KubeContext>,
    pub clusters: Vec<KubeCluster>,
    pub users: Vec<KubeUser>,
}

/// A context with its cluster and user resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedContext {
    pub context: KubeContext,
    pub cluster: KubeCluster,
    pub user: KubeUser,
}

impl ResolvedContext {
    pub fn namespace(&self) -> &str {
        self.context.namespace.as_deref().unwrap_or("default")
    }
}

impl KubeConfig {
    pub fn context(&self, name: &str) -> Option<&KubeContext> {
        self.contexts.iter().find(|c| c.name == name)
    }

    pub fn resolve(&self, name: &str) -> Result<ResolvedContext> {
        let Some(context) = self.context(name) else {
            return err(format!("Context `{name}` is not in the kubeconfig"));
        };
        let Some(cluster) = self.clusters.iter().find(|c| c.name == context.cluster) else {
            return err(format!(
                "Cluster `{}` of context `{name}` is not in the kubeconfig",
                context.cluster
            ));
        };
        let user = self
            .users
            .iter()
            .find(|u| u.name == context.user)
            .cloned()
            .unwrap_or_default();
        Ok(ResolvedContext {
            context: context.clone(),
            cluster: cluster.clone(),
            user,
        })
    }

    /// Merge another kubeconfig (`KUBECONFIG=a:b`): the first file wins for names that occur
    /// twice and for `current-context`.
    pub fn merge(&mut self, other: KubeConfig) {
        if self.current_context.is_none() {
            self.current_context = other.current_context;
        }
        for c in other.contexts {
            if !self.contexts.iter().any(|x| x.name == c.name) {
                self.contexts.push(c);
            }
        }
        for c in other.clusters {
            if !self.clusters.iter().any(|x| x.name == c.name) {
                self.clusters.push(c);
            }
        }
        for u in other.users {
            if !self.users.iter().any(|x| x.name == u.name) {
                self.users.push(u);
            }
        }
    }
}

fn s(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn b64(y: &Yaml) -> Option<String> {
    use base64::Engine;
    let raw = s(y)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim().as_bytes())
        .ok()?;
    String::from_utf8(bytes).ok()
}

/// Relative file references in a kubeconfig are relative to the kubeconfig's directory.
fn path_in(base_dir: &str, p: Option<String>) -> Option<String> {
    let p = p?;
    if p.starts_with('/') || p.starts_with('~') || p.get(1..3) == Some(":\\") || base_dir.is_empty()
    {
        Some(p)
    } else {
        Some(format!("{}/{p}", base_dir.trim_end_matches(['/', '\\'])))
    }
}

/// Parse a kubeconfig. `base_dir` resolves relative certificate and token paths.
pub fn parse_kubeconfig(content: &str, base_dir: &str) -> Result<KubeConfig> {
    let docs = YamlLoader::load_from_str(content)
        .map_err(|e| crate::Error(format!("Invalid kubeconfig: {e}")))?;
    let Some(root) = docs.into_iter().next() else {
        return Ok(KubeConfig::default());
    };
    let items = |key: &str| -> Vec<Yaml> {
        match &root[key] {
            Yaml::Array(a) => a.clone(),
            _ => Vec::new(),
        }
    };
    let clusters = items("clusters")
        .iter()
        .filter_map(|c| {
            let d = &c["cluster"];
            Some(KubeCluster {
                name: s(&c["name"])?,
                server: s(&d["server"]).unwrap_or_default(),
                ca_data: b64(&d["certificate-authority-data"]),
                ca_file: path_in(base_dir, s(&d["certificate-authority"])),
                insecure_skip_tls_verify: d["insecure-skip-tls-verify"].as_bool().unwrap_or(false),
                tls_server_name: s(&d["tls-server-name"]),
            })
        })
        .collect();
    let users = items("users")
        .iter()
        .filter_map(|u| {
            let d = &u["user"];
            let exec = match &d["exec"] {
                Yaml::Hash(_) => Some(ExecConfig {
                    command: s(&d["exec"]["command"]).unwrap_or_default(),
                    args: match &d["exec"]["args"] {
                        Yaml::Array(a) => a.iter().filter_map(s).collect(),
                        _ => Vec::new(),
                    },
                    env: match &d["exec"]["env"] {
                        Yaml::Array(a) => a
                            .iter()
                            .filter_map(|e| {
                                Some((s(&e["name"])?, s(&e["value"]).unwrap_or_default()))
                            })
                            .collect(),
                        _ => Vec::new(),
                    },
                    api_version: s(&d["exec"]["apiVersion"]),
                }),
                _ => None,
            };
            Some(KubeUser {
                name: s(&u["name"])?,
                token: s(&d["token"]),
                token_file: path_in(base_dir, s(&d["tokenFile"])),
                client_cert_data: b64(&d["client-certificate-data"]),
                client_cert_file: path_in(base_dir, s(&d["client-certificate"])),
                client_key_data: b64(&d["client-key-data"]),
                client_key_file: path_in(base_dir, s(&d["client-key"])),
                username: s(&d["username"]),
                password: s(&d["password"]),
                exec,
            })
        })
        .collect();
    let mut contexts: Vec<KubeContext> = items("contexts")
        .iter()
        .filter_map(|c| {
            let d = &c["context"];
            Some(KubeContext {
                name: s(&c["name"])?,
                cluster: s(&d["cluster"]).unwrap_or_default(),
                user: s(&d["user"]).unwrap_or_default(),
                namespace: s(&d["namespace"]).filter(|n| !n.is_empty()),
            })
        })
        .collect();
    contexts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(KubeConfig {
        current_context: s(&root["current-context"]).filter(|c| !c.is_empty()),
        contexts,
        clusters,
        users,
    })
}

// ---- API objects ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PodPhase {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pod {
    pub name: String,
    pub namespace: String,
    pub phase: PodPhase,
    /// What `kubectl get pods` shows in STATUS (`Running`, `CrashLoopBackOff`, `Completed`).
    pub status: String,
    /// `1/2`.
    pub ready: String,
    pub restarts: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    pub containers: Vec<String>,
    /// Init containers (`spec.initContainers`), shown in the log picker.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub init_containers: Vec<String>,
    /// The `kubectl.kubernetes.io/default-container` annotation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_container_annotation: Option<String>,
    /// Declared container ports (suggestions for Forward Ports).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<ContainerPort>,
    pub labels: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// A port a pod's container declares (`spec.containers[].ports[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerPort {
    pub container: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub protocol: String,
}

impl Pod {
    /// The container `kubectl` picks without `-c`: the default-container annotation when it
    /// names a container of the pod, else the first (non-init) container.
    pub fn default_container(&self) -> Option<&str> {
        self.default_container_annotation
            .as_deref()
            .filter(|a| self.containers.iter().any(|c| c == a))
            .or_else(|| self.containers.first().map(String::as_str))
    }

    /// `container` when the pod has it, else the default container.
    pub fn pick_container<'a>(&'a self, container: Option<&'a str>) -> Option<&'a str> {
        container
            .filter(|c| {
                self.containers.iter().any(|x| x == c)
                    || self.init_containers.iter().any(|x| x == c)
            })
            .or_else(|| self.default_container())
    }

    pub fn healthy(&self) -> bool {
        matches!(self.status.as_str(), "Running" | "Completed")
            && (self.phase != PodPhase::Running || {
                let mut parts = self.ready.split('/');
                parts.next() == parts.next()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deployment {
    pub name: String,
    pub namespace: String,
    pub replicas: i64,
    pub ready_replicas: i64,
    pub updated_replicas: i64,
    pub available_replicas: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
    pub images: Vec<String>,
    pub selector: BTreeMap<String, String>,
}

impl Deployment {
    /// Whether the deployment's selector matches the pod's labels.
    pub fn selects(&self, pod: &Pod) -> bool {
        !self.selector.is_empty()
            && self
                .selector
                .iter()
                .all(|(k, v)| pod.labels.get(k) == Some(v))
    }

    /// `app=web,tier=frontend` for `?labelSelector=`.
    pub fn label_selector(&self) -> String {
        self.selector
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeService {
    pub name: String,
    pub namespace: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ip: Option<String>,
    /// `80:30080/TCP`.
    pub ports: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
}

/// Pods, deployments and services of one namespace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workloads {
    pub pods: Vec<Pod>,
    pub deployments: Vec<Deployment>,
    pub services: Vec<KubeService>,
}

/// The raw API lists a [`Workloads`] is built from.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWorkloads {
    #[serde(default)]
    pub pods: Value,
    #[serde(default)]
    pub deployments: Value,
    #[serde(default)]
    pub services: Value,
}

impl RawWorkloads {
    pub fn parse(&self) -> Workloads {
        Workloads {
            pods: parse_pods(&self.pods),
            deployments: parse_deployments(&self.deployments),
            services: parse_services(&self.services),
        }
    }
}

fn st(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(Value::as_str).map(str::to_string)
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

fn items(list: &Value) -> &[Value] {
    list.get("items")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// `GET /api/v1/namespaces` → names.
pub fn parse_namespaces(list: &Value) -> Vec<String> {
    let mut out: Vec<String> = items(list)
        .iter()
        .filter_map(|n| st(n, "/metadata/name"))
        .collect();
    out.sort();
    out
}

pub fn parse_pod(p: &Value) -> Pod {
    let phase = match st(p, "/status/phase").as_deref() {
        Some("Pending") => PodPhase::Pending,
        Some("Running") => PodPhase::Running,
        Some("Succeeded") => PodPhase::Succeeded,
        Some("Failed") => PodPhase::Failed,
        _ => PodPhase::Unknown,
    };
    let statuses = p
        .pointer("/status/containerStatuses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let names = |ptr: &str| -> Vec<String> {
        p.pointer(ptr)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|c| st(c, "/name")).collect())
            .unwrap_or_default()
    };
    let containers = names("/spec/containers");
    let init_containers = names("/spec/initContainers");
    let ports = p
        .pointer("/spec/containers")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .flat_map(|c| {
                    let container = st(c, "/name").unwrap_or_default();
                    c.get("ports")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(move |port| {
                            Some(ContainerPort {
                                container: container.clone(),
                                port: u16::try_from(port.get("containerPort")?.as_i64()?).ok()?,
                                name: st(&port, "/name"),
                                protocol: st(&port, "/protocol").unwrap_or_else(|| "TCP".into()),
                            })
                        })
                })
                .collect()
        })
        .unwrap_or_default();
    let ready = statuses
        .iter()
        .filter(|c| c.get("ready").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let restarts = statuses
        .iter()
        .map(|c| c.get("restartCount").and_then(Value::as_i64).unwrap_or(0))
        .sum();
    // kubectl's STATUS: a waiting/terminated reason beats the phase.
    let reason = statuses.iter().find_map(|c| {
        st(c, "/state/waiting/reason").or_else(|| {
            st(c, "/state/terminated/reason")
                .filter(|r| r != "Completed" || phase != PodPhase::Running)
        })
    });
    let status = if p.pointer("/metadata/deletionTimestamp").is_some() {
        "Terminating".to_string()
    } else if let Some(r) = st(p, "/status/reason") {
        r
    } else if let Some(r) = reason {
        r
    } else if phase == PodPhase::Succeeded {
        "Completed".to_string()
    } else {
        st(p, "/status/phase").unwrap_or_else(|| "Unknown".into())
    };
    Pod {
        name: st(p, "/metadata/name").unwrap_or_default(),
        namespace: st(p, "/metadata/namespace").unwrap_or_default(),
        phase,
        status,
        ready: format!("{ready}/{}", containers.len().max(statuses.len())),
        restarts,
        created: st(p, "/metadata/creationTimestamp").and_then(|t| parse_rfc3339(&t)),
        node: st(p, "/spec/nodeName"),
        ip: st(p, "/status/podIP"),
        containers,
        init_containers,
        default_container_annotation: p
            .pointer("/metadata/annotations")
            .and_then(|a| a.get("kubectl.kubernetes.io/default-container"))
            .and_then(Value::as_str)
            .map(str::to_string),
        ports,
        labels: labels(p.pointer("/metadata/labels")),
        owner: p
            .pointer("/metadata/ownerReferences/0")
            .and_then(|o| Some(format!("{}/{}", st(o, "/kind")?, st(o, "/name")?))),
    }
}

pub fn parse_pods(list: &Value) -> Vec<Pod> {
    let mut out: Vec<Pod> = items(list).iter().map(parse_pod).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn parse_deployments(list: &Value) -> Vec<Deployment> {
    let n = |d: &Value, ptr: &str| d.pointer(ptr).and_then(Value::as_i64).unwrap_or(0);
    let mut out: Vec<Deployment> = items(list)
        .iter()
        .map(|d| Deployment {
            name: st(d, "/metadata/name").unwrap_or_default(),
            namespace: st(d, "/metadata/namespace").unwrap_or_default(),
            replicas: d
                .pointer("/spec/replicas")
                .and_then(Value::as_i64)
                .unwrap_or(1),
            ready_replicas: n(d, "/status/readyReplicas"),
            updated_replicas: n(d, "/status/updatedReplicas"),
            available_replicas: n(d, "/status/availableReplicas"),
            created: st(d, "/metadata/creationTimestamp").and_then(|t| parse_rfc3339(&t)),
            images: d
                .pointer("/spec/template/spec/containers")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|c| st(c, "/image")).collect())
                .unwrap_or_default(),
            selector: labels(d.pointer("/spec/selector/matchLabels")),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn parse_services(list: &Value) -> Vec<KubeService> {
    let mut out: Vec<KubeService> = items(list)
        .iter()
        .map(|s| KubeService {
            name: st(s, "/metadata/name").unwrap_or_default(),
            namespace: st(s, "/metadata/namespace").unwrap_or_default(),
            kind: st(s, "/spec/type").unwrap_or_else(|| "ClusterIP".into()),
            cluster_ip: st(s, "/spec/clusterIP"),
            external_ip: st(s, "/status/loadBalancer/ingress/0/ip")
                .or_else(|| st(s, "/status/loadBalancer/ingress/0/hostname")),
            ports: s
                .pointer("/spec/ports")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|p| {
                            let port = p.get("port").and_then(Value::as_i64).unwrap_or(0);
                            let proto = st(p, "/protocol").unwrap_or_else(|| "TCP".into());
                            match p.get("nodePort").and_then(Value::as_i64) {
                                Some(np) => format!("{port}:{np}/{proto}"),
                                None => format!("{port}/{proto}"),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            created: st(s, "/metadata/creationTimestamp").and_then(|t| parse_rfc3339(&t)),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `kubectl`-style age column.
pub fn age(created: Option<i64>, now: i64) -> String {
    created
        .map(|c| short_age(c, now))
        .unwrap_or_else(|| "<unknown>".into())
}

/// Percent-encode a query value (label selectors, container names).
pub fn encode_query(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// API paths of the lists the tree needs.
pub fn pods_path(ns: &str) -> String {
    format!("/api/v1/namespaces/{}/pods", encode_query(ns))
}

/// Pods matching a label selector (`app=web`).
pub fn pods_by_selector_path(ns: &str, selector: &str) -> String {
    format!("{}?labelSelector={}", pods_path(ns), encode_query(selector))
}

pub fn deployments_path(ns: &str) -> String {
    format!("/apis/apps/v1/namespaces/{}/deployments", encode_query(ns))
}

pub fn services_path(ns: &str) -> String {
    format!("/api/v1/namespaces/{}/services", encode_query(ns))
}

/// `GET …/pods/<name>/log?follow=true&tailLines=…&container=…`.
pub fn pod_log_path(
    ns: &str,
    pod: &str,
    container: Option<&str>,
    tail: u32,
    follow: bool,
) -> String {
    let mut p = format!(
        "/api/v1/namespaces/{}/pods/{}/log?tailLines={tail}",
        encode_query(ns),
        encode_query(pod)
    );
    if follow {
        p.push_str("&follow=true");
    }
    if let Some(c) = container {
        p.push_str("&container=");
        p.push_str(&encode_query(c));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KUBECONFIG: &str = r#"apiVersion: v1
kind: Config
current-context: kind-acme
clusters:
- name: kind-acme
  cluster:
    server: https://127.0.0.1:6443
    certificate-authority-data: LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCg==
- name: prod
  cluster:
    server: https://k8s.example.com
    certificate-authority: certs/ca.crt
    tls-server-name: api.internal
contexts:
- name: kind-acme
  context:
    cluster: kind-acme
    user: kind-acme
    namespace: shop
- name: prod
  context:
    cluster: prod
    user: ci
users:
- name: kind-acme
  user:
    client-certificate-data: Y2VydA==
    client-key-data: a2V5
- name: ci
  user:
    exec:
      apiVersion: client.authentication.k8s.io/v1
      command: gke-gcloud-auth-plugin
      args: ["--verbose"]
      env:
      - name: FOO
        value: bar
"#;

    #[test]
    fn parses_kubeconfig() {
        let cfg = parse_kubeconfig(KUBECONFIG, "/home/dev/.kube").unwrap();
        assert_eq!(cfg.current_context.as_deref(), Some("kind-acme"));
        assert_eq!(cfg.contexts.len(), 2);
        let kind = cfg.resolve("kind-acme").unwrap();
        assert_eq!(kind.namespace(), "shop");
        assert_eq!(
            kind.cluster.ca_data.as_deref(),
            Some("-----BEGIN CERTIFICATE-----\n")
        );
        assert_eq!(kind.user.client_cert_data.as_deref(), Some("cert"));
        assert_eq!(kind.user.auth_kind(), "Client certificate");
        let prod = cfg.resolve("prod").unwrap();
        assert_eq!(prod.namespace(), "default");
        assert_eq!(
            prod.cluster.ca_file.as_deref(),
            Some("/home/dev/.kube/certs/ca.crt")
        );
        assert_eq!(
            prod.cluster.tls_server_name.as_deref(),
            Some("api.internal")
        );
        let exec = prod.user.exec.as_ref().unwrap();
        assert_eq!(exec.command, "gke-gcloud-auth-plugin");
        assert_eq!(exec.env, [("FOO".to_string(), "bar".to_string())]);
        assert!(cfg.resolve("missing").is_err());
    }

    #[test]
    fn merges_kubeconfigs() {
        let mut a = parse_kubeconfig(KUBECONFIG, "").unwrap();
        let b = parse_kubeconfig(
            "current-context: other\ncontexts:\n- name: other\n  context: { cluster: c, user: u }\n- name: prod\n  context: { cluster: x, user: y }\n",
            "",
        )
        .unwrap();
        a.merge(b);
        assert_eq!(a.current_context.as_deref(), Some("kind-acme"));
        assert_eq!(a.contexts.len(), 3);
        assert_eq!(a.context("prod").unwrap().cluster, "prod");
    }

    #[test]
    fn parses_workloads() {
        let pods = parse_pods(&json!({ "items": [
            {
                "metadata": { "name": "web-7d4b9c-x2kq", "namespace": "shop", "creationTimestamp": "2026-10-20T07:00:00Z",
                              "labels": { "app": "web" }, "ownerReferences": [{ "kind": "ReplicaSet", "name": "web-7d4b9c" }] },
                "spec": { "nodeName": "kind-control-plane", "containers": [{ "name": "php" }, { "name": "nginx" }] },
                "status": { "phase": "Running", "podIP": "10.244.0.12", "containerStatuses": [
                    { "name": "php", "ready": true, "restartCount": 1, "state": { "running": {} } },
                    { "name": "nginx", "ready": false, "restartCount": 2, "state": { "waiting": { "reason": "CrashLoopBackOff" } } }
                ] }
            },
            {
                "metadata": { "name": "migrate-abc", "namespace": "shop" },
                "spec": { "containers": [{ "name": "migrate" }] },
                "status": { "phase": "Succeeded", "containerStatuses": [
                    { "name": "migrate", "ready": false, "restartCount": 0, "state": { "terminated": { "reason": "Completed" } } }
                ] }
            }
        ]}));
        assert_eq!(pods[0].name, "migrate-abc");
        assert_eq!(pods[0].status, "Completed");
        assert!(pods[0].healthy());
        let web = &pods[1];
        assert_eq!(web.status, "CrashLoopBackOff");
        assert_eq!(web.ready, "1/2");
        assert_eq!(web.restarts, 3);
        assert_eq!(web.owner.as_deref(), Some("ReplicaSet/web-7d4b9c"));
        assert!(!web.healthy());
        assert_eq!(web.default_container(), Some("php"));

        let multi = parse_pod(&json!({
            "metadata": { "name": "api-1", "annotations": { "kubectl.kubernetes.io/default-container": "app" } },
            "spec": {
                "initContainers": [{ "name": "migrate" }],
                "containers": [
                    { "name": "istio-proxy", "ports": [{ "containerPort": 15090, "name": "http-envoy-prom" }] },
                    { "name": "app", "ports": [{ "containerPort": 8080, "name": "http", "protocol": "TCP" }] }
                ]
            },
            "status": { "phase": "Running" }
        }));
        assert_eq!(multi.containers, ["istio-proxy", "app"]);
        assert_eq!(multi.init_containers, ["migrate"]);
        assert_eq!(multi.default_container(), Some("app"), "annotation wins");
        assert_eq!(multi.pick_container(Some("migrate")), Some("migrate"));
        assert_eq!(multi.pick_container(Some("nope")), Some("app"));
        assert_eq!(multi.ports[1].port, 8080);
        assert_eq!(multi.ports[1].name.as_deref(), Some("http"));
        let unknown_annotation = Pod {
            default_container_annotation: Some("gone".into()),
            ..multi.clone()
        };
        assert_eq!(unknown_annotation.default_container(), Some("istio-proxy"));

        let deps = parse_deployments(&json!({ "items": [
            { "metadata": { "name": "web", "namespace": "shop" }, "spec": { "replicas": 3,
              "selector": { "matchLabels": { "app": "web" } },
              "template": { "spec": { "containers": [{ "image": "acme/shop:1.4" }] } } },
              "status": { "readyReplicas": 2, "updatedReplicas": 3, "availableReplicas": 2 } }
        ]}));
        assert_eq!(deps[0].ready_replicas, 2);
        assert_eq!(deps[0].images, ["acme/shop:1.4"]);
        assert_eq!(deps[0].label_selector(), "app=web");
        assert!(deps[0].selects(web));
        assert!(!deps[0].selects(&pods[0]));

        let svcs = parse_services(&json!({ "items": [
            { "metadata": { "name": "web", "namespace": "shop" }, "spec": { "type": "NodePort", "clusterIP": "10.96.12.4",
              "ports": [{ "port": 80, "nodePort": 30080, "protocol": "TCP" }] } }
        ]}));
        assert_eq!(svcs[0].ports, ["80:30080/TCP"]);
        assert_eq!(
            parse_namespaces(
                &json!({ "items": [{ "metadata": { "name": "shop" } }, { "metadata": { "name": "default" } }] })
            ),
            ["default", "shop"]
        );
    }

    #[test]
    fn api_paths() {
        assert_eq!(
            pod_log_path("shop", "web-1", Some("php"), 200, true),
            "/api/v1/namespaces/shop/pods/web-1/log?tailLines=200&follow=true&container=php"
        );
        assert_eq!(
            pods_by_selector_path("shop", "app=web"),
            "/api/v1/namespaces/shop/pods?labelSelector=app%3Dweb"
        );
        assert_eq!(
            encode_query("app=web,tier in (a)"),
            "app%3Dweb%2Ctier%20in%20%28a%29"
        );
    }
}
