---
name: add-plugin-or-pack
description: Create a new Forge plugin or language/framework pack (e.g. LuaForge, WebForge, a Laravel or Docker plugin) including manifest, Rust crate, settings and UI feature. Use when adding a new integration that users can enable/disable in Settings > Plugins.
---

# Add a plugin or pack

A **pack** groups plugins (`phpforge`, `webforge`, `luaforge`, `core`; AI plugins live in `core`). A **plugin**
is a manifest plus optional Rust crate plus optional UI feature.

## 1. Manifest

Add `app/src-tauri/plugins/<plugin>.toml` (or a `[[plugin]]` entry in an existing file):

```toml
id = "forge.lua"
name = "Lua"
version = "0.1.0"
pack = "luaforge"
vendor = "Forge"
description = "Lua language support."
depends = ["forge.core"]

[contributions]
languages = ["lua"]

[[contributions.lspServers]]
id = "lua-language-server"
languageId = "lua"
command = "lua-language-server"
fileExtensions = ["lua"]

[[contributions.settings]]
key = "lua.runtimeVersion"
title = "Lua version"
type = "enum"
options = [
  { value = "5.1", label = "Lua 5.1" },
  { value = "5.4", label = "Lua 5.4" },
  { value = "luajit", label = "LuaJIT" },
]
default = "5.4"
group = "lang.lua"   # settings page id, see the add-setting skill
```

Contribution types are defined in `crates/forge-plugin-api/src/contributions.rs`: commands,
keybindings, tool windows, settings, LSP servers, run configuration types, AI providers, chat
agents and code generators.

Then regenerate the browser snapshot and run the consistency test:

```sh
FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app
```

## 2. Rust crate (if the plugin has logic)

- Create `packs/<pack>/<plugin>/` (package name `<pack>-<plugin>`) or `crates/forge-<name>`.
- Add the path to `members` and `[workspace.dependencies]` in the root `Cargo.toml`, then depend
  on it from `app/src-tauri/Cargo.toml`.
- Depend on `forge-core` / `forge-plugin-api` only, never on `forge-app`.
- Unit-test with fixture projects; tests must not need network or system services.
- Expose it to the UI with the `add-ipc-command` skill.

## 3. UI feature

`app/ui/src/features/<plugin>/index.ts` registers tool windows, commands, settings pages, status
items, editor extensions and search providers. Activate only when the project uses it, e.g.
check `composer.json` for the Laravel plugin, and only when the plugin is enabled. Read enablement
from `plugins_list`.

## 4. Docs

- Add the pack or plugin to the README table and to `docs/ROADMAP.md`.
- Add new IPC contracts to `docs/ARCHITECTURE.md`.
