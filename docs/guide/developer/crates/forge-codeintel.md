---
title: forge-codeintel
description: "Code intelligence models: workspace edits with LSP positions, Find Usages grouping, documentation helpers, file structure, navigation bar and related files, Go to Test and Create New Test (pure, WebAssembly-friendly)."
order: 35
tags: [crate, rust, core, wasm]
---

# forge-codeintel

`crates/forge-codeintel` — Code intelligence models: workspace edits with LSP positions, Find Usages grouping, documentation helpers, file structure, navigation bar and related files, Go to Test and Create New Test (pure, WebAssembly-friendly).

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
- `usage_options`: the Find Usages settings: options per kind of element
  (`UsageOptions::effective`, the dialog's checkboxes in `UsageOptions::fields`,
  `UsageTarget`), search scopes (`UsageScope`: Project Files, Open Files, Current File,
  Recently Changed Files, Directory, custom globs) and the text occurrences added on request
  (comments and strings, non-code files).
- `highlight`: Highlight Usages in File and the highlighting of the element at the caret:
  read / write access of usages, the exit points of a function on `return` / `throw` and a
  lexical fallback (the identifier in its scope, comments and strings left out).
- `docs`: the documentation actions of the View menu: External Documentation URLs, Quick
  Definition snippets, Context Info headers and Type Info from hovers and signatures.
- `structure`: the scopes of a document (classes, functions, control blocks, CSS rules,
  HTML elements, Lua blocks) for Sticky Lines, breadcrumbs and the Structure view of web
  files, the custom folding regions, the Structure view grouped by regions, and the stops of
  Next/Previous Method (`Alt+Down` / `Alt+Up`).
- `file_structure`: the File Structure popup's rows (Ctrl+F12): toggles, fuzzy speed search
  that keeps parents, the node at the caret.
- `navbar`: the Navigation Bar's segments (project, folders, file, members at the caret).
- `related`: Related Symbol (Ctrl+Alt+Home): tests, Laravel views / routes / migrations /
  factories / policies, Twig templates, component tests, stories and styles.
- `test_nav`: Go to Test (Ctrl+Shift+T) and the Create New Test dialog: test ↔ subject,
  the dialog's initial values from composer.json / package.json and the generated test.
- `hierarchy`: Type, Method and Call Hierarchy: the data model sources fill (`TypeGraph`,
  `CallNode`), the views of the Hierarchy tool window, scopes, sorting and merging.

## Modules

