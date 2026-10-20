mod commands;
mod companion;
mod plugins;
mod recent;
mod state;

use state::AppState;
use tauri::{Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut builder = tauri::Builder::default();
    // A second `forge …` launch is forwarded to this instance. forge:// links among its
    // arguments arrive through the deep-link plugin's event instead, so they are skipped.
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let args = argv
                .into_iter()
                .skip(1)
                .filter(|a| !a.starts_with("forge:"));
            companion::handle_launch_args(app, args, std::path::Path::new(&cwd), false);
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .manage(companion::CompanionState::load())
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

            // Browser Companion server, restarted when its settings change.
            let handle = app.handle().clone();
            app.state::<AppState>()
                .write_settings()
                .subscribe(move |change| {
                    if change.key == companion::KEY_ENABLED || change.key == companion::KEY_PORT {
                        companion::apply_server_settings(&handle);
                    }
                });
            companion::apply_server_settings(app.handle());

            // forge:// links. Installers register the scheme; dev builds and AppImages
            // register it at runtime.
            #[cfg(any(windows, target_os = "linux"))]
            if cfg!(debug_assertions) || std::env::var_os("APPIMAGE").is_some() {
                if let Err(e) = app.deep_link().register_all() {
                    tracing::warn!("cannot register the forge:// scheme: {e}");
                }
            }
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    match forge_open::request::parse_forge_url(url.as_str()) {
                        Ok(link) => companion::open_link(&handle, &link),
                        Err(e) => tracing::warn!("ignoring link: {e}"),
                    }
                }
            });

            // `forge path/to/project`, `forge src/App.tsx:12` or a forge:// link at startup.
            let cwd = std::env::current_dir().unwrap_or_default();
            companion::handle_launch_args(app.handle(), std::env::args().skip(1), &cwd, true);
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
            commands::companion::companion_status,
            commands::companion::companion_start_pairing,
            commands::companion::companion_cancel_pairing,
            commands::companion::companion_revoke_client,
            commands::companion::companion_ui_ready,
            commands::companion::companion_open_link,
            commands::companion::companion_project_profile,
            commands::companion::companion_set_env_var,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}
