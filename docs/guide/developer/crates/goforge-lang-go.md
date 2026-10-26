---
title: goforge-lang-go
description: "GoForge: Go language support - go.mod / go.work parsing and completion contexts, module versions, test / benchmark / fuzz / example discovery with subtests, the Go Test provider, Go Build command lines, go:generate, go vet and golangci-lint reports, go env; pure, also built to WebAssembly."
order: 296
tags: [crate, rust, pack, go]
---

# goforge-lang-go

`packs/goforge/lang-go` — GoForge: everything about Go that needs no I/O. The desktop
backend and the WebAssembly build of browser mode share it.

## Purpose

- `scan`: a tolerant Go tokenizer (identifiers, decoded string literals, punctuation, lines).
- `gomod`: `go.mod` (`module`, `go`, `toolchain`, `godebug`, `require`, `replace`, `exclude`,
  `retract`, `tool`, `ignore`, single lines and blocks, `// indirect`) and `go.work` (`go`,
  `toolchain`, `use`, `replace`) with positions (UTF-16 columns), problems and the completion
  context at an offset (directive, module path, version of a module, Go version).
- `version`: module versions (semver, pseudo-versions, `+incompatible`), ordering, the latest
  release, proxy `@v/list` answers and module path escaping (`!a` for `A`).
- `discovery`: `TestXxx`, `BenchmarkXxx`, `FuzzXxx` and `ExampleXxx` functions and literal
  subtests / sub-benchmarks (`t.Run("name", func(t *testing.T) {…})`, any depth) with
  `go test`'s names.
- `provider`: `GoTestProvider`, the `forge-testing` provider of the manifest's `go-test`
  framework: gutter targets keyed `<package folder>::<name>`, `go test -json` command lines per
  scope (all, directory, file, test, pattern) and kind (tests, benchmarks, fuzzing), coverage
  (`-coverprofile`, Go coverprofile), profiles (`-cpuprofile`, …), Rerun Failed Tests, `-C`
  for modules in sub folders and location rules from import paths to project paths;
  `debug_test` gives Delve the test binary flags of a configuration.
- `build`: Go Build configurations (`go run` / `go build -o`, package / directory / files,
  tags, Go tool arguments) and module commands (`go mod tidy|download|verify`,
  `go work sync`, `go get`, `go install`).
- `generate`: `//go:generate` directives and `go generate -run '^<directive>$' file.go`.
- `lint`: `go vet -json` and golangci-lint JSON (v1 `--out-format=json`, v2
  `--output.json.path=stdout`) as `GoProblem`s of project files.
- `env`: `GoContext` (SDK, GOPATH, GOFLAGS, build tags, environment, modules), handed to the
  provider through `CommandContext.settings["go.context"]`, and `go env -json`.

## Modules

| Module | Summary |
| --- | --- |
| `scan.rs` | `tokens`, `matching`, `utf16_len`. |
| `gomod.rs` | `parse_go_mod`, `parse_go_work`, `completion_context`, `GO_MOD_DIRECTIVES`. |
| `version.rs` | `is_valid_version`, `is_pseudo`, `compare`, `latest`, `escape_path`, `parse_version_list`. |
| `discovery.rs` | `discover`, `GoTest`, `GoTestKind`, `subtest_name`. |
| `provider.rs` | `GoTestProvider`, `register`, `run_pattern`, `debug_test`. |
| `build.rs` | `build_command`, `build_target`, `module_command`, `GoCommand`. |
| `generate.rs` | `directives`, `generate_args`. |
| `lint.rs` | `parse_vet`, `parse_golangci`, `golangci_args`, `golangci_major`. |
| `env.rs` | `GoContext`, `ModuleRoot`, `parse_go_env`, `CONTEXT_KEY`. |

## Main public items

```rust
pub use build::{build_command, build_target, module_command, GoCommand, GO_BUILD};
pub use discovery::{discover, GoTest, GoTestFile, GoTestKind};
pub use env::{parse_go_env, GoContext, GoEnv, ModuleRoot, CONTEXT_KEY};
pub use gomod::{parse_go_mod, parse_go_work, GoMod, GoWork, ModCompletion};
pub use lint::{parse_golangci, parse_vet, GoProblem};
pub use provider::{debug_test, DebugTest, GoTestProvider, GO_TEST};
pub fn modules_of<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<ModuleRoot>;
```

## Used by

- IPC: `app/src-tauri/src/commands/go.rs` (`go_*`), `commands/run.rs` (the Go Test provider
  in `RunState.tests`, Go Build command lines).
- Browser mode: `crates/forge-wasm/src/go.rs` and `testing.rs`.
- [goforge-delve](goforge-delve.md), [goforge-sdk](goforge-sdk.md).

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md),
  [`forge-testing`](forge-testing.md)
- External: `serde`, `serde_json`

## Tests

Unit tests per module; `tests/fixtures/` holds a real `go vet -json` report and a real
golangci-lint 2.5 JSON report.

```sh
cargo test -p goforge-lang-go
```
