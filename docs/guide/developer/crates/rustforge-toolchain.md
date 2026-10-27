---
title: rustforge-toolchain
description: "RustForge: Rust toolchains on the user's machine - rustup toolchains and the active toolchain, standalone rustc installs, sysroots and std sources, components and cargo plugins, consented rustup installs, the pinned rust-analyzer download with SHA-256 checks, and GDB with rustc's pretty printers as a debug adapter."
order: 301
tags: [crate, rust, pack, toolchain, download, debug]
---

# rustforge-toolchain

`packs/rustforge/toolchain` — RustForge on the user's machine.

## Purpose

- `detect`: `RustStatus` of a project folder — rustup's toolchains, the active one and why,
  a standalone `rustc` without rustup, `rustc -vV`, the sysroot, the standard library sources
  (`rust-src`), installed components, the tools Forge uses (rust-analyzer through
  `rustup which`, Forge's download or the `PATH`; rustfmt; Clippy) and cargo plugins
  (cargo-nextest, cargo-llvm-cov), and rustc's pretty-printer files. Programs run through a
  `Runner` with `RUSTUP_AUTO_INSTALL=0`, so detection never installs anything.
- `install`: command lines for `rustup toolchain install`, `rustup component add` and
  `cargo install --locked` of cargo plugins (names validated; the UI asks first).
- `analyzer`: the rust-analyzer download — the pinned release (`PINNED_RELEASE`), the asset
  of the platform (`rust-analyzer-<target>.gz`, `.zip` on Windows), its SHA-256 digest from
  the GitHub release API, unpacked into `<data dir>/forge/langservers/rust-analyzer`.
- `gdb`: `RustGdbAdapter`, the `rust-gdb` debug adapter: the Debugger plugin's GDB
  (`gdb -i dap`) with rustc's pretty printers loaded before the program.

## Modules

| Module | Summary |
| --- | --- |
| `detect.rs` | `detect`, `RustEnv`, `RustStatus`, `ToolStatus`, `system_runner`. |
| `install.rs` | `toolchain`, `component`, `cargo_plugin`. |
| `analyzer.rs` | `install`, `installed`, `asset_name`, `default_dir`, `PINNED_RELEASE`. |
| `gdb.rs` | `RustGdbAdapter`, `ADAPTER`. |

## Used by

- IPC: `app/src-tauri/src/commands/rust.rs` (`rust_status`, `rust_install_command`,
  `rust_analyzer_download`, `rust_debug_request`, `sync_debug`).

## Dependencies

- Workspace crates: [`rustforge-lang-rust`](rustforge-lang-rust.md),
  [`forge-langservers`](forge-langservers.md), [`forge-dap`](forge-dap.md),
  [`forge-plugin-api`](forge-plugin-api.md)
- External: `serde`, `serde_json`, `dirs`, `tokio`, `flate2`

## Tests

Detection runs against recorded rustup and rustc output in temporary folders; the download
is tested with a local HTTP server (`forge_langservers::managed::testserver`); the GDB
adapter against the Debugger plugin's GDB contribution. An ignored smoke test detects this
machine's toolchains.

```sh
cargo test -p rustforge-toolchain
cargo test -p rustforge-toolchain -- --ignored   # this machine's rustup
```
