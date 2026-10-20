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
- `forge-project` — project type detection and the New Project generators/templates.
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
- Feature crates (`forge-lsp`, `forge-vcs`, `forge-forges`, `forge-ai`, `forge-agent-cli`, ...) are plain libraries
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
  - `registerRunConfigurationType` / `registerDebugLauncher` (`src/features/run`) — Run/Debug
    configuration types and debuggers (see "Terminal and Run/Debug" below).
- Tool windows may render tabs next to their title with `headerContent` (Terminal, Run).
- Keymap focus contexts: `editor`, `input`, `toolwindow`, `global` and `terminal` (xterm.js has
  focus). In the terminal plain Ctrl+letter, Alt+letter and Escape go to the shell; function
  keys, Alt+digit, Ctrl+Shift and Ctrl+Alt combinations still reach the IDE.
  - `registerLineMarkerProvider` (`src/editor/lineMarkers.ts`) — gutter icons per line (route on a
    controller action, "go to template", entity <-> repository), recomputed after edits.
  - `registerDeclarationProvider` (`src/features/lsp/goto.ts`) — framework targets for Ctrl+B /
    Ctrl+Click that a language server cannot resolve (route names, service ids, DQL fields); tried
    before the LSP.
  - `claimFrameworkStatus` (`src/features/project-type`) — a framework plugin with its own status
    widget hides the generic "Laravel 12" / "Symfony 7" one.
  - `openCustomTab({ id: "forge:<kind>/<id>", title, icon?, component })` (`src/state/editor.ts`) —
    editor tabs that show a feature view instead of a file (merge requests, issues). `activeTab()`
    stays file-only; use `isCustomTab(id)` where an id may belong to such a tab.
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
- `fs://changed` `{ changed: string[], removed: string[], git: boolean }` — debounced file watcher
  batches (workspace-relative paths, ignored and excluded paths filtered out; `git` = index,
  HEAD or refs changed). Subscribe with `onFsChanged` (`src/state/fsWatch.ts`); the backend
  already updates the PHP index from it.
- `agent_respond_permission({ sessionId, requestId, decision })` answers an agent's
  `permissionRequest` event; `decision` is `{ behavior: "allow", scope?: "once" | "session",
  updatedInput? }` or `{ behavior: "deny", message? }`.
- Language servers of language packs are registered in Rust with `LspState::register_server`
  (PHP: `commands::php::sync_lsp_server`, run after project detection and on PHP settings
  changes); there are no built-in fallback servers.
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
- Editor navigation: `openFileAt(path, line?, column?, { flash?, focus? })` from
  `src/editor/views.ts` opens a workspace-relative file, puts the caret at the 1-based position,
  centers it and briefly flashes the line.

## Opening files from outside (`forge-open`, Browser Companion)

- Entry points: the browser extension (`browser-extension/`) via the local HTTP API, `forge://`
  links (`tauri-plugin-deep-link`, scheme registered in `tauri.conf.json`) and the launcher
  (`forge path/to/file:12[:3]`, `forge --line 12 file`, `forge file 12 3`). A second launch is
  forwarded to the running window by `tauri-plugin-single-instance`.
- Links: `forge://open?file=<abs or project-relative>&line=&column=` (aliases `url=file://…`, `l`,
  `col`) and `forge://open-symbol?name=UserCardComponent&kind=component`.
- HTTP API on `127.0.0.1:63342` (next nine ports if taken; settings `browserCompanion.*`):
  `GET /api/forge/ping`, `POST /api/forge/pair { code, name }`, `POST /api/forge/open
  { file, line?, column?, project?, symbol? }`, `POST /api/forge/open-symbol { name, kind? }`.
  Requests need a loopback `Host`, no web-page `Origin` (extension origins only) and, except
  ping/pair, `Authorization: Bearer <token>`. Tokens come from one-time 6-digit pairing codes;
  only their SHA-256 hashes are stored (`<config dir>/forge/browser-companion.json`).
- Every request is resolved against the open and recent projects into `companion://open`
  `{ source: "browser" | "link" | "cli", client?, file, line?, column?, target }` with
  `target.kind` `open` (shown right away), `recent` or `outside` (the UI asks first unless the
  source is the CLI). Requests that arrive before the UI is ready are returned by
  `companion_ui_ready()`. Directories from a second launch arrive as `companion://open-project`.
