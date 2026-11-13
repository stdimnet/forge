---
title: forge-ports
description: "Free local ports for servers Forge starts (debug adapters, JDWP, debugpy, Jupyter kernels), probed without listening and outside the ephemeral range."
order: 185
tags: [crate, rust]
---

# forge-ports

`crates/forge-ports` — Free local ports for servers Forge starts (debug adapters, JDWP, debugpy, Jupyter kernels), probed without listening and outside the ephemeral range.

## Purpose

The one place that picks a port for a server Forge starts and then connects to. Two rules
keep Forge from talking to another program on that port:

- Ports come from `20000..32768`, below the ephemeral ranges (Linux 32768-60999, macOS and
  Windows 49152-65535), which the system hands to every program that binds port 0 or
  connects out. On Linux the actual range is read from `/proc` and skipped.
- A port is checked by binding it, never by listening on it. A process spawned at the same
  moment inherits a copy of the probe socket until it executes its program; a listening copy
  would keep accepting connections after the probe closed, a bound copy refuses them and does
  not keep the server from binding (address reuse on Unix; not on Windows, where the option
  lets a socket take over a port in use).

Callers that connect to a server they started still watch for the server exiting before they
connected (`forge-dap`'s connect loop, the Jupyter kernel start).

## Main public items

```rust
pub const PORTS: Range<u16>;                                    // 20000..32768
pub fn free_port() -> io::Result<u16>;
pub fn free_ports(n: usize) -> io::Result<Vec<u16>>;            // n distinct ports
pub fn probe(port: u16) -> io::Result<u16>;                     // bind-only check
pub fn ephemeral_ports() -> Option<RangeInclusive<u16>>;        // Linux
```

## Used by

- Crates: `forge-dap` (`${port}` of adapters Forge starts), `pyforge-jupyter` (feature
  `runtime`: the five kernel ports); as a dev-dependency in tests that need a port nobody
  listens on
- App: `app/src-tauri/src/commands/java.rs` (JDWP, also Kotlin), `python_tools.rs` (debugpy)

## Dependencies

- External: `socket2`

## Tests

Distinct ports outside the ephemeral range, ports in use are not free, and a copy of a probe
socket neither accepts connections nor blocks the server.

```sh
cargo test -p forge-ports
```
