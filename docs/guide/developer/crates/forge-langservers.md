---
title: forge-langservers
description: "Forge IDE: catalog, detection, configuration and installation of language servers."
order: 130
tags: [crate, rust]
---

# forge-langservers

`crates/forge-langservers` — Forge IDE: catalog, detection, configuration and installation of language servers.

## Purpose

Forge's catalog of language servers for many languages, and everything around it:

- `catalog`: the data-driven catalog (`catalog.toml`) of languages and servers with
  commands, initialization options, settings, install hints and licences,
- `detect`: finding installed servers (PATH, npm/nvm/volta, pip/pipx, cargo, go,
  dotnet, gem, Mason, Windows `.cmd` shims),
- `scan`: which languages a project uses,
- `config`: building a `forge_lsp::LspServerConfig` for a project,
- `plan`: which servers start, and the rows of the settings page,
- `install`: running an install command with streamed output,
- `managed`: installs of npm servers without prerequisites (Node.js and packages
  downloaded and verified into Forge's data directory) and their update check,
- `licence`: the Intelephense licence key in the OS keychain.

PHP servers (Intelephense, phpactor) stay in the PHP pack; the app lists them next to
these rows.

## Modules

| Module | Summary |
| --- | --- |
| `bin/forge-langservers.rs` | Pre-install npm language servers the way Forge does (`pnpm langservers` in `app/ui`): Node.js (if missing) and the packages go into Forge's data directory, so `pnpm tauri dev` and installed builds find them at once. |
| `catalog.rs` | The built-in catalog (`catalog.toml`): languages Forge detects in projects and the language servers that serve them. |
| `config.rs` | Turning a catalog entry into a `LspServerConfig` for one project: file routing, user overrides and the project specific bits some servers need (TypeScript SDK, the Vue TypeScript plugin, ESLint's workspace folder, jdtls' data directory, Lua runtime). |
| `detect.rs` | Finding installed language servers: `PATH` plus the places package managers put executables that are often missing from `PATH` in a desktop session (npm global prefix, nvm/fnm/volta, `~/.local/bin` for pip/pipx, `~/.cargo/bin`, `~/go/bin`, `~/.dotnet/tools`, gem user dirs and Neovim's Mason directory). |
| `install.rs` | Running an install command (`npm install …`, `pipx install …`) with streamed output. Commands come from the catalog only and run without a shell. |
| `licence.rs` | Intelephense licence key: kept in the OS keychain (service `forge-lsp`, account `intelephense`), never in settings files. |
| `managed/archive.rs` | Unpacking downloaded archives (`.tgz` npm tarballs, Node.js `.tar.gz` and `.zip` builds) into a directory. Entries are checked before anything is written: absolute paths, `..` components and symlinks pointing outside the destination are refused. |
| `managed/javadebug.rs` | Microsoft's java-debug plugin for JavaForge: the newest release named by Maven Central's `maven-metadata.xml`, checked against the repository's `.sha256` (else `.sha1`), into `<data dir>/forge/langservers/java-debug`. |
| `managed/kotlin.rs` | KotlinForge's downloads, pinned with their SHA-256: JetBrains' kotlin-lsp standalone build for the platform from download.jetbrains.com (`kotlin_lsp_release`, unpacked into `<data dir>/forge/langservers/kotlin-lsp/<version>`, `inspect_kotlin_lsp` / `installed_kotlin_lsp` find `bin/intellij-server`) and the ktlint / detekt `*-all.jar`s from Maven Central (`KTLINT`, `DETEKT`, `install_jar` into `<data dir>/forge/langservers/jars`). kotlin-lsp and kotlin-language-server are pack servers (plugin `forge.kotlin`); so is Metals (plugin `forge.scala`, installed with the user's Coursier by `commands/scala.rs`). |
| `managed/jdtls.rs` | Eclipse JDT LS for JavaForge: the newest milestone (or snapshot) from download.eclipse.org named by `latest.txt`, checked against its `.sha256`, unpacked into `<data dir>/forge/langservers/jdtls/<version>`; `inspect` / `installed` find the equinox launcher and `required_java` reads the Java release the bundles need (21, 25 for newer builds) from their manifests. JDT LS is a pack server (`pack_servers.toml`, plugin `forge.java`). |
| `managed/http.rs` | A small HTTP client for downloads with progress (Node.js builds, npm metadata and tarballs). The system proxy settings (`HTTPS_PROXY`, …) are honoured. |
| `managed/mod.rs` | Zero-prerequisite installs of npm language servers (Intelephense, typescript-language-server, vscode-langservers-extracted, yaml-language-server, …) on the user's machine: |
| `managed/node.rs` | A Node.js runtime for npm language servers: a suitable one on the machine (>= 18), or the official build from nodejs.org (or a mirror) downloaded into `<data dir>/forge/runtimes/node-<version>`, verified against the release's `SHASUMS256.txt`. |
| `managed/npm.rs` | npm packages without npm: registry metadata, tarball download with `dist.integrity` (sha512) verification, unpacking into `<managed dir>/node_modules/<name>` and the `node_modules/.bin` entries detection looks for. |
| `managed/stubs.rs` | PHP stubs for Go to Declaration on PHP's own symbols: a pinned JetBrains phpstorm-stubs release downloaded on first use into `<cache dir>/forge/php-stubs/<version>`, unpacked and checked against a pinned `content_digest` of its files (reproducible from `git archive <commit>`; see the ignored `print_content_digest` test), with its `LICENSE`; `intelephense_stubs` finds Intelephense's own `lib/stub`. |
| `managed/testserver.rs` | A tiny HTTP/1.1 server on localhost for tests: fixed bodies per path, request counts. |
| `managed/updates.rs` | "A newer version is available": at most one registry check a day, remembered in `<managed dir>/.forge-update-check.json`. |
| `plan.rs` | Which servers to start for a project, and the rows of the Language Servers settings page. |
| `prefs.rs` | User preferences for language servers, read from Forge settings. |
| `scan.rs` | Quick scan of a project: which catalog languages occur (by file name, honouring `.gitignore`, capped), which root files exist and which npm packages it depends on. |

## Main public items

```rust
pub use catalog::{Catalog, InstallOption, Language, ServerSpec};
pub use config::{build_config, merge_json, ConfigContext};
pub use detect::{DetectEnv, Detector};
pub use plan::{plan, rows, Decision, LiveState, RowStatus, ServerRow, Skip};
pub use prefs::{server_key, Prefs, ServerPrefs};
pub use scan::{scan_project, ProjectScan};
```

## Used by

- IPC: `langservers_list`, `langservers_apply`, `langservers_install`, `langservers_check_updates`, `langservers_update`, `langservers_cancel_install`, `lsp_workspace_symbols`, `intelephense_licence_status`, `intelephense_set_licence` (`app/src-tauri/src/commands/langservers.rs`)
- UI: `app/ui/src/features/langservers`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-lsp`](forge-lsp.md)
- External: `base64`, `dirs`, `flate2`, `ignore`, `keyring`, `reqwest`, `serde`, `serde_json`, `sha2`, `tar`, `tokio`, `toml`, `tracing`, `zip`
- Features: `default` = `keychain`; `keychain` = `dep:keyring`

## Tests

17 source files, about 5313 lines; 54 unit tests.

```sh
cargo test -p forge-langservers
```
