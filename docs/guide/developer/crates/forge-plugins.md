---
title: forge-plugins
description: "Third-party plugins: manifest validation, .forgeplugin packages, the installed plugin store, permissions, host calls and the marketplace client."
order: 181
tags: [crate, rust, plugins, marketplace, permissions, sdk]
---

# forge-plugins

`crates/forge-plugins` — Forge third-party plugins: manifest validation, `.forgeplugin`
packages, the installed plugin store, permissions and the marketplace client.

## Purpose

Everything that decides what a third-party plugin is and may do; the UI host
(`app/ui/src/features/plugins`) only runs the sandboxed bundles. See the
[Plugin SDK](../plugins/index.md).

| Module | Summary |
| --- | --- |
| `manifest` | `PluginManifest` (`forge-plugin.json`, format 1): id, version, `engines.forge`, `main`, reserved `backend` (WASM / JSON-RPC process, never native), activation events, `Permissions`, `Contributes` (commands, keybindings, status items, tool windows, settings pages, settings, languages); `validate(forge_version)` returns every `Problem` with its JSON path |
| `package` | `Package::read`: a `.forgeplugin` zip, size and path checks, manifest validation, `main` present, SHA-256 (feature `disk`) |
| `store` | `PluginStore` in `<data dir>/forge/plugins` (`installed.json`, one folder per plugin; data in `plugin-data/<id>`): install/update with a staged swap, uninstall, enablement, granted permissions, the installed list for exported settings and the "wanted" list of imported ones; `descriptor` turns a plugin into a `PluginRegistry` entry (pack `thirdParty`) |
| `permissions` | `Guard`: `check_fs` (scopes `${project}` / `${pluginData}`, `..` and symlinks resolved), `check_network` (hosts, `*.domain`), `check_process`, `check_setting`, `check_editor`; `summary` (the install prompt, most dangerous first) and `added` (what an update adds) |
| `host` | `fetch` (granted hosts, redirects re-checked, 16 MB limit) and `run` (granted programs, no shell) (feature `net`) |
| `marketplace` | the index format 1 (`Index::parse` skips broken entries, refuses newer formats), `entries` (newest compatible version, installed version, update available, permission lines), `search`, `artifact_url`, `verify` (size, SHA-256, Ed25519 signatures from trusted keys), `find_version` and the HTTP `Client` (feature `net`) |

`check_listing` makes sure a downloaded package is the listed id, version and permissions.

Features: `disk` (packages and the store), `net` (implies `disk`; downloads and host calls,
default). `forge-wasm` builds it without features for browser mode (`plugins_marketplace`,
`plugins_permission_lines`, `plugins_validate_manifest`).

## Used by

- IPC (`app/src-tauri/src/commands/third_party.rs`): `plugins_installed`, `plugin_inspect`,
  `plugin_install_file`, `plugin_uninstall`, `plugin_bundle`, `plugin_host_read_file`,
  `plugin_host_write_file`, `plugin_host_fetch`, `plugin_host_run`,
  `plugin_host_setting_get`, `plugin_host_setting_set`, `marketplace_list`,
  `marketplace_install`, `marketplace_updates`, `plugins_wanted_dismiss`; event
  `plugins://changed`.
- `AppState::new` registers installed plugins in the `PluginRegistry`.
- Settings Sync and Export Settings write the installed list (`commands/settings_sync.rs`).

## Dependencies

- Workspace crates: `forge-plugin-api`, `forge-keymap` (shortcut validation)
- External: `serde`, `serde_json`, `semver`, `sha2`, `hex`, `base64`, `ed25519-dalek`, `url`;
  `zip`, `dirs` (feature `disk`); `reqwest` (feature `net`)

## Tests

Manifest rules and every problem field, the SDK example and template manifests, package
checks (unsafe paths, missing files, engine ranges), install/update/uninstall in a temporary
folder, imported plugin lists, file scopes with `..` and symlinks, network/process/settings
checks, the permission prompt, index parsing, search, update checks, URLs, checksums,
signatures, and downloads and host fetches from a local mock server.
