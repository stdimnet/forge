---
title: rubyforge-sdk
description: "RubyForge: Ruby on the user's machine - interpreters of rbenv, rvm, chruby, asdf, mise, frum, rv, Homebrew, RubyInstaller and the system with their gem executables, the project's choice, the environment of Ruby commands, consented ruby-lsp installs with the user's gem into Forge's data folder, and rdbg as the `rdbg` debug adapter."
order: 303
tags: [crate, ruby, pack, interpreters, version managers, ruby-lsp, debug]
---

# rubyforge-sdk

`packs/rubyforge/sdk` — RubyForge on the user's machine.

## Purpose

- `detect`: `RubyStatus` of a project — the interpreters of version managers (rbenv with
  `RBENV_ROOT`, rvm with its gem sets, chruby, asdf with `ASDF_DATA_DIR`, mise with
  `MISE_DATA_DIR`, frum, rv) read from their folders, and of Homebrew, RubyInstaller and the
  `PATH` (shims skipped, symlinks resolved) asked for their version with the interpreter only
  (`ruby -e …`, through a `Runner`); `choose` (the setting, the requested version, the
  manager's global version, the newest); gem executables (`gem`, `bundle`, `rake`, `rspec`,
  `rubocop`, `rdbg`, `irb`, `ruby-lsp`) in the interpreter's `bin`, the user gem folder, rvm's
  gem sets, `GEM_HOME`, Forge's ruby-lsp install and the `PATH`; `command_env` (the
  interpreter first on `PATH`, rvm's `GEM_HOME` / `GEM_PATH`).
- `lsp`: Ruby LSP installs — `gem install ruby-lsp --install-dir <data dir>/forge/langservers/ruby/<version>`
  with the user's own RubyGems (nothing bundled or downloaded by Forge itself), the installed
  executable and its launch environment (`GEM_PATH`).
- `rdbg`: `RdbgAdapter`, the `rdbg` debug adapter: attaches to a `rdbg --open` DAP server
  (`localfs`, `nonstop` unless Stop on entry) and asks `forge-dap` to send breakpoints after
  `attach` (`breakpoints_after_request`), because rdbg maps breakpoint paths only then.

## Modules

| Module | Summary |
| --- | --- |
| `detect.rs` | `detect`, `interpreters`, `choose`, `global_version`, `find_tool`, `command_env`, `parse_probe`, `RubyEnv`, `RubyStatus`, `Interpreter`, `system_runner`. |
| `lsp.rs` | `install_command`, `install_dir`, `installed`, `launch_env`, `is_managed`, `default_dir`. |
| `rdbg.rs` | `RdbgAdapter`, `attach_arguments`, `ADAPTER`. |

## Used by

- IPC: `app/src-tauri/src/commands/ruby.rs` (`ruby_status`, `ruby_lsp_install_command`,
  `configure_lsp`, `sync_debug`, `wrap_rdbg`).

## Dependencies

- Workspace crates: [`rubyforge-lang-ruby`](rubyforge-lang-ruby.md), [`forge-dap`](forge-dap.md)
- External: `serde`, `serde_json`, `dirs`

## Tests

Detection runs against fake manager folders in temporary directories with a recorded probe;
ignored smoke tests detect this machine's Rubies and debug a script with a real rdbg through
`forge-dap` (breakpoint, stack, variables).

```sh
cargo test -p rubyforge-sdk
cargo test -p rubyforge-sdk -- --ignored   # this machine's Ruby and rdbg
```
