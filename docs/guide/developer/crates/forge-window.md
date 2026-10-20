---
title: forge-window
description: "Forge window chrome: desktop detection, title bar button layouts and dialog window geometry (pure, also built to WebAssembly)."
order: 260
tags: [crate, rust]
---

# forge-window

`crates/forge-window` — Forge window chrome: desktop detection, title bar button layouts and dialog window geometry (pure, also built to WebAssembly).

## Purpose

Window chrome for Forge's own (undecorated) windows.

- `desktop`: which OS and desktop environment Forge runs on, read from the target OS and
  environment variables (`XDG_CURRENT_DESKTOP`, `DESKTOP_SESSION`, …).
- `chrome`: the title bar style and button layout for the setting
  `appearance.windowControls` (`auto | windows | macos | gnome | kde | linux | native`),
  including GNOME's `button-layout` and KDE's `kwinrc` button order.
- `geometry`: where a dialog window opens (centered on its parent, or where the user left
  it) and the remembered size and position of every dialog.

The crate is pure (no file system, processes or windowing): the Tauri shell reads the
environment and files and passes them in, and the browser-mode UI runs the same code as
WebAssembly.

## Modules

| Module | Summary |
| --- | --- |
| `chrome.rs` | Title bar style and button layout for `appearance.windowControls`. |
| `desktop.rs` | Operating system and desktop environment detection. |
| `geometry.rs` | Where dialog windows open and the geometry Forge remembers for each dialog. |

## Main public items

```rust
pub use chrome::{…};
pub use desktop::{detect_desktop, Desktop, Os};
pub use geometry::{place_dialog, DialogStates, Rect, Size};
pub fn dialog_label(id: &str) -> Option<String> { … }
pub fn dialog_id(label: &str) -> Option<&str> { … }
```

## Used by

- IPC: `window_chrome`, `dialog_window_open`, `dialog_window_close`, `dialog_window_message` (`app/src-tauri/src/commands/window.rs`)
- UI: `app/ui/src/shell/window`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

4 source files, about 1046 lines; 24 unit tests.

```sh
cargo test -p forge-window
```
