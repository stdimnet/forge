//! The Services tool window model: the tree (Docker → Containers / Images / Networks /
//! Volumes, Docker Compose → projects → services → containers, Kubernetes → contexts →
//! namespaces → Pods / Deployments / Services), the details pane (Dashboard rows, available
//! tabs) and which toolbar actions apply to a node. Everything is computed from a
//! [`ServicesInput`], so the desktop app and the browser mock render the same thing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::compose::ComposeFile;
use crate::docker::{Container, ContainerState, DockerSnapshot};
use crate::kube::{age, encode_query, ContainerPort, KubeConfig, Workloads};
use crate::portforward::PortForward;
use crate::time::{format_size, format_utc, long_age};

// ---- input -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerInput {
    pub state: ConnectionState,
    /// `unix:///var/run/docker.sock`.
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<DockerSnapshot>,
}

/// Something loaded on demand: the value, the error, or still loading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Loaded<T> {
    Loading,
    Ready { value: T },
    Failed { error: String },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeInput {
    /// Shown path of the kubeconfig (`~/.kube/config`).
    #[serde(default)]
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<KubeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Setting `kubernetes.defaultContext` (else the kubeconfig's current context).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_context: Option<String>,
    /// Setting `kubernetes.defaultNamespace`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespace: Option<String>,
    /// Namespaces per context.
    #[serde(default)]
    pub namespaces: BTreeMap<String, Loaded<Vec<String>>>,
    /// Workloads per `context/namespace` key ([`workloads_key`]).
    #[serde(default)]
    pub workloads: BTreeMap<String, Loaded<Workloads>>,
}

impl KubeInput {
    pub fn current_context(&self) -> Option<&str> {
        self.default_context
            .as_deref()
            .filter(|c| !c.is_empty())
            .or_else(|| self.config.as_ref()?.current_context.as_deref())
    }
}

pub fn workloads_key(context: &str, namespace: &str) -> String {
    format!("{context}\u{1f}{namespace}")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicesInput {
    pub docker: DockerInput,
    #[serde(default)]
    pub compose_files: Vec<ComposeFile>,
    #[serde(default)]
    pub kube: KubeInput,
    /// Unix seconds, for ages.
    pub now: i64,
    /// Node ids with an action in progress (spinner instead of the status dot).
    #[serde(default)]
    pub busy: Vec<String>,
    /// Active Kubernetes port forwards.
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,
}

// ---- targets and node ids --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Docker,
    DockerGroup {
        group: String,
    },
    Container {
        id: String,
    },
    Image {
        id: String,
    },
    Network {
        id: String,
    },
    Volume {
        name: String,
    },
    ComposeRoot,
    ComposeProject {
        project: String,
    },
    ComposeService {
        project: String,
        service: String,
    },
    KubeRoot,
    KubeContext {
        context: String,
    },
    KubeNamespace {
        context: String,
        namespace: String,
    },
    KubeGroup {
        context: String,
        namespace: String,
        group: String,
    },
    Pod {
        context: String,
        namespace: String,
        name: String,
    },
    Deployment {
        context: String,
        namespace: String,
        name: String,
    },
    KubeService {
        context: String,
        namespace: String,
        name: String,
    },
}

fn decode(seg: &str) -> String {
    let bytes = seg.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = seg
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Target {
    pub fn node_id(&self) -> String {
        let e = encode_query;
        match self {
            Target::Docker => "docker".into(),
            Target::DockerGroup { group } => format!("docker/{group}"),
            Target::Container { id } => format!("docker/container/{id}"),
            Target::Image { id } => format!("docker/image/{}", e(id)),
            Target::Network { id } => format!("docker/network/{id}"),
            Target::Volume { name } => format!("docker/volume/{}", e(name)),
            Target::ComposeRoot => "compose".into(),
            Target::ComposeProject { project } => format!("compose/{}", e(project)),
            Target::ComposeService { project, service } => {
                format!("compose/{}/{}", e(project), e(service))
            }
            Target::KubeRoot => "kube".into(),
            Target::KubeContext { context } => format!("kube/{}", e(context)),
            Target::KubeNamespace { context, namespace } => {
                format!("kube/{}/{}", e(context), e(namespace))
            }
            Target::KubeGroup {
                context,
                namespace,
                group,
            } => format!("kube/{}/{}/{group}", e(context), e(namespace)),
            Target::Pod {
                context,
                namespace,
                name,
            } => format!("kube/{}/{}/pods/{}", e(context), e(namespace), e(name)),
            Target::Deployment {
                context,
                namespace,
                name,
            } => format!(
                "kube/{}/{}/deployments/{}",
                e(context),
                e(namespace),
                e(name)
            ),
            Target::KubeService {
                context,
                namespace,
                name,
            } => format!("kube/{}/{}/services/{}", e(context), e(namespace), e(name)),
        }
    }

    /// The target of a node id. Containers below compose services
    /// (`compose/<p>/<s>/<containerId>`) resolve to the container.
    pub fn parse(id: &str) -> Option<Target> {
        let parts: Vec<String> = id.split('/').map(decode).collect();
        let p: Vec<&str> = parts.iter().map(String::as_str).collect();
        let t = match p.as_slice() {
            ["docker"] => Target::Docker,
            ["docker", g @ ("containers" | "images" | "networks" | "volumes")] => {
                Target::DockerGroup {
                    group: g.to_string(),
                }
            }
            ["docker", "container", id] => Target::Container { id: id.to_string() },
            ["docker", "image", id] => Target::Image { id: id.to_string() },
            ["docker", "network", id] => Target::Network { id: id.to_string() },
            ["docker", "volume", name] => Target::Volume {
                name: name.to_string(),
            },
            ["compose"] => Target::ComposeRoot,
            ["compose", project] => Target::ComposeProject {
                project: project.to_string(),
            },
            ["compose", project, service] => Target::ComposeService {
                project: project.to_string(),
                service: service.to_string(),
            },
            ["compose", _, _, id] => Target::Container { id: id.to_string() },
            ["kube"] => Target::KubeRoot,
            ["kube", c] => Target::KubeContext {
                context: c.to_string(),
            },
            ["kube", c, ns] => Target::KubeNamespace {
                context: c.to_string(),
                namespace: ns.to_string(),
            },
            ["kube", c, ns, g] => Target::KubeGroup {
                context: c.to_string(),
                namespace: ns.to_string(),
                group: g.to_string(),
            },
            ["kube", c, ns, kind, name] => {
                let (context, namespace, name) = (c.to_string(), ns.to_string(), name.to_string());
                match *kind {
                    "pods" => Target::Pod {
                        context,
                        namespace,
                        name,
                    },
                    "deployments" => Target::Deployment {
                        context,
                        namespace,
                        name,
                    },
                    "services" => Target::KubeService {
                        context,
                        namespace,
                        name,
                    },
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(t)
    }
}

// ---- output ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Icon {
    Docker,
    Folder,
    Container,
    Image,
    Network,
    Volume,
    Compose,
    Service,
    Kubernetes,
    Context,
    Namespace,
    Pod,
    Deployment,
    KubeService,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Running,
    Stopped,
    Paused,
    Pending,
    Error,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub icon: Icon,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
    /// Children are loaded when the node is expanded (`services_expand`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lazy: bool,
    /// Expanded when first shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub expanded: bool,
    /// Muted label (stopped containers, empty groups).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub muted: bool,
}

impl Node {
    fn new(target: &Target, label: impl Into<String>, icon: Icon) -> Self {
        Node {
            id: target.node_id(),
            label: label.into(),
            detail: None,
            icon,
            status: None,
            children: Vec::new(),
            lazy: false,
            expanded: false,
            muted: false,
        }
    }

    fn detail(mut self, d: impl Into<String>) -> Self {
        let d = d.into();
        self.detail = (!d.is_empty()).then_some(d);
        self
    }

    fn status(mut self, s: Option<Status>) -> Self {
        self.status = s;
        self
    }

    fn children(mut self, c: Vec<Node>) -> Self {
        self.children = c;
        self
    }

    fn expanded(mut self, e: bool) -> Self {
        self.expanded = e;
        self
    }

    pub fn find(&self, id: &str) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }
}

fn container_status(state: ContainerState) -> Status {
    match state {
        ContainerState::Running => Status::Running,
        ContainerState::Paused => Status::Paused,
        ContainerState::Restarting | ContainerState::Created | ContainerState::Removing => {
            Status::Pending
        }
        ContainerState::Dead => Status::Error,
        ContainerState::Exited => Status::Stopped,
    }
}

fn info_node(parent: &str, text: &str, icon: Icon) -> Node {
    Node {
        id: format!("{parent}/!info"),
        label: text.to_string(),
        detail: None,
        icon,
        status: None,
        children: Vec::new(),
        lazy: false,
        expanded: false,
        muted: true,
    }
}

/// Containers of a compose project, grouped by service.
fn compose_containers<'a>(
    snapshot: Option<&'a DockerSnapshot>,
    project: &str,
) -> BTreeMap<String, Vec<&'a Container>> {
    let mut out: BTreeMap<String, Vec<&Container>> = BTreeMap::new();
    for c in snapshot
        .map(|s| s.containers.as_slice())
        .unwrap_or_default()
    {
        if let Some(r) = c.compose.as_ref().filter(|r| r.project == project) {
            out.entry(r.service.clone()).or_default().push(c);
        }
    }
    for list in out.values_mut() {
        list.sort_by_key(|c| c.compose.as_ref().and_then(|r| r.number).unwrap_or(0));
    }
    out
}

/// Aggregate state of a service's containers: running if all run, error if any died,
/// pending when some run.
fn aggregate(containers: &[&Container]) -> Option<Status> {
    if containers.is_empty() {
        return None;
    }
    let running = containers
        .iter()
        .filter(|c| c.state == ContainerState::Running)
        .count();
    if containers.iter().any(|c| c.state == ContainerState::Dead) {
        Some(Status::Error)
    } else if running == containers.len() {
        Some(Status::Running)
    } else if running > 0
        || containers
            .iter()
            .any(|c| c.state == ContainerState::Restarting)
    {
        Some(Status::Pending)
    } else {
        Some(Status::Stopped)
    }
}

/// Compose projects: from the project's compose files plus projects only known from
/// running containers' labels.
#[derive(Debug, Clone)]
pub struct ComposeProject<'a> {
    pub name: String,
    pub file: Option<&'a ComposeFile>,
    pub services: Vec<String>,
    pub containers: BTreeMap<String, Vec<&'a Container>>,
}

