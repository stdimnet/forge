---
title: goforge-delve
description: "GoForge: the Go debugger - Delve (dlv dap, MIT) as the go debug adapter of the shared debugger: launch and test debugging, attach to a local process, connect to a remote dlv --headless server, core dumps."
order: 298
tags: [crate, rust, pack, go, debug, dap]
---

# goforge-delve

`packs/goforge/delve` — GoForge's debugger on [forge-dap](forge-dap.md).

## Purpose

- `DelveAdapter` implements `DebugAdapterDescriptor` for the adapter type `go`: each session
  starts `dlv dap --listen=127.0.0.1:<port>` in the package's folder (Delve runs `go build`
  there) with the project's environment and the SDK's `bin` first on the `PATH`, and connects
  to it. The built program goes to `<temp>/forge-delve`, never into the project.
- Requests: `launch` with `mode: debug` (Go Build), `mode: test` (Go Test), `mode: core`
  (core dumps); `attach` with `mode: local` (Attach to Process) and `mode: remote` (a running
  `dlv --headless --accept-multiclient` server, Go Remote configurations, `substitutePath`).
- `request(cfg, env, go, root)` builds the `LaunchRequest` of a `go-build`, `go-test` or
  `go-remote` configuration (program, arguments, working directory, environment,
  `buildFlags`, test binary flags from `goforge_lang_go::debug_test`).

## Used by

- `app/src-tauri/src/commands/go.rs`: `sync_debug` registers the adapter while `forge.go` is
  enabled (called by `commands/dap.rs`), `go_debug_request`, `go_delve_status`.
- UI: the `delve` debug launcher in `app/ui/src/features/go/index.tsx`.

## Dependencies

- Workspace crates: [`forge-dap`](forge-dap.md), [`forge-runtools`](forge-runtools.md),
  [`goforge-lang-go`](goforge-lang-go.md)
- External: `serde`, `serde_json`

## Tests

Unit tests for every mode. `tests/delve_smoke.rs` (ignored) debugs a small Go program with a
real Delve and Go toolchain: breakpoint, stack, variables.

```sh
cargo test -p goforge-delve
FORGE_DLV=$(go env GOPATH)/bin/dlv cargo test -p goforge-delve -- --ignored
```
