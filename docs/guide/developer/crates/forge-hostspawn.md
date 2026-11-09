---
title: forge-hostspawn
description: "Forge IDE: run host tools from inside a Flatpak sandbox through flatpak-spawn --host."
order: 112
tags: [crate, rust, flatpak]
---

# forge-hostspawn

`crates/forge-hostspawn` — running host tools from inside a Flatpak sandbox, and the one place
where Forge prepares every child process for Windows. It has no dependencies, so every crate
that starts processes can use it; `forge-process` re-exports it as `forge_process::host`.

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
| `host_command()` | prepare a configured `std::process::Command` for spawning: the Flatpak rewrite, then `local_command()`. Call it before setting stdio, creation flags, process groups or Tokio options (convert with `tokio::process::Command::from`) |
| `local_command()` | the Windows part only, for processes that stay in Forge's environment (`taskkill`, `tasklist`, `reg`, `wsl.exe`): `PATH` + `PATHEXT` lookup, verbatim `cmd /C` lines, `CREATE_NO_WINDOW`; a no-op elsewhere |
| `win::resolve_program()`, `win::pathext()` | pure: find `composer` as `composer.bat`, `npm` as `npm.cmd` (injectable `PATH`, `PATHEXT` and file check) |
| `win::cmd_invocation()` | pure: the verbatim `cmd.exe` argument string `/D /V:OFF /S /C "<line>"` for `["/C", line]` |
| `win::cmd_line()`, `win::cmd_arg()`, `win::cmd_escape()` | pure: build a `cmd /C` line whose arguments stay literal (quotes, spaces, `&`, `%VAR%`) |
| `win::is_cmd()` | `cmd`, `cmd.exe` or a path to it |
| `pty_spec()`, `PTY_WRAPPER` | a terminal shell wrapped with `setsid -c` so the pseudo terminal becomes its controlling terminal on the host |
| `is_sandbox_program()` | Forge's own executable and `/app/…` stay in the sandbox |
| `path_var()`, `env_var()`, `host_env()` | the host's `PATH` and environment (a login shell's, read once) |
| `which()`, `is_file()`, `is_dir()`, `list_dir()`, `metadata()`, `canonicalize()` | look at host paths; system folders are read through `/run/host` (`--filesystem=host-os:ro`) |
| `sandbox_view()`, `sandbox_path_var()`, `host_path()` | translate between host paths and what the sandbox sees |

Details and limits: [Release builds](../release-builds.md#running-host-tools-from-the-sandbox).

## Windows

Every spawn goes through `host_command()` (or `local_command()`), never a bare
`Command::new(…).spawn()`:

- `Command::new` only tries `<name>.exe`. Bare names and extension-less paths are resolved with
  the child's `PATH` and `PATHEXT` first; the standard library then starts `.bat`/`.cmd` files
  through `cmd.exe` with its own safe quoting.
- `cmd.exe` does not parse its command line like C programs. Pass a line as plain arguments,
  `.args(["/C", line])` (options before `/C` are kept); the helper turns it into one verbatim
  argument (`raw_arg`). Never call `raw_arg` yourself before the helper: it rebuilds the
  command and would quote the argument again. To build a line from separate arguments, use
  `win::cmd_line()`: arguments are quoted and `%` is written as `%%cd:~,%` (an empty substring
  of `%cd%`), so `%VAR%` stays literal, as the standard library does for batch files.
- Every child gets `CREATE_NO_WINDOW`: Forge is a GUI application, so console programs would
  otherwise open a console window each.

## Tests

Unit tests cover detection, the rewriting (arguments, `--env`, `--unset-env`, `--directory`,
`PATH` cleanup, `/run/host` translation) and the terminal wrapper. The `win` module's tests
(lookup with a fake file system, quoting, `cmd.exe` invocations) run on every platform;
`windows_tests` start real processes on Windows (a `.bat` on `PATH`, quoted `cmd /C` lines, a
program in a folder with a space); they also pass under Wine 9.
`crates/forge-process/tests/flatpak_spawn.rs` runs a real run configuration and a terminal
through a fake `flatpak-spawn` on `PATH` (`tests/fixtures/flatpak-spawn`).
