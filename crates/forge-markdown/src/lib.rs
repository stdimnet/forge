//! Markdown as a safe AST for the UI, which renders it with Solid components (never as
//! HTML). Raw HTML is never passed through: in chat messages it stays literal text, in
//! documentation `<br>`/`<p>` become line breaks and every other tag is dropped.
//!
//! - [`parse`]: Markdown into [`Block`]s ([`Flavor::Chat`] or [`Flavor::Docs`]).
//! - [`hover`]: language server hover Markdown (Intelephense & co.) split into the parts of
//!   a PHPStorm-style documentation popup.

pub mod ast;
pub mod hover;
mod parse;

pub use ast::{Block, Inline, ListItem};
pub use hover::{parse_hover, HoverDoc, HoverSection};
pub use parse::{parse, parse_inline, Flavor};
