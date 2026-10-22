---
title: Config, cache and data locations
description: Where Forge stores user settings, project files, caches, language servers, scratches and secrets on Linux, macOS and Windows.
order: 60
tags: [files, paths, config, cache, keychain, reference]
---

# Config, cache and data locations

Forge uses the platform's standard folders (Rust's `dirs` crate):

| Placeholder | Linux | macOS | Windows |
| --- | --- | --- | --- |
| `<config dir>` | `$XDG_CONFIG_HOME` or `~/.config` | `~/Library/Application Support` | `%APPDATA%` (`C:\Users\<you>\AppData\Roaming`) |
| `<cache dir>` | `$XDG_CACHE_HOME` or `~/.cache` | `~/Library/Caches` | `%LOCALAPPDATA%` |
| `<data dir>` | `$XDG_DATA_HOME` or `~/.local/share` | `~/Library/Application Support` | `%APPDATA%` |
| `<state dir>` | `$XDG_STATE_HOME` or `~/.local/state` | `~/Library/Application Support` | `%LOCALAPPDATA%` |

## User configuration — `<config dir>/forge/`

| File | Content |
| --- | --- |
| `settings.toml` | user settings (including keymap changes and disabled plugins) |
| `recent-projects.json` | the Welcome screen's recent projects |
| `ai.toml` | AI providers and model choices (no keys) |
| `forges.toml` | GitHub/GitLab/Forgejo accounts (no tokens) |
| `ssh.toml` | saved SSH configurations (no passwords) |
| `dataSources.toml` | global database data sources (no passwords) |
| `browser-companion.json` | paired browsers (SHA-256 hashes of their tokens only) |
| `dialog-windows.json` | size and position of dialog windows |
| `updates.json` | update checks: time of the last check, the skipped version and a postponed ("Later") version |
| `scratches/` | scratch files and buffers |
| `scratches.json` | per-scratch options (the data source of SQL scratches) |

## Caches — `<cache dir>/forge/`

| Path | Content |
| --- | --- |
| `php-index/<project>-<hash>.json` | PHP index cache per project |
| `json-schemas/<sha256 of the URL>.json` | downloaded JSON schemas and the SchemaStore catalog (only with remote JSON schemas allowed), refreshed daily |
| `php-stubs/<version>/` | JetBrains phpstorm-stubs for Go to Declaration on PHP's own symbols (downloaded on first use, checked against a pinned digest of its files, with its `LICENSE`) |
| `packagist/p2/<vendor>/<package>.json`, `packagist/search/` | Packagist package metadata (6 hours) and searches (1 hour) for Composer completion, versions and outdated packages (only while `php.composer.packagist` is on) |
| `intelephense/` | Intelephense's storage (`storagePath`) |
| `jdtls/…` | workspace data of the Java language server |
| `shell-integration/` | the bash and zsh scripts of the terminal's shell integration (also on remote hosts, in their cache dir) |

Caches can be deleted at any time; Forge rebuilds them. Without a platform cache folder the PHP
index falls back to `<project>/.forge/cache/php-index.json`.

## Data — `<data dir>/forge/`

| Path | Content |
| --- | --- |
| `langservers/` | language servers installed by Forge (`node_modules/.bin`, `package.json`, `.forge-update-check.json`) |
| `composer/composer.phar` | composer.phar downloaded from getcomposer.org with Settings \| PHP \| Composer, checked against its published SHA-256 sum |
| `runtimes/node-<version>/` | the Node.js build Forge downloaded for language servers (only when no Node.js 18+ was installed) |
| `projects/<name>-<hash>/bookmarks.json` | the bookmarks and bookmark lists of a project (`<hash>` of the project root, for remote projects of `user@host:/path`); kept out of the project |
| `chat-history/<project>-<hash>/<chat>.json` | the AI chats of each project (the newest 100), see [AI](../user/ai.md) |

## State — `<state dir>/forge/`

| Path | Content |
| --- | --- |
| `layouts/<project>-<hash>.json` | the tool window layout (placements, open tool windows, sizes) and open editor tabs of each project |
| `layouts/_default.json` | the layout stored with Window \| Layouts \| Store Current Layout as Default |

## In the project — `<project>/.forge/`

| File | Content | Commit? |
| --- | --- | --- |
| `settings.toml` | project settings ("For current project") | yes |
| `runConfigurations.toml` | shared run configurations ("Store as project file") | yes |
| `runConfigurations.local.toml` | local run configurations, the selection, templates | no |
| `breakpoints.json` | Xdebug breakpoints | optional |
| `php-servers.json` | PHP servers and path mappings | yes |
| `dataSources.toml` | project data sources (no passwords) | yes |

Add `.forge/runConfigurations.local.toml` (and `.forge/cache/` if it appears) to your
`.gitignore`. In the project root, `http-client.env.json` and `http-client.private.env.json` are
read by the [HTTP client](../user/scratches-and-http.md#environments).

## Secrets — the OS keychain

Secrets are stored with the system keychain (Secret Service/GNOME Keyring/KWallet on Linux,
Keychain on macOS, Credential Manager on Windows), never in files:

| Service | Account | Secret |
| --- | --- | --- |
| `forge-ai` | provider id | AI provider API key |
| `forge-lsp` | `intelephense` | Intelephense licence key |
| `forge-forges` | account id | GitHub/GitLab/Forgejo token |
| `forge-ssh` | SSH configuration id | SSH password or key passphrase |
| `forge-db` | data source id | database password |
| `forge-containers` | `registry:<server>` | Docker registry password |

## Remote hosts

| Path on the host | Content |
| --- | --- |
| `~/.cache/forge-agent/<version>/` | the uploaded Forge agent |
| `~/.ssh/known_hosts` (local) | host keys you accepted |

## WebView data

The desktop app's WebView keeps its own storage in the app's data folder (identifier
`dev.forge.ide`, e.g. `~/.local/share/dev.forge.ide` on Linux). Forge keeps only conveniences
there (the last theme while settings load); everything that matters is in the files above.

## Clean slate

To reset Forge completely, quit it and delete `<config dir>/forge`, `<cache dir>/forge`,
`<data dir>/forge` and `<state dir>/forge`, and remove the `forge-*` entries from your keychain.
