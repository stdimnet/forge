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