pub fn compose_projects(input: &ServicesInput) -> Vec<ComposeProject<'_>> {
    let snapshot = input.docker.snapshot.as_ref();
    let mut out: Vec<ComposeProject> = Vec::new();
    for file in &input.compose_files {
        if out.iter().any(|p| p.name == file.project) {
            continue;
        }
        let containers = compose_containers(snapshot, &file.project);
        let mut services: Vec<String> = file.services.iter().map(|s| s.name.clone()).collect();
        for s in containers.keys() {
            if !services.contains(s) {
                services.push(s.clone());
            }
        }
        out.push(ComposeProject {
            name: file.project.clone(),
            file: Some(file),
            services,
            containers,
        });
    }
    let mut others: Vec<String> = snapshot
        .map(|s| s.containers.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|c| c.compose.as_ref().map(|r| r.project.clone()))
        .filter(|p| !out.iter().any(|x| &x.name == p))
        .collect();
    others.sort();
    others.dedup();
    for name in others {
        let containers = compose_containers(snapshot, &name);
        out.push(ComposeProject {
            services: containers.keys().cloned().collect(),
            name,
            file: None,
            containers,
        });
    }
    out
}

fn docker_nodes(input: &ServicesInput) -> Node {
    let d = &input.docker;
    let root = Target::Docker;
    let mut node = Node::new(&root, "Docker", Icon::Docker)
        .detail(d.endpoint.clone())
        .expanded(true);
    match d.state {
        ConnectionState::Connected => node.status = Some(Status::Running),
        ConnectionState::Connecting => node.status = Some(Status::Busy),
        ConnectionState::Failed => node.status = Some(Status::Error),
        ConnectionState::Disconnected => node.status = Some(Status::Stopped),
    }
    let Some(s) = d
        .snapshot
        .as_ref()
        .filter(|_| d.state == ConnectionState::Connected)
    else {
        let text = match d.state {
            ConnectionState::Connecting => "Connecting…".to_string(),
            ConnectionState::Failed => {
                format!(
                    "Cannot connect: {}",
                    d.error.as_deref().unwrap_or("unknown error")
                )
            }
            _ => "Not connected".to_string(),
        };
        let icon = if d.state == ConnectionState::Failed {
            Icon::Error
        } else {
            Icon::Docker
        };
        node.children = vec![info_node("docker", &text, icon)];
        return node;
    };
    let group = |g: &str, label: &str, children: Vec<Node>, expanded: bool| {
        let n = children.len();
        let mut node = Node::new(
            &Target::DockerGroup { group: g.into() },
            label,
            Icon::Folder,
        )
        .detail(n.to_string())
        .children(children)
        .expanded(expanded);
        node.muted = n == 0;
        node
    };
    let containers = s
        .containers
        .iter()
        .map(|c| {
            let mut n = Node::new(
                &Target::Container { id: c.id.clone() },
                c.name.clone(),
                Icon::Container,
            )
            .detail(c.image.clone())
            .status(Some(container_status(c.state)));
            n.muted = !c.state.is_running();
            n
        })
        .collect();
    let images = s
        .images
        .iter()
        .map(|im| {
            let mut n = Node::new(
                &Target::Image { id: im.id.clone() },
                im.label(),
                Icon::Image,
            )
            .detail(format_size(im.size));
            n.muted = im.dangling();
            n
        })
        .collect();
    let networks = s
        .networks
        .iter()
        .map(|n| {
            Node::new(
                &Target::Network { id: n.id.clone() },
                n.name.clone(),
                Icon::Network,
            )
            .detail(n.driver.clone())
        })
        .collect();
    let volumes = s
        .volumes
        .iter()
        .map(|v| {
            Node::new(
                &Target::Volume {
                    name: v.name.clone(),
                },
                v.name.clone(),
                Icon::Volume,
            )
            .detail(v.driver.clone())
        })
        .collect();
    node.children = vec![
        group("containers", "Containers", containers, true),
        group("images", "Images", images, false),
        group("networks", "Networks", networks, false),
        group("volumes", "Volumes", volumes, false),
    ];
    node
}

fn compose_nodes(input: &ServicesInput) -> Option<Node> {
    let projects = compose_projects(input);
    if projects.is_empty() {
        return None;
    }
    let children = projects
        .iter()
        .map(|p| {
            let services: Vec<Node> = p
                .services
                .iter()
                .map(|s| {
                    let target = Target::ComposeService {
                        project: p.name.clone(),
                        service: s.clone(),
                    };
                    let containers = p.containers.get(s).cloned().unwrap_or_default();
                    let spec = p.file.and_then(|f| f.service(s));
                    let detail = spec
                        .and_then(|sp| {
                            sp.image
                                .clone()
                                .or_else(|| sp.build.as_ref().map(|_| "build".to_string()))
                        })
                        .unwrap_or_default();
                    let mut node = Node::new(&target, s.clone(), Icon::Service)
                        .detail(detail)
                        .status(aggregate(&containers).or(Some(Status::Stopped)));
                    node.muted = containers.is_empty();
                    if containers.len() > 1
                        || containers
                            .iter()
                            .any(|c| c.compose.as_ref().and_then(|r| r.number).unwrap_or(1) > 1)
                    {
                        node.children = containers
                            .iter()
                            .map(|c| {
                                let mut n = Node::new(&target, c.name.clone(), Icon::Container)
                                    .status(Some(container_status(c.state)));
                                n.id = format!("{}/{}", target.node_id(), c.id);
                                n
                            })
                            .collect();
                    }
                    node
                })
                .collect();
            let all: Vec<&Container> = p.containers.values().flatten().copied().collect();
            let detail = p
                .file
                .map(|f| {
                    f.path
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(&f.path)
                        .to_string()
                })
                .unwrap_or_default();
            Node::new(
                &Target::ComposeProject {
                    project: p.name.clone(),
                },
                p.name.clone(),
                Icon::Compose,
            )
            .detail(detail)
            .status(aggregate(&all).or(Some(Status::Stopped)))
            .children(services)
            .expanded(true)
        })
        .collect();
    Some(
        Node::new(&Target::ComposeRoot, "Docker Compose", Icon::Compose)
            .children(children)
            .expanded(true),
    )
}

