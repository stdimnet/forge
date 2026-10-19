mod commands;
mod plugins;
mod state;

use state::AppState;
use tauri::{Emitter, Manager};

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let app_state = AppState::new();
    // Open the project passed on the command line (`forge path/to/project`), if any.
    if let Some(path) = std::env::args().nth(1) {
        match forge_core::Workspace::open(&path) {
            Ok(ws) => app_state.set_workspace(ws),
            Err(e) => tracing::warn!("could not open {path}: {e}"),
        }
    }

    tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            // Forward settings changes to the UI.
            let handle = app.handle().clone();
            app.state::<AppState>()
                .write_settings()
                .subscribe(move |change| {
                    if let Err(e) = handle.emit("settings://changed", change) {
                        tracing::warn!("cannot emit settings change: {e}");
                    }
                });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspace::workspace_open,
            commands::workspace::fs_list_dir,
            commands::workspace::fs_read_file,
            commands::workspace::fs_write_file,
            commands::settings::settings_get_all,
            commands::settings::settings_set,
            commands::settings::settings_schema,
            commands::settings::plugins_list,
            commands::settings::plugin_set_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}
