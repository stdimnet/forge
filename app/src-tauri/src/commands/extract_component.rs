//! Extract Component (WebForge) for Vue single-file components and React JSX
//! (`webforge_fw_vue::extract`, `webforge_fw_react::extract`): Rust analyses the selection
//! (props, imports) and computes the new file and the parent's edits; the UI shows the dialog
//! and applies the result as one undoable workspace edit, the new file included. The target
//! is checked through the project's backend, so remote projects work too.

use forge_core::Result;
use forge_lsp::forge_codeintel::{
    preview_workspace_edit, DocumentChange, Position, Range, TextEdit as LspEdit, WorkspaceEdit,
};
use webforge_lang_web::extract::{ExtractComponentInfo, ExtractedComponent};
use webforge_lang_web::text::join_path;

use crate::commands::refactor::{refuse, to_byte, to_pos, RefactorAnswer};
use crate::commands::workspace::blocking;
use crate::state::AppState;
use tauri::State;

fn is_vue(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".vue")
}

/// Extract Component's analysis of `start..end` (bytes) of `content` (the file `path`).
pub(crate) fn prepare_bytes(
    path: &str,
    content: &str,
    start: usize,
    end: usize,
) -> std::result::Result<ExtractComponentInfo, String> {
    if is_vue(path) {
        webforge_fw_vue::extract::prepare(content, start, end)
    } else {
        webforge_fw_react::extract::prepare(path, content, start, end)
    }
}

/// Whether Extract Component applies to the selection (Refactor This, Alt+Enter).
pub(crate) fn applies(path: &str, content: &str, start: Position, end: Position) -> bool {
    let (s, e) = (to_byte(content, start), to_byte(content, end));
    s != e && prepare_bytes(path, content, s.min(e), s.max(e)).is_ok()
}

/// The extraction as a workspace edit: create the component, edit the parent.
pub(crate) fn answer(path: &str, content: &str, out: ExtractedComponent) -> RefactorAnswer {
    let mut edit = WorkspaceEdit::default();
    edit.changes.push(DocumentChange::Create {
        path: out.path.clone(),
        overwrite: false,
    });
    edit.add_edits(
        out.path.clone(),
        vec![LspEdit {
            range: Range::default(),
            new_text: out.text,
        }],
    );
    edit.add_edits(
        path.to_string(),
        out.edits
            .iter()
            .map(|e| LspEdit {
                range: Range::new(to_pos(content, e.start), to_pos(content, e.end)),
                new_text: e.text.clone(),
            })
            .collect(),
    );
    let preview = preview_workspace_edit(&edit, |p| (p == path).then(|| content.to_string()));
    RefactorAnswer {
        edit,
        preview,
        conflicts: vec![],
        name_ranges: vec![],
        skipped: 0,
    }
}

/// Extract Component: what the selection of a Vue or React file would become.
#[tauri::command]
pub fn refactor_extract_component_prepare(
    path: String,
    start: Position,
    end: Position,
    content: String,
) -> Result<ExtractComponentInfo> {
    let (s, e) = (to_byte(&content, start), to_byte(&content, end));
    prepare_bytes(&path, &content, s.min(e), s.max(e)).map_err(refuse)
}

/// Extract Component into `<directory>/<name>.<extension>`.
#[tauri::command]
pub async fn refactor_extract_component(
    path: String,
    start: Position,
    end: Position,
    content: String,
    name: String,
    directory: String,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    let backend = state.backend()?;
    blocking(move || {
        let (s, e) = (to_byte(&content, start), to_byte(&content, end));
        let (s, e) = (s.min(e), s.max(e));
        let info = prepare_bytes(&path, &content, s, e).map_err(refuse)?;
        let name = name.trim();
        let new_path = join_path(
            directory.trim().trim_matches('/'),
            &format!("{name}.{}", info.extension),
        );
        if backend.read_optional(&new_path)?.is_some() {
            return Err(refuse(format!("File {new_path} already exists")));
        }
        let out = if is_vue(&path) {
            webforge_fw_vue::extract::extract(&path, &content, s, e, name, &new_path)
        } else {
            webforge_fw_react::extract::extract(&path, &content, s, e, name, &new_path)
        }
        .map_err(refuse)?;
        Ok(answer(&path, &content, out))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_becomes_one_workspace_edit() {
        let content = "<template>\n  <div>\n    <b>{{ total }}</b>\n  </div>\n</template>\n";
        let start = Position::new(2, 4);
        let end = Position::new(2, 22);
        assert!(applies("src/A.vue", content, start, end));
        assert!(!applies("src/A.vue", content, start, start));
        let (s, e) = (to_byte(content, start), to_byte(content, end));
        let out =
            webforge_fw_vue::extract::extract("src/A.vue", content, s, e, "Total", "src/Total.vue")
                .unwrap();
        let a = answer("src/A.vue", content, out);
        assert!(
            matches!(&a.edit.changes[0], DocumentChange::Create { path, .. } if path == "src/Total.vue")
        );
        assert_eq!(a.edit.changes.len(), 3);
        assert_eq!(a.preview.len(), 2);
    }
}