fn kube_nodes(input: &ServicesInput) -> Option<Node> {
    let k = &input.kube;
    let root = Target::KubeRoot;
    let mut node = Node::new(&root, "Kubernetes", Icon::Kubernetes)
        .detail(k.path.clone())
        .expanded(true);
    let Some(cfg) = &k.config else {
        let e = k.error.as_ref()?;
        node.children = vec![info_node("kube", e, Icon::Error)];
        return Some(node);
    };
    if cfg.contexts.is_empty() {
        node.children = vec![info_node(
            "kube",
            "No contexts in the kubeconfig",
            Icon::Kubernetes,
        )];
        return Some(node);
    }
    let current = k.current_context();
    node.children = cfg
        .contexts
        .iter()
        .map(|c| {
            let target = Target::KubeContext {
                context: c.name.clone(),
            };
            let server = cfg
                .clusters
                .iter()
                .find(|cl| cl.name == c.cluster)
                .map(|cl| cl.server.clone())
                .unwrap_or_default();
            let is_current = current == Some(c.name.as_str());
            let mut n = Node::new(&target, c.name.clone(), Icon::Context).detail(if is_current {
                format!("current · {server}")
            } else {
                server
            });
            let default_ns = k
                .default_namespace
                .clone()
                .filter(|n| !n.is_empty() && is_current)
                .or_else(|| c.namespace.clone())
                .unwrap_or_else(|| "default".into());
            match k.namespaces.get(&c.name) {
                None => n.lazy = true,
                Some(Loaded::Loading) => {
                    n.status = Some(Status::Busy);
                    n.lazy = true;
                }
                Some(Loaded::Failed { error }) => {
                    n.status = Some(Status::Error);
                    // Listing namespaces may be forbidden: still offer the default one.
                    n.children = vec![
                        namespace_node(input, &c.name, &default_ns, true),
                        info_node(
                            &target.node_id(),
                            &format!("Cannot list namespaces: {error}"),
                            Icon::Error,
                        ),
                    ];
                    n.expanded = true;
                }
                Some(Loaded::Ready { value }) => {
                    n.status = Some(Status::Running);
                    n.expanded = true;
                    let mut names = value.clone();
                    if !names.contains(&default_ns) {
                        names.insert(0, default_ns.clone());
                    }
                    n.children = names
                        .iter()
                        .map(|ns| namespace_node(input, &c.name, ns, ns == &default_ns))
                        .collect();
                }
            }
            n
        })
        .collect();
    Some(node)
}

fn namespace_node(input: &ServicesInput, context: &str, ns: &str, default: bool) -> Node {
    let target = Target::KubeNamespace {
        context: context.into(),
        namespace: ns.into(),
    };
    let mut n = Node::new(&target, ns, Icon::Namespace);
    if default {
        n.detail = Some("default".into());
    }
    match input.kube.workloads.get(&workloads_key(context, ns)) {
        None => n.lazy = true,
        Some(Loaded::Loading) => {
            n.lazy = true;
            n.status = Some(Status::Busy);
        }
        Some(Loaded::Failed { error }) => {
            n.status = Some(Status::Error);
            n.children = vec![info_node(&target.node_id(), error, Icon::Error)];
        }
        Some(Loaded::Ready { value: w }) => {
            n.expanded = default;
            let group = |g: &str, label: &str, children: Vec<Node>| {
                let count = children.len();
                let mut node = Node::new(
                    &Target::KubeGroup {
                        context: context.into(),
                        namespace: ns.into(),
                        group: g.into(),
                    },
                    label,
                    Icon::Folder,
                )
                .detail(count.to_string())
                .children(children)
                .expanded(g == "pods");
                node.muted = count == 0;
                node
            };
            let pods = w
                .pods
                .iter()
                .map(|p| {
                    let status = pod_status(p);
                    let target = Target::Pod {
                        context: context.into(),
                        namespace: ns.into(),
                        name: p.name.clone(),
                    };
                    let id = target.node_id();
                    let forwards: Vec<String> = input
                        .port_forwards
                        .iter()
                        .filter(|f| f.node_id == id)
                        .map(|f| format!("{}→{}", f.local_port, f.remote_port))
                        .collect();
                    let mut detail = format!("{} · {}", p.ready, p.status);
                    if !forwards.is_empty() {
                        detail.push_str(&format!(" · ⇄ {}", forwards.join(", ")));
                    }
                    Node::new(&target, p.name.clone(), Icon::Pod)
                        .detail(detail)
                        .status(Some(status))
                })
                .collect();
            let deployments = w
                .deployments
                .iter()
                .map(|d| {
                    let status = if d.ready_replicas >= d.replicas {
                        Status::Running
                    } else if d.ready_replicas > 0 {
                        Status::Pending
                    } else if d.replicas == 0 {
                        Status::Stopped
                    } else {
                        Status::Error
                    };
                    Node::new(
                        &Target::Deployment {
                            context: context.into(),
                            namespace: ns.into(),
                            name: d.name.clone(),
                        },
                        d.name.clone(),
                        Icon::Deployment,
                    )
                    .detail(format!("{}/{}", d.ready_replicas, d.replicas))
                    .status(Some(status))
                })
                .collect();
            let services = w
                .services
                .iter()
                .map(|s| {
                    Node::new(
                        &Target::KubeService {
                            context: context.into(),
                            namespace: ns.into(),
                            name: s.name.clone(),
                        },
                        s.name.clone(),
                        Icon::KubeService,
                    )
                    .detail(s.kind.clone())
                })
                .collect();
            n.children = vec![
                group("pods", "Pods", pods),
                group("deployments", "Deployments", deployments),
                group("services", "Services", services),
            ];
        }
    }
    n
}

/// Status dot of a pod: completed pods are stopped, starting ones pending.
fn pod_status(p: &crate::kube::Pod) -> Status {
    if p.status == "Completed" {
        Status::Stopped
    } else if p.healthy() {
        Status::Running
    } else if matches!(
        p.status.as_str(),
        "Pending" | "ContainerCreating" | "Terminating" | "PodInitializing"
    ) {
        Status::Pending
    } else {
        Status::Error
    }
}

fn mark_busy(node: &mut Node, busy: &[String]) {
    if busy.iter().any(|b| b == &node.id) {
        node.status = Some(Status::Busy);
    }
    for c in &mut node.children {
        mark_busy(c, busy);
    }
}

/// The whole tree (top-level nodes).
pub fn build_tree(input: &ServicesInput) -> Vec<Node> {
    let mut roots = vec![docker_nodes(input)];
    roots.extend(compose_nodes(input));
    roots.extend(kube_nodes(input));
    for r in &mut roots {
        mark_busy(r, &input.busy);
    }
    roots
}

// ---- actions ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Start,
    Stop,
    Restart,
    Remove,
    Logs,
    Exec,
    Pull,
    Up,
    Down,
    Connect,
    Refresh,
    /// Kubernetes Forward Ports (the UI asks for the ports).
    ForwardPorts,
}

