---
title: forge-hostspawn
description: "Forge IDE: run host tools from inside a Flatpak sandbox through flatpak-spawn --host."
order: 112
tags: [crate, rust, flatpak]
---

# forge-hostspawn

`crates/forge-hostspawn` — running host tools from inside a Flatpak sandbox. It has no
dependencies, so every crate that starts processes can use it; `forge-process` re-exports it as
`forge_process::host`.

## Purpose

In the Flatpak build, PHP, Composer, Node, Git, Docker, language servers and the user's shell
are installed on the host. Every spawn of such a tool is rewritten to
`flatpak-spawn --host --watch-bus --directory=… --env=… <program> <args>`. Outside Flatpak (or
with the setting `system.flatpak.runOnHost` off) every function is a no-op.

## API

| Item | Summary |
| --- | --- |
| `in_flatpak()`, `detect()` | the sandbox is detected from `/.flatpak-info` or `FLATPAK_ID` |
| `set_enabled()`, `active()` | follow the setting "Run tools on the host (Flatpak)" |
| `Spec`, `rewrite()` | pure: a command line (program, args, env changes, cwd) to its `flatpak-spawn` form |
| `host_command()` | rewrite a configured `std::process::Command`; call it before setting stdio, process groups or Tokio options (convert with `tokio::process::Command::from`) |
| `pty_spec()`, `PTY_WRAPPER` | a terminal shell wrapped with `setsid -c` so the pseudo terminal becomes its controlling terminal on the host |
| `is_sandbox_program()` | Forge's own executable and `/app/…` stay in the sandbox |
| `path_var()`, `env_var()`, `host_env()` | the host's `PATH` and environment (a login shell's, read once) |
| `which()`, `is_file()`, `is_dir()`, `list_dir()`, `metadata()`, `canonicalize()` | look at host paths; system folders are read through `/run/host` (`--filesystem=host-os:ro`) |
| `sandbox_view()`, `sandbox_path_var()`, `host_path()` | translate between host paths and what the sandbox sees |

Details and limits: [Release builds](../release-builds.md#running-host-tools-from-the-sandbox).

## Tests

Unit tests cover detection, the rewriting (arguments, `--env`, `--unset-env`, `--directory`,
`PATH` cleanup, `/run/host` translation) and the terminal wrapper.
`crates/forge-process/tests/flatpak_spawn.rs` runs a real run configuration and a terminal
through a fake `flatpak-spawn` on `PATH` (`tests/fixtures/flatpak-spawn`).
