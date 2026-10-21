---
title: forge-codeintel
description: "Code intelligence models: workspace edits with LSP positions and Find Usages grouping (pure, WebAssembly-friendly)."
order: 35
tags: [crate, rust, core, wasm]
---

# forge-codeintel

`crates/forge-codeintel` — Code intelligence models: workspace edits with LSP positions and Find Usages grouping (pure, WebAssembly-friendly).

## Purpose

Code intelligence models shared by the language server client, the language packs, the app
and the browser-mode UI. Pure Rust, so it also runs as WebAssembly.

- `edit`: text edits and workspace edits with LSP positions (0-based line, UTF-16
  character): applying them to file contents atomically (rename, quick fixes), undoing them,
  and previewing the affected lines (Refactoring Preview).
- `actions`: Show Context Actions: language server code actions and a pack's own intentions
  in PHPStorm's order.
- `usages`: Find Usages: the usage type of each reference (method call, import, new
  instance, ...) from its line, and the result tree grouped by usage type and file.

## Modules

| Module | Summary |
| --- | --- |
| `actions.rs` | Context actions (`ContextAction`, `ActionKind`) and `order_actions`: quick fixes first, preferred ones on top, duplicates removed. |
| `edit.rs` | `TextEdit`, `WorkspaceEdit` (`edit` / `create` / `rename` / `delete` changes), `apply_workspace_edit` over a `FileStore` (all or nothing), `revert_applied`, `preview_workspace_edit`, `word_range`, UTF-16 offset helpers. |
| `usages.rs` | `UsageKind` (PHPStorm's usage types and their group labels), `classify_usage` from the line around a reference, `group_usages` into a `UsageTree` by usage type and file (the declaration and duplicates removed). |

## Main public items

```rust
pub use actions::{order_actions, ActionKind, ContextAction};
pub use edit::{apply_text_edits, apply_workspace_edit, preview_workspace_edit, revert_applied, word_range, … };
pub use usages::{classify_usage, group_usages, GroupRequest, UsageKind, UsageLocation, UsageTree, … };
```

## Used by

- IPC: `code_find_usages`, `code_navigate`, `code_goto_declaration`, `code_symbol_info`, `code_line_markers`, `code_prepare_rename`, `code_rename`, `code_apply_edit`, `code_revert_edit`, `code_context_actions`, `code_resolve_action`, `code_execute_command` (`app/src-tauri/src/commands/codeintel.rs`), `library_read` (`commands/library.rs`)
- Crates: `forge-lsp` (workspace edits, locations), `phpforge-lang-php` (usage locations and classification)
- UI: `app/ui/src/features/codeintel` (Show Usages popup, Find tool window, gutter markers, library files), `app/ui/src/features/lsp/goto.ts`
- Browser mode: `codeintel_group_usages`, `codeintel_apply_edit`, `codeintel_preview_edit`, `codeintel_order_actions`, `codeintel_word_range` in `crates/forge-wasm/src/codeintel.rs`, used by `app/ui/src/mock/codeintelMock.ts`

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

4 source files, about 1500 lines; 13 unit tests.

```sh
cargo test -p forge-codeintel
```
