---
title: rustforge-lang-rust
description: "RustForge: Rust and Cargo support - cargo metadata workspaces, rustup and rust-toolchain.toml parsing, run / test / doctest discovery, the Cargo Test provider, Cargo command lines, Clippy messages with fixes, rustfmt and rust-analyzer settings, debugger pretty printers (pure, also built to WebAssembly)."
order: 300
tags: [crate, rust, pack, cargo, testing]
---

# rustforge-lang-rust

`packs/rustforge/lang-rust` — RustForge's Rust and Cargo support. Pure Rust (no I/O), shared
by the desktop backend and the WebAssembly build of browser mode.

## Purpose

- `metadata`: a `CargoWorkspace` from `cargo metadata --format-version 1`: members, targets
  (lib, bin, test, bench, example, build script, proc macro), features with their defaults,
  direct dependencies and a dependency tree per member from the `resolve` graph (packages
  seen before are `repeated`, like `cargo tree`'s `(*)`); `module_path` / `crate_of` give the
  crate and module of a source file.
- `toolchain`: `rustup toolchain list -v`, `rustup show active-toolchain` (default,
  `rust-toolchain.toml`, directory override, `RUSTUP_TOOLCHAIN`), `rust-toolchain(.toml)`,
  `rustc -vV` and `rustup component list --installed`.
- `scan`: a small Rust lexer (strings, raw strings, chars, lifetimes, nested comments) and
  brace tracking: `fn main`, `#[test]` / `#[<runtime>::test]`, `#[bench]`, test modules and
  doctests named like rustdoc (`cart::Cart::total`, the line of the fence).
- `context`: `CargoContext` (cargo, toolchain, nightly, test runner and format, installed
  plugins, environment, features, workspace members and files), handed to the provider
  through `CommandContext.settings["rust.context"]`; `location_rules` map libtest's
  `<crate>::<module>` locations to files, deepest modules first.
- `command`: the "Cargo" run configuration (any cargo command with package, target,
  features, profile, target triple and toolchain, the user's own options winning), Cargo
  tool window actions and `fn main` run targets.
- `provider`: `CargoTestProvider` for the manifest's `cargo-test` framework: discovery keyed
  `<file>::<libtest name>`, `cargo test` (libtest JSON on nightly, plain output otherwise),
  `cargo bench`, `cargo test --doc`, `cargo nextest run --message-format libtest-json`,
  `cargo llvm-cov --lcov` for coverage, Rerun Failed Tests by exact names; `debug_test`
  builds a test binary for the debugger (`cargo test --no-run --message-format json`).
- `messages`: cargo's JSON messages — executables (`compiler-artifact`), rustc and Clippy
  diagnostics as `RustProblem`s with their suggestions as fixes, build failures.
- `analyzer`: rust-analyzer's configuration (check on save with Clippy, features, target,
  build scripts, proc macros; off in untrusted projects).
- `format`: rustfmt arguments (edition, `--config-path`) and the config files `cargo fmt`
  would find.
- `debug`: rustc's pretty printers for LLDB (`initCommands`) and GDB (adapter arguments).

## Modules

| Module | Summary |
| --- | --- |
| `metadata.rs` | `parse_metadata`, `CargoWorkspace`, `CargoPackage`, `CargoTarget`, `DependencyNode`, `module_path`, `crate_of`. |
| `toolchain.rs` | `parse_toolchain_list`, `parse_active_toolchain`, `parse_toolchain_file`, `parse_rustc_version`, `parse_components`. |
| `scan.rs` | `scan`, `RustItem`, `ItemKind`. |
| `context.rs` | `CargoContext`, `CONTEXT_KEY`, `location_rules`. |
| `command.rs` | `command_line`, `action_command`, `selection_args`, `run_targets`, `CARGO`. |
| `provider.rs` | `CargoTestProvider`, `register`, `discover_file`, `locate`, `debug_test`, `CARGO_TEST`. |
| `messages.rs` | `artifacts`, `pick_executable`, `problems`, `build_failure`, `utf16_column`. |
| `analyzer.rs` | `AnalyzerSettings`. |
| `format.rs` | `config_candidates`, `rustfmt_args`. |
| `debug.rs` | `PrettyPrinters`. |

## Used by

- IPC: `app/src-tauri/src/commands/rust.rs` (`rust_*`, `cargo_*`), `commands/run.rs` (the
  Cargo Test provider in `RunState.tests`, Cargo command lines), `commands/testing.rs`
  (discovery of `.rs` files with the workspace).
- Browser mode: `crates/forge-wasm/src/rust.rs` and `testing.rs`.
- [rustforge-toolchain](rustforge-toolchain.md).

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md),
  [`forge-testing`](forge-testing.md)
- External: `serde`, `serde_json`, `toml`

## Tests

Unit tests per module against recorded fixtures in `tests/fixtures/`: `cargo metadata` of
the `shop` demo workspace (`tests/fixtures/shop`, also browser mode's demo) and of Forge
itself (`--no-deps`), `cargo clippy` / `cargo build` / `cargo test --no-run` JSON messages,
rustup and rustc output.

```sh
cargo test -p rustforge-lang-rust
```