/// Toolbar actions that apply to a node, in toolbar order.
pub fn actions(input: &ServicesInput, target: &Target) -> Vec<Action> {
    use Action::*;
    let connected = input.docker.state == ConnectionState::Connected;
    let snapshot = input.docker.snapshot.as_ref();
    let container = |id: &str| snapshot.and_then(|s| s.containers.iter().find(|c| c.id == id));
    match target {
        Target::Docker => {
            if connected {
                vec![Refresh]
            } else {
                vec![Connect, Refresh]
            }
        }
        Target::Container { id } => match container(id) {
            Some(c) if c.state.is_running() => vec![Stop, Restart, Remove, Logs, Exec, Pull],
            Some(_) => vec![Start, Remove, Logs, Pull],
            None => vec![],
        },
        Target::Image { .. } => vec![Pull, Remove],
        Target::Volume { .. } | Target::Network { .. } => vec![Remove],
        Target::DockerGroup { group } if group == "images" => vec![Pull, Refresh],
        Target::ComposeProject { project } => {
            let running = compose_projects(input)
                .iter()
                .find(|p| &p.name == project)
                .is_some_and(|p| {
                    p.containers
                        .values()
                        .flatten()
                        .any(|c| c.state.is_running())
                });
            if running {
                vec![Up, Stop, Restart, Down, Logs]
            } else {
                vec![Up, Down, Logs]
            }
        }
        Target::ComposeService { project, service } => {
            let projects = compose_projects(input);
            let containers = projects
                .iter()
                .find(|p| &p.name == project)
                .and_then(|p| p.containers.get(service))
                .cloned()
                .unwrap_or_default();
            if containers.iter().any(|c| c.state.is_running()) {
                vec![Up, Stop, Restart, Down, Logs, Exec]
            } else if containers.is_empty() {
                vec![Up]
            } else {
                vec![Up, Down, Logs]
            }
        }
        Target::KubeContext { .. } | Target::KubeNamespace { .. } | Target::KubeRoot => {
            vec![Refresh]
        }
        Target::Pod { .. } => vec![Remove, Logs, Exec, ForwardPorts],
        Target::Deployment { .. } => vec![Restart, Logs],
        _ => vec![Refresh],
    }
}

// ---- details ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Tab {
    Dashboard,
    Log,
    Console,
    Inspection,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub label: String,
    pub value: String,
    /// `http://localhost:8000` for published ports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Status of each row (first cell gets the dot), when the rows are services/pods.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status: Vec<Option<Status>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Row>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<Table>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    pub node_id: String,
    pub target: Target,
    pub title: String,
    pub icon: Icon,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    /// `Running · Up 2 hours`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub tabs: Vec<Tab>,
    pub sections: Vec<Section>,
    pub actions: Vec<Action>,
    /// Text for the empty Dashboard (connection errors, hints).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Containers of a pod for the Log / Exec / Attached Console picker.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub containers: Vec<ContainerChoice>,
    /// Active port forwards: the pod's, or all of them on the Kubernetes nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_forwards: Vec<PortForward>,
    /// Ports the pod's containers declare (Forward Ports suggestions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<ContainerPort>,
}

/// A container in the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerChoice {
    pub name: String,
    /// An init container (logs only).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub init: bool,
    /// The container `kubectl` uses without `-c`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
}

fn row(label: &str, value: impl Into<String>) -> Row {
    Row {
        label: label.into(),
        value: value.into(),
        url: None,
        status: None,
    }
}

fn section(title: &str, rows: Vec<Row>) -> Section {
    Section {
        title: title.into(),
        rows: rows.into_iter().filter(|r| !r.value.is_empty()).collect(),
        table: None,
    }
}

fn table_section(
    title: &str,
    columns: &[&str],
    rows: Vec<Vec<String>>,
    status: Vec<Option<Status>>,
) -> Section {
    Section {
        title: title.into(),
        rows: Vec::new(),
        table: Some(Table {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
            status,
        }),
    }
}

fn state_label(s: ContainerState) -> &'static str {
    match s {
        ContainerState::Created => "Created",
        ContainerState::Running => "Running",
        ContainerState::Paused => "Paused",
        ContainerState::Restarting => "Restarting",
        ContainerState::Removing => "Removing",
        ContainerState::Exited => "Exited",
        ContainerState::Dead => "Dead",
    }
}

fn container_sections(c: &Container, input: &ServicesInput) -> Vec<Section> {
    let ports: Vec<Row> = c
        .ports
        .iter()
        .map(|p| Row {
            label: format!("{}/{}", p.private_port, p.protocol),
            value: p.label(),
            url: p
                .public_port
                .filter(|_| p.protocol == "tcp")
                .map(|pp| format!("http://localhost:{pp}")),
            status: None,
        })
        .collect();
    let mut sections = vec![section(
        "Container",
        vec![
            row("Name", c.name.clone()),
            row("ID", c.short_id().to_string()),
            row("Image", c.image.clone()),
            Row {
                status: Some(container_status(c.state)),
                ..row("Status", format!("{} · {}", state_label(c.state), c.status))
            },
            row(
                "Created",
                if c.created > 0 {
                    format!(
                        "{} ({} ago)",
                        format_utc(c.created),
                        long_age(c.created, input.now)
                    )
                } else {
                    String::new()
                },
            ),
            row("Command", c.command.clone()),
        ],
    )];
    if let Some(r) = &c.compose {
        sections.push(section(
            "Docker Compose",
            vec![
                row("Project", r.project.clone()),
                row("Service", r.service.clone()),
                row(
                    "Working directory",
                    r.working_dir.clone().unwrap_or_default(),
                ),
                row("Config files", r.config_files.join(", ")),
            ],
        ));
    }
    if !ports.is_empty() {
        sections.push(section("Port Bindings", ports));
    }
    if !c.networks.is_empty() {
        sections.push(section(
            "Networks",
            c.networks
                .iter()
                .map(|(n, ip)| row(n, ip.clone()))
                .collect(),
        ));
    }
    if !c.mounts.is_empty() {
        sections.push(table_section(
            "Volume Bindings",
            &["Host / volume", "Container path", "Mode"],
            c.mounts
                .iter()
                .map(|m| {
                    vec![
                        m.name.clone().unwrap_or_else(|| m.source.clone()),
                        m.destination.clone(),
                        if m.read_write {
                            "rw".into()
                        } else {
                            "ro".into()
                        },
                    ]
                })
                .collect(),
            Vec::new(),
        ));
    }
    let labels: Vec<Row> = c
        .labels
        .iter()
        .filter(|(k, _)| !k.starts_with("com.docker.compose."))
        .map(|(k, v)| row(k, v.clone()))
        .collect();
    if !labels.is_empty() {
        sections.push(section("Labels", labels));
    }
    sections
}

