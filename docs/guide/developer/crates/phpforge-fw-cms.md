---
title: phpforge-fw-cms
description: "PHPForge: Drupal, Joomla and Shopware basics (detection, Drupal hook completion, implementations and hook docs)."
order: 313
tags: [crate, rust, pack]
---

# phpforge-fw-cms

`packs/phpforge/fw-cms` — PHPForge: Drupal, Joomla and Shopware basics (detection, Drupal hook
completion, implementations and hook docs). Pure Rust, shared with the browser mode via
WebAssembly.

## Purpose

The `forge.cms` plugin is one plugin for three CMS because each part is small: detection with
version and console, and Drupal's hooks. File types and template languages come from the
plugin manifest (`.module` & co. as PHP) and the editor (Shopware's `sw_*` Twig tags).
`scan` / `scan_files` build a `Snapshot` (the detected CMS, Drupal extensions from their
`.info.yml`, `hook_…` functions of the project's `*.api.php` files); `api::answer` answers the
IDE's queries with UTF-16 offsets.

## Modules

| Module | Summary |
| --- | --- |
| `detect.rs` | Drupal (composer packages, `core/lib/Drupal.php`, a module's `.info.yml`), Joomla (`joomla.xml`, an extension manifest) and Shopware 5 / 6 (composer packages, plugin type, `engine/Shopware`) with version, kind (site or extension) and console (`vendor/bin/drush`, `drush`, `bin/console`). |
| `drupal_hooks.rs` | The bundled list of core hooks: names and parameters from Drupal's API, summaries written for Forge; placeholders (`FORM_ID`, `ENTITY_TYPE`, `HOOK`, `N`) match implementation names. |
| `drupal.rs` | The extension a file belongs to, completion of `<module>_<hook>` after `function`, the implementations of a file (gutter marks) and the hook docs (the `*.api.php` declaration, else api.drupal.org). |
| `api.rs` | `Query` (`detect`, `refresh`, `complete`, `goto`, `gutter`) and `answer`. |

## Main public items

```rust
pub struct Snapshot { pub cms: Vec<CmsInfo>, pub extensions: Vec<Extension>, pub api_hooks: Vec<(String, Location)> }
pub fn scan(root: &Path) -> Snapshot { … }
pub fn scan_files(files: &HashMap<String, String>) -> Snapshot { … }
pub fn api::answer(snap: &Snapshot, query: api::Query) -> serde_json::Value { … }
pub fn detect::detect(probe: &dyn Probe, which: &dyn Fn(&str) -> Option<String>) -> Vec<CmsInfo> { … }
pub fn drupal_hooks::hook_for(suffix: &str) -> Option<&(&str, &str, &str)> { … }
```

## Used by

- IPC: `cms_detect`, `cms_refresh`, `cms_complete`, `cms_goto`, `cms_gutter` (`app/src-tauri/src/commands/cms.rs`)
- Backend: `forge_backend::frameworks` (`FrameworkCall::Cms`)
- UI: `app/ui/src/features/cms`
- Browser mode: `crates/forge-wasm/src/wordpress.rs` (`cms_query`) over the `?project=drupal` demo

## Dependencies

- External: `serde`, `serde_json`

## Tests

```sh
cargo test -p phpforge-fw-cms
```
