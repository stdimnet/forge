---
title: forge-keymap
description: "PHPStorm-compatible keymap: shortcut model, default keymaps, overrides, conflicts and the key resolver."
order: 110
tags: [crate, rust]
---

# forge-keymap

`crates/forge-keymap` — PHPStorm-compatible keymap: shortcut model, default keymaps, overrides, conflicts and the key resolver.

## Purpose

Forge's PHPStorm-compatible keymap engine.

- `shortcut`: parsing ("Ctrl+K Ctrl+C", "Shift Shift", "Cmd+,"), canonical ids, display
  strings ("Ctrl+Shift+A" / "⇧⌘A") and key names of DOM keyboard events.
- `catalog`: PHPStorm's default keymaps (Windows/Linux and macOS) and action aliases.
- `engine`: bindings, conflict detection and the `Resolver` state machine that turns
  key events into actions (chords, double taps, focus contexts).
- `keymap`: the `KeymapService` combining the catalog, plugin actions, registered
  commands and user overrides from the settings.

The crate is pure (no file system or async runtime): the desktop app wraps it in IPC
commands, and browser mode runs the same code as WebAssembly (`crates/forge-wasm`).

## Modules

| Module | Summary |
| --- | --- |
| `catalog.rs` | PHPStorm's default keymaps as data: "Windows/Linux" (`$default`) and "macOS". |
| `engine.rs` | Bindings, conflict detection and the resolver state machine that turns key events into action ids. |
| `keymap.rs` | The keymap as the IDE sees it: the PHPStorm catalog, actions contributed by plugins and commands, the user's overrides from the settings, and the resolver built from all of it. |
| `shortcut.rs` | Keyboard shortcut model: parsing of PHPStorm-style strings, formatting for display (Windows/Linux text or macOS symbols) and conversion from DOM keyboard events. |

## Main public items

```rust
pub use catalog::{catalog, ActionContext, ActionDef, KeymapId};
pub use engine::{FocusContext, KeyInput, Pending, Resolution, Resolver};
pub use keymap::{…};
pub use shortcut::{KeyEvent, Platform, Shortcut};
```

## Used by

- IPC: `keymap_actions`, `keymap_register_actions`, `keymap_set_override`, `keymap_conflicts`, `keymap_find_by_shortcut`, `keymap_resolve`, `keymap_import_legacy` (`app/src-tauri/src/commands/keymap.rs`)
- UI: `app/ui/src/keymap/`, `app/ui/src/features/keymapSettings`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

5 source files, about 3205 lines; 71 unit tests.

```sh
cargo test -p forge-keymap
```