| Module | Summary |
| --- | --- |
| `actions.rs` | Context actions (`ContextAction`, `ActionKind`) and `order_actions`: quick fixes first, preferred ones on top, duplicates removed, "fix all" actions (`source.fixAll*`, "Fix all …") moved into the `children` of their server's quick fix (`nest_fix_all`); `bulb_for` picks the red / yellow light bulb. |
| `docs.rs` | `symbol_at` (the identifier at the caret with its qualifier, call, tag and CSS context, imported frameworks) and `external_doc_url` (php.net, Laravel API, MDN, Vue, React, Lua manual); `definition_snippet` (docblock and attributes above, body to the matching brace or Lua `end`, at most 80 lines); `context_headers` (enclosing class, function and CSS rule headers); `type_from_hover` / `type_from_signature`; `language_of`. |
| `file_structure.rs` | `file_structure_view(FileStructureRequest)` → `FileStructureView { rows, selected, hasFields, hasLambdas, hasInherited }` over Structure view nodes (JSON): "Show Fields" / "Show Inherited" / "Show Lambdas" / "Sort Alphabetically", fuzzy matching with `forge-fuzzy` keeping the parents of matches, the best match or the node at the caret (`caretLine`, `enclosing` scope headers) selected. |
| `hierarchy.rs` | The model of Type / Method / Call Hierarchy: `HierarchyItem` (id, name, kind, container, LSP position, `handle` with a server's own item), `TypeGraph { root, types: TypeEntry { item, extends, implements, method? }[], method? }` and `CallNode { item, calls: CallSite { path, range, preview }[] }`. `type_view` (Class Hierarchy: superclass chain then subtypes; Supertypes; Subtypes), `method_view` (subtypes of the topmost declaring type with `MethodMark` defined / abstract / shouldDefine / inherited, "Hide Classes Where Method Is Not Implemented"), `call_view` (one level of callers / callees), `HierarchyScope` All / Production / Test / This Class (parents of matches stay), alphabetical sorting, `merge_type_graphs` / `merge_call_nodes` (entries at the same place are one), `is_test_path`. Cycles and huge graphs are cut. |
| `navbar.rs` | `nav_bar_segments(NavBarRequest { project, path, scopes, showMembers })` → `NavSegment { kind: project / directory / file / member, label, path, scopeKind?, line?, column? }[]`; members are the class-like and function scopes around the caret. |
| `related.rs` | `related(RelatedRequest { path, text }, files, read)` → `RelatedItem { kind, group, name, path, line, column }[]` grouped in a fixed order. Text scans, no parser: PHP class ↔ `*Test.php`, Laravel `view()` / `View::make` / `Route::view` / Inertia, `routes/*.php` lines naming a controller, model ↔ migrations (`_create_<table>_table`, `$table`), factory, policy, seeder; Symfony `render()` / `#[Template]` ↔ `templates/`; JS/TS/Vue/Svelte files ↔ `*.test.*`, `*.spec.*`, `__tests__/`, `tests/`, `*.stories.*`, style sheets. The caller lists the files and reads them lazily. |
| `test_nav.rs` | `test_targets(TestNavRequest { path, text, composerPath }, files, read)` → `TestTargets { isTest, name, targets, canCreate }` (Related Symbol's test items, best folder match first); `new_test_plan` → `NewTestPlan { frameworks, framework, subject, subjectFqn, className, namespace, directory, superclass, superclasses, fileName, members }`: PHPUnit / Pest from composer.json, namespace and folder from `autoload-dev` PSR-4 rules (`forge-file-templates`), Laravel `tests/Unit` / `tests/Feature` mirroring `app/`, Symfony `App\Tests` and `WebTestCase`, public methods from `editor_structure`; Vitest / Jest from package.json and the project's test layout (colocated, `__tests__`, `tests/`, `.test` / `.spec`), exports from a line scan. `test_source(TestSourceRequest)` → `TestSource { path, text, line }`: PHPUnit class with `setUp()` / `tearDown()` and `test<Method>()` stubs, Pest `beforeEach` / `test()` blocks, Vitest / Jest `describe` / `it` with the subject imported. |
| `edit.rs` | `TextEdit`, `WorkspaceEdit` (`edit` / `create` / `rename` / `delete` changes), `apply_workspace_edit` over a `FileStore` (all or nothing), `revert_applied`, `preview_workspace_edit`, `word_range`, UTF-16 offset helpers. |
| `structure.rs` | `editor_structure(text, language)` → `EditorStructure { scopes, regions }`: braces (PHP without inline HTML and heredocs, JS/TS with template literals, CSS, C-like), tags (HTML/XML/Vue/templates, `<script>` / `<style>` bodies as code) and Lua's block keywords outside strings and comments; each `Scope` has its kind, crumb name (`User`, `orders()`, `forEach callback`, `div#app.card`), header line and column, end and parent, and whether it sticks or is a crumb. `Region`s come from `// region` / `#region` / `<!-- region -->` / `/* region */` / `-- region` / `<editor-fold desc defaultstate>` comments. `group_by_regions` nests Structure view nodes (JSON with `line`) under their regions; `method_stops` / `method_target` give Next/Previous Method's stops (class-like, function, method and CSS rule names, closing braces of classes); `STRUCTURE_LANGUAGES` lists the languages with scopes. |
| `rename.rs` | `edit_from_locations` (references → `WorkspaceEdit`, keeping `$` where the old text had it, dynamic usages skipped), `validate_identifier`, `is_class_file` / `suggest_file_rename` / `renamed_path` (PHPStorm renames `User.php` with `class User`), `edit_conflicts` (overlapping edits, renames onto existing files) and `plan_edit` (`EditPlan`: text edits of open editors, the backend's part, files to save first, tabs to reopen or close). |
| `usages.rs` | `UsageKind` (PHPStorm's usage types and their group labels), `classify_usage` from the line around a reference, `group_usages` into a `UsageTree` by usage type and file (the declaration and duplicates removed). |

## Main public items

```rust
pub use actions::{bulb_for, nest_fix_all, order_actions, ActionKind, Bulb, ContextAction};
pub use docs::{context_headers, definition_snippet, external_doc_url, symbol_at, type_from_hover, type_from_signature, … };
pub use rename::{edit_conflicts, edit_from_locations, plan_edit, suggest_file_rename, validate_identifier, EditPlan, RenameConflict, … };
pub use file_structure::{file_structure_view, FileStructureRequest, FileStructureView};
pub use hierarchy::{call_view, merge_call_nodes, merge_type_graphs, method_view, type_view, CallNode, HierarchyItem, TypeGraph, … };
pub use navbar::{nav_bar_segments, NavBarRequest, NavSegment};
pub use related::{related, RelatedItem, RelatedKind, RelatedRequest};
pub use test_nav::{
    new_test_plan, test_source, test_targets, NewTestPlan, TestFramework, TestMember, TestNavRequest,
    TestSource, TestSourceRequest, TestTargets,
};
pub use edit::{apply_text_edits, apply_workspace_edit, preview_workspace_edit, revert_applied, word_range, … };
pub use structure::{editor_structure, group_by_regions, method_stops, method_target, EditorStructure, MethodStop, Region, Scope, ScopeKind, STRUCTURE_LANGUAGES};
pub use usages::{classify_usage, group_usages, GroupRequest, UsageKind, UsageLocation, UsageTree, … };
```

## Used by

- IPC: `code_find_usages`, `code_navigate`, `code_goto_declaration`, `code_symbol_info`, `code_line_markers`, `code_prepare_rename`, `code_rename`, `code_plan_edit`, `code_apply_edit`, `code_revert_edit`, `code_context_actions`, `code_resolve_action`, `code_execute_command`, `code_external_doc`, `code_quick_definition`, `code_type_info`, `code_context_info`, `code_related`, `code_test_targets`, `code_new_test_plan`, `code_create_test` (`app/src-tauri/src/commands/codeintel.rs`), `library_read` (`commands/library.rs`)
- Crates: `forge-lsp` (workspace edits, locations), `phpforge-lang-php` (usage locations and classification), `forge-backend` (`FrameworkCall::Related`, `TestTargets`, `NewTestPlan`, run next to the files)
- UI: `app/ui/src/features/documentation` (Quick Documentation, Documentation tool window, Quick Definition, Type Info, Context Info, Error Description), `app/ui/src/features/codeintel` (Show Usages popup, Find tool window, gutter markers, library files, in-place rename, Refactoring Preview, intention popup and light bulb, refactoring undo), `app/ui/src/features/lsp/goto.ts`, `features/fileStructure` (File Structure popup), `features/navBar` (Navigation Bar), `features/related` (Related Symbol), `features/goToTest` (Go to Test, Create New Test dialog; action rows in `features/navigation/declarationChooser.ts`)
- Browser mode: `codeintel_group_usages`, `codeintel_apply_edit`, `codeintel_preview_edit`, `codeintel_order_actions`, `codeintel_context_actions`, `codeintel_rename_edit`, `codeintel_plan_edit`, `codeintel_word_range`, `codeintel_doc_url`, `codeintel_definition_snippet`, `codeintel_context_headers`, `codeintel_type_info`, `codeintel_related`, `codeintel_test_targets`, `codeintel_new_test_plan`, `codeintel_test_source` in `crates/forge-wasm/src/codeintel.rs`, used by `app/ui/src/mock/codeintelMock.ts` `documentationMock.ts` and `relatedMock.ts`, `goToTestMock.ts`; `file_structure_view` and `nav_bar_segments` (`crates/forge-wasm/src/structure.rs`) run synchronously in the desktop app too

## Dependencies

- Workspace crates: [`forge-fuzzy`](forge-fuzzy.md), [`forge-file-templates`](forge-file-templates.md)
- External: `serde`, `serde_json`

## Tests

9 source files, about 7300 lines; 59 unit tests.

```sh
cargo test -p forge-codeintel
```