- IPC (Settings | Tools | Browser Companion): `companion_status`, `companion_start_pairing`,
  `companion_cancel_pairing`, `companion_revoke_client({ id })`, `companion_open_link({ url })`,
  `companion_project_profile({ envKeys, files })`, `companion_set_env_var({ file: ".env" |
  ".env.local", key, value })`; `companion://status` reports server restarts.

## Project types and New Project (`forge-project`)

- Detection reads markers in the project root and one level below (`composer.json`/`composer.lock`,
  `artisan`, `bin/console`, `wp-config.php`, `package.json`/lock files/`node_modules`, `angular.json`,
  `tsconfig.json`, `*.lua`, `.luarc.json`, `*.rockspec`, `conf.lua`, `init.lua`, `Cargo.toml`, `go.mod`,
  `pyproject.toml`, `Dockerfile`/`compose.yaml`, `.git`).
- `project_detect() -> ProjectInfo { languages, frameworks: { id, name, version?, language, dir? }[],
  packs, tools, primary? }`; `workspace_open` also emits `project://detected` `{ root, info }`, and
  `workspace_recent` returns each project's `primary` for the Welcome screen badge.
- Plugins opt in with `projectTypes = ["laravel"]` in their manifest (language, framework, pack or
  tool ids); without it they are always active. In the UI use `projectInfo()`, `usesFramework("laravel")`
  and `isPluginActive("forge.laravel")` from `src/state/project.ts`.
- New Project wizard (command `NewProject`): `project_generators_list() -> Generator[]`,
  `project_plan({ generator, location, name, options }) -> { target, preview, files?, missingTools,
  fallback?, error? }`, `project_create({ generator, location, name, options }) -> root` streaming
  `project://create` events (`{ type: "step" | "output" | "done" | "failed", ... }`),
  `project_create_cancel()`. Generators run official CLIs (composer, symfony, npm, npx) or write
  offline templates embedded from `crates/forge-project/templates/`. The browser copy of the generator
  list is `app/ui/src/mock/projectGenerators.json` (`FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`).
- The default location is the setting `system.defaultProjectDirectory` (`~/Projects`).

## Terminal and Run/Debug (`forge-process`)

- Terminal: `terminal_open({ options: { id?, cwd?, shell?, env?, cols, rows } }) -> { id, shell, cwd }`,
  `terminal_write({ id, data })`, `terminal_resize({ id, cols, rows })`, `terminal_close({ id })`.
  Output arrives on `terminal://<id>` as `{ type: "output", data } | { type: "exited", code }`; pass
  your own `id` to subscribe before the shell starts. Defaults: `terminal.shellPath` or `$SHELL` /
  pwsh / `%ComSpec%`, start directory `terminal.startDirectory` or the project root, extra
  variables from `terminal.environment`.
- Configurations: `run_configurations_load() -> { configurations, selected?, templates? }` and
  `run_configurations_save({ configurations })`. Shared ones ("Store as project file") are written
  to `<project>/.forge/runConfigurations.toml`, local ones, the selection and templates to
  `.forge/runConfigurations.local.toml`. A configuration is `{ name, type, storeAsProjectFile?,
  workingDirectory?, env?, interpreter?, interpreterOptions?, arguments?, options?: Record<string,
  string>, beforeLaunch?: { type: "runConfiguration" | "shell", value }[] }`.
- Runs: `run_start({ request: { runId?, configuration? | command?, env?, interpreterArgs? } }) ->
  { runId, commandLine }` streams `run://<runId>`: `{ type: "started", pid, commandLine }`,
  `{ type: "output", stream: "stdout" | "stderr", text }`, `{ type: "exited", code, signal }`.
  `run_stop({ runId })` (SIGTERM to the process group, SIGKILL after 2 s; `taskkill /T` on
  Windows), `run_write({ runId, data })` (stdin), `run_dispose({ runId })`,
  `run_command_line({ request })` (preview).
- Configuration types: the backend's `KindRegistry` builds command lines for `php-script`,
  `phpunit` (always `--teamcity`), `artisan`, `composer`, `shell` and `npm`; the PHP interpreter
  defaults to the setting `php.interpreter`. A manifest `runConfigurationTypes` entry can declare
  `fields` and a `command` template (`["${interpreter}", "${file}", "${arguments}"]`, see
  `lua-script`) and works without code. In the UI, `registerRunConfigurationType({ id, name, icon,
  fields, interpreter?, argumentsLabel?, testRunner?, buildCommand? })` adds the form to the
  Run/Debug Configurations dialog; `buildCommand` builds the command in the UI instead.
