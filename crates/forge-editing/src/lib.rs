//! Editor actions that need a little language knowledge, computed in Rust so the desktop
//! backend and the browser-mode WebAssembly build share them ("Rust computes, TypeScript
//! renders"):
//!
//! - [`postfix`]: postfix completion templates (`$user.if`, `items.forof`, `list.ipairs`) for
//!   PHP, JavaScript/TypeScript and Lua, and their expansion.
//! - [`statement`]: Complete Current Statement (`Ctrl+Shift+Enter`): missing `;`, closing
//!   parentheses and braces, Lua's `then … end`.
//! - [`hippie`]: Cyclic Expand Word (`Alt+/`): words that complete the prefix at the caret,
//!   from the current file first and then from the other open files.
//! - [`clipboard`]: the clipboard history behind Paste from History (`Ctrl+Shift+V`).
//!
//! Offsets at the API boundary are UTF-16 code units, like JavaScript string indexes and
//! CodeMirror positions.

pub mod clipboard;
pub mod hippie;
pub mod lang;
pub mod postfix;
pub mod statement;
mod text;

pub use clipboard::{ClipEntry, ClipItem, ClipboardHistory};
pub use hippie::{expand_word, ExpandRequest, ExpandResult};
pub use lang::Lang;
pub use postfix::{
    complete_postfix, templates, PostfixCompletion, PostfixItem, PostfixRequest, TemplateInfo,
};
pub use statement::{complete_statement, StatementEdit, StatementRequest};
