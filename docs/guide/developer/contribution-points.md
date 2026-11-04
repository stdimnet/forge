---
title: Contribution points
description: Every UI extension API of Forge (registerToolWindow, registerCommand, registerSettingsPage, registerEditorExtension, registerSearchProvider, registerLanguage, registerKeymapActions and more) with examples.
order: 60
tags: [ui, extension points, solidjs, api]
---

# Contribution points

Features plug into the UI through `register*` functions. A feature is a folder
`app/ui/src/features/<name>/` whose `index.ts` calls them; add one import line to
`app/ui/src/features/index.ts`. Registrations with the same `id` replace earlier ones (that is
how real tool windows replace placeholders, and how the Blade pack replaces a syntax mode).

On the Rust side, plugins contribute **data** through their manifest (see
[Writing a plugin or pack](plugins-and-packs.md)); this page is about the UI.

> **Note:** the examples are shortened sketches; helper names such as `laravelApi.goto` stand
> for a feature's typed IPC client. The modules in the table are the reference.

## Overview

| Function | Module | Contributes |
| --- | --- | --- |
| `registerToolWindow` | `features/registry.ts` | a panel on a stripe |
| `registerCommand` | `state/commands.ts` | an action with an id (Find Action, keymap, menus) |
| `registerKeymapActions` | `keymap/store.ts` | plugin actions with default shortcuts |
| `registerSettingsPage` | `features/settingsPages.ts` | a page in the Settings dialog |
| `registerMainMenuEntries` | `features/mainMenu` | entries in the main menu (`"Tools"`, `"Tools/_Laravel"`, `"View/Tool Windows"`) with `before`/`after`/`when` |
| `registerMainMenuDynamic` / `registerMainMenuToggle` | `features/mainMenu` | dynamic lists (`recentProjects`, `toolWindows`) and check marks |
| `chooseDeclarationAt` | `features/navigation/chooseDeclaration.ts` | the shared "Choose Declaration" popup for several targets |
| `registerStatusItem` | `features/statusItems.ts` | a status bar widget |
| `registerToolbarItem` | `features/toolbarItems.ts` | a main toolbar widget left of the run widget |
| `registerEditorExtension` | `editor/extensions.ts` | CodeMirror extensions per file |
| `registerLanguage` | `editor/languages.ts` | the syntax mode of a file type id |
| `registerLineMarkerProvider` | `editor/lineMarkers.ts` | gutter icons |
| `registerDeclarationProvider` | `features/lsp/goto.ts` | framework targets for Go to Declaration (tried before the LSP) |
| `registerFormatProvider` | `features/lsp/format.ts` | a formatter tried before the language server |
| `registerCompletionKind` | `features/lsp/completion.ts` | icons/colors for custom completion kinds |
| `registerSearchProvider` | `features/search/providers.ts` | candidates for a Search Everywhere tab |
| `registerIconTheme` / `contributes.iconThemes` | `features/iconThemes/state.ts` / plugin manifest | a file icon theme (see [Icon themes](icon-themes.md)) |
| `registerStructureProvider` | `features/php/StructureView.tsx` | the Structure tool window for another language |
| `registerContextMenu` | `features/contextMenu/state.ts` | entries of a context menu (e.g. `"editor"`) |
| `registerProjectViewSection` | `features/project/ProjectView.tsx` | top-level nodes below the project tree |
| `registerFileSystem` | `state/editor.ts` | editor tabs for a URI prefix (`scratch://`) |
| `openCustomTab` | `state/editor.ts` | an editor tab that shows a feature view (`forge:<kind>/<id>`) |
| `registerEditorView` | `shell/editorViews.ts` | non-file editor tabs opened with `openViewTab` (consoles, table editors) |
| `registerFileEditor` | `shell/editorViews.ts` | an editor for some text files instead of the plain one (Markdown preview, SVG preview, CSV table) |
| `registerBinaryFileView` | `state/editor.ts` | binary files that open as a view tab (images) |
| `registerRunConfigurationType` | `features/run/types.ts` | a Run/Debug configuration type |
| `registerDebugLauncher` | `features/run/types.ts` | a debugger for Debug (`Shift+F9`) |
| `registerDebugAdapter` | `features/debug/adapters.ts` | a Debug Adapter Protocol debugger: the run configuration types it debugs and how they become launch requests (with `[[contributions.debugAdapters]]` or a Rust `DebugAdapterDescriptor`) |
| `[[contributions.testFrameworks]]` / `TestFrameworkProvider` | manifest / `forge-testing` | a test framework of the shared test runner (gutter, test tree, rerun, coverage) |
| `[[contributions.inspections]]` / `[[contributions.inspectionMappings]]` / `InspectionProvider` / `ProblemSource` | manifest / `forge-inspections` | an inspection (profiles, severities, suppressions, Inspect Code) and which published problems belong to it |
| `registerBeforeLaunchHandler` | `features/run/runStore.ts` | a Before launch task kind the run feature does not run itself (`externalTool`) |
| `registerEndpointsView` | `features/endpoints/registry.ts` | a view of the shared Endpoints tool window (one tab per pack when several apply: Python web frameworks, Spring and JAX-RS) |
| `registerServicesProvider` | `features/services/providers.ts` | nodes, details and actions of the Services tool window next to Docker and Kubernetes (the Spring Boot run dashboard) |
| `registerRunAnythingProvider` | `features/runAnything/providers.ts` | commands offered in Run Anything (Artisan, Symfony console, command line tools) |
| `registerDialog` / `openDialogWindow` | `shell/window/dialogs.ts` | a large dialog in its own window |
| `registerChatAgent` | `features/ai-chat/agents.ts` | an agent in the AI Chat picker |
| `claimFrameworkStatus` | `features/project-type` | hide the generic framework status widget |
| `registerMock` | `ipc/index.ts` | a browser-mode handler for an IPC command |

