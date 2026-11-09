---
title: Settings
description: Declaring settings in plugin manifests, reading them in Rust and the UI, settings pages, project scope, validation and secrets.
order: 100
tags: [settings, manifest, schema]
---

# Settings

Recipe: [`add-setting`](../../../.claude/skills/add-setting/SKILL.md). User documentation:
[Settings and themes](../user/settings-and-themes.md); every key:
[settings reference](../reference/settings.md).

## Model

`forge_core::Settings` merges four layers: schema defaults < user
(`<config dir>/forge/settings.toml`) < Settings for New Projects
(`<config dir>/forge/new-project-settings.toml`, scope `newProjects`, only settings declared
with `project = true`) < project (`<project>/.forge/settings.toml`). Settings without
`project = true` are **user-only**: `forge_plugin_api::apply_setting_specs` registers the
schema's defaults and marks them (`Settings::set_user_only`), `resolve`, `get*` and `effective`
skip their project values, and `set` refuses to store them in the project layer. A project's
file comes with its (possibly untrusted) checkout, so it never chooses IDE settings such as
`vcs.git.path` or `updates.endpoint`. Keys are
dotted (`editor.font.size`) and stored as nested TOML tables. Values are JSON so they cross IPC
unchanged. Writing a value persists its layer and notifies subscribers with the new effective
value (`SettingsChange { key, value, scope }`), which the shell forwards as `settings://changed`.

IPC: `settings_get_all` (effective values and the `user`, `newProjects` and `project` layers),
`settings_set(key, value, scope)`, `settings_schema`, `settings_search`, `settings_validate`,
`plugins_list`, `plugin_set_enabled`.

In the UI, `scopeFor(key, forNewProjects?)` picks the layer: project-level settings go to the
open project, or to `newProjects` when no project is open or the dialog edits the Settings for
New Projects (`openSettingsForNewProjects()`, `forNewProjects()` in `dialogState.ts`).
`saveSettings` removes an override instead of storing a value equal to what the layers below
give (`inheritedValue`). Every schema page gets **Restore Defaults** (`restorePageDefaults`)
next to **Reset** (discard the page's draft).

## Declaring a setting

```toml
[[contributions.settings]]
key = "projectView.openFilesWithSingleClick"
title = "Open files with single click"
description = "Shown under the option."
type = "bool"            # bool | int | number | string | enum | path | list
default = true
group = "appearance"     # settings page id
section = "Project View" # optional heading on the page
order = 40
project = true           # optional: may be stored per project (otherwise user-only)
# int/number: min = 10, max = 20 (number: step)
# enum: options = [{ value = "auto", label = "Platform default" }, …]
```

Regenerate the snapshot: `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`.

## Pages

`group` is a page id. Pages generated from the schema are listed in `schemaPages` in
`app/ui/src/features/settings/index.tsx`; features register their own pages with
`registerSettingsPage` and usually render `<SchemaPage pageId="…" />` (optionally with extra
content, see the Docker page's Test Connection). Custom pages (tables, test buttons) are plain
components that use `draftValue`/`setDraft` from `features/settings/dialogState.ts` so OK,
Cancel and Apply work. Pages whose edits are not plain settings keep them in their own store
and register it with `registerDraftSource({ modified, hasChanges, apply, discard, resetPage })`
(the Code Style schemes do), so the tree marks, Reset, Apply, OK and Cancel include them. Open
a page programmatically with `openSettings("<page id>")`.

The dialog's search, highlighting and validation are computed in Rust
(`forge_plugin_api::settings_model`, `settings_validate`).

## Reading settings

- UI: `getSetting<boolean>("projectView.openFilesWithSingleClick", false)` from
  `src/state/settings.ts`; reactive in effects and JSX. `saveSetting(key, value, scope)` writes.
- Rust: `state.read_settings().get_as::<bool>("…")`; subscribe with `Settings::subscribe`
  (listeners run under the settings lock — do work on another thread, as `lib.rs` does).

## Rules

- Secrets (API keys, tokens, passwords, licence keys) never go into settings: store them with the
  `keyring` crate (see `forge-ai`, `forge-db`, `forge-forges`, `forge-langservers::licence`).
- Do not keep persistent state in `localStorage`; use settings.
- A declared setting should be applied; if it is not yet, the user docs must say so.
