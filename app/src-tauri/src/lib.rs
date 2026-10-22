mod commands;
mod companion;
mod plugins;
mod recent;
mod startup;
mod state;

use state::AppState;
use tauri::{AppHandle, Emitter, Manager};
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

    // Signed updates. The public key is compiled in from FORGE_UPDATER_PUBKEY.
    let mut updater = tauri_plugin_updater::Builder::new();
    if let Some(key) = commands::updates::pubkey() {
        updater = updater.pubkey(key);
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(updater.build())
        .manage(app_state)
        .manage(startup::StartupState::default())
        .manage(commands::updates::UpdatesState::default())
        .manage(companion::CompanionState::load())
        .manage(commands::ai::AiState::load())
        .manage(commands::agent::AgentState::default())
        .manage(commands::db::DbState::load())
        .manage(commands::forges::ForgesState::load())
        .manage(commands::services::ServicesState::default())
        .manage(commands::spell::SpellState::default())
        .manage(commands::json_schema::JsonSchemaState::default())
        .manage(commands::composer::ComposerState::default())
        .manage(commands::window::DialogWindowsState::default())
        .on_window_event(|window, event| {
            startup::on_splash_event(window, event);
            commands::window::on_window_event(window, event);
        })
        .manage(commands::scratches::HttpClientState::default())
        .setup(|app| {
            app.manage(commands::debug::DebugState::new(app.handle().clone()));

            // forge:// links. Installers register the scheme; dev builds and AppImages
            // register it at runtime. Links that arrive while Forge starts are queued until
            // the workbench is ready (see `companion::dispatch`).
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

            // The splash window and the loading steps (settings, plugins, the project from
            // the command line or the last session, services, updates), then the workbench.
            startup::begin(app.handle());
            Ok(())
        })
        .manage(commands::lsp::LspState::default())
        .manage(commands::library::LibraryState::default())
        .manage(commands::workspace::WatchState::default())
        .manage(commands::remote::RemoteState::default())
        .manage(commands::laravel::LaravelState::default())
        .manage(commands::lua::LuaState::default())
        .manage(commands::project::CreateState::default())
        .manage(commands::langservers::LangServersState::default())
        .manage(commands::terminal::TerminalState::default())
        .manage(run_state)
        .manage(commands::symfony::SymfonyState::default())
        .manage(commands::workbench::BookmarksState::default())
        .manage(commands::clipboard::ClipboardState::default())
        .manage(commands::git_clone::CloneState::default())
        .invoke_handler(tauri::generate_handler![
            commands::workspace::workspace_open,
            commands::git_clone::forges_repositories,
            commands::git_clone::git_clone_directory_name,
            commands::git_clone::git_clone_home,
            commands::git_clone::git_clone_test,
            commands::git_clone::git_clone,
            commands::git_clone::git_clone_cancel,
            commands::git_extras::git_worktrees,
            commands::git_extras::git_worktree_add,
            commands::git_extras::git_worktree_remove,
            commands::git_extras::git_worktree_prune,
            commands::git_extras::git_worktree_lock,
            commands::git_extras::git_worktree_unlock,
            commands::git_extras::git_message_history,
            commands::git_extras::git_signing_info,
            commands::remote::remote_project_connection,
            commands::workspace::workspace_current,
            commands::workspace::workspace_recent,
            commands::workspace::fs_list_dir,
            commands::workspace::fs_read_file,
            commands::workspace::fs_write_file,
            commands::editor::fs_read_text,
            commands::editor::fs_write_text,
            commands::editor::text_prepare_save,
            commands::editor::editor_postfix_templates,
            commands::editor::editor_postfix_complete,
            commands::editor::editor_complete_statement,
            commands::editor::editor_expand_word,
            commands::clipboard::clipboard_history_add,
            commands::clipboard::clipboard_history_list,
            commands::clipboard::clipboard_history_remove,
            commands::clipboard::clipboard_history_promote,
            commands::templates::templates_list,
            commands::templates::templates_validate,
            commands::templates::templates_php_namespace,
            commands::templates::templates_php_directory,
            commands::templates::templates_create,
            commands::editor::editor_file_format,
            commands::editor::vcs_inspect_commit_message,
            commands::project_state::layout_load,
            commands::project_state::layout_save,
            commands::project_state::layout_default_load,
            commands::project_state::layout_default_save,
            commands::project_state::agent_history_list,
            commands::project_state::agent_history_load,
            commands::project_state::agent_history_save,
            commands::project_state::agent_history_delete,
            commands::workspace::fs_create_file,
            commands::workspace::fs_create_dir,
            commands::workspace::fs_rename,
            commands::workspace::fs_delete,
            commands::workspace::fs_copy,
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
            commands::keymap::keymap_actions,
            commands::keymap::keymap_register_actions,
            commands::keymap::keymap_set_override,
            commands::keymap::keymap_conflicts,
            commands::keymap::keymap_find_by_shortcut,
            commands::keymap::keymap_resolve,
            commands::keymap::keymap_main_menu,
            commands::keymap::keymap_import_legacy,
            commands::vim::vim_read_rc,
            commands::settings::plugin_set_enabled,
            commands::settings::settings_search,
            commands::settings::settings_validate,
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
            commands::lsp::lsp_inlay_hints,
            commands::lsp::lsp_status,
            commands::lsp::lsp_register_server,
            commands::codeintel::code_find_usages,
            commands::codeintel::code_highlight_usages,
            commands::codeintel::code_usage_target,
            commands::codeintel::code_navigate,
            commands::hierarchy::code_type_hierarchy,
            commands::hierarchy::code_method_hierarchy,
            commands::hierarchy::code_call_hierarchy,
            commands::hierarchy::code_call_hierarchy_calls,
            commands::codeintel::code_goto_declaration,
            commands::codeintel::code_symbol_info,
            commands::codeintel::code_external_doc,
            commands::codeintel::code_quick_definition,
            commands::codeintel::code_type_info,
            commands::codeintel::code_context_info,
            commands::codeintel::code_related,
            commands::codeintel::code_test_targets,
            commands::codeintel::code_new_test_plan,
            commands::codeintel::code_create_test,
            commands::codeintel::code_line_markers,
            commands::library::library_read,
            commands::codeintel::code_prepare_rename,
            commands::codeintel::code_rename,
            commands::codeintel::code_refactor_actions,
            commands::codeintel::code_refactor_this,
            commands::refactor::refactor_introduce_prepare,
            commands::refactor::refactor_introduce,
            commands::refactor::refactor_extract_method_prepare,
            commands::refactor::refactor_extract_method,
            commands::refactor::refactor_inline_prepare,
            commands::refactor::refactor_inline,
            commands::refactor_move::refactor_move_prepare,
            commands::refactor_move::refactor_move_class,
            commands::refactor_move::refactor_copy_class,
            commands::refactor_move::refactor_move_files,
            commands::refactor_move::refactor_safe_delete_prepare,
            commands::refactor_move::refactor_safe_delete_files,
            commands::refactor_move::refactor_change_signature_prepare,
            commands::refactor_move::refactor_change_signature,
            commands::refactor_move::refactor_make_static_prepare,
            commands::refactor_move::refactor_make_static,
            commands::refactor_move::refactor_move_members,
            commands::refactor_move::refactor_move_function,
            commands::refactor_hierarchy::refactor_hierarchy_prepare,
            commands::refactor_hierarchy::refactor_extract_interface,
            commands::refactor_hierarchy::refactor_extract_superclass,
            commands::refactor_hierarchy::refactor_pull_up,
            commands::refactor_hierarchy::refactor_push_down,
            commands::extract_component::refactor_extract_component_prepare,
            commands::extract_component::refactor_extract_component,
            commands::codeintel::code_plan_edit,
            commands::codeintel::code_apply_edit,
            commands::codeintel::code_revert_edit,
            commands::codeintel::code_context_actions,
            commands::codeintel::php_import_class,
            commands::imports::imports_optimize,
            commands::imports::imports_optimize_paths,
            commands::imports::php_imports_on_the_fly,
            commands::imports::php_import_hint,
            commands::imports::php_import_reference,
            commands::imports::code_imports_on_the_fly,
            commands::codeintel::code_resolve_action,
            commands::codeintel::code_execute_command,
            commands::langservers::langservers_list,
            commands::langservers::langservers_apply,
            commands::langservers::langservers_install,
            commands::langservers::langservers_cancel_install,
            commands::langservers::langservers_check_updates,
            commands::langservers::langservers_update,
            commands::langservers::lsp_workspace_symbols,
            commands::langservers::intelephense_licence_status,
            commands::langservers::intelephense_set_licence,
            commands::workspace::fs_find_files,
            commands::workspace::find_in_files,
            commands::workspace::find_in_files_paged,
            commands::workspace::find_page,
            commands::workspace::replace_preview,
            commands::workspace::replace_in_files,
            commands::workspace::undo_replace,
            commands::workbench::todo_scan,
            commands::scopes::scopes_files,
            commands::scopes::scopes_test_roots,
            commands::ssr::ssr_templates,
            commands::ssr::ssr_check,
            commands::ssr::ssr_search,
            commands::ssr::ssr_replace,
            commands::stacktrace::stacktrace_analyze,
            commands::workbench::bookmarks_get,
            commands::workbench::bookmarks_apply,
            commands::search::fuzzy_filter,
            commands::search::search_rank,
            commands::search::filetypes_detect,
            commands::search::filetypes_template_delimiters,
            commands::search::search_find_symbols,
            commands::search::search_status,
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
            commands::git::git_add_to_gitignore,
            commands::git::git_create_tag,
            commands::git::git_stash,
            commands::git::git_stash_list,
            commands::git::git_unstash,
            commands::git::git_drop_stash,
            commands::git::git_remotes,
            commands::git::git_add_remote,
            commands::git::git_remove_remote,
            commands::git::git_conflicts,
            commands::git::git_merge_file,
            commands::git::git_accept_conflict_side,
            commands::git::git_resolve_merge,
            commands::git::git_crlf_files,
            commands::git_ops::git_pull_with,
            commands::git_ops::git_merge_with,
            commands::git_ops::git_rebase_with,
            commands::git_ops::git_operation_state,
            commands::git_ops::git_operation_action,
            commands::git_ops::git_reset_head,
            commands::git_ops::git_tag_with,
            commands::git_ops::git_resolve_commit,
            commands::git_ops::git_edit_remote,
            commands::git_ops::git_stash_files,
            commands::git_ops::git_stash_file,
            commands::git_ops::git_stash_clear,
            commands::git_ops::git_stash_branch,
            commands::git_ops::git_shelf_list,
            commands::git_ops::git_shelve,
            commands::git_ops::git_unshelve,
            commands::git_ops::git_shelf_delete,
            commands::git_ops::git_shelf_rename,
            commands::git_ops::git_shelf_file,
            commands::git_log::git_log_actions,
            commands::git_log::git_cherry_pick,
            commands::git_log::git_revert_commits,
            commands::git_log::git_undo_commit,
            commands::git_log::git_undo_rewrite,
            commands::git_log::git_reword,
            commands::git_log::git_squash_commits,
            commands::git_log::git_drop_commits,
            commands::git_log::git_rebase_plan,
            commands::git_log::git_interactive_rebase,
            commands::git_log::git_commits_patch,
            commands::git_changes::git_changelists,
            commands::git_changes::git_changelist_create,
            commands::git_changes::git_changelist_edit,
            commands::git_changes::git_changelist_set_active,
            commands::git_changes::git_changelist_delete,
            commands::git_changes::git_changelist_move,
            commands::git_changes::git_file_hunks,
            commands::git_changes::git_commit_files,
            commands::git_changes::git_create_patch,
            commands::git_changes::git_read_patch,
            commands::git_changes::git_write_patch,
            commands::git_changes::git_patch_preview,
            commands::git_changes::git_apply_patch,
            commands::git_log::git_tree_files,
            commands::git_history::git_file_history,
            commands::git_history::git_block_history,
            commands::git_history::git_blame_revision,
            commands::git_history::git_changes_since,
            commands::compare::compare_dirs,
            commands::compare::compare_copy,
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
            commands::php::php_tidy_blank_lines,
            commands::code_style::code_style_model,
            commands::code_style::code_style_schemes,
            commands::code_style::code_style_save,
            commands::code_style::code_style_preview,
            commands::code_style::code_style_export,
            commands::code_style::code_style_import,
            commands::code_style::code_style_post_format,
            commands::php::php_language_level,
            commands::php_interpreters::php_interpreters_detect,
            commands::php_interpreters::php_interpreter_info,
            commands::php_interpreters::php_interpreter_phpinfo,
            commands::php_interpreters::php_interpreter_mappings,
            commands::php_interpreters::php_interpreter_compose_services,
            commands::php_interpreters::php_interpreter_wsl_distributions,
            commands::php::php_composer_outdated,
            commands::composer::composer_project,
            commands::composer::composer_markers,
            commands::composer::composer_complete,
            commands::composer::composer_hover,
            commands::composer::composer_search,
            commands::composer::composer_package,
            commands::composer::composer_audit,
            commands::composer::composer_download_phar,
            commands::php::php_detect_servers,
            commands::php::php_lsp_config,
            commands::lua::lua_index_workspace,
            commands::lua::lua_files_changed,
            commands::lua::lua_search_symbols,
            commands::lua::lua_file_structure,
            commands::lua::lua_hover,
            commands::lua::lua_goto,
            commands::lua::lua_run_markers,
            commands::lua::lua_format,
            commands::save_actions::save_actions_plan,
            commands::save_actions::save_actions_changed_lines,
            commands::save_actions::save_actions_php,
            commands::save_actions::save_actions_run_tools,
            commands::save_actions::save_actions_info,
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
            commands::scratches::scratches_list,
            commands::scratches::scratches_languages,
            commands::scratches::scratches_create,
            commands::scratches::scratches_create_buffer,
            commands::scratches::scratches_read,
            commands::scratches::scratches_write,
            commands::scratches::scratches_rename,
            commands::scratches::scratches_delete,
            commands::scratches::scratches_set_data_source,
            commands::scratches::scratches_markers,
            commands::scratches::scratches_http_run,
            commands::scratches::scratches_http_environments,
            commands::output::output_file_links,
            commands::markdown::markdown_parse,
            commands::injections::injections_find,
            commands::injections::injections_at,
            commands::injections::injections_fragment,
            commands::injections::injections_escape,
            commands::injections::injections_inject,
            commands::injections::injections_rules,
            commands::injections::injections_languages,
            commands::live_templates::live_templates_catalog,
            commands::live_templates::live_templates_applicable,
            commands::live_templates::live_templates_expand,
            commands::live_templates::live_templates_surround_items,
            commands::live_templates::live_templates_surround,
            commands::live_templates::live_templates_unwrap,
            commands::live_templates::live_templates_variables,
            commands::live_templates::live_templates_user,
            commands::emmet::emmet_expand,
            commands::emmet::emmet_wrap,
            commands::emmet::emmet_edit_point,
            commands::emmet::emmet_update_tag,
            commands::emmet::emmet_remove_tag,
            commands::emmet::colors_find,
            commands::emmet::colors_format,
            commands::emmet::colors_parse,
            commands::spell::spell_inspect,
            commands::spell::spell_context_actions,
            commands::spell::spell_dictionaries,
            commands::json_schema::json_schema_validate,
            commands::json_schema::json_schema_complete,
            commands::json_schema::json_schema_hover,
            commands::json_schema::json_schema_goto,
            commands::json_schema::json_schema_read,
            commands::json_schema::json_schema_builtins,
            commands::markdown::markdown_hover,
            commands::markdown::markdown_document,
            commands::markdown::markdown_anchor_line,
            commands::viewers::image_read,
            commands::viewers::csv_parse,
            commands::viewers::csv_edit,
            commands::viewers::csv_sort_order,
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
            commands::debug::debug_complete,
            commands::debug::debug_web_page_start,
            commands::debug::debug_page_url,
            commands::debug::debug_validate_cli,
            commands::debug::debug_validate_web,
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
            commands::web::npm_packages,
            commands::web::npm_gutter,
            commands::web::web_components,
            commands::web::web_goto,
            commands::web::web_gutter,
            commands::doctrine::doctrine_entities,
            commands::doctrine::doctrine_complete,
            commands::doctrine::doctrine_goto,
            commands::doctrine::doctrine_diagnostics,
            commands::doctrine::doctrine_gutter,
            commands::doctrine::doctrine_highlight,
            commands::services::services_tree,
            commands::services::services_refresh,
            commands::services::services_compose_changed,
            commands::services::services_expand,
            commands::services::services_details,
            commands::services::services_action,
            commands::services::services_inspect,
            commands::services::services_console,
            commands::services::services_logs_start,
            commands::services::services_logs_stop,
            commands::services::services_terminal,
            commands::services::services_pull,
            commands::services::services_compose_up,
            commands::services::services_dockerfile_run,
            commands::services::services_gutter,
            commands::services::services_status,
            commands::services::services_port_forward_start,
            commands::services::services_port_forward_stop,
            commands::services::services_port_forwards,
            commands::services::docker_contexts,
            commands::services::docker_test_connection,
            commands::services::docker_registry_login,
            commands::services::docker_registry_logout,
            commands::services::docker_registry_status,
            commands::remote::remote_ssh_configs,
            commands::remote::remote_ssh_config_save,
            commands::remote::remote_ssh_config_delete,
            commands::remote::remote_ssh_hosts,
            commands::remote::remote_validate_config,
            commands::remote::remote_test_connection,
            commands::remote::remote_connect,
            commands::remote::remote_browse,
            commands::remote::remote_open_project,
            commands::remote::remote_disconnect,
            commands::remote::remote_status,
            commands::remote::remote_reconnect,
            commands::window::window_chrome,
            commands::window::reveal_in_file_manager,
            commands::updates::updates_check,
            commands::updates::updates_respond,
            commands::updates::updates_status,
            commands::updates::updates_clear_skipped,
            startup::splash_state,
            startup::startup_workbench_ready,
            commands::window::dialog_window_open,
            commands::window::dialog_window_close,
            commands::window::dialog_window_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}

/// Startup step "Loading plugins": the settings listeners of the built-in plugins and the
/// servers they run (Browser Companion). Called once from the startup thread.
pub(crate) fn activate_plugins(app: &AppHandle) {
    // Forward settings changes to the UI.
    let handle = app.clone();
    app.state::<AppState>()
        .write_settings()
        .subscribe(move |change| {
            if let Err(e) = handle.emit("settings://changed", change) {
                tracing::warn!("cannot emit settings change: {e}");
            }
        });

    // Forge draws the title bars unless `appearance.windowControls` is `native`.
    let handle = app.clone();
    app.state::<AppState>()
        .write_settings()
        .subscribe(move |change| {
            if change.key == commands::window::SETTING_KEY {
                let h = handle.clone();
                // Listeners run under the settings lock; read it on another thread.
                std::thread::spawn(move || commands::window::apply_decorations(&h));
            }
        });

    // Browser Companion server, restarted when its settings change.
    let handle = app.clone();
    app.state::<AppState>()
        .write_settings()
        .subscribe(move |change| {
            if change.key == companion::KEY_ENABLED || change.key == companion::KEY_PORT {
                companion::apply_server_settings(&handle);
            }
        });
    companion::apply_server_settings(app);

    // Docker / Kubernetes reconnect when their settings change (they connect in the
    // "background services" step).
    let handle = app.clone();
    app.state::<AppState>()
        .write_settings()
        .subscribe(move |change| {
            if commands::services::affects_services(&change.key) {
                let h = handle.clone();
                // Listeners run under the settings lock; read them on another thread.
                std::thread::spawn(move || commands::services::reconfigure(&h));
            }
        });

    // The PHP import style (code style Imports tab, Auto Import) of the refactorings.
    let handle = app.clone();
    app.state::<AppState>()
        .write_settings()
        .subscribe(move |change| {
            if commands::imports::affects_import_style(&change.key) {
                let h = handle.clone();
                // Listeners run under the settings lock; read them on another thread.
                std::thread::spawn(move || {
                    commands::imports::import_style(&h.state::<AppState>());
                });
            }
        });

    // Reconfigure (restart) the PHP language server when its settings or the enabled
    // plugins change. Listeners run under the settings lock, so the work happens on
    // another thread.
    let handle = app.clone();
    app.state::<AppState>()
        .write_settings()
        .subscribe(move |change| {
            if phpforge_lang_php::lsp::affects_server(&change.key)
                || change.key == plugins::DISABLED_PLUGINS_KEY
            {
                commands::php::spawn_sync_lsp_server(&handle);
            }
        });
}
