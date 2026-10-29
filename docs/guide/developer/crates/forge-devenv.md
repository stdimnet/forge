---
title: forge-devenv
description: "Forge IDE Dev Containers and WSL: devcontainer.json, docker command lines, WSL distributions and path mapping, and the Forge agent over a process's stdio."
order: 215
tags: [crate, rust]
---

# forge-devenv

`crates/forge-devenv` — Forge IDE Dev Containers and WSL: devcontainer.json, docker command lines, WSL distributions and path mapping, and the Forge agent over a process's stdio.

## Purpose

Dev Containers and WSL projects, like PHPStorm / JetBrains Gateway: the project's
services run in a container or a WSL distribution through the Forge agent
(`crates/forge-remote-agent`), which Forge starts over the stdio of `docker exec` or
`wsl.exe` instead of SSH; the project then opens as a `forge_backend::RemoteBackend`
(`RemoteHost.kind` = `RemoteKind::DevContainer` / `RemoteKind::Wsl`).

Pure modules (also built for WebAssembly without the `runtime` feature):
- `jsonc`: JSON with comments and trailing commas.
- `devcontainer`: `devcontainer.json` locations, parsing and resolving.
- `docker`: Docker CLI command lines and the `docker ps` list of dev containers.
- `wsl`: `wsl.exe -l -v` (UTF-16) parsing, command lines and Windows ↔ Linux paths.

With `runtime` (the default):
- `runner`: processes on this machine behind a trait (fakes in tests).
- `agent`: copying the agent in and starting it over a process's stdio.
- `connection`: a live agent connection that restarts the agent when it ends.
- `up`: building / starting a dev container with a build log.

## Modules

| Module | Summary |
| --- | --- |
| `agent.rs` | The agent over `docker exec -i` / `wsl.exe -d <distro> --exec`: the probe, upload (`cat`) and start commands of `forge_remote::deploy` wrapped by an `AgentTarget` (`DockerTarget`, `WslTarget`), the ready marker skipped, then the usual `RpcClient`. |
| `connection.rs` | `StdioConnection`: the `forge_backend::remote::Connection` of such a project; restarts the agent with backoff (1–30 s) when its process ends and calls the reconnect hooks (`RemoteBackend::reattach`). |
| `devcontainer.rs` | `devcontainer.json` model (`image`, `build`, `dockerComposeFile` + `service`, `workspaceFolder`, `workspaceMount`, `forwardPorts`, lifecycle commands, `remoteUser`, `containerEnv`, `mounts`, `features`, …), `resolve` against the local folder (absolute paths, `${localWorkspaceFolder}` and the other variables, defaults), configuration locations and the gutter line. |
| `docker.rs` | `docker build` / `pull` / `run`, the compose override file and `docker compose up` / `down`, `docker exec` for lifecycle commands, the agent and terminals, the labels (`forge.devcontainer.*`, `devcontainer.local_folder`, `devcontainer.config_file`) and the `docker ps` list. |
| `jsonc.rs` | Comment and trailing-comma removal that keeps positions. |
| `runner.rs` | `Runner` (`output`, `stream`, `spawn_stdio`) and `SystemRunner` (tokio processes, no console window on Windows). |
| `up.rs` | Create or start a dev container: find it by label, pull / build / compose up, `docker run`, `onCreateCommand` / `postCreateCommand` / `postStartCommand`, every step and output line as `LogLine`. |
| `wsl.rs` | UTF-16LE decoding of `wsl.exe` output, `wsl.exe -l -v` parsing (localized headers and states), `--exec` / `--cd` command lines, `C:\…` ↔ `/mnt/c/…` and `\\wsl$\<distro>\…` / `\\wsl.localhost\…` ↔ Linux paths. |

## Used by

- IPC: `devcontainer_configs`, `devcontainer_gutter`, `devcontainer_list`, `devcontainer_up`, `devcontainer_cancel`, `devcontainer_action`, `devcontainer_open`, `wsl_distributions`, `wsl_map_path`, `wsl_windows_path`, `wsl_connect`, `wsl_browse`, `wsl_open_project`, `wsl_disconnect` (`app/src-tauri/src/commands/devenv.rs`)
- UI: `app/ui/src/features/devenv`
- Browser mode: built into `forge-wasm` (`devcontainer_resolve`, `devcontainer_gutter`, `devcontainer_parse_list`, `wsl_parse_list`, `wsl_map_path`, `wsl_windows_path`)

## Dependencies

- Workspace crates: [`forge-backend`](forge-backend.md), [`forge-core`](forge-core.md), [`forge-remote`](forge-remote.md)
- External: `serde`, `serde_json`, `tokio`, `tracing`
- Features: `default` = `runtime`; `runtime` = `dep:tokio`, `dep:tracing`, `dep:forge-backend`, `dep:forge-core`, `dep:forge-remote`

## Tests

Unit tests with a fake `docker` / `wsl.exe` runner (no Docker, WSL or network): devcontainer.json
parsing and resolving, command lines, the `docker ps` and UTF-16 `wsl.exe -l -v` fixtures, path
mapping, the build flow, and the agent copied in and started over an in-memory stdio pipe
serving a `RemoteBackend` that survives an agent restart.

```sh
cargo test -p forge-devenv
```
