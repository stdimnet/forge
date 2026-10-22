//! Settings | Editor | Code Style: schemes, previews, import/export and the pass Reformat
//! Code runs after the language server. The logic lives in `forge-codestyle`; schemes are
//! stored in the user and project settings layers.

use forge_codestyle::{
    self as codestyle, CodeStyleModel, ExportFormat, ImportedScheme, Layers, PostFormatRequest,
    SchemesState, Values, WriteScope,
};
use forge_core::{ForgeError, Result, Settings, SettingsScope};
use serde_json::{Map, Value};
use tauri::State;

use crate::state::AppState;

fn layers(settings: &Settings) -> (codestyle::scheme::Layer, Option<codestyle::scheme::Layer>) {
    let user = settings.layer_values(SettingsScope::User).clone();
    let project = settings
        .has_project()
        .then(|| settings.layer_values(SettingsScope::Project).clone());
    (user, project)
}

/// The selected scheme's values.
pub(crate) fn code_style_values(state: &AppState) -> Values {
    let settings = state.read_settings();
    let (user, project) = layers(&settings);
    codestyle::selected_values(Layers {
        user: &user,
        project: project.as_ref(),
    })
}

/// The effective settings with the selected scheme's code style keys
/// (`php.codeStyle.indent`, …), for readers of those keys.
pub(crate) fn code_style_settings(state: &AppState) -> Map<String, Value> {
    let settings = state.read_settings();
    let effective: Map<String, Value> = settings.effective().into_iter().collect();
    let (user, project) = layers(&settings);
    codestyle::effective_settings(
        &effective,
        Layers {
            user: &user,
            project: project.as_ref(),
        },
    )
}

/// The options of every Code Style page.
#[tauri::command]
pub fn code_style_model() -> CodeStyleModel {
    codestyle::model().clone()
}

/// The schemes (Default, other IDE schemes, Project) and the selected one.
#[tauri::command]
pub fn code_style_schemes(state: State<'_, AppState>) -> SchemesState {
    let settings = state.read_settings();
    let (user, project) = layers(&settings);
    codestyle::load(Layers {
        user: &user,
        project: project.as_ref(),
    })
}

/// Store the schemes edited on the Code Style pages (Apply / OK).
#[tauri::command]
pub fn code_style_save(schemes: SchemesState, state: State<'_, AppState>) -> Result<SchemesState> {
    let mut settings = state.write_settings();
    let (user, project) = layers(&settings);
    let writes = codestyle::plan_save(
        Layers {
            user: &user,
            project: project.as_ref(),
        },
        &schemes,
    )
    .map_err(ForgeError::Other)?;
    settings.set_many(
        writes
            .into_iter()
            .map(|w| {
                let scope = match w.scope {
                    WriteScope::User => SettingsScope::User,
                    WriteScope::Project => SettingsScope::Project,
                };
                (w.key, w.value.unwrap_or(Value::Null), scope)
            })
            .collect(),
    )?;
    let (user, project) = layers(&settings);
    Ok(codestyle::load(Layers {
        user: &user,
        project: project.as_ref(),
    }))
}

/// The sample of a page formatted with `values`.
#[tauri::command]
pub fn code_style_preview(language: String, values: Values) -> Option<String> {
    codestyle::preview(&language, &values)
}

/// A scheme as JSON or PHPStorm XML; with `path` it is also written to that file (a file the
/// user picked in a save dialog).
#[tauri::command]
pub async fn code_style_export(
    name: String,
    values: Values,
    format: ExportFormat,
    path: Option<String>,
) -> Result<String> {
    let text = codestyle::export(&name, &values, format);
    if let Some(p) = path {
        tokio::fs::write(&p, &text)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?;
    }
    Ok(text)
}

/// Read a scheme from `text`, or from the file at `path` (picked in an open dialog).
#[tauri::command]
pub async fn code_style_import(
    text: Option<String>,
    path: Option<String>,
) -> Result<ImportedScheme> {
    let text = match (text, path) {
        (Some(t), _) => t,
        (None, Some(p)) => tokio::fs::read_to_string(&p)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?,
        (None, None) => return Err(ForgeError::Other("nothing to import".into())),
    };
    codestyle::import(&text).map_err(ForgeError::Other)
}

/// Reformat Code, after the language server: the options it does not know, blank lines,
/// indents on empty lines and the formatter markers.
#[tauri::command]
pub fn code_style_post_format(request: PostFormatRequest, state: State<'_, AppState>) -> String {
    codestyle::post_format(&request, &code_style_values(&state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_without_project() {
        let s = Settings::new();
        let (user, project) = layers(&s);
        assert!(user.is_empty());
        assert!(project.is_none());
    }
}
