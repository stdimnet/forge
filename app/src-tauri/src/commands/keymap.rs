//! Keymap commands: the action catalog with shortcuts, user overrides (stored in the
//! settings), conflict checks and the key resolver. Logic lives in `forge-keymap`.

use std::sync::Mutex;

use tauri::State;

use forge_core::{Result, SettingsScope};
use forge_keymap::{
    legacy_changes, ActionRegistration, CommandInfo, KeyInput, KeymapChange, KeymapService,
    KeymapSettings, KeymapSnapshot, MainMenu, MainMenuRequest, Platform, Resolution, SettingWrite,
    ShortcutCheck,
};

use crate::state::AppState;

/// Plugin actions, registered commands and the resolver state.
pub struct KeymapState(Mutex<KeymapService>);

impl Default for KeymapState {
    fn default() -> Self {
        Self(Mutex::new(KeymapService::new(Platform::host())))
    }
}

/// Run `f` on the keymap service with the current keymap settings.
fn with_service<T>(
    keymap: &KeymapState,
    app: &AppState,
    f: impl FnOnce(&mut KeymapService) -> T,
) -> T {
    let settings = {
        let s = app.read_settings();
        KeymapSettings::from_values(|k| s.get(k).cloned())
    };
    let mut service = keymap.0.lock().expect("keymap lock poisoned");
    service.update_settings(settings);
    f(&mut service)
}

fn write_settings(app: &AppState, writes: Vec<SettingWrite>) -> Result<()> {
    let mut settings = app.write_settings();
    for w in writes {
        settings.set(&w.key, w.value, SettingsScope::User)?;
    }
    Ok(())
}

/// All actions with their shortcuts in the active keymap.
#[tauri::command]
pub fn keymap_actions(keymap: State<'_, KeymapState>, app: State<'_, AppState>) -> KeymapSnapshot {
    with_service(&keymap, &app, |s| s.snapshot())
}

/// Register plugin actions (`registerKeymapActions`) and/or replace the list of UI commands.
#[tauri::command]
pub fn keymap_register_actions(
    actions: Option<Vec<ActionRegistration>>,
    commands: Option<Vec<CommandInfo>>,
    keymap: State<'_, KeymapState>,
    app: State<'_, AppState>,
) -> KeymapSnapshot {
    with_service(&keymap, &app, |s| {
        if let Some(actions) = actions.filter(|a| !a.is_empty()) {
            s.register_actions(actions);
        }
        if let Some(commands) = commands {
            s.set_commands(commands);
        }
        s.snapshot()
    })
}

/// Select a keymap or change shortcuts; stored in the user settings. Returns the new state.
#[tauri::command]
pub fn keymap_set_override(
    change: KeymapChange,
    keymap: State<'_, KeymapState>,
    app: State<'_, AppState>,
) -> Result<KeymapSnapshot> {
    let writes = with_service(&keymap, &app, |s| s.apply(change));
    write_settings(&app, writes)?;
    Ok(with_service(&keymap, &app, |s| s.snapshot()))
}

/// Check a shortcut before assigning it to `action_id`.
#[tauri::command]
pub fn keymap_conflicts(
    shortcut: String,
    action_id: Option<String>,
    keymap: State<'_, KeymapState>,
    app: State<'_, AppState>,
) -> ShortcutCheck {
    with_service(&keymap, &app, |s| {
        s.conflicts(&shortcut, action_id.as_deref())
    })
}

/// Ids of the actions bound to `shortcut` (a single stroke also finds chords).
#[tauri::command]
pub fn keymap_find_by_shortcut(
    shortcut: String,
    keymap: State<'_, KeymapState>,
    app: State<'_, AppState>,
) -> Vec<String> {
    with_service(&keymap, &app, |s| s.find_by_shortcut(&shortcut))
}

/// The main menu (hamburger menu / menu bar) with plugin contributions and the dynamic lists
/// the UI sends; labels, shortcuts and enabled states come from the keymap.
#[tauri::command]
pub fn keymap_main_menu(
    request: MainMenuRequest,
    keymap: State<'_, KeymapState>,
    app: State<'_, AppState>,
) -> MainMenu {
    with_service(&keymap, &app, |s| s.main_menu(&request))
}

/// Feed one key event to the resolver. The UI normally resolves keys synchronously with the
/// same Rust code compiled to WebAssembly (it must decide on `preventDefault` before the
/// event returns); this command is its fallback until that module is loaded.
#[tauri::command]
pub fn keymap_resolve(
    event: KeyInput,
    keymap: State<'_, KeymapState>,
    app: State<'_, AppState>,
) -> Resolution {
    with_service(&keymap, &app, |s| s.resolve(&event))
}

/// Move a keymap saved by an earlier version in the browser storage into the settings.
/// Settings the user already has are kept. Returns the keys written.
#[tauri::command]
pub fn keymap_import_legacy(
    active: Option<String>,
    overrides: Option<String>,
    app: State<'_, AppState>,
) -> Result<Vec<String>> {
    let writes: Vec<SettingWrite> = {
        let s = app.read_settings();
        let user = s.layer_values(SettingsScope::User);
        legacy_changes(active.as_deref(), overrides.as_deref())
            .into_iter()
            .filter(|w| !user.contains_key(&w.key))
            .collect()
    };
    let keys = writes.iter().map(|w| w.key.clone()).collect();
    write_settings(&app, writes)?;
    Ok(keys)
}
