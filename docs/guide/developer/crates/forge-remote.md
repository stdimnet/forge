---
title: forge-remote
description: "Forge IDE remote development: SSH connections, host keys, ~/.ssh/config and the Forge agent deployment."
order: 210
tags: [crate, rust]
---

# forge-remote

`crates/forge-remote` — Forge IDE remote development: SSH connections, host keys, ~/.ssh/config and the Forge agent deployment.

## Purpose

Remote development over SSH, like JetBrains Gateway: Forge connects to a host, installs
its headless agent (`crates/forge-remote-agent`) there and opens the project through a
`forge_backend::RemoteBackend`, so indexing, Intelephense, git, terminals and runs all
happen next to the files.

Pure modules (also built for WebAssembly without the `client` feature):
- `ssh_config`: `~/.ssh/config` host blocks.
- `known_hosts`: host key verification and fingerprints.
- `deploy`: probing a host and planning the agent upload.
- `config`: saved SSH configurations.

With `client` (the default):
- `session`: SSH connections with russh (password, key file, ssh-agent; known_hosts
  check with a fingerprint confirmation).
- `agent`: installing and starting the agent.
- `connection`: a live connection to an agent with reconnects and latency.
- `store`: saved configurations on disk and their secrets in the keychain.
- `shell`: SSH terminal sessions (`SshShell::open(params, cols, rows, sink)`, a
  `forge_backend::Terminal` on a pseudo terminal of an SSH channel).

## Modules

| Module | Summary |
| --- | --- |
| `agent.rs` | Installing and starting the Forge agent over an SSH session. |
| `config.rs` | Saved SSH configurations (Settings \| Tools \| SSH Configurations), stored in `<config dir>/forge/ssh.toml`. Passwords and key passphrases never go there: they live in the OS keychain. `parse_config` ("Parse config file ~/.ssh/config", on by default) decides whether `~/.ssh/config` resolves the host. |
| `connection.rs` | A live connection to the Forge agent on a host: connects (SSH + agent start), measures latency, notices a dead connection and reconnects with backoff. |
| `deploy.rs` | Installing the Forge agent on a remote host: probe the host with one shell command, decide whether the agent must be uploaded, upload it through `cat` (no SFTP needed) and start it. |
| `known_hosts.rs` | OpenSSH `known_hosts`: which host keys are trusted. |
| `session.rs` | SSH sessions with russh: host key verification against `known_hosts` (unknown and changed keys need the user's confirmation of the fingerprint), password, key file and ssh-agent authentication, command execution and streams, jump hosts (`ProxyJump`, one session tunnelled through the previous one) and `ProxyCommand`, port forwarding (`direct-tcpip` channels and remote `tcpip-forward` listeners) and forwarded Unix sockets (the Docker API over SSH). |
| `shell.rs` | SSH terminal sessions (Tools \| Start SSH Session…): `SshSession::open_shell` (`pty-req` for `xterm-256color` + `shell`), output as UTF-8 to a `forge_process::PtySink`, input, window changes, the exit status and closing. |
| `ssh_config.rs` | OpenSSH client configuration (`~/.ssh/config`): host blocks with patterns and the keywords Forge uses to connect. Like `ssh`, the first value obtained for a keyword wins and `IdentityFile` accumulates. |
| `store.rs` | Saved SSH configurations on disk (`<config dir>/forge/ssh.toml`), their passwords and passphrases in the OS keychain (service `forge-ssh`, account = configuration id), and the user's OpenSSH files (`~/.ssh/config`, `~/.ssh/known_hosts`). |

## Main public items

```rust
pub use config::{AuthMethod, SshConfiguration, SshConfigurations};
pub use known_hosts::HostKeyStatus;
```

## Used by

- IPC: `remote_ssh_configs`, `remote_ssh_config_save`, `remote_validate_config`, `remote_ssh_config_delete`, `remote_ssh_hosts`, `remote_test_connection`, `remote_connect`, `remote_browse`, `remote_open_project`, `remote_disconnect`, `remote_status`, `remote_reconnect` (`app/src-tauri/src/commands/remote.rs`)
- IPC: `terminal_open({ options: { ssh: { configId? | serverId?, secret?, confirmed? } } })` (`app/src-tauri/src/commands/terminal.rs`)
- [`forge-db`](forge-db.md): SSH tunnels of data sources (`forge_db::ssh`)
- UI: `app/ui/src/features/remote`, `app/ui/src/features/terminal/ssh.tsx`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`forge-backend`](forge-backend.md), [`forge-core`](forge-core.md), [`forge-process`](forge-process.md)
- External: `base64`, `dirs`, `flate2`, `hmac`, `keyring`, `russh`, `serde`, `serde_json`, `sha1`, `sha2`, `tokio`, `toml`, `tracing`
- Features: `default` = `client`; `client` = `dep:forge-backend`, `dep:forge-core`, `dep:forge-process`, `dep:russh`, `dep:tokio`, `dep:tracing`, `dep:dirs`, `dep:keyring`, `dep:toml`, `dep:flate2`

## Tests

10 source files; unit tests and integration tests in `tests/`: `ssh.rs` (agent, reconnects, port forwarding, jump hosts) and `ssh_shell.rs` (SSH terminal sessions against an in-process server: pty request, UTF-8 output split across packets, input, window changes, exit codes, closing, wrong passwords).

```sh
cargo test -p forge-remote
```
