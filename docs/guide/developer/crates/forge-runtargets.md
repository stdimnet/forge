---
title: forge-runtargets
description: "Run targets (Run on: local machine, Docker image, Docker Compose service, SSH, WSL, Dev Container): the target model, automatic and user path mappings, and the command lines that run, test and debug there with port forwarding; pure, also built to WebAssembly."
order: 221
tags: [crate, rust, run targets, docker, ssh, wsl, dev container, path mappings]
---

# forge-runtargets

`crates/forge-runtargets` — run targets ("Run on") and path mappings (pure, also built to
WebAssembly).

## Purpose

What [Run targets](../../user/run-targets.md) compute: where a run executes and the command
line that starts it there from this machine. The PHP CLI interpreters
([`phpforge-interpreters`](../../user/php.md#cli-interpreters)) build their Docker, Compose,
SSH and WSL command lines with it, and Dev Container projects ([`forge-devenv`](forge-devenv.md))
share its `docker exec` command line.

## Modules

| Module | Summary |
| --- | --- |
| `mapping.rs` | `PathMapping` (local ↔ remote), `Mapper` (most specific pair wins, both directions, `map_arg` for `--opt=/path` arguments), `normalize`, `join`, `is_absolute`, Docker Compose bind mounts (`parse_volume`, `compose_mappings`) and WSL paths (`wsl_path`). |
| `target.rs` | `RunTarget` (`kind`: `local`, `dockerImage`, `composeService`, `ssh`, `wsl`, `devContainer`, plus image, run options, Docker context, compose files, service, lifecycle, host, port, user, key file, distribution, container, project path, mappings, environment), `automatic` / `effective` mappings, `Invocation` (program, unmapped program arguments, arguments, working directory, environment, published ports, `DebugPort`), `wrap` (the local command line), `debug_target` (`DebugTarget`: IDE host, target host, mappings for debuggers), `docker_exec_args`, `Launcher` / `run_on` (starting through a launcher; fakes in tests). |

## Main public items

```rust
pub struct RunTarget { pub kind: TargetKind, … }
pub fn wrap(target: &RunTarget, inv: &Invocation, ctx: &TargetContext) -> Result<Wrapped, String>;
pub fn context(target: &RunTarget, root: &str, compose: Option<(&str, &[String])>, docker: &str) -> TargetContext;
pub fn debug_target(target: &RunTarget, mappings: &[PathMapping]) -> DebugTarget;
pub enum DebugDirection { ConnectsToIde, IdeConnects }
```

## Used by

- `forge-process` (`RunConfiguration.run_on`), `phpforge-interpreters` (`wrap`, `mapping`),
  `forge-devenv` (`docker exec`)
- IPC: `run_start` / `run_command_line` (configurations with `runOn`), `run_target_info`
  (`app/src-tauri/src/commands/run.rs`, `commands/testing.rs`)
- UI: `app/ui/src/features/run/RunOn.tsx` (the Run on row of the Run/Debug Configurations dialog)
- Browser mode: built into `forge-wasm` (`run_target_wrap`)

## Dependencies

- Workspace crates: [`forge-runtools`](forge-runtools.md) (argument splitting)
- External: `serde`

## Tests

Command lines of every target with a fake launcher (Docker image mounts and options, Compose
exec, SSH quoting with `-L` / `-R`, WSL drive paths, Dev Container exec), path mappings and the
serialized form.

```sh
cargo test -p forge-runtargets
```
