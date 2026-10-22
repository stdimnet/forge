---
title: forge-vim
description: "Vim emulation (IdeaVim-style): modes, motions, operators, text objects, registers, ex commands and ~/.ideavimrc parsing (pure, built to WebAssembly)."
order: 255
tags: [crate, rust, vim]
---

# forge-vim

`crates/forge-vim` — Vim emulation (IdeaVim-style): modes, motions, operators, text objects, registers, ex commands and ~/.ideavimrc parsing (pure, built to WebAssembly).

## Purpose

The engine behind [Vim emulation](../../user/vim-emulation.md). It is pure Rust so it compiles
to WebAssembly: the editor calls it synchronously for every key press (a keydown handler
must decide on `preventDefault` before it returns), in the desktop app and in browser mode.

- `Vim` holds the global state (options, registers, mappings, macros, search, the
  shortcut handlers) and one buffer per attached editor: a copy of the document, mode,
  caret, marks and pending keys.
- The UI sends each key with a `Sync`: the document changes since the last call (batches
  of CodeMirror changes, UTF-16 offsets), the selection, the visible lines and the indent
  unit. The `Response` carries sequential edits, the new selection, the mode, the pending
  keys, the command line, a message and effects for the UI: undo/redo through the
  editor's history, IDE actions (`:action`, `gd`, `:w`), opening files, clipboard writes,
  scrolling and reloading the rc file. Effects the UI performs asynchronously pause the
  remaining keys of a mapping or macro (`resume`).
- `claims` tells the keymap resolver which keys Vim takes from the IDE keymap in the focused
  editor (plain keys outside Insert mode, `Escape`, and the `Ctrl` shortcuts whose handler is
  Vim in the current mode).

## Modules

| Module | Summary |
| --- | --- |
| `engine.rs` | The state machine: typeahead and mappings (with `timeoutlen`), Normal/Visual/Insert/Replace/command-line modes, operators, dot repeat, macros, marks, search, scrolling and responses. |
| `command.rs` | Parsing of Normal and Visual mode commands (`"a2d3w`, `ci"`, `gUU`, `<C-w>v`). |
| `motions.rs` | Word, paragraph, bracket and character motions. |
| `textobj.rs` | `iw aw iW aW`, quotes, brackets (with counts), tags and paragraphs. |
| `ex.rs` | Ex commands with ranges: `:w :q :e :s :g :v :normal :d :y :j :> :< :m :t :sort :set :map :action :registers :marks …`. |
| `rc.rs` | `~/.ideavimrc`: `set`, mappings, `let mapleader`, `sethandler`, `if has('ide')`, `source` inlining, problem reports. |
| `handlers.rs` | The shortcut conflict table (Vim or IDE per `Ctrl` shortcut and mode) and `sethandler`. |
| `keys.rs` | Vim key notation (`<C-r>`, `<leader>`, `<Action>(Id)`) and DOM key events. |
| `pattern.rs` | Vim regular expressions translated to a syntax shared by Rust's `regex` and JavaScript (search highlights), and `:s` replacement strings. |
| `registers.rs` | Unnamed, numbered, small delete, named (append), black hole, read-only and clipboard registers. |
| `options.rs` | `:set` options with Vim's abbreviations and `no…`/`inv…`/`…!`/`…?`/`…+=`. |
| `buffer.rs` | The document copy: lines, character classes, UTF-16 conversion. |

## Main public items

```rust
pub use engine::{Change, Effect, Handle, Response, SelRange, Sync, Viewport, Vim};
pub use handlers::{HandlerOverride, HandlerRow};
pub use keys::DomKey;
pub use rc::RcReport;
```

## Used by

- Browser mode and the desktop UI: `forge-wasm` (`vim.rs`: `vim_attach`, `vim_key`,
  `vim_timeout`, `vim_configure`, `vim_load_rc`, `vim_handlers`, …); `keymap_handle_key` asks
  `claims` before resolving a key in the editor
- IPC: `vim_read_rc` (`app/src-tauri/src/commands/vim.rs`, inlines `source` lines with
  `rc::expand_sources`)
- UI: `app/ui/src/features/vim/`

## Dependencies

- Workspace crates: none
- External: `regex`, `serde`, `serde_json`

## Tests

```sh
cargo test -p forge-vim
```

Unit tests per module plus behaviour tests in `tests/engine.rs` that type keys into a buffer
with the caret marked as `|` (`check("foo b|ar", "ciwqux<Esc>", "foo qu|x")`).