## Tool windows

```tsx
import { Container } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";

registerToolWindow({
  id: "services",            // same id as a placeholder replaces it
  title: "Services",
  icon: Container,           // lucide-solid
  anchor: "bottom",          // default place: "left" | "right" | "bottom"
  split: false,              // optional: start in the split group (Left Bottom, Right Bottom, Bottom Right)
  order: 30,                 // default position in the stripe group
  shortcut: "Alt+8",         // tooltip only; the binding comes from the keymap
  component: ServicesView,
  actions: () => <HeaderButtons />,   // optional header buttons
  headerContent: () => <Tabs />,      // optional, e.g. tabs next to the title
  when: () => usesFramework("docker"), // optional: only for matching projects
});
```

Users can move every tool window to another place, reorder it or remove it from the sidebar
(saved per project), so never assume a window sits at its `anchor`: open and check it with
`showToolWindow(id)`, `toggleToolWindow(id)`, `hideToolWindow(id)` and `isToolWindowVisible(id)`
from `state/layout.ts`.

Recipe: [`add-tool-window`](../../../.claude/skills/add-tool-window/SKILL.md).

## Commands

```ts
import { registerCommand } from "@/state/commands";

registerCommand({ id: "ActivateStructureToolWindow", title: "Structure", category: "View",
  run: () => toggleToolWindow("structure") });
```

Use a PHPStorm action id when one exists: the command then gets PHPStorm's default shortcut and
appears in Settings | Keymap. Otherwise use `<area>.<verb>` and, if it needs a shortcut, register
a keymap action ([Keymap actions](keymap-actions.md)). `runCommand(id)` runs any command.

## Settings pages

```tsx
registerSettingsPage({
  id: "lang.lua", title: "Lua", group: "Languages & Frameworks", parent: undefined, order: 30,
  keywords: ["love", "stylua"],
  component: () => <SchemaPage pageId="lang.lua" />,
});
```

`SchemaPage` renders every setting whose `group` is the page id. See [Settings](settings.md).

## Icon themes

```ts
registerIconTheme({ id: "forge.example.icons", label: "Example", load: async () => ({ definition, svgs }) });
```

Third-party plugins contribute themes declaratively (`contributes.iconThemes` with an
`icon-theme.json` and SVGs). Show file icons with `<FileIcon name path? dir? open? />` so the
active theme is asked first. Format, matching rules and look hooks: [Icon themes](icon-themes.md).

## Status bar and toolbar items

```ts
registerStatusItem({ id: "framework.laravel", align: "right", order: 80,
  component: LaravelStatus, when: () => laravelActive() });
registerToolbarItem({ id: "php.debug.listen", order: 10, component: ListenButton });
```

