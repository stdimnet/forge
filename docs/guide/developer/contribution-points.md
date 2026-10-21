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
| `registerStructureProvider` | `features/php/StructureView.tsx` | the Structure tool window for another language |
| `registerContextMenu` | `features/contextMenu/state.ts` | entries of a context menu (e.g. `"editor"`) |
| `registerProjectViewSection` | `features/project/ProjectView.tsx` | top-level nodes below the project tree |
| `registerFileSystem` | `state/editor.ts` | editor tabs for a URI prefix (`scratch://`) |
| `openCustomTab` | `state/editor.ts` | an editor tab that shows a feature view (`forge:<kind>/<id>`) |
| `registerEditorView` | `shell/editorViews.ts` | non-file editor tabs opened with `openViewTab` (consoles, table editors) |
| `registerRunConfigurationType` | `features/run/types.ts` | a Run/Debug configuration type |
| `registerDebugLauncher` | `features/run/types.ts` | a debugger for Debug (`Shift+F9`) |
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
  anchor: "bottom",          // "left" | "right" | "bottom"
  order: 30,                 // position in the stripe
  shortcut: "Alt+8",         // tooltip only; the binding comes from the keymap
  component: ServicesView,
  actions: () => <HeaderButtons />,   // optional header buttons
  headerContent: () => <Tabs />,      // optional, e.g. tabs next to the title
  when: () => usesFramework("docker"), // optional: only for matching projects
});
```

Recipe: [`add-tool-window`](../../../.claude/skills/add-tool-window/SKILL.md).

## Commands

```ts
import { registerCommand } from "@/state/commands";

registerCommand({ id: "ActivateStructureToolWindow", title: "Structure", category: "View",
  run: () => toggleToolWindow("left", "structure") });
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
