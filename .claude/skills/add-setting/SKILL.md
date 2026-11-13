---
name: add-setting
description: Add a user/project setting to Forge and show it in the Settings dialog (Ctrl+Alt+S). Use when a feature needs a configurable option, a new settings page, or when reading settings in Rust or the UI.
---

# Add a setting

Settings are layered: schema defaults < user (`<config dir>/forge/settings.toml`) < Settings for
New Projects (`new-project-settings.toml`, project-level settings only) < project
(`<project>/.forge/settings.toml`). Keys are dotted (`editor.fontSize`).

## 1. Declare it in the schema

In the owning plugin manifest (`app/src-tauri/plugins/<plugin>.toml`):

```toml
[[contributions.settings]]
key = "projectView.openFilesWithSingleClick"
title = "Open files with single click"
description = "Shown under the option."
type = "bool"          # bool | int | number | string | enum | path | list | objects
default = false
group = "appearance"   # id of the settings page that shows it
section = "Project View"  # optional heading inside the page
order = 40
```

`objects` is a list of tables with string values (optional `unique = ["a", "b"]` rejects
duplicate entries); its page shows the editor registered with `registerObjectSettingEditor`
(see `templates.delimiters`).

Enums list their options as `{ value, label }` tables (see `appearance.theme` in
`app/src-tauri/plugins/core.toml`).

Regenerate the browser snapshot: `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`.

`group` is a **page id**. Existing schema pages are listed in `schemaPages` in
`app/ui/src/features/settings/index.tsx` (`appearance`, `system`, `editor.general`,
`editor.font`, `editor.code-style.php`, `editor.inlay-hints`, `vcs.git`, `lang.php`,
`lang.php.composer`, `tools.terminal`). For a new page, render the schema with `SchemaPage`:

```ts
registerSettingsPage({
  id: "lang.lua", title: "Lua", group: "Languages & Frameworks",
  component: () => <SchemaPage pageId="lang.lua" />,
});
```

Search, Apply/Cancel and "Reset" work without extra code.

## 2. Read it

- UI: `getSetting<boolean>("projectView.openFilesWithSingleClick", false)` from
  `app/ui/src/state/settings.ts`. The value is reactive, so a Solid effect or JSX re-runs on change.
- Rust: `state.read_settings().get_as::<bool>("…")`; subscribe for changes via
  `Settings::subscribe` (see `app/src-tauri/src/lib.rs`).

## 3. Custom pages

When a page needs more than a schema form (lists, test buttons, tables), register a component:

```ts
registerSettingsPage({
  id: "tools.ai-providers",
  title: "AI Providers",
  group: "Tools",
  order: 10,
  keywords: ["LLM", "API key"],
  component: AiProvidersPage,
});
```

Open a page programmatically with `openSettings("<page id>")`
(`app/ui/src/features/settings/dialogState.ts`).

## Rules

- The manifest's `title`, `description`, `section` and option labels are the English source
  text. Their German translations go into `crates/forge-i18n/locales/de/plugins.json` under the
  derived keys (`setting.<key>.title`, `.description`, `.option.<value>`,
  `setting.section.<page>.<slug>`); regenerate the English file with
  `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`. A custom page uses `t()` from `@/i18n`
  for its own text and registers its title as `t("settings.page.<id>")`.

- Secrets such as API keys, tokens and passwords never go into settings files. Store them in the
  OS keychain with the `keyring` crate, as `crates/forge-ai` does.
- Do not keep state in `localStorage` that should persist; use settings.