## Editor extensions and languages

```ts
registerEditorExtension(({ path }) =>
  path.endsWith(".php") ? [myCompletion(path), myGutter(path)] : null);

registerLanguage({ id: "neon", load: () => yaml() });
```

Factories run per opened file and return CodeMirror extensions (or `null`). `registerLanguage`
maps a **file type id** (decided in Rust by `forge-filetypes`) to a syntax mode. See
[Adding a language](adding-a-language.md).

## Line markers

```ts
registerLineMarkerProvider({
  appliesTo: (p) => p.endsWith(".php"),
  watch: () => laravelActive(),                 // recompute when this changes
  markers: async (path, text) => [{ line: 12, icon: Route, title: "GET /orders",
    className: "marker-route", onClick: (view, e) => openRoute() }],
});
```

## Go to Declaration and formatting

```ts
registerDeclarationProvider(async (path, view, pos) => {
  const targets = await laravelApi.goto(path, view.state.doc.toString(), pos);
  if (!targets.length) return false;   // let the next provider or the LSP try
  await openFileAt(targets[0].path, targets[0].line + 1);
  return true;
});

registerFormatProvider(async (path, view) => path.endsWith(".lua") && (await formatWithStyLua(path, view)));
```

When a provider finds several targets, show the shared popup instead of guessing:

```ts
import { chooseDeclarationAt } from "@/features/navigation/chooseDeclaration";

chooseDeclarationAt(view, pos, targets /* { path, line, column?, preview?, name?, container? }[], 0-based */,
  { title: "Choose Declaration" });
```

After the providers, Go to Declaration asks Rust (`code_goto_declaration`: language servers,
then the PHP index and the PHP stubs). On a declaration the answer has `onDeclaration` and the
UI shows the usages (`showUsagesAt` in `features/codeintel/usages.ts`); targets outside the
project come as read-only `library://…` paths. Ctrl+hover asks `code_symbol_info` for the range
to underline and the declaration for the tooltip.

## Main menu

The menu structure, labels, mnemonics (`_` before the letter), shortcuts and enabled states are
computed in Rust (`forge-keymap::menu`, IPC `keymap_main_menu`); an entry is enabled when some
command implements its action. Add entries from a manifest:

```toml
[[plugin.contributions.mainMenu]]
path = "Tools/_Laravel"            # submenus are created as needed
items = [
  { type = "action", id = "Laravel.RunArtisan", label = "Run _Artisan Command..." },
  { type = "separator" },
]
# optional: before = "<action id>", after = "<action id>"; items may be checkable = true
```

or from the UI with `registerMainMenuEntries(path, entries, { before?, after?, when? })`.
`registerMainMenuToggle(actionId, isOn)` reports the check mark of a checkable item.

## Search Everywhere

```ts
registerSearchProvider({
  id: "php.classes", tab: "classes", title: "Classes", order: 0,
  async search(query, { limit }) {
    const hits = await phpApi.searchSymbols(query, "class", limit);
    return hits.map((h) => ({ id: h.fqn, label: h.name, detail: h.namespace, kind: "class",
      score: h.score, matches: h.positions, run: () => openFileAt(h.path, h.line) }));
  },
});
```

Providers only **collect** candidates. Ranking happens in Rust (`search_rank`): items with a
`match` spec are matched there; pre-ranked items (`score` + `matches`) are taken as they are.

## Run configuration types and debuggers

```ts
registerRunConfigurationType({
  id: "artisan", name: "Artisan", icon: Terminal, interpreter: "php", debuggable: true,
  fields: [{ key: "command", label: "Command", required: true, placeholder: "migrate --seed" }],
});

registerDebugLauncher({
  id: "xdebug",
  canDebug: (cfg) => PHP_TYPES.has(cfg.type),
  launch: async (cfg, ctx) => { const env = await debugLauncher.prepare(); await ctx.run({ env }); },
});
```

