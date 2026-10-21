//! Text files in the editor (`forge-text`): reading and writing with their encoding and
//! line separator, the on-save cleanup, and the effective format of a file (indent,
//! separators, guides) from the code style settings, EditorConfig and the file's content.

use serde::Deserialize;
use tauri::State;

use forge_core::{ForgeError, Result};
use forge_text::commit::{inspect_message, MessageProblem, MessageRules};
use forge_text::editorconfig::EditorConfigFile;
use forge_text::file::{read_text, write_text, TextFile};
use forge_text::save::{SaveRequest, SaveResult};
use forge_text::{Encoding, FileFormat, FormatRequest, LineSeparator};

use super::workspace::blocking;
use crate::state::AppState;

/// Read a file, detecting its encoding unless `encoding` is given ("Reload in …").
#[tauri::command]
pub async fn fs_read_text(
    path: String,
    encoding: Option<Encoding>,
    state: State<'_, AppState>,
) -> Result<TextFile> {
    let backend = state.backend()?;
    blocking(move || {
        let bytes = backend.read_bytes(&path)?;
        Ok(read_text(&bytes, encoding))
    })
    .await
}

/// Write `text` (`\n` separators) with the file's separator and encoding.
#[tauri::command]
pub async fn fs_write_text(
    path: String,
    text: String,
    eol: Option<LineSeparator>,
    encoding: Option<Encoding>,
    bom: Option<bool>,
    state: State<'_, AppState>,
) -> Result<()> {
    let backend = state.backend()?;
    let bytes = encode_text(&text, eol, encoding, bom)?;
    blocking(move || backend.write_bytes(&path, &bytes)).await
}

fn encode_text(
    text: &str,
    eol: Option<LineSeparator>,
    encoding: Option<Encoding>,
    bom: Option<bool>,
) -> Result<Vec<u8>> {
    write_text(
        text,
        eol.unwrap_or_default(),
        encoding.unwrap_or_default(),
        bom.unwrap_or(false),
    )
    .map_err(|e| ForgeError::Other(e.to_string()))
}

/// The on-save cleanup (trailing spaces, final line break).
#[tauri::command]
pub fn text_prepare_save(request: SaveRequest) -> SaveResult {
    forge_text::prepare_save(&request)
}

/// Commit message inspections (Settings | Version Control).
#[tauri::command]
pub fn vcs_inspect_commit_message(message: String, rules: MessageRules) -> Vec<MessageProblem> {
    inspect_message(&message, &rules)
}

/// What the UI sends for [`editor_file_format`]; settings and `.editorconfig` files are
/// added here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFormatRequest {
    pub path: String,
    pub language_id: Option<String>,
    pub text: Option<String>,
    pub file_type_indent: Option<String>,
}

/// The effective format of an open file.
#[tauri::command]
pub async fn editor_file_format(
    request: FileFormatRequest,
    state: State<'_, AppState>,
) -> Result<FileFormat> {
    let settings: serde_json::Map<String, serde_json::Value> =
        state.read_settings().effective().into_iter().collect();
    let backend = state.backend().ok();
    let path = request.path.clone();
    let editorconfigs = blocking(move || {
        let Some(backend) = backend else {
            return Ok(Vec::new());
        };
        let mut files = Vec::new();
        for dir in folders_of(&path) {
            let file = if dir.is_empty() {
                ".editorconfig".to_string()
            } else {
                format!("{dir}/.editorconfig")
            };
            if let Ok(Some(content)) = backend.read_optional(&file) {
                files.push(EditorConfigFile { dir, content });
            }
        }
        Ok(files)
    })
    .await?;
    Ok(forge_text::resolve_format(&FormatRequest {
        path: request.path,
        language_id: request.language_id,
        text: request.text,
        file_type_indent: request.file_type_indent,
        settings,
        editorconfigs,
    }))
}

/// The project root (`""`) and every folder down to the file's folder.
fn folders_of(path: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let parts: Vec<&str> = path.split('/').collect();
    for i in 1..parts.len() {
        out.push(parts[..i].join("/"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_from_the_root() {
        assert_eq!(folders_of("a/b/c.php"), vec!["", "a", "a/b"]);
        assert_eq!(folders_of("c.php"), vec![""]);
    }

    #[test]
    fn encodes_with_separator_and_reports_unmappable_characters() {
        let bytes = encode_text("a\nb", Some(LineSeparator::Crlf), None, Some(true)).unwrap();
        assert_eq!(bytes, b"\xEF\xBB\xBFa\r\nb");
        let err = encode_text("\u{4e2d}", None, Some(Encoding::Iso8859_1), None).unwrap_err();
        assert!(err.to_string().contains("ISO-8859-1"));
    }
}
