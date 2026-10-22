//! Emmet (`forge-emmet`) and color values (`forge-colors`): abbreviation expansion, Surround
//! with Emmet, edit points, Update Tag / Remove Tag, and the colors behind the editor's color
//! previews and picker. The editor runs the same functions as WebAssembly where it must
//! answer at once (the expand key, swatches after each edit).

use forge_colors::{ColorMatch, FindRequest, FormatRequest, Parsed};
use forge_emmet::{Edit, EditPointRequest, ExpandRequest, Expansion, TagRequest, WrapRequest};

/// Expand the abbreviation before the caret.
#[tauri::command]
pub fn emmet_expand(request: ExpandRequest) -> Option<Expansion> {
    forge_emmet::expand(&request)
}

/// Surround with Emmet: wrap the selection (or line) with an abbreviation.
#[tauri::command]
pub fn emmet_wrap(request: WrapRequest) -> Result<Expansion, String> {
    forge_emmet::wrap(&request)
}

/// Go to Next/Previous Edit Point.
#[tauri::command]
pub fn emmet_edit_point(request: EditPointRequest) -> Option<usize> {
    forge_emmet::edit_point(&request)
}

/// Update Tag with Emmet.
#[tauri::command]
pub fn emmet_update_tag(request: TagRequest) -> Result<Vec<Edit>, String> {
    forge_emmet::update_tag(&request)
}

/// Remove the tag around the caret, keeping its content.
#[tauri::command]
pub fn emmet_remove_tag(request: TagRequest) -> Result<Vec<Edit>, String> {
    forge_emmet::remove_tag(&request)
}

/// Colors of a document, for the gutter swatches.
#[tauri::command]
pub fn colors_find(request: FindRequest) -> Vec<ColorMatch> {
    forge_colors::find(&request)
}

/// A picked color in the notation of the text it replaces.
#[tauri::command]
pub fn colors_format(request: FormatRequest) -> String {
    forge_colors::format_color(&request)
}

/// Parse a color typed into the color picker.
#[tauri::command]
pub fn colors_parse(text: String) -> Option<Parsed> {
    forge_colors::parse_color(&text)
}
