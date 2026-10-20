//! Markdown as a safe AST (`forge-markdown`) for chat messages and documentation popups.

use forge_markdown::{Block, Flavor, HoverDoc};

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
