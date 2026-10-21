//! Language injections (`forge-injections`): SQL, HTML, CSS, JSON and RegExp inside PHP and
//! JavaScript/TypeScript strings. The same functions run as WebAssembly in the editor (it
//! needs the answers at once while typing) and in browser mode.

use forge_core::Settings;
use forge_injections::{
    Edit, FindRequest, Fragment, InjectRequest, Injection, InjectionSettings, Language,
    LiteralKind, OffsetRequest, RuleInfo,
};
use forge_lsp::forge_codeintel::ContextAction;

/// Setting with the ids of the turned-off built-in rules.
pub const DISABLED_KEY: &str = "editor.injections.disabled";
/// Setting with the places turned off with "Un-inject Language" (`path#key`).
pub const UNINJECTED_KEY: &str = "editor.injections.uninjected";

/// The injection settings in effect.
pub fn settings_of(settings: &Settings) -> InjectionSettings {
    InjectionSettings {
        disabled: settings
            .get_as::<Vec<String>>(DISABLED_KEY)
            .unwrap_or_default(),
        uninjected: settings
            .get_as::<Vec<String>>(UNINJECTED_KEY)
            .unwrap_or_default(),
    }
}

/// Every injection of a file's text.
#[tauri::command]
pub fn injections_find(request: FindRequest) -> Vec<Injection> {
    forge_injections::find(&request)
}

/// The injection at an offset.
#[tauri::command]
pub fn injections_at(request: OffsetRequest) -> Option<Injection> {
    forge_injections::injection_at(&request)
}

/// The fragment of the injected literal at an offset, for Edit Fragment.
#[tauri::command]
pub fn injections_fragment(request: OffsetRequest) -> Option<Fragment> {
    forge_injections::fragment(&request)
}

/// Text typed into a fragment, escaped for its literal.
#[tauri::command]
pub fn injections_escape(kind: LiteralKind, text: String) -> String {
    forge_injections::escape(kind, &text)
}

/// The `language=` comment that injects a language into the literal at an offset.
#[tauri::command]
pub fn injections_inject(request: InjectRequest) -> Option<Edit> {
    forge_injections::inject(&request)
}

/// The built-in rules (Settings | Editor | Language Injections).
#[tauri::command]
pub fn injections_rules() -> Vec<RuleInfo> {
    forge_injections::rule_infos()
}

/// The languages that can be injected.
#[tauri::command]
pub fn injections_languages() -> Vec<Language> {
    forge_injections::languages()
}

/// The injection intentions at an offset, for Show Context Actions.
pub fn context_actions(
    path: &str,
    text: &str,
    offset: usize,
    settings: InjectionSettings,
) -> Vec<ContextAction> {
    forge_injections::context_actions(&OffsetRequest {
        text: text.to_string(),
        path: path.to_string(),
        offset,
        settings,
    })
}
