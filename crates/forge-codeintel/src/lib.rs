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
//! - [`docs`]: External Documentation URLs, Quick Definition snippets, Context Info headers
//!   and Type Info from hovers and signatures.

pub mod actions;
pub mod docs;
pub mod edit;
pub mod inlay;
pub mod rename;
pub mod usages;

pub use actions::{bulb_for, nest_fix_all, order_actions, ActionKind, Bulb, ContextAction};

pub use docs::{
    context_headers, definition_snippet, external_doc_url, language_of, symbol_at, type_from_hover,
    type_from_signature, ContextLine, DocSymbol, DocTarget, SnippetRange,
};
pub use edit::{
    apply_text_edits, apply_workspace_edit, position_at, preview_workspace_edit, revert_applied,
    utf16_offset, word_range, AppliedFile, DocumentChange, EditError, FilePreview, FileStore,
    MemoryStore, Position, PreviewRow, Range, TextEdit, WorkspaceEdit,
};
pub use rename::{
    edit_conflicts, edit_from_locations, is_class_file, plan_edit, ranges_in, renamed_path,
    suggest_file_rename, validate_identifier, EditPlan, EditorEdits, FileRename, LocationEdit,
    RenameConflict,
};
pub use usages::{
    classify_usage, group_usages, GroupRequest, UsageFile, UsageGroup, UsageKind, UsageLocation,
    UsageRow, UsageTree,
};
