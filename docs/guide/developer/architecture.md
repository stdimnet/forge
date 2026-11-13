---
title: Architecture overview
description: How Forge is put together - the Tauri shell, Rust crates and packs, the SolidJS UI, IPC, project backends, browser mode and plugins.
order: 10
tags: [architecture, tauri, rust, solidjs, ipc]
---

# Architecture overview

This page is the map. [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) is the detailed contract
(every IPC command and event, payload shapes, conventions per area); read it before larger
changes.

```text
┌──────────────────────────── Forge desktop app (one native binary) ───────────────────────────┐
│  SolidJS UI (app/ui) ── WebView ──────────────────────────────────────────────────────────── │
│    features/<name>/  register tool windows, commands, settings pages, editor extensions …    │
│    src/ipc/          typed clients ──invoke/listen──┐                                         │
│                                                      ▼                                        │
│  Tauri shell (app/src-tauri)   commands/<area>.rs  (thin: parse args, call crates, emit)      │
│                                                      │                                        │
│  Rust crates (crates/forge-*)  core, plugin-api, lsp, vcs, keymap, fuzzy, filetypes, ai, …    │
│  Packs (packs/<pack>/<plugin>) phpforge, webforge, luaforge, javaforge                        │
│                                                      │                                        │
│  forge-backend: Backend trait ── LocalBackend (this machine)                                  │
│                              └─ RemoteBackend ──SSH── forge-remote-agent on a Linux host      │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Layers

**The shell** (`app/src-tauri`, crate `forge-app`, binary `forge`) creates the window, holds
the application state, registers IPC commands (`src/commands/<area>.rs`, listed in
`src/lib.rs`) and forwards events. It is kept thin: it parses arguments, calls library crates and
emits events. Built-in plugin manifests are TOML files in `app/src-tauri/plugins/`.

**Rust crates** (`crates/forge-*`) hold the logic: workspace and settings (`forge-core`), the
plugin contract (`forge-plugin-api`), project detection and generators (`forge-project`), LSP
client (`forge-lsp`) and server catalog (`forge-langservers`), git (`forge-vcs`), code hosts
(`forge-forges`), AI providers and agent CLIs (`forge-ai`, `forge-agent-cli`), processes and run
output (`forge-process`, `forge-output`), the debugger (`forge-debug`), databases (`forge-db`,
`forge-sql`), containers (`forge-containers`), the keymap engine (`forge-keymap`), fuzzy search
(`forge-fuzzy`), file types (`forge-filetypes`), remote development (`forge-backend`,
`forge-remote`, `forge-remote-agent`), deployment (`forge-deploy`) and more. Each crate has its own unit tests.

**Packs** (`packs/<pack>/<plugin>`) are language and framework crates: PHPForge (`lang-php`,
`fw-laravel`, `fw-symfony`, `orm-doctrine`, `dql`), WebForge (`lang-web`, `fw-react`,
`fw-vue`, `fw-angular`), LuaForge (`lang-lua`), JavaForge (`build`, `lang-java`, `jdk`). They depend on core crates, never on the app.

**The UI** (`app/ui`, SolidJS + TypeScript + CodeMirror 6, icons from `lucide-solid`) renders
and forwards events. A feature is a folder `src/features/<name>/` whose `index.ts` registers its
contributions (tool windows, commands, settings pages, editor extensions, …); see
[Contribution points](contribution-points.md).

## IPC

The UI calls Rust with `invoke("<command>", args)` and subscribes to events with
`listen("<area>://<event>")`, always through a typed client in `app/ui/src/ipc/<area>.ts`.
Commands return `forge_core::Result<T>`; errors arrive as strings. Long work streams progress
as events (`run://<id>`, `lsp://diagnostics`, `agent://<session>`, …). See
[IPC commands and browser mocks](ipc-and-mocks.md).

## Browser mode

The whole UI also runs in a plain browser (`pnpm dev`): every IPC command has a mock in
`app/ui/src/mock/`, and pure Rust crates run as WebAssembly (`crates/forge-wasm`) so the mocks
do not duplicate logic. Demo projects make every feature explorable and screenshot-able. See
[The WebAssembly bridge](wasm-bridge.md).

## Plugins

Plugins declare contributions as data in TOML manifests: commands, keybindings, tool windows,
typed settings, file types, LSP servers, run configuration types, AI providers, chat agents and
code generators (`forge_plugin_api::Contributions`). `PluginRegistry` tracks enablement
(cascading over `depends`). Plugins with `projectTypes` activate only for projects that use
them. Browser mode reads a snapshot of the manifests, `app/ui/src/mock/builtinPlugins.json`. See
[Writing a plugin or pack](plugins-and-packs.md).

## Settings

Layered settings (defaults < user `settings.toml` < project `.forge/settings.toml`) with a schema
from the manifests; the UI reads them with `getSetting(key)`, Rust with
`read_settings().get_as::<T>(key)`; changes are broadcast as `settings://changed`. Secrets go to
the OS keychain. See [Settings](settings.md).

## Project backends and remote development

Every project command goes through the open project's `forge_backend::Backend`: files, search,
git, watching, terminals, processes, language server launching, the PHP index, Find in Files,
framework analysis (`framework(FrameworkCall)` for Laravel, Symfony and Doctrine), WebForge
analysis (`web(WebCall)`: npm packages, components, navigation, gutter), batched file reads for
language indexes (`collect_files`, the Lua index), catalog language server planning
(`langservers`) and installs (`install_langserver`, `langserver_updates`), executable lookup
(`find_executable`, AI agent CLIs, StyLua) and SSH tunnels (`tunnels()`: database tunnels, the
Xdebug reverse tunnel, HTTP requests sent from the host and the host's Docker socket). Project
`.forge/` files go through `forge_core::ProjectFiles`. `LocalBackend` serves a local folder;
`RemoteBackend` talks to `forge-remote-agent` over SSH with a length-prefixed MessagePack
protocol (calls, events and byte channels; protocol 5). Every feature works in remote
projects; there is no local-only fallback any more.

## Keymap

The keymap engine is Rust (`forge-keymap`): PHPStorm's default keymaps, overrides from
settings, conflicts and a resolver state machine for chords, double taps and focus contexts.
The UI captures key events and runs the resolved action. The resolver runs synchronously as
WebAssembly in both desktop and browser mode because `preventDefault` must be decided before the
handler returns. See [Keymap actions](keymap-actions.md).

## Windows

All windows are undecorated; Forge draws title bars itself, styled per desktop
(`forge-window`). Large dialogs open as separate native windows (`registerDialog` /
`openDialogWindow`); in browser mode they open in-page.

## Conventions

- Layout, wording and shortcuts follow PHPStorm; colors are Forge's own (CSS variables in
  `app/ui/src/theme/theme.css`, dark and light).
- Rust computes, TypeScript renders ([details](rust-computes.md)).
- Shortcuts only through the keymap; settings through the schema; secrets in the keychain; no
  `localStorage` for persistent state.
- Caches in the user cache dir, never in the project; mocks never write to real projects.
