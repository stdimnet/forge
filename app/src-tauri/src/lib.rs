mod commands;
mod plugins;
mod recent;
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
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .manage(commands::ai::AiState::load())
        .manage(commands::agent::AgentState::default())
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
            // Reconfigure (restart) the PHP language server when its settings or the
            // enabled plugins change. Listeners run under the settings lock, so the work
            // happens on another thread.
            let handle = app.handle().clone();
            app.state::<AppState>()
                .write_settings()
                .subscribe(move |change| {
                    if phpforge_lang_php::lsp::affects_server(&change.key)
                        || change.key == plugins::DISABLED_PLUGINS_KEY
                    {
                        commands::php::spawn_sync_lsp_server(&handle);
                    }
                });
            // A project passed on the command line is already open.
            commands::php::spawn_sync_lsp_server(app.handle());
            Ok(())
        })
        .manage(commands::lsp::LspState::default())
        .manage(commands::php::PhpState::default())
        .manage(commands::project::CreateState::default())
        .invoke_handler(tauri::generate_handler![
            commands::workspace::workspace_open,
            commands::workspace::workspace_current,
            commands::workspace::workspace_recent,
            commands::workspace::fs_list_dir,
            commands::workspace::fs_read_file,
            commands::workspace::fs_write_file,
            commands::ai::ai_complete,
            commands::ai::ai_stream,
            commands::ai::ai_cancel,
            commands::ai::ai_providers_list,
            commands::ai::ai_provider_save,
            commands::ai::ai_provider_delete,
            commands::ai::ai_provider_test,
            commands::ai::ai_models,
            commands::ai::ai_set_default,
            commands::agent::agent_list,
            commands::agent::agent_start,
            commands::agent::agent_send,
            commands::agent::agent_interrupt,
            commands::agent::agent_stop,
            commands::settings::settings_get_all,
            commands::settings::settings_set,
            commands::settings::settings_schema,
            commands::settings::plugins_list,
            commands::settings::plugin_set_enabled,
            commands::lsp::lsp_did_open,
            commands::lsp::lsp_did_change,
            commands::lsp::lsp_did_save,
            commands::lsp::lsp_did_close,
            commands::lsp::lsp_hover,
            commands::lsp::lsp_completion,
            commands::lsp::lsp_completion_resolve,
            commands::lsp::lsp_definition,
            commands::lsp::lsp_references,
            commands::lsp::lsp_document_symbols,
            commands::lsp::lsp_signature_help,
            commands::lsp::lsp_format,
            commands::lsp::lsp_status,
            commands::lsp::lsp_register_server,
            commands::workspace::fs_find_files,
            commands::git::git_info,
            commands::git::git_status,
            commands::git::git_show,
            commands::git::git_diff,
            commands::git::git_diff_head,
            commands::git::git_blame,
            commands::git::git_log,
            commands::git::git_commit_details,
            commands::git::git_branches,
            commands::git::git_checkout,
            commands::git::git_create_branch,
            commands::git::git_delete_branch,
            commands::git::git_merge,
            commands::git::git_rebase,
            commands::git::git_stage,
            commands::git::git_unstage,
            commands::git::git_rollback,
            commands::git::git_commit,
            commands::git::git_push,
            commands::git::git_pull,
            commands::git::git_fetch,
            commands::php::php_index_workspace,
            commands::php::php_search_symbols,
            commands::php::php_file_structure,
            commands::php::php_hover,
            commands::php::php_generate_targets,
            commands::php::php_generate,
            commands::php::php_detect_servers,
            commands::php::php_lsp_config,
            commands::project::project_detect,
            commands::project::project_generators_list,
            commands::project::project_plan,
            commands::project::project_create,
            commands::project::project_create_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}
