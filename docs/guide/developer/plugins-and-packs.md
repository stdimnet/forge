---
title: Writing a plugin or pack
description: A walkthrough for adding a Forge plugin - manifest, Rust crate, IPC, UI feature, settings, activation per project, browser mock, tests and docs.
order: 80
tags: [plugins, packs, manifest, walkthrough]
---

# Writing a plugin or pack

A **pack** groups plugins (`core`, `phpforge`, `webforge`, `luaforge`, `javaforge`; AI plugins live in
`core`). A **plugin** is a manifest, plus an optional Rust crate, plus an optional UI feature.
Recipe: [`add-plugin-or-pack`](../../../.claude/skills/add-plugin-or-pack/SKILL.md).

This page is about plugins built into Forge. Plugins that users install themselves (a
`.forgeplugin` file or the Marketplace) use the [Plugin SDK](plugins/index.md) instead.

This walkthrough builds a small plugin, "Makefile targets": a tool window that lists the targets
of the project's `Makefile` and runs them.

## 1. Manifest

Create `app/src-tauri/plugins/make.toml` (or add a `[[plugin]]` table to an existing file):

```toml
id = "forge.make"
name = "Make"
version = "0.1.0"
pack = "core"
vendor = "Forge"
description = "Makefile targets: list and run them."
depends = ["forge.run"]
projectTypes = ["make"]          # activate only for projects that use it

[[contributions.commands]]
id = "make.reload"
title = "Reload Makefile Targets"
category = "Tools"

[[contributions.toolWindows]]
id = "make"
title = "Make"
anchor = "left"
icon = "hammer"
order = 40

[[contributions.settings]]
key = "make.executable"
title = "make executable"
type = "path"
default = "make"
group = "tools.make"             # settings page id
order = 0
```

Contribution types (`crates/forge-plugin-api/src/contributions.rs`): `languages`, `fileTypes`,
`commands`, `keybindings`, `toolWindows`, `settings`, `lspServers`, `runConfigurationTypes`,
`debugAdapters` (see [Contribution points](contribution-points.md#debug-adapters)),
`aiProviders`, `chatAgents`, `codeGenerators`, `mainMenu` (see [Contribution points](contribution-points.md#main-menu)). A run configuration type with `fields` and a
`command` template works without UI code (see `lua-script` in `luaforge.toml`).

Project types are ids from `forge-project` detection (languages, frameworks, packs, tools).
`make` is not detected yet, so you would add it to `crates/forge-project/src/detect.rs` (a
`Makefile` in a scanned folder → `c.tool("make")`) with a test. Without `projectTypes` a plugin is
always active.

Regenerate the browser snapshot and run the manifest tests:

```sh
FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app
```

## 2. Rust crate

If the plugin has logic (parsing the Makefile), create a crate:

- `crates/forge-make` (or `packs/<pack>/<plugin>`, package name `<pack>-<plugin>`);
- add it to `members` and `[workspace.dependencies]` in the root `Cargo.toml` and depend on it
  from `app/src-tauri/Cargo.toml`;
- depend only on `forge-core` / `forge-plugin-api` (and other library crates), never on
  `forge-app`;
- keep the parser pure so it can run as WebAssembly;
- unit-test with fixture projects; no network, no installed tools.

```rust
/// Targets of a Makefile with their 0-based line.
pub fn targets(makefile: &str) -> Vec<Target> { … }
```

## 3. IPC

Add `make_targets` to `app/src-tauri/src/commands/make.rs`, read the file through the project's
backend so remote projects work, register it in `lib.rs`, and write the typed client
`app/ui/src/ipc/make.ts`. See [IPC commands and browser mocks](ipc-and-mocks.md).

## 4. UI feature

`app/ui/src/features/make/index.tsx`:

```tsx
import { Hammer } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { isPluginActive } from "@/state/project";

const active = () => isPluginActive("forge.make");

registerToolWindow({ id: "make", title: "Make", icon: Hammer, anchor: "left", order: 40,
  component: MakeView, when: active });
registerCommand({ id: "make.reload", title: "Reload Makefile Targets", category: "Tools",
  run: () => void reload() });
registerSettingsPage({ id: "tools.make", title: "Make", group: "Tools",
  component: () => <SchemaPage pageId="tools.make" /> });
```

Add `import "./make";` to `app/ui/src/features/index.ts`. Running a target can reuse the Run tool
window with a temporary `shell` configuration.

UI rules: PHPStorm layout and wording, CSS variables from `src/theme/theme.css` only (both
themes), icons from `lucide-solid`, shortcuts only through the keymap.

## 5. Browser mock

Expose `targets` through `crates/forge-wasm` and call it from `app/ui/src/mock/makeMock.ts` on a
demo `Makefile` (add one to the demo project). The feature must work in `pnpm dev`.

## 6. Check and document

- Tests: `cargo test -p forge-make`, vitest for UI logic.
- A screenshot scenario in `app/ui/scripts/scenarios/` ([Screenshots](screenshots.md)); look
  at the images in both themes.
- [Quality gates](quality-gates.md).
- Docs: add the plugin to the README table and `docs/ROADMAP.md`, new IPC contracts to
  `docs/ARCHITECTURE.md`, and a user page under `docs/guide/user/` (plus the reference tables
  if you added settings, actions or file types).

## Packs

A new pack is a new `pack` value in manifests plus a folder `packs/<pack>/`. Packs appear as
groups in Settings | Plugins, and `forge-project` maps languages to packs (`php` → `phpforge`)
in `pack_for_language` in `detect.rs`.
