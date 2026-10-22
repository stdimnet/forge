---
title: phpforge-composer
description: "PHPForge: Composer support (version constraints, composer.json/lock analysis, Packagist metadata, outdated packages, audit advisories, completion); pure, also built to WebAssembly, with the Packagist client behind the `net` feature."
order: 285
tags: [crate, rust, pack, composer]
---

# phpforge-composer

`packs/phpforge/composer` — PHPForge: Composer support (version constraints, composer.json/lock analysis, Packagist metadata, outdated packages, audit advisories, completion).

## Purpose

Everything the [Composer](../../user/composer.md) UI shows is computed here. The crate is pure
and builds to WebAssembly for browser mode; the `net` feature (desktop app only) adds the
Packagist client and the composer.phar download.

- Composer's version and constraint semantics: `^`, `~`, wildcards, ranges, `||`, hyphen
  ranges, `@dev`/`@beta` stability flags, branches and aliases, the recommended constraint of
  `composer require`.
- composer.json requirements with their source ranges, installed packages from composer.lock or
  installed.json, outdated markers with Composer's statuses, `composer audit` reports placed as
  problems in composer.json and composer.lock.
- Packagist's Composer 2 metadata (minified version lists) and search results; completion and
  hover in `require` / `require-dev` built on `forge_jsonschema::complete::json_cursor`.

## Modules

| Module | Summary |
| --- | --- |
| `version.rs` | Versions (`v1.2.3`, `2.0.0-RC1`, `1.x-dev`, `dev-main`), normalized and ordered like Composer (`dev < alpha < beta < RC < stable < patch`), stability. |
| `constraint.rs` | Constraint parsing and matching (`Constraint::matches`, `allows` with a minimum stability, `latest_allowed`) and `recommended(version)` (`^2.1`, `^0.4.2`, `^3.0@beta`). |
| `lock.rs` | Installed packages from composer.lock (`packages`, `packages-dev`) and vendor/composer/installed.json (Composer 1 and 2 formats). |
| `manifest.rs` | Requirements of composer.json with byte ranges, platform packages, package name validation, lock file entries and UTF-16 / line-column offsets. |
| `packagist.rs` | Packagist p2 metadata (minified lists expanded, `__unset`), search results, URLs. |
| `outdated.rs` | Outdated markers (installed vs latest, `semver-safe-update` / `update-possible`, the constraint for the update) and `composer outdated --format=json`. |
| `audit.rs` | `composer audit --format=json` (advisories as arrays or index-keyed objects, abandoned packages) and their problems in composer.json and composer.lock. |
| `complete.rs` | Completion of package names and versions and the package hover, in two steps (`request` / `hover_target`, then `items` / `hover` with the fetched data). |
| `net.rs` (`net`) | `Packagist`: metadata and search cached in `<cache dir>/forge/packagist` with a time to live and a stale fallback offline; `download_phar` (getcomposer.org, SHA-256 checked, into `<data dir>/forge/composer`). |

## Used by

- IPC: `composer_project`, `composer_markers`, `composer_complete`, `composer_hover`,
  `composer_search`, `composer_package`, `composer_audit`, `composer_download_phar`
  (`app/src-tauri/src/commands/composer.rs`); `php_composer_outdated` (through
  phpforge-lang-php)
- UI: `app/ui/src/features/composer/`
- Browser mode: built into `forge-wasm` (`composer.rs`)

## Dependencies

- Workspace crates: forge-jsonschema
- External: `serde`, `serde_json`; with `net`: `reqwest`, `sha2`, `tokio`, `dirs`

## Tests

10 source files, about 2800 lines; 29 unit tests. Network tests talk to a local fake server
(`testserver.rs`), never to Packagist.

```sh
cargo test -p phpforge-composer --features net
```
