---
title: forge-containers
description: "Forge IDE containers: Docker Engine API, Docker Compose and Kubernetes for the Services tool window."
order: 40
tags: [crate, rust]
---

# forge-containers

`crates/forge-containers` — Forge IDE containers: Docker Engine API, Docker Compose and Kubernetes for the Services tool window.

## Purpose

Containers for the Services tool window: Docker, Docker Compose and Kubernetes.

Pure parts (no I/O, built for WebAssembly too):
- `docker`: Docker Engine API JSON (containers, images, volumes, networks) into models.
- `compose`: `compose.yaml` / `docker-compose.yml` parsing, project names, command lines
  and the mapping of containers to services by their labels.
- `kube`: kubeconfig parsing and Kubernetes API objects (pods, deployments, services).
- `logs`: the multiplexed Docker log stream and Kubernetes log lines.
- `pull`: image pull progress from the Engine's JSON messages.
- `endpoint`: where the Docker daemon is (`DOCKER_HOST`, settings, platform defaults).
- `tree`: the Services tree, details (Dashboard / Inspection) and toolbar state.
- `gutter`: ▶ markers for compose services and Dockerfiles.

With the `runtime` feature: an HTTP/1.1 client over Unix sockets, Windows named pipes and
TCP (+TLS) (`http`), the Docker Engine client (`engine`), the Kubernetes API client
(`kubeclient`), compose / CLI processes (`cli`) and registry credentials in the OS
keychain (`secrets`), and the `hub` that holds the Services state for the app.

## Modules

| Module | Summary |
| --- | --- |
| `cli.rs` | `docker compose …`, `docker build` / `docker run` (Dockerfile ▶) and other CLI runs with their output streamed line by line. Compose has no Engine API, so these are processes; everything else talks to the Engine directly (`crate::engine`). |
| `compose.rs` | Docker Compose files (`compose.yaml`, `docker-compose.yml`, overrides): services with their images, builds, ports, profiles and env files, the project name, `${VAR}` interpolation from the project's `.env`, and the `docker compose` command lines. |
| `docker.rs` | Docker Engine API objects (`GET /containers/json`, `/images/json`, `/volumes`, `/networks`, `/version`) as models for the tree. The raw JSON is parsed leniently: missing fields get defaults so older and newer Engine versions both work. |
| `endpoint.rs` | Where the Docker daemon listens: Settings \| Build, Execution, Deployment \| Docker (`docker.connection`, `docker.socketPath`, `docker.tcp.*`), else `DOCKER_HOST`, else the platform default (named pipe on Windows; `/var/run/docker.sock`, Docker Desktop's `~/.docker/run/docker.sock`, rootless `$XDG_RUNTIME_DIR/docker.sock` or Podman elsewhere). |
| `engine.rs` | Docker Engine API client (no CLI): snapshot of containers, images, volumes and networks, container lifecycle, inspect, log streaming, image pulls with progress and the event stream that tells the tree to refresh. |
| `exec.rs` | Interactive command lines that run in a terminal PTY: a shell inside a container (`docker exec -it`), attaching to a container's console (`docker attach`) and a shell in a pod (`kubectl exec -it`). |
| `gutter.rs` | ▶ gutter markers: on `services:` (up all) and every service of a compose file (up one), and on the first `FROM` of a Dockerfile (build the image and run a container), plus the command lines those markers run. |
| `http.rs` | A small HTTP/1.1 client for the Docker Engine API and the Kubernetes API server: one connection per request over a Unix socket, a Windows named pipe or TCP (optionally TLS), `Content-Length`, chunked and read-to-close bodies, and streaming bodies for logs, events and pulls. |
| `hub.rs` | The Services backend: one `Hub` per app holds the Docker connection (with a watcher that follows the Engine's events and reconnects), the project's compose files, the kubeconfig with lazily loaded namespaces and workloads, running log streams and the consoles (Log tabs). |
| `kube.rs` | Kubernetes: kubeconfig files (clusters, users, contexts) and the API objects the tree shows (namespaces, pods, deployments, services). |
| `kubeclient.rs` | Kubernetes API client for one kubeconfig context: TLS with the cluster's CA (rustls), bearer tokens, client certificates, basic auth and exec credential plugins (`gke-gcloud-auth-plugin`, `aws eks get-token`, …). |
| `logs.rs` | Container log streams. |
| `pull.rs` | Image pulls: `POST /images/create?fromImage=…` answers with one JSON message per line (`{"status":"Downloading","id":"a1b2","progressDetail":{"current":1,"total":9}}`). |
| `secrets.rs` | Docker registry credentials in the OS keychain (service `forge-containers`, account `registry:<server>`). The list of servers and user names is not secret and is passed in by the caller (settings); only passwords and tokens go to the keychain. |
| `time.rs` | Timestamps: RFC 3339 parsing (Kubernetes, volumes) and the "5 minutes ago" texts. |
| `tree.rs` | The Services tool window model: the tree (Docker → Containers / Images / Networks / Volumes, Docker Compose → projects → services → containers, Kubernetes → contexts → namespaces → Pods / Deployments / Services), the details pane (Dashboard rows, available tabs) and which toolbar actions apply to a node. |

## Main public items

```rust
pub struct Error(pub String);
```

## Used by

- IPC: `services_tree`, `services_refresh`, `services_compose_changed`, `services_expand`, `services_details`, `services_action`, `services_inspect`, `services_console`, `services_logs_start`, `services_logs_stop`, `services_terminal`, `services_pull`, `services_compose_up`, `services_dockerfile_run`, `services_gutter`, `services_status`, `docker_test_connection`, `docker_registry_login`, `docker_registry_logout`, `docker_registry_status` (`app/src-tauri/src/commands/services.rs`)
- UI: `app/ui/src/features/services`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md)
- External: `base64`, `dirs`, `keyring`, `rustls`, `serde`, `serde_json`, `tokio`, `tokio-rustls`, `tracing`, `webpki-roots`, `yaml-rust2`
- Features: `default` = `runtime`; `runtime` = `dep:tokio`, `dep:rustls`, `dep:tokio-rustls`, `dep:webpki-roots`, `dep:keyring`, `dep:dirs`, `dep:tracing`

## Tests

17 source files, about 8968 lines; 50 unit tests.

```sh
cargo test -p forge-containers
```
