# Architecture

```
crates/            Rust core libraries (forge-*), language-agnostic
packs/<pack>/      Plugins grouped by pack: phpforge, webforge, luaforge, aiforge
plugins/           Generic plugins (git, database, docker, xdebug, ...)
app/src-tauri/     Desktop shell: Tauri setup + IPC commands
app/ui/            SolidJS UI
```

## Backend (Rust)

- `forge-core` — workspace (sandboxed path resolution), file system, settings, events.
- `forge-plugin-api` — `Plugin` trait, `PluginManifest`, `Contributions`. Packs depend only on this.
- Plugins declare contributions (commands, keybindings, tool windows, typed settings, LSP
  servers, run configuration types, AI providers, chat agents, code generators) as serde data;
  `PluginRegistry` tracks enablement (cascading over `depends`) and answers lookups. Built-in
  manifests are TOML files in `app/src-tauri/plugins/`; `app/ui/src/mock/builtinPlugins.json`
  is their browser-mode snapshot (regenerate with `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`).
- Settings (`forge_core::Settings`) are layered: schema defaults < user
  (`<config dir>/forge/settings.toml`) < project (`<root>/.forge/settings.toml`), keyed by dotted
  names. IPC: `settings_get_all`, `settings_set(key, value, scope)`, `settings_schema`,
  `plugins_list`, `plugin_set_enabled`; changes are emitted as `settings://changed`. In the UI read
  them with `getSetting(key)` from `src/state/settings.ts`.
- Feature crates (`forge-lsp`, `forge-vcs`, `forge-ai`, `forge-agent-cli`, ...) are plain libraries
  with their own tests; `app/src-tauri` wires them to IPC.
- IPC commands live in `app/src-tauri/src/commands/<area>.rs`, are registered in `lib.rs`, and
  return `forge_core::Result<T>` (errors serialize to strings).

## UI (SolidJS)

- `src/ipc/` — `invoke`/`listen` wrappers. In a plain browser, calls go to mock handlers
  registered with `registerMock` (see `src/mock/`), so every feature works without Rust and can
  be screenshotted.
- Contribution points (the UI side of plugins):
  - `registerToolWindow` (`src/features/registry.ts`) — panels on the left/right/bottom stripes.
  - `registerStatusItem` (`src/features/statusItems.ts`) — status bar widgets.
  - `registerCommand` (`src/state/commands.ts`) — actions with PHPStorm-style ids, bindable to keys.
  - `registerSettingsPage` (`src/features/settingsPages.ts`) — pages in the Settings dialog.
  - `registerEditorExtension` (`src/editor/extensions.ts`) — CodeMirror extensions per file.
- A feature is a folder `src/features/<name>/` with an `index.ts` that registers its
  contributions; add one import line to `src/features/index.ts`. Browser mocks go to
  `src/mock/<name>Mock.ts` and are imported from `src/mock/index.ts`.
- Styling uses the CSS variables from `src/theme/theme.css` only (dark + light), icons from
  `lucide-solid`.

## Conventions

- Layout, wording and shortcuts follow PHPStorm; colors are Forge's own.
- Every UI change is checked with `pnpm screenshots`.
- Commits: small, imperative subject lines, no co-author trailers.

## Shared IPC contracts

- `ai_complete({ request: { system?: string, messages: { role: "user" | "assistant", content: string }[], provider?: string, model?: string, maxTokens?: number } }) -> string`
  — one-shot completion with the default (or given) provider. Used by e.g. AI commit messages.
- `ai_stream({ request, streamId })` emits `ai://stream/<streamId>` events `{ delta?: string, done?: boolean, error?: string }`.
