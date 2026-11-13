---
title: Plugin API reference
description: The forge-plugin.json manifest (format 1) - ids, engines, main, activation events, permissions, contributions - and every call of the forge API (API version 1).
order: 20
tags: [plugins, sdk, api, manifest, activation events, contributions]
---

# Plugin API reference

The types are in [`sdk/plugin-api/index.d.ts`](../../../../sdk/plugin-api/index.d.ts)
(`@forge/plugin-api`); Rust validates manifests in `forge_plugins::manifest`.

## The manifest: `forge-plugin.json`

```json
{
  "manifestVersion": 1,
  "id": "acme.todo-sync",
  "name": "TODO Sync",
  "version": "2.3.0",
  "publisher": "Acme",
  "description": "Turns TODO comments into issues.",
  "license": "MIT",
  "homepage": "https://acme.dev/todo-sync",
  "engines": { "forge": ">=0.1.0" },
  "main": "dist/plugin.js",
  "activationEvents": ["workspaceContains:.todo-sync.json", "onCommand:acme.todo-sync.push"],
  "permissions": { "network": ["api.acme.dev"], "fs": { "read": ["${project}"] } },
  "contributes": { "commands": [{ "id": "acme.todo-sync.push", "title": "Push TODOs" }] }
}
```

| Field | Rules |
| --- | --- |
| `manifestVersion` | `1` |
| `id` | lowercase reverse-DNS (`publisher.name`, letters, digits, `-`), at most 64 characters; `forge.*` is reserved |
| `name` | 1 to 64 characters |
| `version` | SemVer (`1.2.3`, `1.0.0-beta.1`) |
| `engines.forge` | SemVer range of Forge versions (`>=0.1.0`, `^1.2`); installing on another version is refused. Pre-releases of Forge count as their release (`0.1.1-beta.2` as `0.1.1`) |
| `main` | the UI bundle, a relative `.js` path inside the package |
| `backend` | reserved: `{ "kind": "wasm", "module" }` or `{ "kind": "process", "command", "args" }`; not run yet (see [Plugin SDK](index.md#what-is-deferred)) |
| `activationEvents` | when `main` is loaded, see below |
| `permissions` | see [Permissions](permissions.md) |
| `contributes` | see below |
| `l10n` | optional translations by locale: `{ "de": { "plugin.<id>.name": "…", "action.<command id>": "…", "toolwindow.<id>.title": "…", "setting.<key>.title": "…" } }`; keys and message syntax as in [Localization](../localization.md#plugin-manifests) |

Every contributed id (commands, status items, tool windows, settings pages) and setting key
must start with the plugin id and a dot: `acme.todo-sync.push`.

### Activation events

| Event | Loads the plugin |
| --- | --- |
| `onStartup` | when Forge starts (and after install) |
| `onCommand:<command id>` | the first time the command runs |
| `onView:<tool window id>` | when the tool window is opened |
| `onLanguage:<file type id>` | when a file of that type becomes the active editor (`markdown`, `php`, …) |
| `workspaceContains:<path>` | when the open project has that file (relative path) |

Commands and tool windows also load their plugin on first use without an event.

### Contributions

| Key | Entries |
| --- | --- |
| `commands` | `{ id, title, category? }`: actions in Find Action and the Keymap settings (group Plugins/<name>) |
| `keybindings` | `{ command, key, keymap? }`: default shortcuts, e.g. `Ctrl+Alt+Shift+W`; `keymap: "macos"` or `"default"` limits one to a platform keymap |
| `statusItems` | `{ id, alignment: "left" \| "right", order, command? }`: text set with `forge.statusBar.update`, clicking runs `command` |
| `toolWindows` | `{ id, title, anchor: "left" \| "right" \| "bottom", icon?, order }`: content set with `forge.toolWindows.setHtml` |
| `settingsPages` | `{ id, title }`: a page under Tools in the Settings dialog showing the plugin's settings |
| `settings` | typed settings like the built-in ones (`key`, `title`, `description`, `type` = `bool`, `int`, `number`, `string`, `enum`, `path`, `list`, `default`, `group` = one of the plugin's pages, `order`) |
| `languages` | file types: `{ id, name, extensions, fileNames, indentSize? }` |
| `iconThemes` | `{ id, label, path }`: a file icon theme, `path` = an `icon-theme.json` in a folder of the package next to its SVGs; no code and no permission needed (see [Icon themes](../icon-themes.md)) |

## The `forge` API

Inside the bundle, `forge` is a global; every call returns a promise.

### Lifecycle

```ts
export function activate(context: PluginContext): void | Promise<void>;
```

`context.plugin` is `{ id, name, version }`; push disposables to `context.subscriptions`.

### `forge.commands`

| Call | Does |
| --- | --- |
| `register(id, handler)` | implement a contributed command |
| `execute(id)` | run any action: a PHPStorm action id (`ReformatCode`) or a plugin command |

### `forge.statusBar`

`update(id, { text, tooltip? })` sets a status item; empty text hides it.

### `forge.toolWindows`

| Call | Does |
| --- | --- |
| `setHtml(id, html)` | the tool window's content, rendered in its own sandboxed frame with `--bg`, `--fg`, `--fg-muted`, `--accent` and `--border` from the current theme |
| `postMessage(id, data)` | send data to the frame's scripts (`forgeView.onMessage(cb)`) |
| `onMessage(id, callback)` | receive what the frame sends with `forgeView.postMessage(data)` |

### `forge.editor`

| Call | Permission | Does |
| --- | --- | --- |
| `getActive()` | `editor: read` | `{ path, language, text, selection: { from, to } }` or `null` |
| `onDidChange(callback)` | `editor: read` | the active editor or its text changed (debounced) |
| `replaceSelection(text)` | `editor: write` | replace the selection of the active editor (one undo step) |

### `forge.window`

`showMessage(text, type?)` shows a balloon (`info`, `warning`, `error`) in the Plugins
notification group.

### `forge.settings`

`get(key)`, `set(key, value)` (user settings, checked against the setting's type) and
`onDidChange(callback)` for the plugin's own settings, or keys granted by `permissions.settings`.

### `forge.workspace`

`root()`: the open project's folder, or `null`. `onDidChangeProject(callback)` follows the
window's project lifecycle: the callback gets `{ phase, name, root }` with `phase` `"closing"`
(the project closes: forget what you cached about it), `"opened"` (another project opened in
this window) or `"ready"` (its index is built). Sandboxed plugins are told about a close;
they cannot stop it.

```js
forge.workspace.onDidChangeProject(({ phase, root }) => {
  if (phase === "opened") refresh(root);
});
```

### `forge.fs`, `forge.http`, `forge.process`

| Call | Permission |
| --- | --- |
| `fs.readFile(path)` | `fs.read` (or `fs.write`) scope covering the absolute path |
| `fs.writeFile(path, contents)` | `fs.write` scope; in the project only outside Safe Mode |
| `http.fetch(url, { method, headers, body })` | `network` host; redirects only to granted hosts; returns `{ status, headers, body }` |
| `process.run(program, args)` | `process` program; runs in the project folder, never in Safe Mode; returns `{ exitCode, stdout, stderr }` |

Refused calls reject with a message like
``plugin `acme.todo-sync` has no permission to connect to evil.example``.
