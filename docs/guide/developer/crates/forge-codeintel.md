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
  in PHPStorm's order, "fix all" actions nested under their quick fix, and the light bulb.
- `rename`: Rename (Shift+F6): the edit from references (PHP index fallback), identifier
  validation, the class file rename, conflicts, and the plan that splits an edit between open
  editors and the project backend.
- `usages`: Find Usages: the usage type of each reference (method call, import, new
  instance, ...) from its line, and the result tree grouped by usage type and file.

## Modules

| Module | Summary |
| --- | --- |
| `actions.rs` | Context actions (`ContextAction`, `ActionKind`) and `order_actions`: quick fixes first, preferred ones on top, duplicates removed, "fix all" actions (`source.fixAll*`, "Fix all …") moved into the `children` of their server's quick fix (`nest_fix_all`); `bulb_for` picks the red / yellow light bulb. |
| `edit.rs` | `TextEdit`, `WorkspaceEdit` (`edit` / `create` / `rename` / `delete` changes), `apply_workspace_edit` over a `FileStore` (all or nothing), `revert_applied`, `preview_workspace_edit`, `word_range`, UTF-16 offset helpers. |
| `rename.rs` | `edit_from_locations` (references → `WorkspaceEdit`, keeping `$` where the old text had it, dynamic usages skipped), `validate_identifier`, `is_class_file` / `suggest_file_rename` / `renamed_path` (PHPStorm renames `User.php` with `class User`), `edit_conflicts` (overlapping edits, renames onto existing files) and `plan_edit` (`EditPlan`: text edits of open editors, the backend's part, files to save first, tabs to reopen or close). |
| `usages.rs` | `UsageKind` (PHPStorm's usage types and their group labels), `classify_usage` from the line around a reference, `group_usages` into a `UsageTree` by usage type and file (the declaration and duplicates removed). |

## Main public items

```rust
pub use actions::{bulb_for, nest_fix_all, order_actions, ActionKind, Bulb, ContextAction};
pub use rename::{edit_conflicts, edit_from_locations, plan_edit, suggest_file_rename, validate_identifier, EditPlan, RenameConflict, … };
pub use edit::{apply_text_edits, apply_workspace_edit, preview_workspace_edit, revert_applied, word_range, … };
pub use usages::{classify_usage, group_usages, GroupRequest, UsageKind, UsageLocation, UsageTree, … };
```

## Used by

- IPC: `code_find_usages`, `code_navigate`, `code_goto_declaration`, `code_symbol_info`, `code_line_markers`, `code_prepare_rename`, `code_rename`, `code_plan_edit`, `code_apply_edit`, `code_revert_edit`, `code_context_actions`, `code_resolve_action`, `code_execute_command` (`app/src-tauri/src/commands/codeintel.rs`), `library_read` (`commands/library.rs`)
- Crates: `forge-lsp` (workspace edits, locations), `phpforge-lang-php` (usage locations and classification)
- UI: `app/ui/src/features/codeintel` (Show Usages popup, Find tool window, gutter markers, library files, in-place rename, Refactoring Preview, intention popup and light bulb, refactoring undo), `app/ui/src/features/lsp/goto.ts`
- Browser mode: `codeintel_group_usages`, `codeintel_apply_edit`, `codeintel_preview_edit`, `codeintel_order_actions`, `codeintel_context_actions`, `codeintel_rename_edit`, `codeintel_plan_edit`, `codeintel_word_range` in `crates/forge-wasm/src/codeintel.rs`, used by `app/ui/src/mock/codeintelMock.ts`

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

5 source files, about 2100 lines; 19 unit tests.

```sh
cargo test -p forge-codeintel
```
