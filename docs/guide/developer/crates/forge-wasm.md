---
title: forge-wasm
description: "WebAssembly build of Forge's pure Rust logic for the browser-mode UI."
order: 260
tags: [crate, rust]
---

# forge-wasm

`crates/forge-wasm` — WebAssembly build of Forge's pure Rust logic for the browser-mode UI.

## Purpose

WebAssembly bridge for the browser-mode UI.

Forge's rule is "Rust computes, TypeScript renders". In the desktop app the UI calls Rust
through Tauri IPC; in browser mode (`pnpm dev`, screenshots, tests) the same Rust code runs
as WebAssembly so mocks never re-implement logic in TypeScript.

Only **pure** crates belong here (no file system, processes, network or tokio). Each area
adds a module that wraps its crate with `#[wasm_bindgen]` functions taking and returning
JSON strings, mirroring the matching IPC command:

```rust
#[wasm_bindgen]
pub fn keymap_resolve(request_json: &str) -> String { … }
```

Rebuild the bundle with `pnpm --dir app/ui wasm` (output: `app/ui/src/wasm/pkg`).

## Modules

| Module | Summary |
| --- | --- |
| `agents.rs` | Agent CLIs (`forge_agent_cli`) for the browser-mode agent mock: the built-in agents' profiles (what `agent_list` reports besides detection) and the stream parsers, so the mock replays recorded CLI output through the same Rust code as the desktop app. |
| `containers.rs` | Services tool window (`forge-containers`) for the browser-mode mock: the mock keeps a fake Docker daemon as Engine API JSON, compose files and a kubeconfig as text, and asks Rust for the models, the tree, the details pane, the gutter markers, command lines and pull progress, like the desktop app's hub. |
| `filetypes.rs` | File type detection (`forge-filetypes`), mirroring the `filetypes_detect` IPC command. The desktop app builds the registry from its plugin manifests; browser mode loads the same manifests (the `builtinPlugins.json` snapshot) once with `filetypes_load`. |
| `keymap.rs` | Keymap bridge (`forge-keymap`). |
| `lua.rs` | LuaForge (`luaforge-lang-lua`), mirroring the `lua_*` IPC commands for the browser-mode mock: the mock hands over the demo project's Lua files once (`lua_index_files`), then asks for structure, hover, Go to Declaration, symbol search and run markers. |
| `parsers.rs` | Parsers and models of process output, SQL, DQL, Markdown and the AI chat, mirroring the IPC commands of the desktop app for the browser-mode mocks. |
| `parsers/chat.rs` | The AI chat conversation (`forge_agent_cli::conversation`) for the browser-mode agent: the mock feeds its scripted events into a `Chat` and emits the `chat` updates like the desktop backend. |
| `parsers/dql.rs` | DQL in PHP strings (`phpforge-dql`) for the browser-mode Doctrine backend. Sites are found by the lexical finder (the desktop app uses tree-sitter); the entity snapshot comes from the mock's demo mappings. |
| `parsers/markdown.rs` | Markdown (`forge-markdown`) for browser mode, mirroring `markdown_parse` / `markdown_hover`. |
| `parsers/output.rs` | Run output (`forge-output`) for the browser-mode run backend and terminal: the mock feeds process events into a `RunOutput` and emits the rendered events like the desktop app. |
| `parsers/sql.rs` | SQL text (`forge-sql`) for the browser-mode database backend and console. |
| `remote.rs` | Remote development (`forge-remote` without its SSH client): `~/.ssh/config` hosts, SSH configuration validation and `known_hosts` checks, mirroring `remote_ssh_hosts`, `remote_validate_config` and the host key step of `remote_connect`. |
| `scratches.rs` | Scratch files (`forge-scratches`), mirroring the pure parts of the `scratches_*` IPC commands for the browser-mode mock: the language list, naming, gutter markers, run command lines and the `.http` parser, request resolution (variables, dynamic variables, body files), response handlers and rendering. |
| `search.rs` | Fuzzy matching and Search Everywhere ranking (`forge-fuzzy`), mirroring the `fuzzy_filter`, `search_rank` and `fs_find_files` IPC commands. |
| `settings.rs` | The Settings dialog model (`forge_plugin_api::settings_model`), mirroring the `settings_search` and `settings_validate` IPC commands. |
| `web.rs` | WebForge (`packs/webforge/*`) for the browser-mode mock: npm packages and scripts, package manager detection, and React/Vue/Angular component navigation over the demo project's files, mirroring the `npm_*` and `web_*` IPC commands. |
| `window.rs` | Window chrome (`forge_window`), mirroring the `window_chrome` IPC command for browser mode. |

## Main public items

```rust
pub fn bridge_version() -> String { … }
```

## Used by

- IPC: none: it mirrors IPC commands as WebAssembly functions for browser mode
- UI: `app/ui/src/wasm/`, `app/ui/src/mock/`
- Browser mode: is the WebAssembly bridge

## Dependencies

- Workspace crates: [`forge-agent-cli`](forge-agent-cli.md), [`forge-containers`](forge-containers.md), [`forge-filetypes`](forge-filetypes.md), [`forge-find`](forge-find.md), [`forge-fuzzy`](forge-fuzzy.md), [`forge-keymap`](forge-keymap.md), [`forge-markdown`](forge-markdown.md), [`forge-output`](forge-output.md), [`forge-plugin-api`](forge-plugin-api.md), [`forge-remote`](forge-remote.md), [`forge-scratches`](forge-scratches.md), [`forge-sql`](forge-sql.md), [`forge-window`](forge-window.md), [`luaforge-lang-lua`](luaforge-lang-lua.md), [`phpforge-dql`](phpforge-dql.md), [`webforge-fw-angular`](webforge-fw-angular.md), [`webforge-fw-react`](webforge-fw-react.md), [`webforge-fw-vue`](webforge-fw-vue.md), [`webforge-lang-web`](webforge-lang-web.md)
- External: `serde`, `serde_json`, `wasm-bindgen`

## Tests

18 source files, about 2393 lines; 23 unit tests.

```sh
cargo test -p forge-wasm
```
