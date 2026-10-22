//! Live templates (`forge-livetemplates`): the template catalog for Settings | Editor | Live
//! Templates, expansion (Tab, Insert Live Template, completion), Surround With and
//! Unwrap/Remove. The same functions run as WebAssembly in the editor and in browser mode.

use forge_livetemplates::{
    ApplicableRequest, ApplicableResult, Catalog, ExpandRequest, Expansion, SurroundItem,
    SurroundRequest, TemplateSettings, UnwrapOption, UnwrapRequest,
};

/// Groups with the user's changes applied, and the "Applicable in" context tree.
#[tauri::command]
pub fn live_templates_catalog(settings: TemplateSettings) -> Catalog {
    forge_livetemplates::catalog(&settings)
}

/// Templates applicable at the caret (Insert Live Template, completion).
#[tauri::command]
pub fn live_templates_applicable(request: ApplicableRequest) -> ApplicableResult {
    forge_livetemplates::applicable(&request)
}

/// Expand the abbreviation before the caret or the requested template.
#[tauri::command]
pub fn live_templates_expand(mut request: ExpandRequest) -> Option<Expansion> {
    if request.env.user.is_empty() {
        request.env.user = live_templates_user();
    }
    forge_livetemplates::expand(&request)
}

/// Entries of the Surround With popup.
#[tauri::command]
pub fn live_templates_surround_items(request: SurroundRequest) -> Vec<SurroundItem> {
    forge_livetemplates::surround_items(&request)
}

/// Surround the selection with the chosen entry.
#[tauri::command]
pub fn live_templates_surround(mut request: SurroundRequest) -> Option<Expansion> {
    if request.env.user.is_empty() {
        request.env.user = live_templates_user();
    }
    forge_livetemplates::surround_apply(&request)
}

/// Entries of the Unwrap/Remove popup.
#[tauri::command]
pub fn live_templates_unwrap(request: UnwrapRequest) -> Vec<UnwrapOption> {
    forge_livetemplates::unwrap_options(&request)
}

/// Variables of a template text, for "Edit Template Variables".
#[tauri::command]
pub fn live_templates_variables(text: String) -> Vec<String> {
    forge_livetemplates::variable_names(&text)
}

/// The OS user name, for the `user()` expression.
#[tauri::command]
pub fn live_templates_user() -> String {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
}
