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

    let app_state = AppState::new();
    let run_state = commands::run::RunState::new(&app_state);

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
        .manage(app_state)
        .manage(companion::CompanionState::load())
        .manage(commands::ai::AiState::load())
        .manage(commands::agent::AgentState::default())
        .manage(commands::db::DbState::load())
        .manage(commands::forges::ForgesState::load())
        .setup(|app| {
            app.manage(commands::debug::DebugState::new(app.handle().clone()));
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

            // A project passed on the command line is already open.
            commands::php::spawn_sync_lsp_server(app.handle());
            let root = app
                .state::<AppState>()
                .with_workspace(|ws| Ok(ws.root().to_path_buf()));
            if let Ok(root) = root {
                commands::workspace::start_watching(app.handle(), &root);
                // Data sources of the project passed on the command line.
                commands::db::project_opened(app.handle(), root);
            }
            Ok(())
        })
        .manage(commands::lsp::LspState::default())
        .manage(commands::workspace::WatchState::default())
        .manage(commands::php::PhpState::default())
        .manage(commands::laravel::LaravelState::default())
        .manage(commands::project::CreateState::default())
        .manage(commands::langservers::LangServersState::default())
        .manage(commands::terminal::TerminalState::default())
        .manage(run_state)
        .manage(commands::symfony::SymfonyState::default())
        .manage(commands::doctrine::DoctrineState::default())
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
            commands::agent::agent_respond_permission,
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
            commands::langservers::langservers_list,
            commands::langservers::langservers_apply,
            commands::langservers::langservers_install,
            commands::langservers::langservers_cancel_install,
            commands::langservers::lsp_workspace_symbols,
            commands::langservers::intelephense_licence_status,
            commands::langservers::intelephense_set_licence,
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
            commands::forges::forges_accounts,
            commands::forges::forges_login,
            commands::forges::forges_logout,
            commands::forges::forges_verify,
            commands::forges::forges_repo,
            commands::forges::forges_merge_requests,
            commands::forges::forges_merge_request,
            commands::forges::forges_mr_files,
            commands::forges::forges_discussions,
            commands::forges::forges_create_mr,
            commands::forges::forges_comment,
            commands::forges::forges_reply,
            commands::forges::forges_resolve,
            commands::forges::forges_approve,
            commands::forges::forges_merge,
            commands::forges::forges_close,
            commands::forges::forges_issues,
            commands::forges::forges_issue,
            commands::forges::forges_issue_comment,
            commands::forges::forges_pipelines,
            commands::forges::forges_ci_status,
            commands::forges::forges_members,
            commands::forges::forges_file,
            commands::forges::forges_checkout,
            commands::php::php_index_workspace,
            commands::php::php_search_symbols,
            commands::php::php_file_structure,
            commands::php::php_hover,
            commands::php::php_generate_targets,
            commands::php::php_generate,
            commands::php::php_detect_servers,
            commands::php::php_lsp_config,
            commands::laravel::laravel_detect,
            commands::laravel::laravel_routes,
            commands::laravel::laravel_complete,
            commands::laravel::laravel_goto,
            commands::laravel::laravel_gutter,
            commands::laravel::laravel_artisan_commands,
            commands::laravel::laravel_artisan_run,
            commands::laravel::laravel_artisan_cancel,
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
            commands::terminal::terminal_open,
            commands::terminal::terminal_write,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_close,
            commands::run::run_configurations_load,
            commands::run::run_configurations_save,
            commands::run::run_command_line,
            commands::run::run_start,
            commands::run::run_stop,
            commands::run::run_write,
            commands::run::run_dispose,
            commands::output::output_file_links,
            commands::markdown::markdown_parse,
            commands::markdown::markdown_hover,
            commands::debug::debug_status,
            commands::debug::debug_listen_start,
            commands::debug::debug_listen_stop,
            commands::debug::debug_sessions,
            commands::debug::debug_accept_connection,
            commands::debug::debug_ignore_connection,
            commands::debug::debug_step,
            commands::debug::debug_pause,
            commands::debug::debug_stop,
            commands::debug::debug_detach,
            commands::debug::debug_run_to_cursor,
            commands::debug::debug_stack,
            commands::debug::debug_contexts,
            commands::debug::debug_variables,
            commands::debug::debug_property,
            commands::debug::debug_eval,
            commands::debug::debug_set_value,
            commands::debug::debug_breakpoints,
            commands::debug::debug_breakpoint_add,
            commands::debug::debug_breakpoint_update,
            commands::debug::debug_breakpoint_remove,
            commands::debug::debug_breakpoint_toggle,
            commands::debug::debug_breakpoints_mute,
            commands::debug::debug_watches,
            commands::debug::debug_watches_set,
            commands::debug::debug_servers,
            commands::debug::debug_servers_save,
            commands::debug::debug_launch_env,
            commands::db::db_data_sources_list,
            commands::db::db_data_source_save,
            commands::db::db_data_source_delete,
            commands::db::db_test_connection,
            commands::db::db_disconnect,
            commands::db::db_schemas,
            commands::db::db_introspect,
            commands::db::db_table_info,
            commands::db::db_table_ddl,
            commands::db::db_table_data,
            commands::db::db_apply_changes,
            commands::db::db_session_open,
            commands::db::db_session_close,
            commands::db::db_execute,
            commands::db::db_fetch,
            commands::db::db_set_auto_commit,
            commands::db::db_commit,
            commands::db::db_rollback,
            commands::db::db_cancel,
            commands::db::db_detect_env,
            commands::db::db_add_from_env,
            commands::db::db_export,
            commands::db::db_statement_at,
            commands::symfony::symfony_detect,
            commands::symfony::symfony_refresh,
            commands::symfony::symfony_routes,
            commands::symfony::symfony_services,
            commands::symfony::symfony_parameters,
            commands::symfony::symfony_templates,
            commands::symfony::symfony_console_commands,
            commands::symfony::symfony_complete,
            commands::symfony::symfony_goto,
            commands::symfony::symfony_gutter,
            commands::symfony::symfony_console_run,
            commands::symfony::symfony_console_cancel,
            commands::doctrine::doctrine_entities,
            commands::doctrine::doctrine_complete,
            commands::doctrine::doctrine_goto,
            commands::doctrine::doctrine_diagnostics,
            commands::doctrine::doctrine_gutter,
            commands::doctrine::doctrine_highlight,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}
