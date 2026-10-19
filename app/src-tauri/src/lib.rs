mod commands;
mod state;

use state::AppState;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let app_state = AppState::default();
    // Open the project passed on the command line (`forge path/to/project`), if any.
    if let Some(path) = std::env::args().nth(1) {
        match forge_core::Workspace::open(&path) {
            Ok(ws) => app_state.set_workspace(ws),
            Err(e) => tracing::warn!("could not open {path}: {e}"),
        }
    }

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::workspace::workspace_open,
            commands::workspace::fs_list_dir,
            commands::workspace::fs_read_file,
            commands::workspace::fs_write_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}
