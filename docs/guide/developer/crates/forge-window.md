---
title: forge-window
description: "Forge window chrome: desktop detection, title bar button layouts and dialog window geometry (pure, also built to WebAssembly)."
order: 270
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
- `light_edit`: the tabs of the LightEdit window (files opened without a project): a file
  opened again reuses its tab, closing activates the neighbour, ids `light-edit:<path>`, and
  case-insensitive paths on Windows and macOS.
- `projects`: the registry of running project windows (one Forge instance each, oldest
  first), Next / Previous Project Window with wrap-around, pruning of instances that no longer
  answer, and the `forge-window <token> info|focus` control lines between instances.
- `reveal`: the commands that show a file in the system file manager (Explorer's `/select,`,
  `open -R`, the freedesktop `FileManager1.ShowItems` D-Bus call with `xdg-open` as fallback)
  and the file manager's name for Select In….

The crate is pure (no file system, processes or windowing): the Tauri shell reads the
environment and files and passes them in, and the browser-mode UI runs the same code as
WebAssembly.

## Modules

| Module | Summary |
| --- | --- |
| `chrome.rs` | Title bar style and button layout for `appearance.windowControls`. |
| `desktop.rs` | Operating system and desktop environment detection. |
| `geometry.rs` | Where dialog windows open and the geometry Forge remembers for each dialog. |
| `light_edit.rs` | The LightEdit window's tab model (`LightEditTabs`, `LightEditSnapshot`). |
| `projects.rs` | The project window registry and the control protocol between Forge instances. |
| `reveal.rs` | Commands that reveal a file in Explorer, Finder or the Linux file manager. |

## Main public items

```rust
pub use chrome::{…};
pub use desktop::{detect_desktop, Desktop, Os};
pub use geometry::{place_dialog, DialogStates, Rect, Size};
pub use light_edit::{LightEditFile, LightEditSnapshot, LightEditTabs, LIGHT_EDIT_ID};
pub use projects::{control_line, parse_control_line, ControlRequest, ProjectWindowEntry, ProjectWindowInfo, ProjectWindows};
pub use reveal::{file_manager_name, reveal_commands, RevealCommand};
pub fn dialog_label(id: &str) -> Option<String> { … }
pub fn dialog_id(label: &str) -> Option<&str> { … }
```

## Used by

- IPC: `window_chrome`, `reveal_in_file_manager`, `dialog_window_open`, `dialog_window_close`, `dialog_window_message` (`app/src-tauri/src/commands/window.rs`); `project_windows_list`, `project_window_focus`, `project_window_cycle`, `project_window_open` (`app/src-tauri/src/commands/project_windows.rs`); `light_edit_state`, `light_edit_open`, `light_edit_close`, `light_edit_activate`, `light_edit_read`, `light_edit_write`, `light_edit_open_project` (`app/src-tauri/src/commands/light_edit.rs`)
- UI: `app/ui/src/shell/window`
- Browser mode: built into `forge-wasm` (`light_edit_apply` for the LightEdit mock)

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

6 source files, about 1400 lines; 30 unit tests.

```sh
cargo test -p forge-window
```
