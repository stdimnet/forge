//! Markdown as a safe AST for the UI, which renders it with Solid components (never as
//! HTML). Raw HTML is never passed through: in chat messages it stays literal text, in
//! documentation `<br>`/`<p>` become line breaks and every other tag is dropped.
//!
//! - [`parse`]: Markdown into [`Block`]s ([`Flavor::Chat`] or [`Flavor::Docs`]).
//! - [`parse_document`]: a Markdown file for the preview (GitHub-flavored, with source lines,
//!   front matter and footnotes).
//! - [`hover`]: language server hover Markdown (Intelephense & co.) split into the parts of
//!   a PHPStorm-style documentation popup.
//! - [`doc_comments`]: PHPDoc / JSDoc / LuaDoc comments of a file in the same parts, for
//!   the editor's rendered view of documentation comments (Reader mode).

pub mod ast;
mod doc_comment;
mod document;
pub mod hover;
mod parse;

pub use ast::{Block, Inline, ListItem};
pub use doc_comment::{doc_comments, doc_style, parse_doc_comment, DocComment, DocStyle};
pub use document::{
    anchor_line, parse_document, Document, Footnote, FrontMatter, FrontMatterEntry, LineRange,
};
pub use hover::{parse_hover, HoverDoc, HoverSection};
pub use parse::{parse, parse_inline, Flavor};
