//! Color scheme commands (Settings | Editor | Color Scheme): the page snapshot, changes stored
//! in the settings, and IntelliJ `.icls` import/export. Logic lives in `forge-colorscheme`.

use tauri::State;

use forge_colorscheme::{
    export_icls, import_icls, Bundled, SchemeChange, SchemeSettings, SchemesSnapshot, SettingWrite,
};
use forge_core::{ForgeError, Result, SettingsScope};

use crate::state::AppState;

fn settings_of(app: &AppState) -> SchemeSettings {
    let s = app.read_settings();
    SchemeSettings::from_values(|k| s.get(k).cloned())
}

fn write_settings(app: &AppState, writes: Vec<SettingWrite>) -> Result<()> {
    let mut settings = app.write_settings();
    for w in writes {
        settings.set(&w.key, w.value, SettingsScope::User)?;
    }
    Ok(())
}

/// The bundled scheme that follows the UI theme: `dark` and `high_contrast` describe it.
fn following(dark: bool, high_contrast: Option<bool>) -> Bundled {
    Bundled::for_theme(dark, high_contrast.unwrap_or(false))
}

/// Schemes, the attributes of the active one, the preview and the editor CSS. `dark` and
/// `high_contrast`: the kind of UI theme (the scheme that follows the theme).
#[tauri::command]
pub fn color_scheme_state(
    dark: bool,
    high_contrast: Option<bool>,
    app: State<'_, AppState>,
) -> SchemesSnapshot {
    settings_of(&app).snapshot(following(dark, high_contrast))
}

/// Select, duplicate, rename or delete a scheme, or change an attribute of the active user
/// scheme; stored in the user settings. Returns the new state.
#[tauri::command]
pub fn color_scheme_change(
    change: SchemeChange,
    dark: bool,
    high_contrast: Option<bool>,
    app: State<'_, AppState>,
) -> Result<SchemesSnapshot> {
    let theme = following(dark, high_contrast);
    let writes = settings_of(&app).apply(change, theme);
    write_settings(&app, writes)?;
    Ok(settings_of(&app).snapshot(theme))
}

/// Import an .icls file (its `text`, or the file at `path` picked in an open dialog) as a new
/// user scheme and select it.
#[tauri::command]
pub async fn color_scheme_import(
    text: Option<String>,
    path: Option<String>,
    dark: bool,
    high_contrast: Option<bool>,
    app: State<'_, AppState>,
) -> Result<SchemesSnapshot> {
    let text = match (text, path) {
        (Some(t), _) => t,
        (None, Some(p)) => tokio::fs::read_to_string(&p)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?,
        (None, None) => return Err(ForgeError::Other("nothing to import".into())),
    };
    let imported = import_icls(&text).map_err(ForgeError::Other)?;
    let writes = settings_of(&app).add_imported(imported.scheme);
    write_settings(&app, writes)?;
    Ok(settings_of(&app).snapshot(following(dark, high_contrast)))
}

/// Scheme `name` as an .icls file; with `path` it is also written there (a file the user
/// picked in a save dialog).
#[tauri::command]
pub async fn color_scheme_export(
    name: String,
    path: Option<String>,
    app: State<'_, AppState>,
) -> Result<String> {
    let text = export_icls(&settings_of(&app), &name);
    if let Some(p) = path {
        tokio::fs::write(&p, &text)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?;
    }
    Ok(text)
}
