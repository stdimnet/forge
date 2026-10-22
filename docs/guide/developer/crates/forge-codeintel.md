---
title: forge-codeintel
description: "Code intelligence models: workspace edits with LSP positions, Find Usages grouping and documentation helpers (pure, WebAssembly-friendly)."
order: 35
tags: [crate, rust, core, wasm]
---

# forge-codeintel

`crates/forge-codeintel` — Code intelligence models: workspace edits with LSP positions, Find Usages grouping and documentation helpers (pure, WebAssembly-friendly).

## Purpose

Code intelligence models shared by the language server client, the language packs, the app
and the browser-mode UI. Pure Rust, so it also runs as WebAssembly.

- `edit`: text edits and workspace edits with LSP positions (0-based line, UTF-16
  character): applying them to file contents atomically (rename, quick fixes), undoing them,
  and previewing the affected lines (Refactoring Preview).
- `actions`: Show Context Actions: language server code actions and a pack's own intentions
  in PHPStorm's order, "fix all" actions nested under their quick fix, and the light bulb.
  An action may carry a `caret` (where the caret goes after its edit) and `data` (arguments
  of its UI action, which makes it a fix rather than a link); `lsp_creates` tells whether a
  language server already offers to create a symbol (Create from usage).
- `rename`: Rename (Shift+F6): the edit from references (PHP index fallback), identifier
  validation, the class file rename, conflicts, and the plan that splits an edit between open
  editors and the project backend.
- `usages`: Find Usages: the usage type of each reference (method call, import, new
  instance, ...) from its line, and the result tree grouped by usage type and file.
- `docs`: the documentation actions of the View menu: External Documentation URLs, Quick
  Definition snippets, Context Info headers and Type Info from hovers and signatures.
- `structure`: the scopes of a document (classes, functions, control blocks, CSS rules,
  HTML elements, Lua blocks) for Sticky Lines, breadcrumbs and the Structure view of web
  files, the custom folding regions, and the Structure view grouped by regions.

## Modules

| Module | Summary |
| --- | --- |
| `actions.rs` | Context actions (`ContextAction`, `ActionKind`) and `order_actions`: quick fixes first, preferred ones on top, duplicates removed, "fix all" actions (`source.fixAll*`, "Fix all …") moved into the `children` of their server's quick fix (`nest_fix_all`); `bulb_for` picks the red / yellow light bulb. |
| `docs.rs` | `symbol_at` (the identifier at the caret with its qualifier, call, tag and CSS context, imported frameworks) and `external_doc_url` (php.net, Laravel API, MDN, Vue, React, Lua manual); `definition_snippet` (docblock and attributes above, body to the matching brace or Lua `end`, at most 80 lines); `context_headers` (enclosing class, function and CSS rule headers); `type_from_hover` / `type_from_signature`; `language_of`. |
| `edit.rs` | `TextEdit`, `WorkspaceEdit` (`edit` / `create` / `rename` / `delete` changes), `apply_workspace_edit` over a `FileStore` (all or nothing), `revert_applied`, `preview_workspace_edit`, `word_range`, UTF-16 offset helpers. |
| `structure.rs` | `editor_structure(text, language)` → `EditorStructure { scopes, regions }`: braces (PHP without inline HTML and heredocs, JS/TS with template literals, CSS, C-like), tags (HTML/XML/Vue/templates, `<script>` / `<style>` bodies as code) and Lua's block keywords outside strings and comments; each `Scope` has its kind, crumb name (`User`, `orders()`, `forEach callback`, `div#app.card`), header line and column, end and parent, and whether it sticks or is a crumb. `Region`s come from `// region` / `#region` / `<!-- region -->` / `/* region */` / `-- region` / `<editor-fold desc defaultstate>` comments. `group_by_regions` nests Structure view nodes (JSON with `line`) under their regions; `STRUCTURE_LANGUAGES` lists the languages with scopes. |
| `rename.rs` | `edit_from_locations` (references → `WorkspaceEdit`, keeping `$` where the old text had it, dynamic usages skipped), `validate_identifier`, `is_class_file` / `suggest_file_rename` / `renamed_path` (PHPStorm renames `User.php` with `class User`), `edit_conflicts` (overlapping edits, renames onto existing files) and `plan_edit` (`EditPlan`: text edits of open editors, the backend's part, files to save first, tabs to reopen or close). |
| `usages.rs` | `UsageKind` (PHPStorm's usage types and their group labels), `classify_usage` from the line around a reference, `group_usages` into a `UsageTree` by usage type and file (the declaration and duplicates removed). |

## Main public items

```rust
pub use actions::{bulb_for, nest_fix_all, order_actions, ActionKind, Bulb, ContextAction};
pub use docs::{context_headers, definition_snippet, external_doc_url, symbol_at, type_from_hover, type_from_signature, … };
pub use rename::{edit_conflicts, edit_from_locations, plan_edit, suggest_file_rename, validate_identifier, EditPlan, RenameConflict, … };
pub use edit::{apply_text_edits, apply_workspace_edit, preview_workspace_edit, revert_applied, word_range, … };
pub use structure::{editor_structure, group_by_regions, EditorStructure, Region, Scope, ScopeKind, STRUCTURE_LANGUAGES};
pub use usages::{classify_usage, group_usages, GroupRequest, UsageKind, UsageLocation, UsageTree, … };
```

## Used by

- IPC: `code_find_usages`, `code_navigate`, `code_goto_declaration`, `code_symbol_info`, `code_line_markers`, `code_prepare_rename`, `code_rename`, `code_plan_edit`, `code_apply_edit`, `code_revert_edit`, `code_context_actions`, `code_resolve_action`, `code_execute_command`, `code_external_doc`, `code_quick_definition`, `code_type_info`, `code_context_info` (`app/src-tauri/src/commands/codeintel.rs`), `library_read` (`commands/library.rs`)
- Crates: `forge-lsp` (workspace edits, locations), `phpforge-lang-php` (usage locations and classification)
- UI: `app/ui/src/features/documentation` (Quick Documentation, Documentation tool window, Quick Definition, Type Info, Context Info, Error Description), `app/ui/src/features/codeintel` (Show Usages popup, Find tool window, gutter markers, library files, in-place rename, Refactoring Preview, intention popup and light bulb, refactoring undo), `app/ui/src/features/lsp/goto.ts`
- Browser mode: `codeintel_group_usages`, `codeintel_apply_edit`, `codeintel_preview_edit`, `codeintel_order_actions`, `codeintel_context_actions`, `codeintel_rename_edit`, `codeintel_plan_edit`, `codeintel_word_range`, `codeintel_doc_url`, `codeintel_definition_snippet`, `codeintel_context_headers`, `codeintel_type_info` in `crates/forge-wasm/src/codeintel.rs`, used by `app/ui/src/mock/codeintelMock.ts` and `documentationMock.ts`

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

6 source files, about 4150 lines; 35 unit tests.

```sh
cargo test -p forge-codeintel
```
