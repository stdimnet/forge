---
title: Plugin SDK
description: The public plugin SDK - how third-party Forge plugins work (sandboxed UI bundles, permissions, manifest), the design trade-offs, and what is deferred.
order: 0
tags: [plugins, sdk, third-party, sandbox, permissions, marketplace]
---

# Plugin SDK

Forge's own packs are Rust crates and SolidJS features compiled into Forge (see
[Writing a plugin or pack](../plugins-and-packs.md)). **Third-party plugins** are different:
they are installed by users at run time, from a `.forgeplugin` file or a marketplace, and
must not be able to harm the user or Forge. This section is the public SDK for them.

| Page | Covers |
| --- | --- |
| [Getting started](getting-started.md) | `pnpm plugin new`, build, pack, install, the Word Count example |
| [API reference](api-reference.md) | the manifest (`forge-plugin.json`) and the `forge` API |
| [Permissions](permissions.md) | what plugins may declare, how Forge enforces it, Safe Mode |
| [Publishing](publishing.md) | `.forgeplugin` packages, the marketplace index format, checksums and signatures |

## How plugins run

```text
 Forge webview (main window)                     Rust backend
 ┌──────────────────────────────────────┐        ┌───────────────────────────────┐
 │ plugin host (features/plugins)       │  IPC   │ commands/third_party.rs       │
 │  registers manifest contributions    │ ─────► │  plugin_host_* checked by     │
 │  ┌──────────────────────────────┐    │        │  forge_plugins::permissions   │
 │  │ <iframe sandbox=allow-scripts>│   │        │ forge_plugins::store          │
 │  │  runtime.js: `forge` global   │◄──┼──msg──►│  <data dir>/forge/plugins     │
 │  │  plugin bundle (dist/plugin.js)│  │        └───────────────────────────────┘
 │  └──────────────────────────────┘    │
 └──────────────────────────────────────┘
```

- A plugin's **UI bundle** is one JavaScript file (an esbuild IIFE that defines
  `__forgePlugin` with `activate(context)`). It runs in its own `<iframe sandbox="allow-scripts">`
  without `allow-same-origin`: an opaque origin with no access to Forge's DOM, storage, cookies
  or Tauri IPC, and a Content Security Policy of `default-src 'none'`, so it cannot fetch or
  load anything by itself.
- The only way out is `postMessage` to the host through the `forge` API. The host answers UI
  calls itself (status bar text, tool window HTML, editor text when the plugin may read it)
  and forwards everything else to Rust (`plugin_host_read_file`, `plugin_host_fetch`,
  `plugin_host_run`, `plugin_host_setting_*`), where `forge_plugins::permissions::Guard`
  checks it against the permissions the user granted on install.
- **Manifest contributions** (commands, keybindings, status items, tool windows, settings
  pages, settings, file types) are registered from `forge-plugin.json` before any plugin code
  runs, so Find Action, the Keymap settings and the Settings dialog know them. The code is
  loaded on its **activation events**.
- Rust validates manifests and packages, stores installed plugins in the user data directory
  (never in a project), keeps their enablement and granted permissions, and runs the
  marketplace client. All of it is in the crate [`forge-plugins`](../crates/forge-plugins.md).
- **No native code is ever loaded.** The manifest format has no field for it, and a
  `backend` of kind `native` does not parse.

## Trade-offs

| Choice | Why | Cost |
| --- | --- | --- |
| UI plugins as sandboxed iframes, not modules in Forge's page | a plugin cannot read Forge's state, the IPC bridge or other plugins | everything is asynchronous messages; plugins cannot add CodeMirror extensions or SolidJS components directly |
| Declarative contributions in the manifest | listed and bindable before activation; Rust validates them | dynamic contributions (computed at run time) are not possible in API 1 |
| Tool windows as HTML in a second sandboxed frame | any UI toolkit works inside it | no access to Forge's components; colors come as CSS variables |
| Permissions granted as a whole on install | one clear prompt, like browser extensions | no per-call prompts; a plugin needing more must ship an update, which asks again |
| Ids prefixed with the plugin id | plugins cannot replace built-in actions or each other's settings | slightly longer ids |

## What is deferred

This is the first version of the SDK. Not there yet:

- **Backend logic.** The manifest reserves `backend` for a WebAssembly component
  (`{ "kind": "wasm", "module": "…" }`, to run under a capability-based WASI host such as
  wasmtime with the same permissions) and for an external program speaking JSON-RPC over stdio
  (`{ "kind": "process", "command": "…" }`). Both parse and validate, but this Forge version
  does not run them; plugins that need logic outside the UI use `forge.process.run` with the
  `process` permission for now.
- **Editor extensions** (decorations, completion, inspections) and **syntax highlighting**
  for plugin languages: plugin file types get names, extensions and indentation, highlighted as
  plain text.
- **Project settings** of plugins: plugin settings are user settings.
- **A public marketplace.** Forge reads any index in the documented format; none is hosted by
  the Forge project yet, and the default URL is a placeholder.
- Per-plugin network access from tool window frames (they have none; go through `forge.http`).
