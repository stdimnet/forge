//! File and code templates (`forge-file-templates`): the template catalog for Settings |
//! Editor | File and Code Templates and the New menus, PSR-4 namespace ↔ folder mapping for
//! the New PHP Class dialog, and File | New ▸ <template>. Project files are read and written
//! through the backend, so this works in remote projects too.

use serde::Serialize;
use tauri::State;

use forge_core::{ForgeError, Result};
use forge_file_templates::{
    self as templates, psr4, CustomTemplate, NewFile, NewFileRequest, TemplateInfo, VariableInfo,
};

use super::workspace::blocking;
use crate::state::AppState;

/// Everything the settings page and the New menus show.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCatalog {
    pub templates: Vec<TemplateInfo>,
    pub variables: Vec<VariableInfo>,
}

fn custom_templates(state: &AppState) -> Vec<CustomTemplate> {
    state
        .read_settings()
        .get(templates::CUSTOM_KEY)
        .map(templates::parse_custom)
        .unwrap_or_default()
}

/// The user's code templates by name, for the PHP code generators.
pub fn code_templates(state: &AppState) -> std::collections::BTreeMap<String, String> {
    templates::code_overrides(&custom_templates(state))
}

/// The catalog with the user's templates from the settings, or with `custom` (the settings
/// page shows its unapplied edits).
#[tauri::command]
pub fn templates_list(
    custom: Option<serde_json::Value>,
    state: State<'_, AppState>,
) -> TemplateCatalog {
    let custom = match custom {
        Some(value) => templates::parse_custom(&value),
        None => custom_templates(&state),
    };
    TemplateCatalog {
        templates: templates::catalog(&custom),
        variables: templates::predefined_variables(),
    }
}

/// A syntax error of a template being edited (`Line 3: Missing #end for #if`), or `None`.
#[tauri::command]
pub fn templates_validate(text: String) -> Option<String> {
    templates::validate(&text).err().map(|e| e.to_string())
}

/// The PSR-4 rules of the project's composer.json (Settings | PHP | Composer picks the file).
pub(crate) async fn psr4_roots(state: &AppState) -> Result<Vec<psr4::Psr4Root>> {
    let json_path = state
        .read_settings()
        .get_as::<String>("php.composer.json")
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "composer.json".into());
    // Mark Directory as | Sources Root changes of Settings | Directories.
    let overrides = state
        .read_settings()
        .get_as::<Vec<psr4::SourceRootSetting>>(phpforge_lang_php::project::SOURCE_ROOTS_KEY)
        .unwrap_or_default();
    let backend = state.backend()?;
    let path = json_path.clone();
    let text = blocking(move || backend.read_optional(&path)).await?;
    let base = text
        .map(|t| psr4::roots(&t, &psr4::composer_dir(&json_path)))
        .unwrap_or_default();
    Ok(psr4::apply_overrides(&base, &overrides))
}

/// The namespace of new classes in `dir`, from the PSR-4 rules.
#[tauri::command]
pub async fn templates_php_namespace(
    dir: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    let roots = psr4_roots(&state).await?;
    Ok(psr4::namespace_for_dir(&roots, &dir))
}

/// The folder for classes of `namespace`, from the PSR-4 rules.
#[tauri::command]
pub async fn templates_php_directory(
    namespace: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    let roots = psr4_roots(&state).await?;
    Ok(psr4::dir_for_namespace(&roots, &namespace))
}

/// The user's file templates and the context of new files (refactorings render the PHP
/// Class / Interface templates with them).
pub(crate) fn render_context(
    state: &AppState,
) -> Result<(Vec<CustomTemplate>, templates::TemplateContext)> {
    let backend = state.backend()?;
    let context = templates::TemplateContext {
        project_name: backend.info().name.clone(),
        user: std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_default(),
        now: None,
    };
    Ok((custom_templates(state), context))
}

/// File | New ▸ <template>: render the template and create the file (never replaces one).
#[tauri::command]
pub async fn templates_create(
    mut request: NewFileRequest,
    state: State<'_, AppState>,
    lh: State<'_, crate::commands::local_history::LocalHistoryState>,
) -> Result<NewFile> {
    let backend = state.backend()?;
    if request.context.project_name.is_empty() {
        request.context.project_name = backend.info().name.clone();
    }
    if request.context.user.is_empty() {
        request.context.user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_default();
    }
    let file = templates::new_file(&request, &custom_templates(&state))
        .map_err(|e| ForgeError::Other(e.message))?;
    let (path, text) = (file.path.clone(), file.text.clone());
    blocking(move || {
        backend.create_file(&path)?;
        backend.write_file(&path, &text)
    })
    .await?;
    crate::commands::local_history::created(
        &lh,
        &file.path,
        forge_localhistory::Content::Bytes(file.text.clone().into_bytes()),
    );
    Ok(file)
}
