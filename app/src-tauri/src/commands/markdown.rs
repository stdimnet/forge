//! Markdown as a safe AST (`forge-markdown`) for chat messages, documentation popups and
//! the Markdown preview.

use forge_markdown::{Block, Document, Flavor, HoverDoc};

/// Parse Markdown (`chat` or `docs` flavor) into blocks the UI renders.
#[tauri::command]
pub fn markdown_parse(text: String, flavor: Option<Flavor>) -> Vec<Block> {
    forge_markdown::parse(&text, flavor.unwrap_or_default())
}

/// Split a language server hover into owner, signature, description and tag sections.
#[tauri::command]
pub fn markdown_hover(markdown: String) -> HoverDoc {
    forge_markdown::parse_hover(&markdown)
}

/// A Markdown file for the preview: GitHub-flavored blocks with their source lines, the
/// front matter and the footnotes.
#[tauri::command]
pub fn markdown_document(text: String) -> Document {
    forge_markdown::parse_document(&text)
}

/// The 0-based line of the heading with anchor `anchor` (links like `guide.md#install`).
#[tauri::command]
pub fn markdown_anchor_line(text: String, anchor: String) -> Option<u32> {
    forge_markdown::anchor_line(&text, &anchor)
}