/// Details pane content for a node; `None` for unknown ids.
pub fn details(input: &ServicesInput, node_id: &str) -> Option<Details> {
    let target = Target::parse(node_id)?;
    let tree = build_tree(input);
    let node = tree.iter().find_map(|r| r.find(node_id));
    let snapshot = input.docker.snapshot.as_ref();
    let mut d = Details {
        node_id: node_id.to_string(),
        target: target.clone(),
        title: node.map(|n| n.label.clone()).unwrap_or_default(),
        icon: node.map(|n| n.icon).unwrap_or(Icon::Folder),
        status: node.and_then(|n| n.status),
        subtitle: None,
        tabs: vec![Tab::Dashboard],
        sections: Vec::new(),
        actions: actions(input, &target),
        message: None,
        containers: Vec::new(),
        port_forwards: Vec::new(),
        ports: Vec::new(),
    };
    match &target {
        Target::Docker => {
            let dk = &input.docker;
            d.subtitle = Some(match dk.state {
                ConnectionState::Connected => "Connected".into(),
                ConnectionState::Connecting => "Connecting…".into(),
                ConnectionState::Failed => "Connection failed".into(),
                ConnectionState::Disconnected => "Not connected".into(),
            });
            if dk.state == ConnectionState::Failed {
                d.message = Some(format!(
                    "Cannot connect to the Docker daemon at {}: {}\nCheck Settings | Build, Execution, Deployment | Docker.",
                    dk.endpoint,
                    dk.error.as_deref().unwrap_or("unknown error")
                ));
            }
            let mut rows = vec![row("Connection", dk.endpoint.clone())];
            if let Some(s) = snapshot {
                if let Some(v) = &s.version {
                    rows.push(row(
                        "Engine",
                        v.platform
                            .clone()
                            .map(|p| format!("{p} {}", v.version))
                            .unwrap_or_else(|| v.version.clone()),
                    ));
                    rows.push(row("API version", v.api_version.clone()));
                    rows.push(row("OS / architecture", format!("{}/{}", v.os, v.arch)));
                }
                let running = s.containers.iter().filter(|c| c.state.is_running()).count();
                rows.push(row(
                    "Containers",
                    format!("{} ({running} running)", s.containers.len()),
                ));
                rows.push(row("Images", s.images.len().to_string()));
                rows.push(row("Volumes", s.volumes.len().to_string()));
                rows.push(row("Networks", s.networks.len().to_string()));
            }
            d.sections.push(section("Docker", rows));
        }
        Target::DockerGroup { group } => {
            let s = snapshot?;
            d.sections.push(match group.as_str() {
                "containers" => table_section(
                    "Containers",
                    &["Name", "Image", "Status", "Ports"],
                    s.containers
                        .iter()
                        .map(|c| {
                            vec![
                                c.name.clone(),
                                c.image.clone(),
                                c.status.clone(),
                                c.ports
                                    .iter()
                                    .map(|p| p.label())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            ]
                        })
                        .collect(),
                    s.containers
                        .iter()
                        .map(|c| Some(container_status(c.state)))
                        .collect(),
                ),
                "images" => table_section(
                    "Images",
                    &["Repository:Tag", "ID", "Created", "Size"],
                    s.images
                        .iter()
                        .map(|im| {
                            vec![
                                im.label(),
                                crate::docker::short_id(&im.id).into(),
                                format!("{} ago", long_age(im.created, input.now)),
                                format_size(im.size),
                            ]
                        })
                        .collect(),
                    Vec::new(),
                ),
                "networks" => table_section(
                    "Networks",
                    &["Name", "Driver", "Scope", "Subnet"],
                    s.networks
                        .iter()
                        .map(|n| {
                            vec![
                                n.name.clone(),
                                n.driver.clone(),
                                n.scope.clone(),
                                n.subnet.clone().unwrap_or_default(),
                            ]
                        })
                        .collect(),
                    Vec::new(),
                ),
                _ => table_section(
                    "Volumes",
                    &["Name", "Driver", "Mountpoint"],
                    s.volumes
                        .iter()
                        .map(|v| vec![v.name.clone(), v.driver.clone(), v.mountpoint.clone()])
                        .collect(),
                    Vec::new(),
                ),
            });
        }
        Target::Container { id } => {
            let c = snapshot?.containers.iter().find(|c| &c.id == id)?;
            d.title = c.name.clone();
            d.icon = Icon::Container;
            d.status = Some(container_status(c.state));
            d.subtitle = Some(c.status.clone());
            d.tabs = vec![Tab::Dashboard, Tab::Log, Tab::Console, Tab::Inspection];
            d.sections = container_sections(c, input);
        }
        Target::Image { id } => {
            let s = snapshot?;
            let im = s.images.iter().find(|i| &i.id == id)?;
            d.tabs = vec![Tab::Dashboard, Tab::Inspection];
            d.subtitle = Some(format_size(im.size));
            d.sections.push(section(
                "Image",
                vec![
                    row(
                        "Tags",
                        im.repo_tags
                            .iter()
                            .filter(|t| *t != "<none>:<none>")
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    row("ID", crate::docker::short_id(&im.id).to_string()),
                    row(
                        "Created",
                        if im.created > 0 {
                            format!(
                                "{} ({} ago)",
                                format_utc(im.created),
                                long_age(im.created, input.now)
                            )
                        } else {
                            String::new()
                        },
                    ),
                    row("Size", format_size(im.size)),
                    row("Digests", im.repo_digests.join("\n")),
                ],
            ));
            let used: Vec<&Container> = s
                .containers
                .iter()
                .filter(|c| &c.image_id == id || im.repo_tags.contains(&c.image))
                .collect();
            if !used.is_empty() {
                d.sections.push(table_section(
                    "Containers",
                    &["Name", "Status"],
                    used.iter()
                        .map(|c| vec![c.name.clone(), c.status.clone()])
                        .collect(),
                    used.iter()
                        .map(|c| Some(container_status(c.state)))
                        .collect(),
                ));
            }
        }
        Target::Volume { name } => {
            let s = snapshot?;
            let v = s.volumes.iter().find(|v| &v.name == name)?;
            d.tabs = vec![Tab::Dashboard, Tab::Inspection];
            d.sections.push(section(
                "Volume",
                vec![
                    row("Name", v.name.clone()),
                    row("Driver", v.driver.clone()),
                    row("Mountpoint", v.mountpoint.clone()),
                    row("Scope", v.scope.clone()),
                    row("Created", v.created.map(format_utc).unwrap_or_default()),
                ],
            ));
            let used: Vec<&Container> = s
                .containers
                .iter()
                .filter(|c| c.mounts.iter().any(|m| m.name.as_deref() == Some(name)))
                .collect();
            if !used.is_empty() {
                d.sections.push(table_section(
                    "Used by",
                    &["Container", "Status"],
                    used.iter()
                        .map(|c| vec![c.name.clone(), c.status.clone()])
                        .collect(),
                    used.iter()
                        .map(|c| Some(container_status(c.state)))
                        .collect(),
                ));
            }
        }
        Target::Network { id } => {
            let s = snapshot?;
            let n = s.networks.iter().find(|n| &n.id == id)?;
            d.tabs = vec![Tab::Dashboard, Tab::Inspection];
            d.sections.push(section(
                "Network",
                vec![
                    row("Name", n.name.clone()),
                    row("ID", crate::docker::short_id(&n.id).to_string()),
                    row("Driver", n.driver.clone()),
                    row("Scope", n.scope.clone()),
                    row("Subnet", n.subnet.clone().unwrap_or_default()),
                    row("Gateway", n.gateway.clone().unwrap_or_default()),
                    row(
                        "Internal",
                        if n.internal {
                            "yes".into()
                        } else {
                            String::new()
                        },
                    ),
                ],
            ));
            let attached: Vec<&Container> = s
                .containers
                .iter()
                .filter(|c| c.networks.contains_key(&n.name))
                .collect();
            if !attached.is_empty() {
                d.sections.push(table_section(
                    "Containers",
                    &["Name", "IP address"],
                    attached
                        .iter()
                        .map(|c| vec![c.name.clone(), c.networks[&n.name].clone()])
                        .collect(),
                    attached
                        .iter()
                        .map(|c| Some(container_status(c.state)))
                        .collect(),
                ));
            }
        }
        Target::ComposeRoot => {
            let projects = compose_projects(input);
            d.sections.push(table_section(
                "Projects",
                &["Project", "File", "Services"],
                projects
                    .iter()
                    .map(|p| {
                        vec![
                            p.name.clone(),
                            p.file
                                .map(|f| f.path.clone())
                                .unwrap_or_else(|| "(running only)".into()),
                            p.services.len().to_string(),
                        ]
                    })
                    .collect(),
                projects
                    .iter()
                    .map(|p| {
                        aggregate(&p.containers.values().flatten().copied().collect::<Vec<_>>())
                            .or(Some(Status::Stopped))
                    })
                    .collect(),
            ));
        }
        Target::ComposeProject { project } => {
            let projects = compose_projects(input);
            let p = projects.iter().find(|p| &p.name == project)?;
            d.tabs = vec![Tab::Dashboard, Tab::Log];
            let running = p
                .containers
                .values()
                .flatten()
                .filter(|c| c.state.is_running())
                .count();
            d.subtitle = Some(format!(
                "{running} of {} containers running",
                p.containers.values().flatten().count()
            ));
            let mut rows = vec![row("Project", p.name.clone())];
            if let Some(f) = p.file {
                rows.push(row("Compose file", f.path.clone()));
                rows.push(row("Profiles", f.profiles.join(", ")));
                rows.push(row("Volumes", f.volumes.join(", ")));
                rows.push(row("Networks", f.networks.join(", ")));
                if let Some(e) = &f.error {
                    rows.push(row("Problem", e.clone()));
                }
            } else {
                rows.push(row(
                    "Compose file",
                    "Not in this project (found from running containers)".to_string(),
                ));
            }
            d.sections.push(section("Docker Compose", rows));
            let mut table_rows = Vec::new();
            let mut status = Vec::new();
            for s in &p.services {
                let containers = p.containers.get(s).cloned().unwrap_or_default();
                let spec = p.file.and_then(|f| f.service(s));
                let image = spec
                    .and_then(|sp| sp.image.clone())
                    .or_else(|| containers.first().map(|c| c.image.clone()))
                    .or_else(|| {
                        spec.and_then(|sp| sp.build.as_ref())
                            .map(|b| format!("build {}", b.context))
                    })
                    .unwrap_or_default();
                let state = containers
                    .first()
                    .map(|c| c.status.clone())
                    .unwrap_or_else(|| "Not created".into());
                let ports = containers
                    .iter()
                    .flat_map(|c| c.ports.iter().map(|p| p.label()))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ports = if ports.is_empty() {
                    spec.map(|sp| sp.ports.join(", ")).unwrap_or_default()
                } else {
                    ports
                };
                table_rows.push(vec![s.clone(), image, state, ports]);
                status.push(aggregate(&containers).or(Some(Status::Stopped)));
            }
            d.sections.push(table_section(
                "Services",
                &["Service", "Image", "State", "Ports"],
                table_rows,
                status,
            ));
        }
        Target::ComposeService { project, service } => {
            let projects = compose_projects(input);
            let p = projects.iter().find(|p| &p.name == project)?;
            let containers = p.containers.get(service).cloned().unwrap_or_default();
            let spec = p.file.and_then(|f| f.service(service));
            d.tabs = if containers.is_empty() {
                vec![Tab::Dashboard]
            } else {
                vec![Tab::Dashboard, Tab::Log, Tab::Console, Tab::Inspection]
            };
            d.subtitle = Some(match containers.first() {
                Some(c) => c.status.clone(),
                None => "Not created; Up starts it".into(),
            });
            if let Some(sp) = spec {
                d.sections.push(section(
                    "Service",
                    vec![
                        row("Image", sp.image.clone().unwrap_or_default()),
                        row(
                            "Build",
                            sp.build
                                .as_ref()
                                .map(|b| match &b.dockerfile {
                                    Some(f) => format!("{} ({f})", b.context),
                                    None => b.context.clone(),
                                })
                                .unwrap_or_default(),
                        ),
                        row(
                            "Container name",
                            sp.container_name.clone().unwrap_or_default(),
                        ),
                        row("Command", sp.command.clone().unwrap_or_default()),
                        row("Ports", sp.ports.join(", ")),
                        row("Depends on", sp.depends_on.join(", ")),
                        row("Profiles", sp.profiles.join(", ")),
                        row("Env files", sp.env_files.join(", ")),
                        row("Restart", sp.restart.clone().unwrap_or_default()),
                        row("Volumes", sp.volumes.join("\n")),
                    ],
                ));
                if !sp.environment.is_empty() {
                    d.sections.push(section(
                        "Environment",
                        sp.environment
                            .iter()
                            .map(|e| {
                                row(
                                    &e.name,
                                    e.value
                                        .clone()
                                        .unwrap_or_else(|| "(from the environment)".into()),
                                )
                            })
                            .collect(),
                    ));
                }
            }
            if let [c] = containers.as_slice() {
                d.sections.extend(
                    container_sections(c, input)
                        .into_iter()
                        .filter(|s| s.title != "Docker Compose" && s.title != "Labels"),
                );
            } else if !containers.is_empty() {
                d.sections.push(table_section(
                    "Containers",
                    &["Name", "Status", "Ports"],
                    containers
                        .iter()
                        .map(|c| {
                            vec![
                                c.name.clone(),
                                c.status.clone(),
                                c.ports
                                    .iter()
                                    .map(|p| p.label())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            ]
                        })
                        .collect(),
                    containers
                        .iter()
                        .map(|c| Some(container_status(c.state)))
                        .collect(),
                ));
            }
        }
        Target::KubeRoot => {
            let k = &input.kube;
            let mut rows = vec![row("Kubeconfig", k.path.clone())];
            if let Some(c) = k.current_context() {
                rows.push(row("Current context", c.to_string()));
            }
            if let Some(e) = &k.error {
                d.message = Some(e.clone());
            }
            d.sections.push(section("Kubernetes", rows));
            d.port_forwards = input.port_forwards.clone();
        }
        Target::KubeContext { context } => {
            d.port_forwards = input
                .port_forwards
                .iter()
                .filter(|f| &f.context == context)
                .cloned()
                .collect();
            let cfg = input.kube.config.as_ref()?;
            let r = cfg.resolve(context).ok()?;
            d.sections.push(section(
                "Context",
                vec![
                    row("Name", context.clone()),
                    row("Cluster", r.cluster.name.clone()),
                    row("Server", r.cluster.server.clone()),
                    row("User", r.user.name.clone()),
                    row("Authentication", r.user.auth_kind().to_string()),
                    row("Namespace", r.namespace().to_string()),
                    row(
                        "TLS",
                        if r.cluster.insecure_skip_tls_verify {
                            "Not verified (insecure-skip-tls-verify)".into()
                        } else {
                            String::new()
                        },
                    ),
                ],
            ));
            if let Some(Loaded::Failed { error }) = input.kube.namespaces.get(context) {
                d.message = Some(error.clone());
            }
        }
        Target::KubeNamespace { context, namespace }
        | Target::KubeGroup {
            context, namespace, ..
        } => {
            let w = match input.kube.workloads.get(&workloads_key(context, namespace)) {
                Some(Loaded::Ready { value }) => value,
                Some(Loaded::Failed { error }) => {
                    d.message = Some(error.clone());
                    return Some(d);
                }
                _ => {
                    d.message = Some("Loading…".into());
                    return Some(d);
                }
            };
            let group = match &target {
                Target::KubeGroup { group, .. } => group.as_str(),
                _ => "",
            };
            if group.is_empty() || group == "pods" {
                d.sections.push(table_section(
                    "Pods",
                    &["Name", "Ready", "Status", "Restarts", "Age"],
                    w.pods
                        .iter()
                        .map(|p| {
                            vec![
                                p.name.clone(),
                                p.ready.clone(),
                                p.status.clone(),
                                p.restarts.to_string(),
                                age(p.created, input.now),
                            ]
                        })
                        .collect(),
                    w.pods.iter().map(|p| Some(pod_status(p))).collect(),
                ));
            }
            if group.is_empty() || group == "deployments" {
                d.sections.push(table_section(
                    "Deployments",
                    &["Name", "Ready", "Up-to-date", "Available", "Age"],
                    w.deployments
                        .iter()
                        .map(|x| {
                            vec![
                                x.name.clone(),
                                format!("{}/{}", x.ready_replicas, x.replicas),
                                x.updated_replicas.to_string(),
                                x.available_replicas.to_string(),
                                age(x.created, input.now),
                            ]
                        })
                        .collect(),
                    Vec::new(),
                ));
            }
            if group.is_empty() || group == "services" {
                d.sections.push(table_section(
                    "Services",
                    &["Name", "Type", "Cluster IP", "External IP", "Ports", "Age"],
                    w.services
                        .iter()
                        .map(|s| {
                            vec![
                                s.name.clone(),
                                s.kind.clone(),
                                s.cluster_ip.clone().unwrap_or_default(),
                                s.external_ip.clone().unwrap_or_else(|| "<none>".into()),
                                s.ports.join(","),
                                age(s.created, input.now),
                            ]
                        })
                        .collect(),
                    Vec::new(),
                ));
            }
        }
        Target::Pod {
            context,
            namespace,
            name,
        } => {
            let Some(Loaded::Ready { value: w }) =
                input.kube.workloads.get(&workloads_key(context, namespace))
            else {
                return Some(d);
            };
            let p = w.pods.iter().find(|p| &p.name == name)?;
            d.tabs = vec![Tab::Dashboard, Tab::Log, Tab::Console, Tab::Inspection];
            let default = p.default_container();
            d.containers = p
                .containers
                .iter()
                .map(|c| ContainerChoice {
                    name: c.clone(),
                    init: false,
                    default: Some(c.as_str()) == default,
                })
                .chain(p.init_containers.iter().map(|c| ContainerChoice {
                    name: c.clone(),
                    init: true,
                    default: false,
                }))
                .collect();
            d.ports = p.ports.clone();
            d.port_forwards = input
                .port_forwards
                .iter()
                .filter(|f| f.node_id == node_id)
                .cloned()
                .collect();
            d.subtitle = Some(format!("{} · {} ready", p.status, p.ready));
            d.sections.push(section(
                "Pod",
                vec![
                    row("Name", p.name.clone()),
                    row("Namespace", p.namespace.clone()),
                    Row {
                        status: d.status,
                        ..row("Status", p.status.clone())
                    },
                    row("Ready", p.ready.clone()),
                    row("Restarts", p.restarts.to_string()),
                    row("Age", age(p.created, input.now)),
                    row("Node", p.node.clone().unwrap_or_default()),
                    row("Pod IP", p.ip.clone().unwrap_or_default()),
                    row("Controlled by", p.owner.clone().unwrap_or_default()),
                    row("Containers", p.containers.join(", ")),
                ],
            ));
            if !p.labels.is_empty() {
                d.sections.push(section(
                    "Labels",
                    p.labels.iter().map(|(k, v)| row(k, v.clone())).collect(),
                ));
            }
        }
        Target::Deployment {
            context,
            namespace,
            name,
        } => {
            let Some(Loaded::Ready { value: w }) =
                input.kube.workloads.get(&workloads_key(context, namespace))
            else {
                return Some(d);
            };
            let x = w.deployments.iter().find(|x| &x.name == name)?;
            d.tabs = vec![Tab::Dashboard, Tab::Log, Tab::Inspection];
            d.subtitle = Some(format!("{}/{} ready", x.ready_replicas, x.replicas));
            d.sections.push(section(
                "Deployment",
                vec![
                    row("Name", x.name.clone()),
                    row("Namespace", x.namespace.clone()),
                    row(
                        "Replicas",
                        format!(
                            "{} desired, {} updated, {} available, {} ready",
                            x.replicas, x.updated_replicas, x.available_replicas, x.ready_replicas
                        ),
                    ),
                    row("Images", x.images.join(", ")),
                    row(
                        "Selector",
                        x.selector
                            .iter()
                            .map(|(k, v)| format!("{k}={v}"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    row("Age", age(x.created, input.now)),
                ],
            ));
            let pods: Vec<_> = w.pods.iter().filter(|p| x.selects(p)).collect();
            if !pods.is_empty() {
                d.sections.push(table_section(
                    "Pods",
                    &["Name", "Ready", "Status", "Restarts"],
                    pods.iter()
                        .map(|p| {
                            vec![
                                p.name.clone(),
                                p.ready.clone(),
                                p.status.clone(),
                                p.restarts.to_string(),
                            ]
                        })
                        .collect(),
                    pods.iter().map(|p| Some(pod_status(p))).collect(),
                ));
            }
        }
        Target::KubeService {
            context,
            namespace,
            name,
        } => {
            let Some(Loaded::Ready { value: w }) =
                input.kube.workloads.get(&workloads_key(context, namespace))
            else {
                return Some(d);
            };
            let s = w.services.iter().find(|s| &s.name == name)?;
            d.tabs = vec![Tab::Dashboard, Tab::Inspection];
            d.sections.push(section(
                "Service",
                vec![
                    row("Name", s.name.clone()),
                    row("Type", s.kind.clone()),
                    row("Cluster IP", s.cluster_ip.clone().unwrap_or_default()),
                    row("External IP", s.external_ip.clone().unwrap_or_default()),
                    row("Ports", s.ports.join(", ")),
                    row("Age", age(s.created, input.now)),
                ],
            ));
        }
    }
    Some(d)
}

/// Pretty JSON for the Inspection tab.
pub fn inspection_text(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::RawSnapshot;
    use crate::kube::{parse_kubeconfig, RawWorkloads};
    use serde_json::json;

    pub(crate) fn input() -> ServicesInput {
        let raw = RawSnapshot {
            containers: json!([
                { "Id": "c-app", "Names": ["/acme-shop-app-1"], "Image": "acme-shop-app", "State": "running", "Status": "Up 2 hours",
                  "Ports": [{ "IP": "0.0.0.0", "PrivatePort": 8000, "PublicPort": 8000, "Type": "tcp" }],
                  "Labels": { "com.docker.compose.project": "acme-shop", "com.docker.compose.service": "app", "com.docker.compose.container-number": "1" } },
                { "Id": "c-mysql", "Names": ["/acme-shop-mysql-1"], "Image": "mysql:8.4", "State": "exited", "Status": "Exited (0) 5 minutes ago",
                  "Labels": { "com.docker.compose.project": "acme-shop", "com.docker.compose.service": "mysql" },
                  "Mounts": [{ "Type": "volume", "Name": "acme-shop_db-data", "Source": "/var/lib/docker/volumes/acme-shop_db-data/_data", "Destination": "/var/lib/mysql", "RW": true }] },
                { "Id": "c-other", "Names": ["/legacy-web-1"], "Image": "nginx", "State": "running", "Status": "Up 1 day",
                  "Labels": { "com.docker.compose.project": "legacy", "com.docker.compose.service": "web" } }
            ]),
            images: json!([{ "Id": "sha256:mysql", "RepoTags": ["mysql:8.4"], "Size": 612_000_000, "Created": 1 }]),
            volumes: json!({ "Volumes": [{ "Name": "acme-shop_db-data", "Driver": "local" }] }),
            networks: json!([{ "Id": "net1", "Name": "acme-shop_default", "Driver": "bridge" }]),
            version: json!({ "Version": "27.3.1", "ApiVersion": "1.47", "Os": "linux", "Arch": "amd64" }),
        };
        let compose = crate::compose::parse(
            "docker-compose.yml",
            "services:\n  app:\n    build: .\n  mysql:\n    image: mysql:8.4\n  redis:\n    image: redis:7-alpine\n",
            &Default::default(),
        );
        let mut compose = compose;
        compose.project = "acme-shop".into();
        let cfg = parse_kubeconfig(
            "current-context: kind-acme\nclusters:\n- name: kind-acme\n  cluster: { server: 'https://127.0.0.1:6443' }\ncontexts:\n- name: kind-acme\n  context: { cluster: kind-acme, user: u, namespace: shop }\nusers:\n- name: u\n  user: { token: abc }\n",
            "",
        )
        .unwrap();
        let mut kube = KubeInput {
            path: "~/.kube/config".into(),
            config: Some(cfg),
            ..Default::default()
        };
        kube.namespaces.insert(
            "kind-acme".into(),
            Loaded::Ready {
                value: vec!["default".into(), "shop".into()],
            },
        );
        let w = RawWorkloads {
            pods: json!({ "items": [{ "metadata": { "name": "web-1", "namespace": "shop", "labels": { "app": "web" } }, "spec": { "containers": [{ "name": "php" }] },
                "status": { "phase": "Running", "containerStatuses": [{ "name": "php", "ready": true, "restartCount": 0, "state": { "running": {} } }] } }] }),
            deployments: json!({ "items": [{ "metadata": { "name": "web", "namespace": "shop" }, "spec": { "replicas": 1, "selector": { "matchLabels": { "app": "web" } } }, "status": { "readyReplicas": 1 } }] }),
            services: json!({ "items": [] }),
        };
        kube.workloads.insert(
            workloads_key("kind-acme", "shop"),
            Loaded::Ready { value: w.parse() },
        );
        ServicesInput {
            docker: DockerInput {
                state: ConnectionState::Connected,
                endpoint: "unix:///var/run/docker.sock".into(),
                error: None,
                snapshot: Some(raw.parse()),
            },
            compose_files: vec![compose],
            kube,
            now: 1_792_487_521,
            busy: vec![],
            port_forwards: vec![],
        }
    }

    #[test]
    fn node_ids_round_trip() {
        let targets = [
            Target::Docker,
            Target::DockerGroup {
                group: "images".into(),
            },
            Target::Image {
                id: "sha256:abc".into(),
            },
            Target::ComposeService {
                project: "acme-shop".into(),
                service: "app".into(),
            },
            Target::Pod {
                context: "arn:aws:eks:eu-1:1:cluster/prod".into(),
                namespace: "shop".into(),
                name: "web-1".into(),
            },
            Target::KubeGroup {
                context: "kind".into(),
                namespace: "a b".into(),
                group: "pods".into(),
            },
        ];
        for t in targets {
            assert_eq!(
                Target::parse(&t.node_id()),
                Some(t.clone()),
                "{}",
                t.node_id()
            );
        }
        assert_eq!(
            Target::parse("compose/acme-shop/app/c-app"),
            Some(Target::Container { id: "c-app".into() })
        );
        assert_eq!(Target::parse("nope/x"), None);
    }

    #[test]
    fn builds_tree() {
        let tree = build_tree(&input());
        let labels: Vec<_> = tree.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["Docker", "Docker Compose", "Kubernetes"]);
        let docker = &tree[0];
        assert_eq!(docker.children[0].label, "Containers");
        assert_eq!(docker.children[0].detail.as_deref(), Some("3"));
        assert_eq!(docker.children[0].children[0].status, Some(Status::Running));
        let compose = &tree[1];
        assert_eq!(
            compose.children.len(),
            2,
            "file project plus the running-only one"
        );
        let acme = &compose.children[0];
        assert_eq!(
            acme.status,
            Some(Status::Pending),
            "app runs, mysql stopped"
        );
        let services: Vec<_> = acme
            .children
            .iter()
            .map(|n| (n.label.as_str(), n.status))
            .collect();
        assert_eq!(
            services,
            [
                ("app", Some(Status::Running)),
                ("mysql", Some(Status::Stopped)),
                ("redis", Some(Status::Stopped))
            ]
        );
        assert!(acme.children[2].muted, "redis has no container");
        assert_eq!(compose.children[1].label, "legacy");
        let kube = &tree[2];
        let ctx = &kube.children[0];
        assert_eq!(
            ctx.detail.as_deref(),
            Some("current · https://127.0.0.1:6443")
        );
        let shop = ctx.children.iter().find(|n| n.label == "shop").unwrap();
        assert!(shop.expanded);
        assert_eq!(shop.children[0].children[0].label, "web-1");
        let default_ns = ctx.children.iter().find(|n| n.label == "default").unwrap();
        assert!(default_ns.lazy);
    }

    #[test]
    fn failed_connection_shows_error() {
        let mut i = input();
        i.docker = DockerInput {
            state: ConnectionState::Failed,
            endpoint: "unix:///var/run/docker.sock".into(),
            error: Some("No such file or directory".into()),
            snapshot: None,
        };
        let tree = build_tree(&i);
        assert_eq!(
            tree[0].children[0].label,
            "Cannot connect: No such file or directory"
        );
        assert_eq!(tree[0].children[0].icon, Icon::Error);
        let d = details(&i, "docker").unwrap();
        assert!(d.message.unwrap().contains("Cannot connect"));
        assert_eq!(d.actions, [Action::Connect, Action::Refresh]);
    }

    #[test]
    fn details_and_actions() {
        let i = input();
        let d = details(&i, "docker/container/c-app").unwrap();
        assert_eq!(
            d.tabs,
            [Tab::Dashboard, Tab::Log, Tab::Console, Tab::Inspection]
        );
        assert_eq!(
            d.actions,
            [
                Action::Stop,
                Action::Restart,
                Action::Remove,
                Action::Logs,
                Action::Exec,
                Action::Pull
            ]
        );
        let ports = d
            .sections
            .iter()
            .find(|s| s.title == "Port Bindings")
            .unwrap();
        assert_eq!(ports.rows[0].url.as_deref(), Some("http://localhost:8000"));
        let d = details(&i, "docker/container/c-mysql").unwrap();
        assert_eq!(d.actions[0], Action::Start);
        let d = details(&i, "compose/acme-shop").unwrap();
        let table = d.sections[1].table.as_ref().unwrap();
        assert_eq!(
            table.rows[2],
            ["redis", "redis:7-alpine", "Not created", ""]
        );
        assert_eq!(d.subtitle.as_deref(), Some("1 of 2 containers running"));
        let d = details(&i, "compose/acme-shop/redis").unwrap();
        assert_eq!(d.actions, [Action::Up]);
        let d = details(&i, "docker/volume/acme-shop_db-data").unwrap();
        assert_eq!(
            d.sections[1].table.as_ref().unwrap().rows[0][0],
            "acme-shop-mysql-1"
        );
        let d = details(
            &i,
            &Target::Deployment {
                context: "kind-acme".into(),
                namespace: "shop".into(),
                name: "web".into(),
            }
            .node_id(),
        )
        .unwrap();
        assert_eq!(d.sections[1].table.as_ref().unwrap().rows[0][0], "web-1");
        let d = details(
            &i,
            &Target::KubeNamespace {
                context: "kind-acme".into(),
                namespace: "shop".into(),
            }
            .node_id(),
        )
        .unwrap();
        assert_eq!(d.sections.len(), 3);
    }

    #[test]
    fn pods_with_containers_and_port_forwards() {
        let mut i = input();
        let pod = Target::Pod {
            context: "kind-acme".into(),
            namespace: "shop".into(),
            name: "web-1".into(),
        }
        .node_id();
        i.port_forwards = vec![PortForward {
            id: 1,
            node_id: pod.clone(),
            context: "kind-acme".into(),
            namespace: "shop".into(),
            pod: "web-1".into(),
            address: "127.0.0.1".into(),
            local_port: 8080,
            remote_port: 80,
            connections: 0,
            error: None,
        }];
        let d = details(&i, &pod).unwrap();
        assert_eq!(
            d.tabs,
            [Tab::Dashboard, Tab::Log, Tab::Console, Tab::Inspection]
        );
        assert_eq!(
            d.actions,
            [
                Action::Remove,
                Action::Logs,
                Action::Exec,
                Action::ForwardPorts
            ]
        );
        assert_eq!(
            d.containers,
            [ContainerChoice {
                name: "php".into(),
                init: false,
                default: true
            }]
        );
        assert_eq!(d.port_forwards.len(), 1);
        let tree = build_tree(&i);
        let node = tree[2].find(&pod).unwrap();
        assert_eq!(node.detail.as_deref(), Some("1/1 · Running · ⇄ 8080→80"));
        assert_eq!(details(&i, "kube").unwrap().port_forwards.len(), 1);
        assert_eq!(
            details(&i, "kube/kind-acme").unwrap().port_forwards.len(),
            1
        );
        let dep = Target::Deployment {
            context: "kind-acme".into(),
            namespace: "shop".into(),
            name: "web".into(),
        }
        .node_id();
        assert!(details(&i, &dep).unwrap().tabs.contains(&Tab::Log));
    }

    #[test]
    fn busy_nodes() {
        let mut i = input();
        i.busy = vec!["compose/acme-shop/mysql".into()];
        let tree = build_tree(&i);
        assert_eq!(tree[1].children[0].children[1].status, Some(Status::Busy));
    }
}