- Debugging: Debug (Shift+F9) calls the first `registerDebugLauncher({ id, canDebug(cfg, type),
  launch(cfg, ctx) })` that accepts the configuration; `ctx.run({ env, interpreterArgs })` starts it
  in the Run tool window (e.g. with `XDEBUG_SESSION` and `-dxdebug.mode=debug`) and returns
  `{ started, exited }`.
- Test runners parse TeamCity service messages (`src/features/run/teamcity.ts`) into the test
  tree; PHPUnit test classes and methods get gutter run icons that create temporary
  configurations.

## Language servers (`forge-lsp`)

- Packs contribute `LspServerConfig { id, name?, languageIds, fileExtensions, command, args,
  initializationOptions, settings }`; Rust code calls `LspState::register_server`, the UI or a
  plugin calls `lsp_register_server({ config })`. Servers start lazily for the first matching file
  and are routed by extension: every server sharing the longest match serves the file (TypeScript +
  ESLint + Tailwind on `.ts`; `blade.php` still beats `php`). An entry `=Dockerfile` matches a whole
  file name. With one `languageIds` entry per `fileExtensions` entry, the matching one is announced
  in `didOpen` (`tsx` → `typescriptreact`).
- Results of several servers are merged in `LspManager`: diagnostics per server (each server
  replaces only its own; missing `source` = server name), completion lists concatenated (items carry
  `raw.forgeServer` so `lsp_completion_resolve` goes back to their server), locations deduplicated,
  hover / signature help / document symbols from the first server with an answer (registration
  order = priority), formatting by the first server that supports it.
- `lsp_did_open|did_change|did_save({ path, text })`, `lsp_did_close({ path })`,
  `lsp_hover|lsp_completion|lsp_definition|lsp_references|lsp_signature_help({ path, line, character })`,
  `lsp_document_symbols({ path })`, `lsp_format({ path, tabSize?, insertSpaces? })`, `lsp_status()`.
  Paths are workspace relative, positions 0-based line / UTF-16 character.
- `lsp_workspace_symbols({ query, limit? }) -> WorkspaceSymbol[]` asks every running server
  (`workspace/symbol`); `{ name, kind, containerName?, path, range?, server }`. It backs the
  Search Everywhere providers `lsp.symbols` (Symbols tab) and `lsp.classes` (Classes tab).
- Events: `lsp://diagnostics` `{ path, diagnostics }` and `lsp://status` `ServerStatus[]`
  (`busy` carries `$/progress` message and percentage; the status bar shows
  "Indexing… TypeScript 45%").

### Catalog of language servers (`forge-langservers`)

