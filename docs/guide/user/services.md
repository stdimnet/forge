---
title: Services
description: The Services tool window - Docker containers, images, volumes and networks, Docker Compose projects, Kubernetes workloads, logs, exec terminals, image pulls, registries and gutter run icons.
order: 190
tags: [docker, compose, kubernetes, services, containers]
---

# Services

The plugin **Docker and Kubernetes** (`forge.services`) fills the **Services** tool window
(`Alt+8`, macOS `Cmd+8`). It talks to the Docker Engine API directly (Unix socket, Windows
named pipe or TCP with TLS) and to the Kubernetes API with your kubeconfig. The docker CLI is
only used for `docker compose`, image builds and interactive terminals; `kubectl` only for pod
terminals.

![Services](../../screenshots/services-docker-dashboard.png)

The tree has three roots:

| Root | Children |
| --- | --- |
| **Docker** | Containers, Images, Networks, Volumes |
| **Docker Compose** | compose projects found in the project root and one folder below, with their services and containers |
| **Kubernetes** | contexts → namespaces → Pods, Deployments, Services |

The status bar shows the Docker connection ("Docker 27.3.1", "Docker: connecting…",
"Docker: not connected"). Forge follows Docker's event stream, so the tree updates by itself,
and retries the connection every 10 seconds.

## Details pane

Selecting a node shows its details on the right, in tabs:

| Tab | Content |
| --- | --- |
| Dashboard | status, image, ports, mounts, networks, environment, labels; tables for compose and Kubernetes objects |
| Log | the log, following new lines; starts with the last `docker.logs.tail` lines (500) |
| Attached Console | output of compose Up/Down, builds and pulls |
| Inspection | the raw JSON (`docker inspect`) |

![Logs](../../screenshots/services-docker-logs.png)

## Actions

The toolbar and the context menu offer what applies to the selected node:

| Node | Actions |
| --- | --- |
| Docker (not connected) | Connect, Refresh |
| Running container | Stop, Restart, Delete, Show Log, Exec (open a terminal in the container), Pull Image |
| Stopped container | Start, Delete, Show Log, Pull Image |
| Image | Pull, Delete |
| Images group | Pull Image…, Refresh |
| Volume, network | Delete |
| Compose project | Up (Run all services), Stop, Restart, Down (`docker compose down`), Show Log |
| Compose service | Up, Stop, Restart, Down, Show Log, Exec |
| Pod | Delete, Show Log, Exec (`kubectl exec`) |
| Deployment | Restart Rollout, Show Log |

**Exec** opens a terminal tab in the Terminal tool window running `docker exec -it` (or
`kubectl exec -it`) with bash when the container has it, else sh. Containers started with a
TTY also offer an attached console (`docker attach`).

![Exec terminal](../../screenshots/services-exec-terminal.png)

![Compose](../../screenshots/services-compose.png)

![Kubernetes pod log](../../screenshots/services-kubernetes-pod-log.png)

## Compose files and Dockerfiles in the editor

- `compose.yaml` / `docker-compose.yml` get a ▶ in the gutter on `services:` (**Run all
  services**) and on each service (`docker compose up` for that service).
- A `Dockerfile` gets a ▶ on its first `FROM`: **Run on 'Docker'** builds the image and runs a
  container; output appears in Attached Console.

![Compose gutter](../../screenshots/services-compose-gutter.png)

Compose files support profiles, env files, `${VAR}` interpolation from the project's `.env`
and override files.

## Pulling images and registries

**Pull Image…** (Find Action **Docker.PullImage**, or the Images group) asks for
`repository[:tag]`, e.g. `redis:7-alpine`, and shows the progress.

![Pull image](../../screenshots/services-pull-image.png)

Private registries: Settings | Build, Execution, Deployment | Docker | **Docker Registry**.
Add the registry (`docker.io`, `ghcr.io`, …), user name and password or access token. The
password goes to the OS keychain (service `forge-containers`); the setting
`docker.registries` only keeps `server<TAB>user`. Without accounts, public images are pulled
anonymously.

## Settings

### Build, Execution, Deployment | Docker

![Docker settings](../../screenshots/settings-docker.png)

| Option | Setting | Default |
| --- | --- | --- |
| Connect to Docker daemon with (`auto`, `unix`, `pipe`, `tcp`) | `docker.connection` | `auto` |
| Socket path (Unix socket or named pipe; empty: the first existing default) | `docker.socketPath` | empty |
| Engine API URL (TCP) | `docker.tcp.url` | `tcp://localhost:2375` |
| Use TLS (verify the daemon certificate) | `docker.tcp.tls` | off |
| Certificates folder (`ca.pem`, `cert.pem`, `key.pem`) | `docker.tcp.certPath` | empty |
| Docker executable (compose, builds, exec) | `docker.cliPath` | `docker` |
| Show the last log lines | `docker.logs.tail` | 500 |

With `auto`, Forge uses `DOCKER_HOST` when set, else the platform default socket
(`/var/run/docker.sock`, Docker Desktop's `~/.docker/run/docker.sock`, rootless
`$XDG_RUNTIME_DIR/docker.sock` or Podman's socket; `\\.\pipe\docker_engine` on Windows).
`ssh://` Docker hosts are not supported yet. **Test Connection** checks
the settings.

### Build, Execution, Deployment | Kubernetes

![Kubernetes settings](../../screenshots/settings-kubernetes.png)

| Option | Setting | Default |
| --- | --- | --- |
| Kubeconfig file (empty: `$KUBECONFIG` or `~/.kube/config`) | `kubernetes.kubeconfig` | empty |
| Default context (empty: the kubeconfig's current-context) | `kubernetes.defaultContext` | empty |
| Default namespace (empty: the context's namespace, else `default`) | `kubernetes.defaultNamespace` | empty |
| kubectl executable | `kubernetes.kubectlPath` | `kubectl` |

Kubernetes authentication supports CA certificates, bearer tokens, client certificates, basic
auth and exec credential plugins (as used by EKS, GKE and AKS).

> **Note:** port forwarding is not available yet (it is being built). The Services tool window
> always talks to the Docker daemon and cluster reachable from this machine, also when a
> remote project is open.
