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
| `new-project-settings.toml` | [Settings for New Projects](../user/settings-and-themes.md#settings-for-new-projects): defaults of project-level settings |
| `recent-projects.json` | the Welcome screen's recent projects |
| `trusted-projects.toml` | [trusted projects and trusted locations](../user/project-trust.md) (`projects`, `locations`: local paths or `ssh://user@host:port/path`) |
| `ai.toml` | AI providers and model choices (no keys) |
| `forges.toml` | GitHub/GitLab/Forgejo accounts (no tokens) |
| `ssh.toml` | saved SSH configurations (no passwords) |
| `dataSources.toml` | global database data sources (no passwords) |
| `browser-companion.json` | paired browsers (SHA-256 hashes of their tokens only) |
| `mcp-server.json` | port and process id of the running MCP server, for `forge --mcp-stdio` (no token) |
| `dialog-windows.json` | size and position of dialog windows |
| `updates.json` | update checks: time of the last check, the skipped version and a postponed ("Later") version |
| `scratches/` | scratch files and buffers |
| `scratches.json` | per-scratch options (the data source of SQL scratches) |

## Caches — `<cache dir>/forge/`

| Path | Content |
| --- | --- |
| `php-index/<project>-<hash>.json` | PHP index cache per project |
| `db-models/<data source>-<hash>.json` | the Database tool window's model of each data source (schemas, tables, routines, roles), so the tree shows without connecting; refreshed with Refresh |
| `project-windows.json` | the running project windows (one Forge instance each) and their local control ports, for Window \| Next Project Window |
| `json-schemas/<sha256 of the URL>.json` | downloaded JSON schemas and the SchemaStore catalog (only with remote JSON schemas allowed), refreshed daily |
| `php-stubs/<version>/` | JetBrains phpstorm-stubs for Go to Declaration on PHP's own symbols (downloaded on first use, checked against a pinned digest of its files, with its `LICENSE`) |
| `packagist/p2/<vendor>/<package>.json`, `packagist/search/` | Packagist package metadata (6 hours) and searches (1 hour) for Composer completion, versions and outdated packages (only while `php.composer.packagist` is on) |
| `vulnerabilities/<project hash>/<tool>-<folder hash>.json` | the last result of each audit tool of the vulnerable dependency check, with the hash of the manifest and lock file it was made from; reused on project open while the files are unchanged and younger than `vulnerabilities.cacheHours` |
| `profiles/<project>/<time>-<name>.pstats` (+ `.json`) | cProfile snapshots of Run \| Profile (Python) for the Profiler tool window; the newest `python.profiler.snapshots` are kept |
| `pypi/json/<name>.json`, `pypi/search/` | PyPI package metadata (12 hours) and searches (1 hour) for Python Packages, requirements completion and latest versions (only while `python.packages.pypi` is on) |
| `code-review/<host>-<server>-<repository>/<number>.json` | review mode of a pull/merge request: viewed files (with a digest of their diff), pending review comments and the AI summary and findings of the last head commit |
| `php-lsp/` | php-lsp's index cache and unpacked PHP stubs (`storagePath`) |
| `jdtls/config-<version>/`, `jdtls/workspaces/<project>-<hash>/` | Eclipse JDT LS: its OSGi configuration and the workspace data of each project (never inside the project) |
| `jupyter/kernel-<key>.json` | connection files of running Jupyter kernels (ports and signing key, readable only by you); deleted when the kernel stops |
| `shell-integration/` | the bash and zsh scripts of the terminal's shell integration (also on remote hosts, in their cache dir) |
| `remote-edit/<server>/…` | temporary copies of the files opened with Edit Remote File (Remote Host tool window); removed when the tab closes |

The search index lives in `<cache dir>/Forge/search/<hash of the project root>.bin`. The
embedded browser engine (the webview) keeps its cache in `<cache dir>/dev.forge.ide` (WebView2 on
Windows: the `Cache` folders of `%LOCALAPPDATA%\dev.forge.ide\EBWebView\Default`).

Caches can be deleted at any time; Forge rebuilds them. File | [Invalidate Caches…](../user/repair-ide.md#invalidate-caches)
deletes them for you on the next start. Without a platform cache folder the PHP
index falls back to `<project>/.forge/cache/php-index.json`.

## Data — `<data dir>/forge/`

| Path | Content |
| --- | --- |
| `langservers/` | language servers installed by Forge (`node_modules/.bin`, `package.json`, `.forge-update-check.json`; `bin/` and `pypi-tools.json` for ruff and ty from PyPI) |
| `composer/composer.phar` | composer.phar downloaded from getcomposer.org with Settings \| PHP \| Composer, checked against its published SHA-256 sum |
| `runtimes/node-<version>/` | the Node.js build Forge downloaded for language servers (only when no Node.js 18+ was installed) |
| `projects/<name>-<hash>/bookmarks.json` | the bookmarks and bookmark lists of a project (`<hash>` of the project root, for remote projects of `user@host:/path`); kept out of the project |
| `projects/<name>-<hash>/local-history/` | the project's [Local History](../user/local-history.md): `changes.jsonl` (one change set per line: saves, refactorings, VCS operations, external changes, labels) and `content/` (file contents, zlib-compressed and stored once per content hash); cleaned up after the configured working days and size; kept out of the project |
| `projects/<name>-<hash>/changelists.json` | the changelists of the Commit tool window (names, comments, the active list, which files and changes belong where); kept out of the project |
| `projects/<name>-<hash>/shelf/<id>/` | shelved changelists (Git ▸ Shelve Changes): `changes.json` (name, date, branch, files) and `shelved.patch`; kept out of the project |
| `pending-maintenance.json` | work for the next start written by Invalidate Caches and Restore Default Settings; removed once it ran |
| `chat-history/<project>-<hash>/<chat>.json` | the AI chats of each project (the newest 100), see [AI](../user/ai.md) |

Restore Default Settings copies `<config dir>/forge` to `<config dir>/forge-backup-<date>-<time>`
first (the time is UTC).

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
| `database/virtual.toml` | virtual foreign keys and columns of the Database tool window | yes |
| `deployment.toml` | deployment servers, mappings, excluded paths and the default server (no passwords) | yes |
| `inspectionProfiles/*.xml` | project inspection profiles in PHPStorm's format (`Project_Default.xml` once it differs from the defaults) | yes |

Add `.forge/runConfigurations.local.toml` (and `.forge/cache/` if it appears) to your
`.gitignore`. In the project root, `http-client.env.json` and `http-client.private.env.json` are
read by the [HTTP client](../user/scratches-and-http.md#environments).

## Secrets — the OS keychain

Secrets are stored with the system keychain (Secret Service/GNOME Keyring/KWallet on Linux,
Keychain on macOS, Credential Manager on Windows), never in files:

| Service | Account | Secret |
| --- | --- | --- |
| `forge-ai` | provider id | AI provider API key |
| `forge-forges` | account id | GitHub/GitLab/Forgejo token |
| `forge-ssh` | SSH configuration id | SSH password or key passphrase |
| `forge-db` | data source id | database password |
| `forge-deploy` | `ftp://user@host:port` | FTP/FTPS deployment password |
| `forge-containers` | `registry:<server>` | Docker registry password |
| `forge-jupyter` | `server:<base URL>` | Jupyter server token |
| `forge-mcp` | `server-token` | bearer token of the MCP server |

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
