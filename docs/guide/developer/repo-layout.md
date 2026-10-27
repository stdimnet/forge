---
title: Repository layout
description: Where code, manifests, UI features, mocks, scripts, docs and skills live in the Forge repository.
order: 20
tags: [repository, layout]
---

# Repository layout

```text
Cargo.toml                 workspace: crates/*, packs/*/*, app/src-tauri
rust-toolchain.toml        pinned stable Rust with rustfmt and clippy
crates/forge-*             Rust libraries (one page each under developer/crates/)
packs/<pack>/<plugin>      language and framework crates, package name <pack>-<plugin>
  phpforge/                lang-php, fw-laravel, fw-symfony, orm-doctrine, dql
  webforge/                lang-web, fw-react, fw-vue, fw-angular
  luaforge/                lang-lua
  javaforge/               build, lang-java, jdk
  kotlinforge/             lang-kotlin
app/
  src-tauri/               the Tauri shell (crate forge-app, binary forge)
    src/lib.rs             builder, state, event wiring, generate_handler![…]
    src/commands/<area>.rs IPC commands per area
    src/companion.rs       launcher arguments, forge:// links, Browser Companion server
    plugins/*.toml         built-in plugin manifests
    capabilities/          Tauri permissions (window labels)
    tauri.conf.json        app config (identifier dev.forge.ide, deep-link scheme forge)
    tauri.agents.conf.json bundles target/agents/ as the agents/ resource folder
  ui/                      SolidJS UI (package @forge/ui)
    src/App.tsx            Welcome screen or workbench
    src/shell/             workbench frame: toolbar, stripes, tool window panels, editor area,
                           status bar, Welcome screen, window chrome and dialog windows
    src/features/<name>/   UI features, each with an index.ts that registers contributions
    src/editor/            CodeMirror setup, languages, file type cache, template overlay,
                           line markers, theme
    src/keymap/            key capture and dispatch (engine: crates/forge-keymap)
    src/ipc/               typed clients for IPC commands
    src/mock/              browser-mode backend: mocks, demo projects, manifest snapshot
    src/state/             global state: workspace, editor tabs, layout, settings, commands
    src/theme/theme.css    CSS variables for both themes
    src/wasm/              loader and generated bindings of crates/forge-wasm (pkg/ committed)
    scripts/               screenshots.mjs, scenarios/*.mjs, build-wasm.mjs, build-agents.mjs
browser-extension/         Forge Companion (Chrome/Firefox, Manifest V3)
docs/
  ARCHITECTURE.md          contribution points and IPC contracts
  ROADMAP.md               milestones and planned branches
  screenshots/             PNGs produced by the scenarios
  guide/                   this guide
.claude/skills/<name>/     step-by-step recipes (also Claude Code skills)
.github/workflows/ci.yml   CI, manual dispatch only
AGENTS.md, CLAUDE.md       rules for coding agents and humans
CONTRIBUTING.md            contributor quick start
```

## Naming

- Rust crates: `forge-<area>` in `crates/`, `<pack>-<plugin>` in `packs/` (e.g.
  `phpforge-fw-laravel`).
- Plugin ids: `forge.<name>` (`forge.laravel`, `forge.web`).
- IPC commands: `<area>_<verb>` (`laravel_routes`, `db_execute`); events `<area>://<event>`.
- Action ids: PHPStorm's (`GotoClass`) or `<Plugin>.<Action>` (`Laravel.RunArtisan`).
- Settings keys: dotted (`php.debug.port`); settings page ids like `lang.php.laravel`.
