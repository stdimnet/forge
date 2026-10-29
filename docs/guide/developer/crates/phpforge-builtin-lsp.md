---
title: phpforge-builtin-lsp
description: "PHPForge: the built-in PHP language server php-lsp (MIT OR Apache-2.0), run by the Forge binary and the remote agent in a hidden `--php-lsp --stdio` mode."
order: 283
tags: [crate, rust, pack, php, lsp, language server, php-lsp]
---

# phpforge-builtin-lsp

`packs/phpforge/builtin-lsp` — PHPForge: the built-in PHP language server
[php-lsp](https://github.com/stdimnet/php-lsp), run by the Forge binary and the remote agent in
a hidden `--php-lsp --stdio` mode.

## Purpose

php-lsp is a pure-Rust PHP language server under MIT OR Apache-2.0, a drop-in for Intelephense
(same CLI flags, `intelephense.*` settings, diagnostics and formatting) with its premium
features free. It is Forge's only PHP language server and is linked into Forge, so users
install nothing. It is a cargo git dependency pinned to one revision
(`rev = "7011d4c1e1b4f27ad8ded67b55ae4fabcd92ef09"`); its embedded JetBrains phpstorm-stubs
(Apache-2.0) and the js-beautify port (MIT) are listed in `THIRD_PARTY_NOTICES.md`.

The server runs in a **separate process** for crash isolation: `forge` (and
`forge-remote-agent`) call `main_hook` first thing in `main`. With `--php-lsp` as the first
argument it runs `php_lsp::run_stdio()` and exits, before Tauri, logging or the agent runtime
start. Otherwise it announces the executable to `phpforge-lang-php`
(`lsp::register_builtin_server`), whose `server_config` then starts `<current exe> --php-lsp
--stdio` through forge-lsp like any other server. Transport, restarts, Safe Mode, remote
projects (the agent runs its own copy on the host) and the Language Servers UI work unchanged.

The in-process `php_lsp::Embedded` was not used: a server bug would take the IDE down, and the
LSP client would need a second, in-memory transport.

## Public items

| Item | Summary |
| --- | --- |
| `FLAG` | `"--php-lsp"`, the first argument of the hidden mode. |
| `VERSION`, `REVISION`, `version_label()` | php-lsp's version and pinned commit, `"0.1.0 (7011d4c)"` (`VERSION` is `php_lsp::VERSION`; a test checks the pinned commit against `Cargo.lock`). |
| `main_hook(args)` | Runs the server in the hidden mode (`--version` prints the version) and returns the exit code; otherwise registers the built-in server and returns `None`. |
| `is_server_mode`, `server_args`, `run_stdio`, `register` | The pieces of `main_hook`. |

## Dependencies

`php-lsp` (git, pinned), `phpforge-lang-php`. It brings `lsp-types` 0.97 and `rowan` 0.17
next to forge-lsp's `lsp-types` 0.95 and tree-sitter-based crates; the server talks to Forge only
through JSON-RPC, so the versions never meet.

## Tests

- Unit tests: the hidden-mode arguments and the pinned version against `Cargo.lock`.
- `tests/server.rs` starts the test host `phpforge-builtin-lsp-host` (`tests/support/host.rs`,
  never shipped) through forge-lsp's `LspClient` with the configuration Forge builds from its
  PHP settings, on a small project in a temporary folder: initialize (capabilities), hover,
  cross-file definition, completion, diagnostics, formatting and rename. No network.
