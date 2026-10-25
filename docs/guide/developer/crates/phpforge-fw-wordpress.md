---
title: phpforge-fw-wordpress
description: "PHPForge: WordPress support (hook index, hook completion and navigation, gutter marks, hook usages, Yoda condition hints, plugin/theme detection, WP-CLI and stubs lookup)."
order: 312
tags: [crate, rust, pack]
---

# phpforge-fw-wordpress

`packs/phpforge/fw-wordpress` — PHPForge: WordPress support (hook index, hook completion and
navigation, gutter marks, hook usages, Yoda condition hints, plugin/theme detection, WP-CLI and
stubs lookup). Pure Rust without system dependencies, so the browser mode runs the same code
through WebAssembly.

## Purpose

What the IDE knows about a WordPress site, plugin or theme. `scan` (a folder) or `scan_files`
(in-memory files) builds a `Snapshot`: the detection (`WordPressInfo`) and the hook index.
`api::answer` answers the IDE's queries on it with UTF-16 offsets; the desktop app runs it
next to the project's files (`forge_backend::frameworks`, `FrameworkCall::WordPress`), the
browser mode through `forge-wasm` (`wordpress_query`).

## Modules

| Module | Summary |
| --- | --- |
| `lexer.rs` | A small, error tolerant PHP tokenizer (inline HTML, comments, strings, heredocs, operators), plus argument splitting of calls. |
| `hooks.rs` | Hook references of a file: registrations (`add_action`, `add_filter`), invocations (`do_action`, `apply_filters`, `_ref_array`, `_deprecated`), removals and checks, with callback, priority and the source line. Dynamic names (`"save_post_{$post->post_type}"`, concatenations) become patterns; `pattern_matches` matches a static name against one. Also function and method declarations. |
| `index.rs` | `HookIndex`: the project's hooks and functions (`vendor/`, `node_modules/`, `uploads/` skipped), the WordPress stubs' functions, `related` (registrations ↔ invocations), `usages` and the per-hook summary (`hooks`). Editor queries overlay the unsaved buffer. |
| `editor.rs` | Hook name completion (`hook_context` is text based, so it works while typing), Go to Declaration (hook ↔ hook, callback → function, WordPress function → stubs), gutter marks and Find Usages with the tree the Find tool window renders (`forge_codeintel::usages`). |
| `core.rs` | Frequently used WordPress core hooks with Forge's own one-line summaries, for projects without WordPress core. |
| `yoda.rs` | "Use Yoda condition" hints of the WordPress Coding Standards with the swapped comparison as quick-fix. |
| `detect.rs` | Site / Composer install / `wp-content` / plugin / theme detection, the plugin and theme headers, WP-CLI (`vendor/bin/wp`, `wp-cli.phar`, `wp` on PATH) and the installed stubs (`vendor/php-stubs/wordpress-stubs`). |
| `api.rs` | `Query` (`detect`, `refresh`, `hooks`, `complete`, `goto`, `gutter`, `usages`, `inspect`) and `answer`. |

## Main public items

```rust
pub struct Location { … }
pub struct Snapshot { pub info: Option<WordPressInfo>, pub index: HookIndex }
pub fn scan(root: &Path) -> Snapshot { … }
pub fn scan_files(files: &HashMap<String, String>) -> Snapshot { … }
pub fn api::answer(snap: &Snapshot, query: api::Query) -> serde_json::Value { … }
pub fn hooks::scan(path: &str, src: &str) -> Vec<HookRef> { … }
pub fn hooks::pattern_matches(pattern: &str, name: &str) -> bool { … }
pub fn yoda::hints(src: &str) -> Vec<YodaHint> { … }
pub fn detect::detect(probe: &dyn Probe, which: &dyn Fn(&str) -> Option<String>) -> Option<WordPressInfo> { … }
```

## Used by

- IPC: `wordpress_detect`, `wordpress_refresh`, `wordpress_hooks`, `wordpress_complete`, `wordpress_goto`, `wordpress_gutter`, `wordpress_usages`, `wordpress_inspect` (`app/src-tauri/src/commands/wordpress.rs`)
- Backend: `forge_backend::frameworks` (`FrameworkCall::WordPress`, cached snapshot)
- UI: `app/ui/src/features/wordpress`
- Browser mode: `crates/forge-wasm/src/wordpress.rs` (`wordpress_query`) over the `?project=wordpress` demo

## Dependencies

- Workspace crates: [`forge-codeintel`](forge-codeintel.md)
- External: `serde`, `serde_json`

## Tests

8 source files; unit tests for the lexer, hook extraction and patterns, the index, completion,
navigation, gutter marks, usages, Yoda hints, detection and the API.

```sh
cargo test -p phpforge-fw-wordpress
```
