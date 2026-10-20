# Architecture

```
crates/            Rust core libraries (forge-*), language-agnostic
packs/<pack>/      Plugins grouped by pack: phpforge, webforge, luaforge
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
  - `registerSearchProvider` (`src/features/search/providers.ts`) — Search Everywhere tabs
    (Classes, Files, Symbols, Actions, Text); language packs plug in class/symbol indexes.
  - `registerKeymapActions` (`src/keymap/store.ts`) — plugin actions with default shortcuts.
- Keymap (`src/keymap/`): PHPStorm's default keymaps (Windows/Linux and macOS) as data, keyed by
  PHPStorm action ids that double as command ids. Implementing an action is just
  `registerCommand({ id: "GotoClass", ... })`; unimplemented actions show "not available yet".
- A feature is a folder `src/features/<name>/` with an `index.ts` that registers its
  contributions; add one import line to `src/features/index.ts`. Browser mocks go to
  `src/mock/<name>Mock.ts` and are imported from `src/mock/index.ts`.
- Styling uses the CSS variables from `src/theme/theme.css` only (dark + light), icons from
  `lucide-solid`.

## Conventions

- Layout, wording and shortcuts follow PHPStorm; colors are Forge's own.
- Every UI change is checked with `pnpm screenshots` (see `.claude/skills/verify-ui-with-screenshots`).
- Project tree: single click selects, double click opens (setting `projectView.openFilesWithSingleClick`).
- Commits: small, imperative subject lines, no co-author trailers.

## Shared IPC contracts

- `fs_find_files({ query, limit? }) -> { path, name, score, positions, matchedPath }[]` — fuzzy
  "Go to File" (camel humps, respects `.gitignore`).

- `ai_complete({ request: { system?: string, messages: { role: "user" | "assistant", content: string }[], provider?: string, model?: string, maxTokens?: number } }) -> string`
  — one-shot completion with the default (or given) provider. Used by e.g. AI commit messages.
- `ai_stream({ request, streamId })` emits `ai://stream/<streamId>` events `{ delta?: string, done?: boolean, error?: string }`.
  Subscribe before invoking; `ai_cancel({ streamId })` stops it (a final `{ done: true }` follows).
  Requests may also carry `temperature?`, `stop?: string[]` and `feature?: "chat" | "commitMessages" | "completion"`
  (picks the model configured for that feature). Use the typed client `src/ipc/ai.ts` (`aiApi`).
- Provider management (Settings | Tools | AI Providers): `ai_providers_list`, `ai_provider_save({ config, apiKey? })`,
  `ai_provider_delete({ id })`, `ai_provider_test({ id }) -> { ok, latencyMs, error?, modelCount? }`,
  `ai_models({ providerId })`, `ai_set_default({ providerId, model?, feature? })`. Config lives in
  `<config dir>/forge/ai.toml`; API keys in the OS keychain (service `forge-ai`) with
  `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `OPENROUTER_API_KEY` as fallback. Backend: `crates/forge-ai`.
- Agent CLIs (`forge-agent-cli`, AI Chat tool window): `agent_list({ executables? }) -> AgentInfo[]`,
  `agent_start({ agentId, options: { model?, permissionMode?, resume?, allowedTools?, executable? } }) -> sessionId`,
  `agent_send({ sessionId, text, context?: { files, selection? } })`, `agent_interrupt({ sessionId })`,
  `agent_stop({ sessionId })`. Events arrive on `agent://<sessionId>` as `AgentEvent` JSON
  (`{ type: "sessionStarted" | "assistantText" | "thinking" | "toolUse" | "toolResult" | "usage" |
  "permissionRequest" | "turnFinished" | "interrupted" | "error" | "exited", ... }`, see `app/ui/src/ipc/agent.ts`).
  New CLIs implement the `AgentCli` trait in Rust and call `registerChatAgent` in the UI.

## Language servers (`forge-lsp`)

- Packs contribute `LspServerConfig { id, name?, languageIds, fileExtensions, command, args,
  initializationOptions, settings }`; Rust code calls `LspState::register_server`, the UI or a
  plugin calls `lsp_register_server({ config })`. Servers start lazily for the first matching file
  and are routed by extension (longest match wins, e.g. `blade.php` over `php`).
- `lsp_did_open|did_change|did_save({ path, text })`, `lsp_did_close({ path })`,
  `lsp_hover|lsp_completion|lsp_definition|lsp_references|lsp_signature_help({ path, line, character })`,
  `lsp_document_symbols({ path })`, `lsp_format({ path, tabSize?, insertSpaces? })`, `lsp_status()`.
  Paths are workspace relative, positions 0-based line / UTF-16 character.
- Events: `lsp://diagnostics` `{ path, diagnostics }` and `lsp://status` `ServerStatus[]`.
