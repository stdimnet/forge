---
title: Plugin permissions
description: What third-party plugins may declare (files, network, programs, settings, editor), how the install prompt shows it, how Forge enforces each call, and how Safe Mode limits plugins.
order: 30
tags: [plugins, sdk, permissions, security, sandbox, safe mode]
---

# Plugin permissions

A plugin declares everything it needs outside its sandbox in `permissions`; the user sees the
list before installing (most sensitive first) and grants it as a whole. An update that asks for
more marks the new entries and must be accepted again.

```json
"permissions": {
  "fs": { "read": ["${project}"], "write": ["${pluginData}"] },
  "network": ["api.acme.dev", "*.cdn.acme.dev"],
  "process": ["git"],
  "settings": ["editor.font.*", "appearance.theme"],
  "editor": ["read", "write"]
}
```

| Permission | Values | Prompt line | Checked by |
| --- | --- | --- | --- |
| `process` | program names, no paths | Start programs (high) | `plugin_host_run` |
| `fs.write` | `${project}`, `${pluginData}`, optionally `/<sub folder>` | Write files (high in the project, low in its data folder) | `plugin_host_write_file` |
| `network` | host names; `*.example.com` covers subdomains only | Connect to the internet (medium) | `plugin_host_fetch` (also every redirect) |
| `editor: write` | | Edit the open file (medium) | the host |
| `fs.read` | as `fs.write`; writable scopes are readable | Read files (medium) | `plugin_host_read_file` |
| `settings` | keys or `prefix.*` | Read and change IDE settings (medium) | `plugin_host_setting_*` |
| `editor: read` | | Read the open file (low) | the host |

What needs no permission: the plugin's own settings (`<plugin id>.…`), its contributions,
balloons, tool window HTML, running IDE actions with `forge.commands.execute`.

## Enforcement

- The plugin's code runs in a sandboxed iframe with an opaque origin and
  `Content-Security-Policy: default-src 'none'`: it cannot reach Forge's DOM, Tauri's IPC,
  storage or the network. Every request goes through the host by `postMessage`.
- File paths must be absolute. Forge resolves `..` and symbolic links before comparing them
  with the granted folders, so a link inside an allowed folder cannot point outside of it.
- `${project}` is the open local project; without one (or for remote projects) no project
  scope applies. `${pluginData}` is `<data dir>/forge/plugin-data/<id>`, removed on uninstall.
- Network requests must be `http` or `https` to a granted host; IP addresses and other schemes
  are refused.
- Programs are found on `PATH` and run without a shell.
- Settings writes are checked against the setting's declared type, like the Settings dialog.
- Downloaded packages must match the permissions their marketplace listing showed, or the
  install is refused.

## Safe Mode

In an untrusted project ([Project trust](../../user/project-trust.md)) Forge refuses:

- loading plugins that declare `process` or a `${project}` write scope;
- `process.run` for every plugin;
- writing files inside the project.

Reading files and the other permissions still work, as reading does not run project code.

## Limits

API 1 grants permissions per plugin, not per call, and cannot narrow a scope after install
other than by uninstalling. Rust enforces file, network, program and settings access; editor
access is enforced by the host in the webview, which is the only place that has the editor.