Without `buildCommand`, the backend's kind registry (`forge_process::config`) builds the command
line. A manifest type with `fields` and a `command` template (see `lua-script`) needs no UI code.
A type may also bring its own `form` (Compound), a `validate` check and `outputFilters` (External
Tools' `$FILE_PATH$:$LINE$` links).

Launching goes through a plan computed in Rust (`run_launch_plan`, `forge-runtools::launch`):
compound members and the ordered Before launch tasks. Kinds the run feature does not handle
itself are registered by their feature:

```ts
registerBeforeLaunchHandler("externalTool", (task) => runToolAndWait(task.value));
```

## Debug adapters

Every language except PHP debugs through a [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/)
adapter ([forge-dap](crates/forge-dap.md)). The shared Debug tool window, breakpoints dialog,
Attach to Process (`Ctrl+Alt+F5`), Attach to DAP Server and Debug Core Dump then work for the
pack without further UI code. A pack contributes an adapter in three layers; most need only
the first two.

1. **Manifest** (how to start the adapter and the request templates):

   ```toml
   [[contributions.debugAdapters]]
   type = "debugpy"
   label = "Python (debugpy)"
   command = "python3"
   args = ["-m", "debugpy.adapter"]
   commandSetting = "python.debugger.path"   # optional: user-chosen executable
   transport = "stdio"                       # stdio | connect (${port} in args) | listen (adapter connects back)
   languages = ["py"]                        # files with line breakpoints
   launch = { program = "${program}", args = "${args}", cwd = "${cwd}", env = "${env}", stopOnEntry = "${stopOnEntry}", console = "internalConsole" }
   attach = { processId = "${pid}" }         # omit: no Attach to Process
   # coreDump = { program = "${program}", coreFile = "${core}" }, coreDumpRequest = "attach"
   ```

   A string that is exactly `"${args}"`, `"${env}"`, `"${pid}"` or `"${stopOnEntry}"` becomes
   an array, object, number or bool; other placeholders (`${program}`, `${cwd}`, `${core}`,
   `${name}`) are replaced inside strings; keys whose placeholder has no value are dropped.
   The run configuration's "launch options" JSON is merged over the result.

2. **UI** (which run configuration types the adapter debugs, and how):

   ```ts
   import { registerDebugAdapter, defaultLaunchRequest } from "@/features/debug/adapters";

   registerDebugAdapter({
     type: "debugpy",
     runConfigurationTypes: ["python-script"],
     resolve: (cfg) => ({ ...defaultLaunchRequest("debugpy", cfg), program: cfg.options?.script }),
   });
   ```

   `resolve` returns a `LaunchRequest` (`mode: "launch" | "attachProcess" | "attachServer" |
   "coreDump"`); without it, `defaultLaunchRequest` reads the common fields (`program`,
   arguments, working directory, environment, `stopOnEntry`, `launchOptions`).

3. **Rust** (adapters that need code to start, e.g. a managed download or a virtualenv):
   implement `forge_dap::DebugAdapterDescriptor` (`info()` and `resolve(&LaunchRequest) ->
   ResolvedLaunch { transport, request, arguments }`) and register it with
   `DapManager::register_adapter`.

Line breakpoints in the adapter's `languages` work automatically; function breakpoints,
watchpoints, exception filters, logpoints, threads, memory, disassembly and registers appear
when the adapter's capabilities say so. Starting a program is refused in Safe Mode; attaching
is not. Tests use the fake adapter (`forge_dap::fake`, binary `forge-dap-fake`) instead of a
real debugger.
## Test frameworks

A pack's test framework plugs into the shared test runner (`crates/forge-testing`): the gutter
▶ icons, a run configuration type, the test tree, Rerun Failed Tests, the Test History,
auto-test, Run with Coverage and Run on (run targets) then work without UI code. Declare it in
the plugin manifest:

```toml
[[contributions.testFrameworks]]
id = "busted"                       # also the run configuration type
name = "busted"
detect = { files = [".busted", "spec/**/*_spec.lua"], dependencies = [{ file = "*.rockspec", contains = "busted" }] }
testFiles = ["*_spec.lua"]          # gutter icons in these files
program = "busted"
programSetting = "lua.busted.path"  # setting with the executable
args = ["--output=TAP"]
report = "tap"                      # teamCity | jUnitStdout | jUnitFile | tap | goTestJson | libtestJson | libtestText | pytestReportLog | jestJson | tox | minitestText
junitFlavor = "generic"             # generic | behat | phpSpec | pytest (JUnit locations)
nameSeparator = " "                 # tests are named by their full name ("Cart totals rounds")
escape = "lua"                      # how ${testPattern} escapes a name: regex | lua | none
discovery = [
  { kind = "describe", pattern = '''^\s*describe\(\s*["'](?P<name>[^"']+)''' },
  { kind = "test", pattern = '''^\s*it\(\s*["'](?P<name>[^"']+)''' },
]
locationRules = [{ pattern = '^example\.com/shop/', replacement = "" }]  # reported paths → project paths
rerun = { join = "\n" }             # Rerun Failed Tests: the `pattern` scope with the failed names
coverage = { args = ["--coverage"], file = "luacov.report.out", format = "lcov" }

[contributions.testFrameworks.scopes]   # arguments per Test scope
all = []
directory = ["${directory}"]
file = ["${file}"]
test = ["${file}", "--filter=${testPattern}"]
pattern = ["--filter=${pattern}"]
```

Placeholders: `${file}`, `${directory}`, `${test}` (full name), `${testName}`, `${testPattern}`,
`${pattern}`, `${reportDir}` (report files of `jUnitFile` frameworks), `${coverageFile}`. An
argument whose placeholders are all empty is dropped; a multi-line `${pattern}` repeats its
argument per line. Discovery nests by indentation; `kind` is `class`, `describe`, `suite` or
`file` for groups and `test`, `method` or `example` for tests. Coverage formats: `lcov`,
`cobertura`, `clover`, `jacoco`, `goCover`. `fields` adds form fields to the framework's run
configuration type (`{ key, label, kind, placeholder, default, options }`, stored in the
configuration's options) for a provider that wraps the spec. Discovery asks the frameworks the
project uses first (`TestFrameworks::discover_in_project` with the detected ids), so frameworks
that share file globs (Jest and Vitest on `*.test.ts`) do not collide.

Frameworks that need code implement `forge_testing::TestFrameworkProvider` (types, `accepts`,
`discover`, `rerun_failed`, optionally `command` and `supports_coverage`) and register it in
the backend's `TestFrameworks` registry (`RunState::new`) and in `forge-wasm`'s `registry()`,
as the PHP frameworks do (`phpforge_test_frameworks::PhpTestFrameworks`). A provider can also
wrap a manifest spec and register under its id after `register_specs`, as WebForge's Node.js
runners do (`webforge_js_testing::register`). New output formats
go into `forge-output` (`formats/`): a `LineParser` for streamed lines or a document parser,
both producing `TestEvent`s.


Frameworks that need code (discovery from parsed source, commands through an interpreter)
implement `forge_testing::TestFrameworkProvider` in Rust instead, like PyForge's
`PythonTestFrameworks` (pytest, unittest, tox in `packs/pyforge/testing`), and register their
run configuration types in the UI. `registerTestFileMatcher(path => bool)` from
`@/features/run` attaches the test gutter to their files, and
`registerContextConfiguration(path => cfg | null)` answers Run Context Configuration in files
without tests.

## Inspections

Every problem Forge shows belongs to an inspection of `crates/forge-inspections`. Declare a
pack's inspections in its manifest:

```toml
[[contributions.inspections]]
id = "PhpUnusedAssignment"          # the id profiles and suppression comments use
name = "Unused assignment"
group = "PHP/Data flow"             # the Settings tree: language / group
language = "php"
severity = "WEAK WARNING"           # ERROR, WARNING, WEAK WARNING, SERVER PROBLEM, TYPO, INFO
cleanup = true                      # its safe fixes run in Code Cleanup
description = "Reports assignments whose value is never read."

[[contributions.inspectionMappings]]   # problems published by an external tool
source = "clippy"                      # the third argument of publishDiagnostics
code = "clippy::*"                     # optional; a trailing * matches a prefix
inspection = "RsClippy"
```

`external = true` keeps the level a tool reports until the profile sets one, `batch = false`
keeps an inspection out of Inspect Code (problems that need a running language server or
tool), `options` adds typed options to the Settings page.

- **Editor problems** need no code: whatever a feature publishes with
  `publishDiagnostics(path, diagnostics, source)` goes through the selected profile (the
  diagnostic's `code` when it is an inspection id, else the mapping of the published source,
  else the mapping of the diagnostic's own `source`, e.g. `PHPStan`): disabled
  inspections drop out, severities follow the profile and suppression comments hide
  problems. Alt+Enter offers the Suppress actions for every problem with an inspection.
- **Inspect Code / the command-line inspector** run Rust providers: implement
  `forge_inspections::InspectionProvider` (`inspect_file` per file, `inspect_project` across
  files) and add it in `app/src-tauri/src/inspections.rs`. An external tool that reports
  problems by file, position and code (PHPStan, Psalm, a linter) implements
  `forge_inspections::ProblemSource`; its problems are mapped with the manifest mappings.

## Run Anything providers

```ts
registerRunAnythingProvider({
  id: "symfony-index",
  title: "Symfony console commands",
  when: () => usesFramework("symfony"),
  commands: async () => (await symfonyApi.consoleCommands()).map((c) => ({ command: `php bin/console ${c.name}`, description: c.description })),
});
```

The popup asks active providers once when it opens; Rust ranks their commands with the other
suggestions, and picking one runs the command line (resolved to a configuration type by
`run_anything_resolve`).

## Dialog windows

```ts
registerDialog({ id: "run-configurations", title: "Run/Debug Configurations",
  width: 960, height: 640, component: ConfigurationsDialog,
  prepare: () => loadConfigurations(), onClosed: (result) => reload() });

const result = await openDialogWindow("run-configurations", { select: "UserTest" });
```

In the desktop app the dialog opens as a native window owned by the caller (geometry remembered
in `dialog-windows.json`); in browser mode it opens in-page. The dialog ends with
`closeDialog(id, result)`. Small popups stay in-page.

## Editor tabs that are not files

- `openCustomTab({ id: "forge:mr/41", title: "#41 …", icon, component })` — feature views such as
  merge requests and issues. `activeTab()` stays file-only; check `isCustomTab(id)`.
- `registerEditorView` + `openViewTab` — database consoles and table editors.
- `registerFileEditor({ id, accepts(path), component })` (`shell/editorViews.ts`) — a richer
  editor for some text files, like PHPStorm's file editor providers; the component gets the
  tab and its editor group and usually embeds `CodeEditor` (Markdown and SVG with
  `TextEditorWithPreview`, CSV/TSV with Text / Table tabs).
- `registerBinaryFileView({ view, accepts(path) })` (`state/editor.ts`) — binary files that
  `openFile` opens as a view tab without reading text (the image editor).
- `registerFileSystem({ prefix: "scratch://", read, write, icon, breadcrumbs })` — files that
  live elsewhere (scratches, read-only `library://` files: PHP stubs and files outside the
  project, see `features/codeintel/library.ts`).

## Editor groups and the Find tool window

- Split editors keep one document per file (`state/editor.ts`, `editor.tabs`) and the tab
  layout as plain data (`state/tabGroups.ts`: groups, pinned tabs, tab limit, closing
  policy; tested in `tabGroups.test.ts`). Use `activeGroupId()`, `groupView(group)`,
  `viewsOf(path)` and `openFileAt(path, line, column)`; never assume a single editor view.
- `openInFindWindow(answer, { kind, title, header, rerun })` (`features/codeintel/usages.ts`)
  pins results in the Find tool window (`Alt+3`): usages (`kind: "usages"`) or text matches
  (`kind: "text"`, see `features/find/state.ts`). Next/Previous Occurrence walk the active tab.

## Notifications

`showBalloon(text, detail?, ms?, actions?, { group?, type?, log? })` (`@/keymap`) shows a
balloon and logs it in the Notifications tool window (`state/notifications.ts`). Give a
feature's balloons a group so users can make them sticky or log-only (Settings | Appearance &
Behavior | Notifications, or the tool window's context menu):

```ts
const showBalloon = balloonsFor("Database"); // registers the group
showBalloon("Database found in .env", "MySQL · shop@localhost", 0, [{ label: "Add data source", run }]);
```

`type` is `info` (default), `warning` or `error`; `log: false` is for transient hints
("Path copied") that do not belong in the history.

## Project awareness

Use `projectInfo()`, `usesFramework("laravel")` and `isPluginActive("forge.laravel")` from
`src/state/project.ts` to activate a feature only when the project uses it, and
`isRemoteProject()` / `workspace()?.remote` from `state/workspace.ts` to name the host in the UI
(features themselves work remotely through the project backend).
