//! Code intelligence models shared by the language server client, the language packs, the
//! app and the browser-mode UI. Pure Rust, so it also runs as WebAssembly.
//!
//! - [`edit`]: text edits and workspace edits with LSP positions (0-based line, UTF-16
//!   character): applying them to file contents atomically (rename, quick fixes), undoing
//!   them, and previewing the affected lines (Refactoring Preview).
//! - [`actions`]: Show Context Actions: language server code actions and a pack's own
//!   intentions in PHPStorm's order.
//! - [`rename`]: Rename (Shift+F6): edits from references, name validation, the class file
//!   rename, conflicts, and the split between open editors and the backend.
//! - [`usages`]: Find Usages: the usage type of each reference (method call, import, new
//!   instance, ...) from its line, and the result tree grouped by usage type and file.
//! - [`usage_options`]: the Find Usages settings: options per kind of element, search
//!   scopes and text occurrences.
//! - [`highlight`]: Highlight Usages in File and the highlighting of the element at the
//!   caret: read / write access, exit points and a lexical fallback.
//! - [`docs`]: External Documentation URLs, Quick Definition snippets, Context Info headers
//!   and Type Info from hovers and signatures.
//! - [`structure`]: the scopes around a line (Sticky Lines, breadcrumbs) and custom folding
//!   regions, grouped into the Structure view.
//! - [`file_structure`]: the File Structure popup's rows (toggles, speed search, caret).
//! - [`navbar`]: the Navigation Bar's segments.
//! - [`related`]: Related Symbol: tests, views, templates, routes, migrations and more.

pub mod actions;
pub mod docs;
pub mod edit;
pub mod file_structure;
pub mod highlight;
pub mod inlay;
pub mod navbar;
pub mod related;
pub mod rename;
pub mod structure;
pub mod usage_options;
pub mod usages;

pub use actions::{
    bulb_for, lsp_creates, nest_fix_all, order_actions, ActionKind, Bulb, ContextAction,
};

pub use docs::{
    context_headers, definition_snippet, external_doc_url, language_of, symbol_at, type_from_hover,
    type_from_signature, ContextLine, DocSymbol, DocTarget, SnippetRange,
};
pub use edit::{
    apply_text_edits, apply_workspace_edit, position_at, preview_workspace_edit, revert_applied,
    utf16_offset, word_range, AppliedFile, Caret, DocumentChange, EditError, FilePreview,
    FileStore, MemoryStore, Position, PreviewRow, Range, TextEdit, WorkspaceEdit,
};
pub use file_structure::{file_structure_view, FileStructureRequest, FileStructureView};
pub use highlight::{
    access_kind, classify_ranges, exit_points, lexical_highlights, lsp_highlight_kind,
    HighlightAnswer, HighlightKind, HighlightTarget, UsageHighlight,
};
pub use navbar::{nav_bar_segments, NavBarRequest, NavSegment};
pub use related::{related, RelatedItem, RelatedKind, RelatedRequest};
pub use rename::{
    edit_conflicts, edit_from_locations, is_class_file, plan_edit, ranges_in, renamed_path,
    suggest_file_rename, validate_identifier, EditPlan, EditorEdits, FileRename, LocationEdit,
    RenameConflict,
};
pub use structure::{
    editor_structure, group_by_regions, method_stops, method_target, EditorStructure, MethodStop,
    Region, Scope, ScopeKind, STRUCTURE_LANGUAGES,
};
pub use usage_options::{
    apply_options, glob_match, is_library_path, same_language, text_occurrences, EffectiveOptions,
    ElementKind, OptionField, OptionSection, SearchScope, UsageOptions, UsageScope, OPTIONS_PREFIX,
};
pub use usages::{
    classify_usage, group_usages, GroupRequest, UsageFile, UsageGroup, UsageKind, UsageLocation,
    UsageRow, UsageTree,
};