- `crates/forge-langservers/catalog.toml` lists languages (extensions, file names, LSP language
  ids) and ~32 servers (TypeScript, Vue, Angular, Svelte, Astro, HTML/CSS/JSON/ESLint, Tailwind,
  YAML, TOML, XML, Markdown, Docker, Bash, SQL, LaTeX, Twig, Lua, Python, Go, Rust, C/C++, Java,
  Kotlin, C#, Ruby, Zig) with command, initialization options, settings, install options and
  licence. `pack_servers.toml` lists servers that packs start themselves (Intelephense, phpactor)
  so they show up on the same settings page.
- When a project's LSP manager is created, `commands::langservers::register_enabled` scans the
  project (honouring `.gitignore`, capped by `langservers.scanLimit`) and registers every enabled
  and installed server whose languages occur (and whose `when` markers exist, e.g. an ESLint config).
  Servers in the same `alternative` group (vtsls / typescript-language-server, pyright /
  basedpyright) are exclusive. Detection looks at PATH, npm/nvm/volta/fnm, `~/.local/bin`,
  composer, cargo, go, dotnet, gem dirs, Mason (`~/.local/share/nvim/mason/bin`) and Windows
  `.cmd` shims.
- Per-server settings: `langservers.servers.<id>.enabled|path|settings` (settings = JSON string
  merged into the server settings; `initializationOptions` key for startup options). Global:
  `langservers.autoStart`, `langservers.scanLimit`, `langservers.lua.environment|version`.
- IPC: `langservers_list() -> ServerRow[]`, `langservers_apply() -> ServerRow[]` (re-plan after
  settings changed), `langservers_install({ id, index }) -> InstallOutcome` with output on
  `langservers://install-output` `{ id, stream, line }`, `langservers_cancel_install({ id })`,
  `langservers://changed`. Install commands come from the catalog only and run without a shell.
- Intelephense licence: `intelephense_licence_status() -> { premium, maskedKey?, error?,
  purchaseUrl }`, `intelephense_set_licence({ key | null })`. The key lives in the OS keychain
  (service `forge-lsp`, account `intelephense`); free mode never sends `licenceKey`. A registered
  `intelephense` config gets the key applied (and restarts) when the manager is created or the key
  changes; pack code building Intelephense configs can call
  `commands::langservers::with_intelephense_licence(&mut config)`. A plaintext
  `php.lsp.licenceKey` setting (or the PHP page's old localStorage value) is migrated once.
- Events: `lsp://diagnostics` `{ path, diagnostics }` and `lsp://status` `ServerStatus[]`.

## PHP debugger (`forge-debug`)

- `DebugServer` is a DBGp server for Xdebug 3: it listens on `php.debug.port` (9003), accepts any
  number of connections, identifies the server (`PHP_IDE_CONFIG=serverName=…`, else
  `$_SERVER['SERVER_NAME']`/`SERVER_PORT`), maps paths and sets the project's breakpoints.
  Breakpoints live in `<root>/.forge/breakpoints.json`, servers with path mappings in
  `<root>/.forge/php-servers.json`. Unknown servers whose script is not a project file wait for
  the "Incoming Connection From Xdebug" dialog (`debug_accept_connection` / `debug_ignore_connection`).
- IPC: `debug_status`, `debug_listen_start() -> port`, `debug_listen_stop`, `debug_sessions`,
  `debug_step({ sessionId, kind: "run" | "stepInto" | "forceStepInto" | "stepOver" | "stepOut" })`,
  `debug_pause|debug_stop|debug_detach({ sessionId })`, `debug_run_to_cursor({ sessionId, file, line })`,
  `debug_stack`, `debug_contexts`, `debug_variables({ sessionId, depth, context? })`,
  `debug_property({ sessionId, fullname, depth, context?, page? })`, `debug_eval({ sessionId, expression, depth? })`,
  `debug_set_value`, `debug_breakpoints`, `debug_breakpoint_add|update|remove|toggle`,
  `debug_breakpoints_mute`, `debug_watches|debug_watches_set`, `debug_servers|debug_servers_save`,
  `debug_launch_env({ serverName? }) -> env`. Files are project relative, lines 1-based.
- Events (tagged `DebugEvent` JSON, `type` = last path segment): `debug://listening`, `debug://session`
  (`SessionInfo` with status and location), `debug://incoming`, `debug://ended`, `debug://output`,
  `debug://breakpoints`, `debug://error`. Typed client: `app/ui/src/features/debug/api.ts`.
- Run configurations start debugging through `debugLauncher` (`app/ui/src/features/debug/index.tsx`):
  `await debugLauncher.prepare(serverName?)` starts listening and returns the environment
  (`XDEBUG_MODE`, `XDEBUG_SESSION`, `XDEBUG_CONFIG`, `PHP_IDE_CONFIG`) for the PHP process;
  `debugLauncher.setStopFallback(fn)` lets "Stop" (Ctrl+F2) stop a run when no debug session is active.
- Main toolbar widgets left of the run configuration selector: `registerToolbarItem`
  (`src/features/toolbarItems.ts`).
## Database tools (`forge-db`)

- Data sources live in `<root>/.forge/dataSources.toml` (project) and
  `<config dir>/forge/dataSources.toml` (global) as `[[dataSource]]` tables; passwords go to the
  OS keychain (service `forge-db`, account = data source id). MySQL/MariaDB, PostgreSQL and SQLite
  via `sqlx`; SQL is sent as raw text, values come back as JSON-safe cells (`null`, booleans,
  numbers within ±2^53, everything else as strings, binary as `{ "$hex": "…" }`).
- IPC (`app/ui/src/ipc/db.ts`): `db_data_sources_list`, `db_data_source_save({ source, password? })`,
  `db_data_source_delete({ id })`, `db_test_connection({ source, password? })`, `db_disconnect`,
  `db_schemas`, `db_introspect({ id, schema })`, `db_table_info`, `db_table_ddl`,
  `db_table_data({ id, request: { schema, table, filter?, order?, offset?, limit? } })`,
  `db_apply_changes({ id, schema, table, changes })` (one transaction, refused without a primary key),
  consoles `db_session_open|close`, `db_execute({ sessionId, sql, pageSize? })`, `db_fetch`,
  `db_set_auto_commit`, `db_commit`, `db_rollback`, `db_cancel`, `.env` suggestions
  `db_detect_env` / `db_add_from_env`, and `db_export({ format: "csv"|"json"|"sql", dialect, table?, columns, rows, path? })`.
- Events: `db://dataSources` (full list after changes and after a project's sources were loaded),
  `db://session` (`SessionInfo` when a console starts/stops running or its transaction changes).
- UI: non-file editor tabs are registered with `registerEditorView` (`src/shell/editorViews.ts`) and
  opened with `openViewTab`; consoles and table editors use it. Editor-area panels can opt into the
  tool-window key context with `data-focus-context="toolwindow"`.
## Laravel (`packs/phpforge/fw-laravel`)

- `Laravel::load(root)` analyses a project statically with the PHP pack's tree-sitter parser:
  routes from `routes/*.php` (verbs, `match`, `view`, `redirect`, `resource`/`apiResource`,
  groups with prefix/name/middleware/controller, included files, the `api` prefix), views and
  Blade components (anonymous, class based, Livewire), custom directives (`Blade::directive`,
  `Blade::if`), config and `.env` keys, Eloquent models (columns from migrations, `$fillable`,
  casts, relations with the related model, scopes, accessors) and translations. The app caches
  it per workspace and reloads when the files change.
- `laravel_detect() -> LaravelInfo { detected, version?, routes, views, components, models,
  configKeys, translations, directives }`.
- `laravel_routes() -> { routes: Route[], source: "static" | "artisan" }`. With the setting
  `laravel.routes.source` = `artisan` (or `auto` when `vendor/` exists) it runs
  `php artisan route:list --json` and keeps the static source locations. A `Route` is
  `{ methods, uri, name?, action, controller?, controllerMethod?, middleware, domain?, source?,
  actionLocation? }`; locations are `{ path, line, character }` (0-based, UTF-16).
- `laravel_complete({ path, offset, content }) -> { from, to, items: { label, kind, detail?,
  documentation?, insertText? }[] } | null` and `laravel_goto({ path, offset, content }) ->
  Location[]` for `route()`, `view()`/`@include`, `config()`, `env()`, `__()`, query builder
  columns (`->where('…')`), relations (`with('…')`), `$model->property`, `Model::scope`,
  `[Controller::class, 'method']`, `<x-component>` and custom directives. Offsets are UTF-16.
- `laravel_gutter({ path, content }) -> { line, kind: "route" | "controller" | "view", tooltip,
  targets }[]`.
- `laravel_artisan_commands() -> { commands, fromProject }` (`php artisan list --format=json`,
  else a built-in list), `laravel_artisan_run({ commandLine, runId }) -> exitCode` streaming
  `laravel://artisan/<runId>` events (`{ type: "started" | "output" | "error" | "exit", ... }`),
  `laravel_artisan_cancel({ runId })`.
- UI (`app/ui/src/features/laravel`): the Blade language (`registerLanguage("blade")`), a
  completion source next to the LSP one, `GotoDeclaration` / Ctrl+Click that try Laravel first,
  gutter icons, the Routes tool window, Run Artisan Command… (`Laravel.RunArtisan`,
  Ctrl+Shift+X), the `framework.laravel` status item and Settings | Languages & Frameworks |
  PHP | Laravel. Tool windows and status items can declare `when` to show only for matching
  projects.
## Symfony (`phpforge-fw-symfony`) and Doctrine (`phpforge-orm-doctrine`)

- Both plugins activate only for projects whose detection reports `symfony` / `doctrine`.
  Settings live under Languages & Frameworks | PHP | Symfony (`symfony.*`) and | Doctrine
  (`doctrine.*`). In browser mode `?project=symfony` opens the Symfony demo project.
- `symfony_detect() -> SymfonyInfo | null` (`{ version, environment, consolePath, hasConsole,
  containerXml?, kernelClass?, routes, services, templates, commands }`), `symfony_refresh()`.
  The backend caches a project snapshot for 10 s; refresh rescans.
- `symfony_routes() -> Route[]` (`{ name, path, methods?, controller?, host?, source:
  "attribute" | "annotation" | "yaml", location? }`), `symfony_services() -> Service[]` (`{ id,
  class?, alias?, public, autowire, autoconfigure, tags?, source: "yaml" | "resource" |
  "container", location? }`), `symfony_parameters()`, `symfony_templates()`,
  `symfony_console_commands()` (application commands first, then framework/bundle ones). Services
  and parameters merge `config/services*.yaml` with `var/cache/<env>/*Container.xml` when present.
- `symfony_complete({ path, content, offset }) -> { from, items: { label, kind: "route" | "service"
  | "template" | "parameter" | "tag" | "filter" | "function" | "test" | "variable", detail?, info?,
  apply? }[] } | null` and `symfony_goto({ path, content, offset }) -> Location[]` for PHP, Twig and
  YAML files. `symfony_gutter({ path, content }) -> { line, kind: "route" | "template" | "controller"
  | "command", title, targets?, command? }[]`. Offsets are UTF-16, `Location` is `{ path, line,
  column }` (0-based).
- `symfony_console_run({ command, args, runId })` streams `symfony://console` events `{ type:
  "started", runId, commandLine } | { type: "output", runId, text, stderr } | { type: "exit", runId,
  code }`; `symfony_console_cancel({ runId })`.
- `doctrine_entities({ refresh? }) -> Entity[]` (`{ class, name, table?, repositoryClass?, path,
  line, source: "attribute" | "annotation" | "xml" | "yaml", embeddable?, mappedSuperclass?,
  fields: { name, type, column?, length?, nullable?, unique?, id?, generated?, embedded?, line }[],
  associations: { name, kind: "manyToOne" | "oneToMany" | "manyToMany" | "oneToOne", target,
  mappedBy?, inversedBy?, line }[] }`).
- DQL inside PHP (`createQuery()` incl. concatenations, heredocs and `Entity::class`, QueryBuilder
  `select/where/join/orderBy/...` and `expr()` fragments with the aliases of the builder chain):
  `doctrine_complete({ path, content, offset }) -> { from, items: { label, kind: "keyword" |
  "function" | "entity" | "alias" | "field" | "association" | "embedded", detail?, apply? }[] } |
  null`, `doctrine_goto`, `doctrine_diagnostics({ path, content }) -> { start, end, message }[]`,
  `doctrine_gutter({ path, content })`. The UI highlights DQL itself (`features/doctrine/dql.ts`,
  same token rules as the Rust lexer).
## Code hosts (`forge-forges`)

- `CodeHost` trait with GitLab (REST v4, `PRIVATE-TOKEN`), Forgejo/Gitea (`/api/v1`, `token`) and
  GitHub (REST + GraphQL for review threads, `Bearer`) implementations. Numbers are the per-project
  ones (`!12` on GitLab, `#12` elsewhere); `Capabilities` tell the UI the wording and which actions
  the host supports (e.g. Forgejo cannot resolve threads through its API).
- The repository is detected from `git remote -v` (https, ssh, scp-like, nested GitLab groups);
  accounts make self-hosted servers known, also with relative URL roots and SSH ports.
- Accounts live in `<config dir>/forge/forges.toml`; tokens in the OS keychain (service
  `forge-forges`, account = account id) with `GITLAB_TOKEN`, `FORGEJO_TOKEN`/`GITEA_TOKEN`,
  `GITHUB_TOKEN`/`GH_TOKEN` as fallback for the public servers.
- IPC (`app/ui/src/ipc/forges.ts`): `forges_accounts`, `forges_login({ kind, server, token, apiUrl? })`
  (verifies before storing), `forges_logout({ id })`, `forges_verify({ id })`, `forges_repo() ->
  { repo, repos, account, capabilities } | null`, `forges_merge_requests({ filter }) -> Page<MergeRequest>`,
  `forges_merge_request({ number })`, `forges_mr_files({ number })` (parsed hunks),
  `forges_discussions({ number })`, `forges_create_mr({ request })`, `forges_comment({ number, body,
  position? })`, `forges_reply({ number, discussionId, body })`, `forges_resolve({ number,
  discussionId, resolved })`, `forges_approve`, `forges_merge({ number, options })`, `forges_close`,
  `forges_issues({ filter })`, `forges_issue`, `forges_issue_comment`, `forges_pipelines({ branch? })`,
  `forges_ci_status({ gitRef })`, `forges_members()`, `forges_file({ path, gitRef })`,
  `forges_checkout({ number, sourceBranch }) -> branch` (emits `git://changed`).
- Errors are worded for balloons: rejected tokens, missing permissions, rate limits (with the reset
  time), unreachable servers.
