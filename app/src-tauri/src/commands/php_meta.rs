//! PHPStorm metadata, JetBrains attributes, include paths and source roots: Forge's PHP
//! inspections and completion (`php_inspect`, `php_complete`), the `#[Language]` places of
//! the language injections, "External Libraries" (`php_libraries`, `php_library_list`) and
//! Settings | Directories (`php_source_roots`). Everything is computed by the PHP index
//! where the project lives (`forge_backend::PhpBackend`).

use forge_core::Result;
use forge_file_templates::psr4::{self, SourceRootEntry, SourceRootSetting};
use phpforge_lang_php::complete::PhpCompletion;
use phpforge_lang_php::injections::LanguagePlace;
use phpforge_lang_php::inspections::PhpProblem;
use phpforge_lang_php::project::{LibraryEntry, LibraryRoot, PhpProjectSettings, SOURCE_ROOTS_KEY};
use serde::Serialize;
use tauri::State;

use crate::commands::library::{LibraryState, LIBRARY_PREFIX};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Include paths and source roots from the settings.
pub fn project_settings(state: &AppState) -> PhpProjectSettings {
    let settings = state.read_settings();
    PhpProjectSettings::from_settings(|k| settings.get(k).cloned())
}

/// Hand the include paths and source roots to the backend (before indexing).
pub fn configure(state: &AppState) -> Result<()> {
    let backend = state.backend()?;
    backend.php().configure(project_settings(state))
}

fn is_php(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    (lower.ends_with(".php") || lower.ends_with(".phtml") || lower.ends_with(".inc"))
        && !lower.ends_with(".blade.php")
        && !path.starts_with(LIBRARY_PREFIX)
}

/// Forge's PHP inspections of a file (UTF-16 offsets).
#[tauri::command]
pub async fn php_inspect(
    path: String,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<PhpProblem>> {
    if !is_php(&path) {
        return Ok(Vec::new());
    }
    let backend = state.backend()?;
    blocking(move || backend.php().inspect(&path, content)).await
}

/// Expected arguments / return values and array shape keys at a UTF-16 offset.
#[tauri::command]
pub async fn php_complete(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<PhpCompletion>> {
    if !is_php(&path) {
        return Ok(None);
    }
    let backend = state.backend()?;
    blocking(move || backend.php().complete(&path, offset, content)).await
}

/// Parameters marked `#[Language]`, for the language injections of PHP files.
#[tauri::command]
pub async fn php_language_places(state: State<'_, AppState>) -> Result<Vec<LanguagePlace>> {
    let backend = state.backend()?;
    blocking(move || backend.php().language_places()).await
}

/// An entry of "External Libraries"; files carry the `library://file/…` path to open.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryNode {
    pub name: String,
    pub path: String,
    pub dir: bool,
    /// What the editor opens (`library://file/<absolute path>`); files only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
}

/// The include paths ("External Libraries" in the Project view).
#[tauri::command]
pub async fn php_libraries(state: State<'_, AppState>) -> Result<Vec<LibraryRoot>> {
    configure(&state)?;
    let backend = state.backend()?;
    blocking(move || backend.php().libraries()).await
}

/// The entries of a folder inside an include path.
#[tauri::command]
pub async fn php_library_list(
    path: String,
    state: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<Vec<LibraryNode>> {
    let backend = state.backend()?;
    let entries: Vec<LibraryEntry> = blocking(move || backend.php().library_list(&path)).await?;
    Ok(entries
        .into_iter()
        .map(|e| LibraryNode {
            open: (!e.dir).then(|| lib.grant(std::path::Path::new(&e.path))),
            name: e.name,
            path: e.path,
            dir: e.dir,
        })
        .collect())
}

/// The source roots of Settings | Directories: composer.json's PSR-4 rules with the user's
/// changes (`php.sourceRoots`).
/// `overrides` are the settings dialog's unsaved changes (else the stored setting).
#[tauri::command]
pub async fn php_source_roots(
    overrides: Option<Vec<SourceRootSetting>>,
    state: State<'_, AppState>,
) -> Result<Vec<SourceRootEntry>> {
    let settings = project_settings(&state);
    let overrides: Vec<SourceRootSetting> = overrides.unwrap_or_else(|| {
        state
            .read_settings()
            .get_as::<Vec<SourceRootSetting>>(SOURCE_ROOTS_KEY)
            .unwrap_or_default()
    });
    let backend = state.backend()?;
    let json_path = settings.composer_json.clone();
    let text = blocking(move || backend.read_optional(&json_path)).await?;
    let base = text
        .map(|t| psr4::roots(&t, &psr4::composer_dir(&settings.composer_json)))
        .unwrap_or_default();
    Ok(psr4::describe_roots(&base, &overrides))
}
