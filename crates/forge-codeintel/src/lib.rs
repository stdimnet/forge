//! Code intelligence models shared by the language server client, the language packs, the
//! app and the browser-mode UI. Pure Rust, so it also runs as WebAssembly.
//!
//! - [`edit`]: text edits and workspace edits with LSP positions (0-based line, UTF-16
//!   character): applying them to file contents atomically (rename, quick fixes), undoing
//!   them, and previewing the affected lines (Refactoring Preview).
//! - [`actions`]: Show Context Actions: language server code actions and a pack's own
//!   intentions in PHPStorm's order.
//! - [`usages`]: Find Usages: the usage type of each reference (method call, import, new
//!   instance, ...) from its line, and the result tree grouped by usage type and file.

pub mod actions;
pub mod edit;
pub mod inlay;
pub mod usages;

pub use actions::{order_actions, ActionKind, ContextAction};

pub use edit::{
    apply_text_edits, apply_workspace_edit, position_at, preview_workspace_edit, revert_applied,
    utf16_offset, word_range, AppliedFile, DocumentChange, EditError, FilePreview, FileStore,
    MemoryStore, Position, PreviewRow, Range, TextEdit, WorkspaceEdit,
};
pub use usages::{
    classify_usage, group_usages, GroupRequest, UsageFile, UsageGroup, UsageKind, UsageLocation,
    UsageRow, UsageTree,
};
