/* tslint:disable */
/* eslint-disable */

/**
 * `{ agent, lines }` → the `AgentEvent[]` those raw stdout lines of the agent's CLI
 * produce.
 */
export function agent_parse_lines(request_json: string): string;

/**
 * Profiles of the built-in agents (`AgentProfile[]`), in picker order.
 */
export function agent_profiles(): string;

/**
 * `{ text, line, files }` → `StepDefinition[]` (`behat_step_definitions`).
 */
export function behat_step_definitions(request_json: string): string;

/**
 * `{ state, ops }` → the new `Bookmarks`.
 */
export function bookmarks_apply(request_json: string): string;

/**
 * Version of the bridge, used by the UI to check that the bundle is current.
 */
export function bridge_version(): string;

/**
 * `{ handle, event: AgentEvent, now }` → `ChatUpdate`.
 */
export function chat_apply(request_json: string): string;

/**
 * `{ handle, turnId, prompt, context }` → `ChatUpdate`.
 */
export function chat_begin_turn(request_json: string): string;

export function chat_drop(request_json: string): string;

/**
 * A new conversation → its handle (a number).
 */
export function chat_new(): string;

/**
 * `{ handle, requestId, status }` → `ChatUpdate | null`.
 */
export function chat_set_permission(request_json: string): string;

/**
 * `{ input, offset, tools, commands }` → `Completion` (`cli_tools_complete`).
 */
export function cli_tools_complete(request_json: string): string;

/**
 * `{ configured, autoDetect, files, composer }` → `Tool[]` (`cli_tools_list`).
 */
export function cli_tools_effective(request_json: string): string;

/**
 * `{ input, tools }` → `{ tool, args, invocation, commandLine }`: what `cli_tools_run`
 * starts (before the interpreter wraps it).
 */
export function cli_tools_parse_input(request_json: string): string;

/**
 * `{ kind, text }` → `CommandDef[]`: a recorded `list --format=json` / `wp cli cmd-dump`.
 */
export function cli_tools_parse_list(request_json: string): string;

/**
 * `{ entry, max? }` → `bool`.
 */
export function clipboard_history_add(args_json: string): string;

/**
 * `{ query?, max? }` → `ClipItem[]`.
 */
export function clipboard_history_list(args_json: string): string;

/**
 * `{ index }` → `ClipEntry | null`.
 */
export function clipboard_history_promote(args_json: string): string;

/**
 * `{ index }` → `bool`.
 */
export function clipboard_history_remove(args_json: string): string;

/**
 * `{ edit, files }` → `{ applied: AppliedFile[], files }` (the files after the edit) or
 * `{ error }`.
 */
export function codeintel_apply_edit(request_json: string): string;

/**
 * `ContextAction[]` → `{ actions, bulb }` like `code_context_actions`.
 */
export function codeintel_context_actions(request_json: string): string;

/**
 * `{ text, line, path }` → `ContextLine[]` (Context Info).
 */
export function codeintel_context_headers(request_json: string): string;

/**
 * `{ text, line, path }` → `SnippetRange` (Quick Definition).
 */
export function codeintel_definition_snippet(request_json: string): string;

/**
 * `{ text, position, path, declaration? }` → `{ url, symbol }`: the External
 * Documentation page (like `code_external_doc`).
 */
export function codeintel_doc_url(request_json: string): string;

/**
 * `{ usages, options, scope, element, path, word, files }` → `{ usages, scope }`: the
 * usages with text occurrences, in the scope and filtered by the options (like
 * `code_find_usages`).
 */
export function codeintel_filter_usages(request_json: string): string;

/**
 * `{ usages, declaration?, files }` → `{ byType, byFile }` (like `code_find_usages`).
 */
export function codeintel_group_usages(request_json: string): string;

/**
 * `{ text, path, line, character, ranges?, declaration?, title?, element? }` →
 * `HighlightAnswer | null` (like `code_highlight_usages`): exit points on `return` /
 * `throw`, the given ranges classified as reads and writes, else the lexical fallback.
 */
export function codeintel_highlight_usages(request_json: string): string;

/**
 * `{ hints, text, languageId?, settings }` → the hints to draw (like `lsp_inlay_hints`).
 */
export function codeintel_inlay_hints(request_json: string): string;

/**
 * `{ path, text, composerPath?, files }` → `NewTestPlan | null` (like `code_new_test_plan`).
 */
export function codeintel_new_test_plan(request_json: string): string;

/**
 * `ContextAction[]` → the same, in PHPStorm's order without duplicates.
 */
export function codeintel_order_actions(request_json: string): string;

/**
 * `{ edit, open }` → `EditPlan` (like `code_plan_edit`).
 */
export function codeintel_plan_edit(request_json: string): string;

/**
 * `{ edit, files }` → `FilePreview[]` (Refactoring Preview rows).
 */
export function codeintel_preview_edit(request_json: string): string;

/**
 * Refactor This in JS/TS: the language server's refactorings worded like PHPStorm
 * (`ContextAction[]` -> `ContextAction[]`).
 */
export function codeintel_refactor_this_actions(request_json: string): string;

/**
 * `{ path, text, files }` → `RelatedItem[]` (like `code_related`).
 */
export function codeintel_related(request_json: string): string;

/**
 * `{ locations, oldName, newName, declaration?, renameFile?, files }` → `{ edit, skipped,
 * fileRename, conflicts }` or `{ error }`: the index part of `code_rename`.
 */
export function codeintel_rename_edit(request_json: string): string;

/**
 * `TestSourceRequest` → `TestSource` or `{ error }` (the rendering of `code_create_test`).
 */
export function codeintel_test_source(request_json: string): string;

/**
 * `{ path, text, files }` → `TestTargets` (like `code_test_targets`).
 */
export function codeintel_test_targets(request_json: string): string;

/**
 * `{ hover?, signature? }` → the type Type Info shows, or `null`.
 */
export function codeintel_type_info(request_json: string): string;

/**
 * `{ element, title }` → `UsageTarget` (the Find Usages dialog's checkboxes).
 */
export function codeintel_usage_target(request_json: string): string;

/**
 * `{ text, position }` → `[range, word] | null`.
 */
export function codeintel_word_range(request_json: string): string;

/**
 * `{ name, values, format }` → the exported text.
 */
export function codestyle_export(request_json: string): string;

/**
 * `{ user, project, request }` → `FileFormat`, with the selected scheme's code style
 * (as `editor_file_format` resolves it).
 */
export function codestyle_file_format(request_json: string): string;

/**
 * `{ text }` → `ImportedScheme` or `{ error }`.
 */
export function codestyle_import(request_json: string): string;

/**
 * `()` → `CodeStyleModel`.
 */
export function codestyle_model(): string;

/**
 * `{ user, project, schemes }` → `{ writes: SettingWrite[] }` or `{ error }`.
 */
export function codestyle_plan_save(request_json: string): string;

/**
 * `{ user, project, request }` → the text after the code style pass.
 */
export function codestyle_post_format(request_json: string): string;

/**
 * `{ language, values }` → the formatted sample (or `null`).
 */
export function codestyle_preview(request_json: string): string;

/**
 * `{ user, project }` → `SchemesState`.
 */
export function codestyle_schemes(request_json: string): string;

/**
 * `FindRequest` → `ColorMatch[]`.
 */
export function colors_find(request_json: string): string;

/**
 * `FormatRequest` → string.
 */
export function colors_format(request_json: string): string;

/**
 * `{ text }` → `{ color, hex } | null`.
 */
export function colors_parse(request_json: string): string;

/**
 * `{ left: {path, dir, text}[], right: … }` (paths relative to each folder) → `DirCompare`.
 */
export function compare_dir_listings(request_json: string): string;

/**
 * `{ report, text (composer.json), lock }` → `{ jsonProblems, lockProblems }`.
 */
export function composer_audit_problems(request_json: string): string;

/**
 * `{ text, offset, lock, catalog }` → `CompletionOut | null`.
 */
export function composer_complete(request_json: string): string;

/**
 * `{ text, offset, lock, catalog }` → `HoverOut | null`.
 */
export function composer_hover(request_json: string): string;

/**
 * `{ text, lock, catalog }` → `OutdatedMarker[]`.
 */
export function composer_markers(request_json: string): string;

/**
 * `{ text, lock?, installedJson? }` → `ComposerProject` without the path fields.
 */
export function composer_project(request_json: string): string;

/**
 * Composer script names of a `composer.json` text.
 */
export function composer_scripts(composer_json: string): string;

/**
 * `{ query, catalog }` → `SearchHit[]` (the demo search).
 */
export function composer_search(request_json: string): string;

/**
 * `ComposeCommand` → arguments after `docker`.
 */
export function containers_compose_args(request_json: string): string;

/**
 * `{ path, content, env?, project? }` → `ComposeFile`.
 */
export function containers_compose_parse(request_json: string): string;

/**
 * `{ metas: [meta.json text], current }` → `DockerContext[]` (the mock's
 * `~/.docker/contexts`).
 */
export function containers_docker_contexts(request_json: string): string;

/**
 * Engine API responses (`RawSnapshot`) → `DockerSnapshot`.
 */
export function containers_docker_snapshot(raw_json: string): string;

/**
 * `{ path, project, content }` → `DockerfileRun`.
 */
export function containers_dockerfile_plan(request_json: string): string;

/**
 * `{ content, baseDir }` → `KubeConfig`.
 */
export function containers_kubeconfig_parse(request_json: string): string;

/**
 * `{ spec }` → `[local, remote][]` (Forward Ports), or an error.
 */
export function containers_port_forward_specs(request_json: string): string;

/**
 * `{ pod, container, text }` → the text with every line prefixed like
 * `kubectl logs --prefix` (deployment logs).
 */
export function containers_prefix_log(request_json: string): string;

/**
 * `{ image }` → handle for [`containers_pull_push`].
 */
export function containers_pull_new(request_json: string): string;

/**
 * `{ handle, text }` → `{ lines, state }`; `text: null` finishes and drops the handle.
 */
export function containers_pull_push(request_json: string): string;

/**
 * Exec / attach / kubectl exec command lines (`TerminalCommand`), mirroring
 * `services_terminal`.
 */
export function containers_terminal_command(request_json: string): string;

/**
 * `RawWorkloads` (API lists) → `Workloads`.
 */
export function containers_workloads_parse(raw_json: string): string;

/**
 * `{ text, delimiter, edits }` → the new text.
 */
export function csv_edit(request_json: string): string;

/**
 * `{ text, path?, delimiter? }` → `CsvTable`.
 */
export function csv_parse(request_json: string): string;

/**
 * `{ rows, column, descending, headerRows }` → row indices.
 */
export function csv_sort_order(request_json: string): string;

/**
 * Mirrors `doctrine_complete`: `{ path, content, offset, snapshot }` → `DqlCompletion | null`.
 */
export function dql_complete(request_json: string): string;

/**
 * Mirrors `doctrine_diagnostics`: `{ path, content, snapshot }` → `SourceDiagnostic[]`.
 */
export function dql_diagnostics(request_json: string): string;

/**
 * Mirrors `doctrine_goto`: `{ path, content, offset, snapshot }` → `Location[]`.
 */
export function dql_goto(request_json: string): string;

/**
 * Mirrors `doctrine_highlight`: `{ path, content, snapshot }` → `SourceHighlight`.
 */
export function dql_highlight(request_json: string): string;

/**
 * `{ ops: EditOp[], list? }` → `{ place: EditPlace | null, places?: EditPlace[] }`.
 */
export function edit_history(request_json: string): string;

/**
 * `StatementRequest` → `StatementEdit | null`.
 */
export function editing_complete_statement(request_json: string): string;

/**
 * `ExpandRequest` → `ExpandResult`.
 */
export function editing_expand_word(request_json: string): string;

/**
 * `PostfixRequest` → `PostfixCompletion | null`.
 */
export function editing_postfix_complete(request_json: string): string;

/**
 * `{}` → `TemplateInfo[]`.
 */
export function editing_postfix_templates(_request_json: string): string;

/**
 * `{ ranges, line, forward }` → the index of the hunk to show, or `null`.
 */
export function editor_change_target(request_json: string): string;

/**
 * `{ text, language }` → `{ line, endLine, column, doc: HoverDoc }[]`.
 */
export function editor_doc_comments(request_json: string): string;

/**
 * `{ text, language, line, column, forward }` → `{ line, column } | null`.
 */
export function editor_method_target(request_json: string): string;

/**
 * `{ text, language }` → `{ scopes: Scope[], regions: Region[] }`.
 */
export function editor_structure(request_json: string): string;

/**
 * `{ id, label }[]` of the languages with Sticky Lines and breadcrumbs.
 */
export function editor_structure_languages(): string;

/**
 * `EditPointRequest` → offset | null.
 */
export function emmet_edit_point(request_json: string): string;

/**
 * `ExpandRequest` → `Expansion | null`.
 */
export function emmet_expand(request_json: string): string;

/**
 * `TagRequest` → `Edit[]` (or `{ error }`).
 */
export function emmet_remove_tag(request_json: string): string;

/**
 * `TagRequest` → `Edit[]` (or `{ error }`).
 */
export function emmet_update_tag(request_json: string): string;

/**
 * `WrapRequest` → `Expansion` (or `{ error }`).
 */
export function emmet_wrap(request_json: string): string;

/**
 * `{ tool, context, root, name }` → `ToolCommand`.
 */
export function external_tool_command(request_json: string): string;

/**
 * `{ context, root, name }` → `MacroInfo[]`.
 */
export function external_tool_macros(request_json: string): string;

/**
 * `{ tool, others }` → problems.
 */
export function external_tool_validate(request_json: string): string;

/**
 * `ColorsRequest` → `(string | null)[]`: the palette color of every target.
 */
export function file_colors_resolve(request_json: string): string;

/**
 * `FileStructureRequest` → `{ rows, selected?, hasFields, hasLambdas, hasInherited }`.
 */
export function file_structure_view(request_json: string): string;

/**
 * `DetectRequest` → `FileTypeInfo[]`.
 */
export function filetypes_detect(request_json: string): string;

/**
 * Register the file types of plugins (`PluginInfo[]` JSON) on top of the bundled ones.
 * Returns the number of known file types.
 */
export function filetypes_load(plugins_json: string): string;

/**
 * Mirrors the `filetypes_template_delimiters` IPC command.
 */
export function filetypes_template_delimiters(): string;

/**
 * `{ request, files }` → `FindResult`: Find in Files over in-memory files, mirroring the
 * `find_in_files` command (the desktop app walks the project instead).
 */
export function find_in_files(request_json: string): string;

/**
 * `{ result, offset, count }` → `FileMatches[]`: rows of a Find in Files result.
 */
export function find_page(request_json: string): string;

/**
 * `{ result, id, count }` → `FindPaged`: the totals and first rows of a Find in Files result.
 */
export function find_paged(request_json: string): string;

/**
 * `FilterRequest` → `FilterHit[]`.
 */
export function fuzzy_filter(request_json: string): string;

export function hierarchy_call_view(request_json: string): string;

export function hierarchy_method_view(request_json: string): string;

export function hierarchy_type_view(request_json: string): string;

/**
 * `{ path, base64 }` → `ImageData` (the mock reads demo images from base64).
 */
export function image_describe(request_json: string): string;

/**
 * `{ files, changed, removed }` → the folder notification.
 */
export function imports_folder_summary(files: number, changed: number, removed: number): string;

/**
 * `{ old, new }` → the one LSP edit turning `old` into `new` (or null).
 */
export function imports_minimal_edit(request_json: string): string;

/**
 * `{ old, new, removed, rearranged }` → `OptimizeAnswer` (like `imports_optimize`): the
 * editor change and the notification.
 */
export function imports_optimize_answer(request_json: string): string;

/**
 * `{ files, changed? }` → the files Optimize Imports processes.
 */
export function imports_targets(request_json: string): string;

/**
 * `{ text, path, offset, disabled?, uninjected? }` → `Injection | null`.
 */
export function injections_at(request_json: string): string;

/**
 * `{ text, path, offset, disabled?, uninjected? }` → `ContextAction[]`.
 */
export function injections_context_actions(request_json: string): string;

/**
 * `{ kind, text }` → the text escaped for a literal of that kind.
 */
export function injections_escape(request_json: string): string;

/**
 * `{ text, path, disabled?, uninjected? }` → `Injection[]`.
 */
export function injections_find(request_json: string): string;

/**
 * `{ text, path, offset, disabled?, uninjected? }` → `Fragment | null`.
 */
export function injections_fragment(request_json: string): string;

/**
 * `{ text, path, offset, language }` → `Edit | null`.
 */
export function injections_inject(request_json: string): string;

/**
 * The injectable languages.
 */
export function injections_languages(): string;

/**
 * The built-in rules for Settings | Editor | Language Injections.
 */
export function injections_rules(): string;

/**
 * Like `json_schema_builtins`.
 */
export function json_schema_builtins(): string;

/**
 * Like `json_schema_complete`.
 */
export function json_schema_complete(request_json: string): string;

/**
 * Like `json_schema_goto`.
 */
export function json_schema_goto(request_json: string): string;

/**
 * Like `json_schema_hover`.
 */
export function json_schema_hover(request_json: string): string;

/**
 * A bundled schema's text for `json_schema_read`, or `{ error }`.
 */
export function json_schema_read(location: string): string;

/**
 * `{ path, language, content, mappings, files }` → `{ answer: { schema, problems } }` or
 * `{ needs: [path] }`, like `json_schema_validate`.
 */
export function json_schema_validate(request_json: string): string;

/**
 * `{settings, hostPlatform}` → `KeymapSnapshot`.
 */
export function keymap_actions(json: string): string;

/**
 * `{settings, hostPlatform, shortcut, actionId?}` → `ShortcutCheck`.
 */
export function keymap_conflicts(json: string): string;

/**
 * `{settings, hostPlatform, shortcut}` → action ids.
 */
export function keymap_find_by_shortcut(json: string): string;

/**
 * `{text, platform}` → display string ("Ctrl+K, Ctrl+C" / "⇧⌘A").
 */
export function keymap_format(json: string): string;

/**
 * Resolve one `KeyInput` with the sync resolver → `Resolution`. Keys that Vim emulation
 * takes in the focused editor (`crate::vim::claims`) are left to the editor.
 */
export function keymap_handle_key(input_json: string): string;

/**
 * `{active, overrides}` (raw localStorage strings) → `SettingWrite[]`.
 */
export function keymap_legacy_changes(json: string): string;

/**
 * Load the bindings of a `KeymapSnapshot` into the sync resolver; returns their count.
 */
export function keymap_load_bindings(snapshot_json: string): string;

/**
 * `{settings, hostPlatform, request}` → `MainMenu`.
 */
export function keymap_main_menu(json: string): string;

/**
 * `{settings, hostPlatform, actions?, commands?}` → `KeymapSnapshot`.
 */
export function keymap_register_actions(json: string): string;

/**
 * `{settings, hostPlatform, event}` → `Resolution` (resolver state kept between calls).
 */
export function keymap_resolve(json: string): string;

/**
 * `{settings, hostPlatform, change}` → `SettingWrite[]`.
 */
export function keymap_set_override(json: string): string;

/**
 * DOM key event → canonical keystroke (for "press a shortcut" fields).
 */
export function keymap_stroke_of_event(event_json: string): string;

/**
 * `ApplicableRequest` → `ApplicableResult`.
 */
export function live_templates_applicable(request_json: string): string;

/**
 * `TemplateSettings` → `Catalog`.
 */
export function live_templates_catalog(settings_json: string): string;

/**
 * `ExpandRequest` → `Expansion | null`.
 */
export function live_templates_expand(request_json: string): string;

/**
 * `SurroundRequest` (with `id`) → `Expansion | null`.
 */
export function live_templates_surround(request_json: string): string;

/**
 * `SurroundRequest` → `SurroundItem[]`.
 */
export function live_templates_surround_items(request_json: string): string;

/**
 * `UnwrapRequest` → `UnwrapOption[]`.
 */
export function live_templates_unwrap(request_json: string): string;

/**
 * `{ text }` → variable names.
 */
export function live_templates_variables(request_json: string): string;

/**
 * `{ op, … }` → the result of the operation (see [`Call`]).
 */
export function local_history_call(request_json: string): string;

/**
 * `{ path, content? }` -> `FileStructure | null`.
 */
export function lua_file_structure(request_json: string): string;

/**
 * `{ path, offset, content? }` -> `Location[]`.
 */
export function lua_goto(request_json: string): string;

/**
 * `{ path, offset, content? }` -> `Hover | null`.
 */
export function lua_hover(request_json: string): string;

/**
 * Replace the indexed files: `{ files: { path: text }, requirePaths?, name? }` -> stats.
 * Files other than `.lua` are ignored, except `.luarc.json`.
 */
export function lua_index_files(request_json: string): string;

/**
 * `{ path, content? }` -> `RunMarker[]`.
 */
export function lua_run_markers(request_json: string): string;

/**
 * `{ query, kind: "class" | "symbol", limit? }` -> `SymbolHit[]`.
 */
export function lua_search_symbols(request_json: string): string;

/**
 * `{ text, anchor }` → 0-based line or `null`.
 */
export function markdown_anchor_line(request_json: string): string;

/**
 * `{ text }` → `Document` (the Markdown preview).
 */
export function markdown_document(request_json: string): string;

/**
 * `{ markdown }` → `HoverDoc`.
 */
export function markdown_hover(request_json: string): string;

/**
 * `{ text, flavor? }` → `Block[]`.
 */
export function markdown_parse(request_json: string): string;

/**
 * `{ project, path, scopes, showMembers }` → `NavSegment[]`.
 */
export function nav_bar_segments(request_json: string): string;

/**
 * Mirrors the `output_file_links` command: `{ text, root }` → `FileLink[]`.
 */
export function output_file_links(request_json: string): string;

/**
 * `{ interpreter, root, composeDir?, volumes? }` → the automatic `PathMapping[]`.
 */
export function php_interpreter_automatic_mappings(request_json: string): string;

/**
 * `{ phpV, phpIni, phpM }` (outputs of `php -v`, `php --ini`, `php -m`) → `InterpreterInfo`.
 */
export function php_interpreter_parse_info(request_json: string): string;

/**
 * `WrapRequest` → `{ program, args, cwd, env, kind, pathMappings }`.
 */
export function php_interpreter_wrap(request_json: string): string;

/**
 * `{ composerJson, composerPath, sourceRoots }` → `SourceRootEntry[]` (Settings | Directories).
 */
export function php_source_roots(request_json: string): string;

/**
 * `{ knownHosts, host, port, keyType, key, accept? }` → `{ status, fingerprint, ..., knownHosts? }`
 * (the verdict, and the updated file when a confirmed key was added).
 */
export function remote_check_host_key(request_json: string): string;

/**
 * `{ text, home, user }` → the concrete `Host` aliases (`HostEntry[]`).
 */
export function remote_ssh_hosts(request_json: string): string;

/**
 * `SshConfiguration` → problems worded for the dialog (`string[]`, empty when valid).
 */
export function remote_validate_config(config_json: string): string;

/**
 * `{ request, with, files }` → `{ files: [{ path, text, count }] }`: the files a replace changes,
 * with their new text (the browser-mode demo project keeps them in memory).
 */
export function replace_in_files(request_json: string): string;

/**
 * `{ request, with, files }` → the changed lines per file, like the `replace_preview` command.
 */
export function replace_preview(request_json: string): string;

/**
 * `{ history, entry }` → the new history.
 */
export function run_anything_push_history(request_json: string): string;

/**
 * `{ command, cwd? }` → the temporary `RunConfiguration`.
 */
export function run_anything_resolve(request_json: string): string;

/**
 * `SuggestRequest` (with npm / Composer scripts and PHP files filled in) → `SuggestResponse`.
 */
export function run_anything_suggest(request_json: string): string;

/**
 * `{ configurations, target, packageManager? }` → `LaunchPlan`.
 */
export function run_launch_plan(request_json: string): string;

export function run_output_drop(request_json: string): string;

/**
 * `{ handle }` → `{ tree, summary, counts }` of a test run (`null` without tests), for the
 * browser-mode Test History.
 */
export function run_output_export(request_json: string): string;

/**
 * `{ handle, input: RunInput }` → `OutputEvent[]`.
 */
export function run_output_handle(request_json: string): string;

/**
 * `RunOutputOptions` → handle (a number) for [`run_output_handle`].
 */
export function run_output_new(options_json: string): string;

/**
 * `{ base, original, formatted }` → the text with only changed lines reformatted.
 */
export function save_actions_changed_lines(request_json: string): string;

/**
 * `{}` → the file types of the "Reformat code" filter.
 */
export function save_actions_file_types(_request_json: string): string;

/**
 * `{ path, trigger, settings }` → `SavePlan`.
 */
export function save_actions_plan(request_json: string): string;

/**
 * `{ pattern }` → `ParseError | null`.
 */
export function scopes_check(request_json: string): string;

/**
 * `{ composerJson }` → `string[]`: the test folders of a `composer.json` (browser mode's
 * `scopes_test_roots`).
 */
export function scopes_composer_test_roots(request_json: string): string;

/**
 * `{ filter, paths }` → `boolean[]`: which files a scope contains.
 */
export function scopes_contains(request_json: string): string;

/**
 * `EditRequest` → the new pattern (Include / Exclude buttons).
 */
export function scopes_edit(request_json: string): string;

/**
 * `ScopeSet` → `{ entries: ScopeEntry[], problems: [name, message][] }`.
 */
export function scopes_entries(request_json: string): string;

/**
 * `{ files, filter }` → `ScopeTree`.
 */
export function scopes_tree(request_json: string): string;

/**
 * `{ public?, private?, name? }` → `{ names, default, variables }`: the environments of
 * `http-client.env.json` / `http-client.private.env.json` and the variables of `name`.
 */
export function scratches_http_environments(request_json: string): string;

/**
 * `{ script, response, globals }` → `{ report, globals, text }`: run a response handler.
 */
export function scratches_http_handler(request_json: string): string;

/**
 * `{ request?: HttpRequest, response?: HttpResponse, error?: string }` → console text.
 */
export function scratches_http_render(request_json: string): string;

/**
 * `{ content, line, env?, globals?, timestamp?, seed?, base?, files? }` → `{ request,
 * problems, handler }` for the request at `line`, or `null`. `files` maps paths (relative
 * to the project or the scratches) to contents for `< ./body.json` and `> ./handler.js`,
 * resolved against `base`, the `.http` file's directory.
 */
export function scratches_http_request(request_json: string): string;

/**
 * `LanguageRequest` → `LanguageItem[]` (the New Scratch File popup).
 */
export function scratches_languages(request_json: string): string;

/**
 * Register the plugins' file types (`PluginInfo[]` JSON) for the language list.
 */
export function scratches_load(plugins_json: string): string;

/**
 * `{ name, content }` → gutter markers.
 */
export function scratches_markers(request_json: string): string;

/**
 * `{ language, existing }` → `{ name, extension }` of a new scratch (`scratch_1.php`).
 */
export function scratches_new_name(request_json: string): string;

/**
 * `{ existing: [name, modified][] }` → `{ name, reused }` for "New Scratch Buffer".
 */
export function scratches_next_buffer(request_json: string): string;

/**
 * `{ file, context: RunContext }` → `RunCommand`.
 */
export function scratches_run_command(request_json: string): string;

/**
 * `{ name }` → the scratch's runner (`php`, `http`, …) or `null`.
 */
export function scratches_runner(request_json: string): string;

/**
 * `{ name }` → `null` or the reason the name is invalid.
 */
export function scratches_validate_name(request_json: string): string;

/**
 * `{ files, query, typesOnly, limit, skipExtensions }` → `SymbolHit[]`: the classes and
 * symbols the desktop app's search index finds (`forge-search-core`), over in-memory files.
 */
export function search_find_symbols(request_json: string): string;

/**
 * `{ query, paths, limit }` → `FileMatch[]`: "Go to File" over a list of paths (the
 * browser-mode demo project instead of a file walk).
 */
export function search_match_files(request_json: string): string;

/**
 * `RankRequest` → `RankResponse`.
 */
export function search_rank(request_json: string): string;

/**
 * `{ input, nodeId }` → `Details | null`, mirroring `services_details`.
 */
export function services_details(request_json: string): string;

/**
 * `{ path, content }` → `GutterMarker[]`, mirroring `services_gutter`.
 */
export function services_gutter(request_json: string): string;

/**
 * `ServicesInput` → `Node[]`, mirroring `services_tree`.
 */
export function services_tree(input_json: string): string;

/**
 * `SearchRequest` → `SearchResponse`.
 */
export function settings_search(request_json: string): string;

/**
 * `{ spec, value }` → the problem with the value, or `null` when it is valid.
 */
export function settings_validate(request_json: string): string;

/**
 * `ActionsRequest` → `ContextAction[]`, like `spell_context_actions`.
 */
export function spell_context_actions(request_json: string): string;

/**
 * The bundled dictionaries, like `spell_dictionaries` without custom ones.
 */
export function spell_dictionaries(): string;

/**
 * `InspectRequest` → `Problem[]`, like `spell_inspect`.
 */
export function spell_inspect(request_json: string): string;

/**
 * `{ dialect, schema, table, pk, changes }` → the statements, or `{ error }`.
 */
export function sql_change_statements(request_json: string): string;

/**
 * Mirrors `db_export`: `{ format, dialect, table, columns, rows }` → the text.
 */
export function sql_export(request_json: string): string;

/**
 * `{ statements: string[] }` → their leading keywords, upper-cased.
 */
export function sql_leading_keywords(request_json: string): string;

/**
 * `{ sql, dialect }` → `Statement[]` (byte offsets, like `db_execute`).
 */
export function sql_split(request_json: string): string;

/**
 * Mirrors `db_statement_at`: `{ sql, from, to, dialect }` → `Statement | null`.
 */
export function sql_statement_at(request_json: string): string;

/**
 * `{}` → `SavedTemplate[]` (the built-in templates).
 */
export function ssr_templates(_request_json: string): string;

/**
 * `{ template, replacement? }` → `TemplateCheck` without the syntax check.
 */
export function ssr_variables(request_json: string): string;

/**
 * `{ text, context, files }` → `TraceAnalysis`.
 */
export function stacktrace_analyze(request_json: string): string;

/**
 * `{ nodes, regions }` → the nodes with region nodes (`kind: "region"`) around members.
 */
export function structure_group_by_regions(request_json: string): string;

/**
 * `{ custom }` (the `fileTemplates.custom` value) → `{ templates, variables }`.
 */
export function templates_list(request_json: string): string;

/**
 * `{ request: NewFileRequest, custom }` → `NewFile`.
 */
export function templates_new_file(request_json: string): string;

/**
 * `{ composerJson, composerPath, namespace }` → `string | null`.
 */
export function templates_php_directory(request_json: string): string;

/**
 * `{ composerJson, composerPath, dir }` → `string | null`.
 */
export function templates_php_namespace(request_json: string): string;

/**
 * `{ text }` → `string | null`.
 */
export function templates_validate(request_json: string): string;

/**
 * `{ uri, root? }` → `LinkTarget | null`.
 */
export function terminal_link_target(request_json: string): string;

/**
 * `{ code, data, previous? }` → `OscAction`.
 */
export function terminal_parse_osc(request_json: string): string;

/**
 * `{ configuration, framework, reportDir }` → `TestCommand` (the mock command lines).
 */
export function test_command(request_json: string): string;

/**
 * `{ path, text }` → `TestFile` (`test_discover`).
 */
export function test_discover(request_json: string): string;

/**
 * `{ configured, files, composerJson }` → `FrameworkConfig[]` (`test_frameworks_list`).
 */
export function test_frameworks_effective(request_json: string): string;

/**
 * `{ tree, root }` → `{ nodes, summary }` (`test_history_open`).
 */
export function test_history_import(request_json: string): string;

/**
 * `{ configuration, failed }` → `Configuration | null` (`test_rerun_failed`).
 */
export function test_rerun_failed(request_json: string): string;

/**
 * `FormatRequest` (settings and `.editorconfig` files included) → `FileFormat`.
 */
export function text_file_format(request_json: string): string;

/**
 * `{ message, rules }` → `MessageProblem[]`.
 */
export function text_inspect_commit_message(request_json: string): string;

/**
 * `SaveRequest` → `SaveResult`.
 */
export function text_prepare_save(request_json: string): string;

/**
 * `{ bytes, encoding? }` → `TextFile`.
 */
export function text_read(request_json: string): string;

/**
 * `{ text, eol?, encoding?, bom? }` → `{ bytes }` or `{ error }`.
 */
export function text_write(request_json: string): string;

/**
 * `{ request, files }` → `TodoResult`.
 */
export function todo_scan(request_json: string): string;

/**
 * `{ base, current }` → `ChangeHunk[]`; with `include` (hunk ids) → the text a partial
 * commit stores (`string`).
 */
export function vcs_change_hunks(request_json: string): string;

/**
 * `{ state, changed, hunks, op }` → `{ state, view, error }`: the changelist model of
 * `git_changelist_*` for the git mock (`forge_vcs::changelists`).
 */
export function vcs_changelists(request_json: string): string;

/**
 * `SelectionFacts` → `LogActionStates`: which Git Log actions the git mock's selection allows.
 */
export function vcs_log_action_states(facts_json: string): string;

/**
 * `{ base, left, right }` → `MergeModel`.
 */
export function vcs_merge(request_json: string): string;

/**
 * `{ patch, base, files }` → `PatchPreviewFile[]` (Apply Patch dialog in the git mock).
 */
export function vcs_patch_preview(request_json: string): string;

/**
 * `RebaseCommit[]` (oldest first) → the initial `RebaseEntry[]` of the rebase dialog.
 */
export function vcs_rebase_autosquash(commits_json: string): string;

/**
 * `RebaseEntry[]` (oldest first) → `{ steps, messages }`, the todo the git mock applies.
 */
export function vcs_rebase_todo(entries_json: string): string;

/**
 * `{text}` → `{handle}`: start tracking an editor.
 */
export function vim_attach(json: string): string;

/**
 * `{enabled, handlers}` → `true`: apply the settings.
 */
export function vim_configure(json: string): string;

/**
 * `{handle}` → `true`: forget an editor.
 */
export function vim_detach(json: string): string;

/**
 * `{handle | null}` → `true`: the focused editor (null on blur).
 */
export function vim_focus(json: string): string;

/**
 * `{handlers}` → `HandlerRow[]`: the handler table with these settings (plus the rc file).
 */
export function vim_handlers(json: string): string;

/**
 * `{handle, event?, sync}` → `Response`. Without `event` it only syncs and continues
 * queued keys (after the effects of a `resume` answer were performed).
 */
export function vim_key(json: string): string;

/**
 * `{text}` → `RcReport`: apply a `~/.ideavimrc`.
 */
export function vim_load_rc(json: string): string;

/**
 * `{text}` → `true`: the system clipboard changed (the `+` and `*` registers).
 */
export function vim_set_clipboard(json: string): string;

/**
 * `{handle}` → `Response`: the editor's mode and caret without a key.
 */
export function vim_state(json: string): string;

/**
 * `{handle, sync}` → `Response`: `timeoutlen` passed without a key.
 */
export function vim_timeout(json: string): string;

/**
 * `{ framework, files, paths }` -> `Component[]`.
 */
export function web_components(request_json: string): string;

/**
 * Extract Component: `{ path, content, start, end, name, newPath }` -> `ExtractedComponent`.
 */
export function web_extract_component(request_json: string): string;

/**
 * Extract Component's analysis: `{ path, content, start, end }` -> `ExtractComponentInfo`.
 */
export function web_extract_component_prepare(request_json: string): string;

/**
 * `{ framework, files, paths, path, content, offset }` -> `Location[]`.
 */
export function web_goto(request_json: string): string;

/**
 * `{ framework, files, paths, path, content }` -> `GutterMark[]`.
 */
export function web_gutter(request_json: string): string;

/**
 * `{ path, content }` -> `GutterMark[]` (run icons of a package.json).
 */
export function web_npm_gutter(request_json: string): string;

/**
 * `{ files, paths, packageManager? }` -> `NpmPackage[]`.
 */
export function web_npm_packages(request_json: string): string;

/**
 * `{ setting, os, desktop }` → `WindowChrome`.
 */
export function window_chrome(request_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly agent_parse_lines: (a: number, b: number, c: number) => void;
    readonly agent_profiles: (a: number) => void;
    readonly behat_step_definitions: (a: number, b: number, c: number) => void;
    readonly bookmarks_apply: (a: number, b: number, c: number) => void;
    readonly bridge_version: (a: number) => void;
    readonly chat_apply: (a: number, b: number, c: number) => void;
    readonly chat_begin_turn: (a: number, b: number, c: number) => void;
    readonly chat_drop: (a: number, b: number, c: number) => void;
    readonly chat_new: (a: number) => void;
    readonly chat_set_permission: (a: number, b: number, c: number) => void;
    readonly cli_tools_complete: (a: number, b: number, c: number) => void;
    readonly cli_tools_effective: (a: number, b: number, c: number) => void;
    readonly cli_tools_parse_input: (a: number, b: number, c: number) => void;
    readonly cli_tools_parse_list: (a: number, b: number, c: number) => void;
    readonly clipboard_history_add: (a: number, b: number, c: number) => void;
    readonly clipboard_history_list: (a: number, b: number, c: number) => void;
    readonly clipboard_history_promote: (a: number, b: number, c: number) => void;
    readonly clipboard_history_remove: (a: number, b: number, c: number) => void;
    readonly codeintel_apply_edit: (a: number, b: number, c: number) => void;
    readonly codeintel_context_actions: (a: number, b: number, c: number) => void;
    readonly codeintel_context_headers: (a: number, b: number, c: number) => void;
    readonly codeintel_definition_snippet: (a: number, b: number, c: number) => void;
    readonly codeintel_doc_url: (a: number, b: number, c: number) => void;
    readonly codeintel_filter_usages: (a: number, b: number, c: number) => void;
    readonly codeintel_group_usages: (a: number, b: number, c: number) => void;
    readonly codeintel_highlight_usages: (a: number, b: number, c: number) => void;
    readonly codeintel_inlay_hints: (a: number, b: number, c: number) => void;
    readonly codeintel_new_test_plan: (a: number, b: number, c: number) => void;
    readonly codeintel_order_actions: (a: number, b: number, c: number) => void;
    readonly codeintel_plan_edit: (a: number, b: number, c: number) => void;
    readonly codeintel_preview_edit: (a: number, b: number, c: number) => void;
    readonly codeintel_refactor_this_actions: (a: number, b: number, c: number) => void;
    readonly codeintel_related: (a: number, b: number, c: number) => void;
    readonly codeintel_rename_edit: (a: number, b: number, c: number) => void;
    readonly codeintel_test_source: (a: number, b: number, c: number) => void;
    readonly codeintel_test_targets: (a: number, b: number, c: number) => void;
    readonly codeintel_type_info: (a: number, b: number, c: number) => void;
    readonly codeintel_usage_target: (a: number, b: number, c: number) => void;
    readonly codeintel_word_range: (a: number, b: number, c: number) => void;
    readonly codestyle_export: (a: number, b: number, c: number) => void;
    readonly codestyle_file_format: (a: number, b: number, c: number) => void;
    readonly codestyle_import: (a: number, b: number, c: number) => void;
    readonly codestyle_model: (a: number) => void;
    readonly codestyle_plan_save: (a: number, b: number, c: number) => void;
    readonly codestyle_post_format: (a: number, b: number, c: number) => void;
    readonly codestyle_preview: (a: number, b: number, c: number) => void;
    readonly codestyle_schemes: (a: number, b: number, c: number) => void;
    readonly colors_find: (a: number, b: number, c: number) => void;
    readonly colors_format: (a: number, b: number, c: number) => void;
    readonly colors_parse: (a: number, b: number, c: number) => void;
    readonly compare_dir_listings: (a: number, b: number, c: number) => void;
    readonly composer_audit_problems: (a: number, b: number, c: number) => void;
    readonly composer_complete: (a: number, b: number, c: number) => void;
    readonly composer_hover: (a: number, b: number, c: number) => void;
    readonly composer_markers: (a: number, b: number, c: number) => void;
    readonly composer_project: (a: number, b: number, c: number) => void;
    readonly composer_scripts: (a: number, b: number, c: number) => void;
    readonly composer_search: (a: number, b: number, c: number) => void;
    readonly containers_compose_args: (a: number, b: number, c: number) => void;
    readonly containers_compose_parse: (a: number, b: number, c: number) => void;
    readonly containers_docker_contexts: (a: number, b: number, c: number) => void;
    readonly containers_docker_snapshot: (a: number, b: number, c: number) => void;
    readonly containers_dockerfile_plan: (a: number, b: number, c: number) => void;
    readonly containers_kubeconfig_parse: (a: number, b: number, c: number) => void;
    readonly containers_port_forward_specs: (a: number, b: number, c: number) => void;
    readonly containers_prefix_log: (a: number, b: number, c: number) => void;
    readonly containers_pull_new: (a: number, b: number, c: number) => void;
    readonly containers_pull_push: (a: number, b: number, c: number) => void;
    readonly containers_terminal_command: (a: number, b: number, c: number) => void;
    readonly containers_workloads_parse: (a: number, b: number, c: number) => void;
    readonly csv_edit: (a: number, b: number, c: number) => void;
    readonly csv_parse: (a: number, b: number, c: number) => void;
    readonly csv_sort_order: (a: number, b: number, c: number) => void;
    readonly dql_complete: (a: number, b: number, c: number) => void;
    readonly dql_diagnostics: (a: number, b: number, c: number) => void;
    readonly dql_goto: (a: number, b: number, c: number) => void;
    readonly dql_highlight: (a: number, b: number, c: number) => void;
    readonly edit_history: (a: number, b: number, c: number) => void;
    readonly editing_complete_statement: (a: number, b: number, c: number) => void;
    readonly editing_expand_word: (a: number, b: number, c: number) => void;
    readonly editing_postfix_complete: (a: number, b: number, c: number) => void;
    readonly editing_postfix_templates: (a: number, b: number, c: number) => void;
    readonly editor_change_target: (a: number, b: number, c: number) => void;
    readonly editor_doc_comments: (a: number, b: number, c: number) => void;
    readonly editor_method_target: (a: number, b: number, c: number) => void;
    readonly editor_structure: (a: number, b: number, c: number) => void;
    readonly editor_structure_languages: (a: number) => void;
    readonly emmet_edit_point: (a: number, b: number, c: number) => void;
    readonly emmet_expand: (a: number, b: number, c: number) => void;
    readonly emmet_remove_tag: (a: number, b: number, c: number) => void;
    readonly emmet_update_tag: (a: number, b: number, c: number) => void;
    readonly emmet_wrap: (a: number, b: number, c: number) => void;
    readonly external_tool_command: (a: number, b: number, c: number) => void;
    readonly external_tool_macros: (a: number, b: number, c: number) => void;
    readonly external_tool_validate: (a: number, b: number, c: number) => void;
    readonly file_colors_resolve: (a: number, b: number, c: number) => void;
    readonly file_structure_view: (a: number, b: number, c: number) => void;
    readonly filetypes_detect: (a: number, b: number, c: number) => void;
    readonly filetypes_load: (a: number, b: number, c: number) => void;
    readonly filetypes_template_delimiters: (a: number) => void;
    readonly find_in_files: (a: number, b: number, c: number) => void;
    readonly find_page: (a: number, b: number, c: number) => void;
    readonly find_paged: (a: number, b: number, c: number) => void;
    readonly fuzzy_filter: (a: number, b: number, c: number) => void;
    readonly hierarchy_call_view: (a: number, b: number, c: number) => void;
    readonly hierarchy_method_view: (a: number, b: number, c: number) => void;
    readonly hierarchy_type_view: (a: number, b: number, c: number) => void;
    readonly image_describe: (a: number, b: number, c: number) => void;
    readonly imports_folder_summary: (a: number, b: number, c: number, d: number) => void;
    readonly imports_minimal_edit: (a: number, b: number, c: number) => void;
    readonly imports_optimize_answer: (a: number, b: number, c: number) => void;
    readonly imports_targets: (a: number, b: number, c: number) => void;
    readonly injections_at: (a: number, b: number, c: number) => void;
    readonly injections_context_actions: (a: number, b: number, c: number) => void;
    readonly injections_escape: (a: number, b: number, c: number) => void;
    readonly injections_find: (a: number, b: number, c: number) => void;
    readonly injections_fragment: (a: number, b: number, c: number) => void;
    readonly injections_inject: (a: number, b: number, c: number) => void;
    readonly injections_languages: (a: number) => void;
    readonly injections_rules: (a: number) => void;
    readonly json_schema_builtins: (a: number) => void;
    readonly json_schema_complete: (a: number, b: number, c: number) => void;
    readonly json_schema_goto: (a: number, b: number, c: number) => void;
    readonly json_schema_hover: (a: number, b: number, c: number) => void;
    readonly json_schema_read: (a: number, b: number, c: number) => void;
    readonly json_schema_validate: (a: number, b: number, c: number) => void;
    readonly keymap_actions: (a: number, b: number, c: number) => void;
    readonly keymap_conflicts: (a: number, b: number, c: number) => void;
    readonly keymap_find_by_shortcut: (a: number, b: number, c: number) => void;
    readonly keymap_format: (a: number, b: number, c: number) => void;
    readonly keymap_handle_key: (a: number, b: number, c: number) => void;
    readonly keymap_legacy_changes: (a: number, b: number, c: number) => void;
    readonly keymap_load_bindings: (a: number, b: number, c: number) => void;
    readonly keymap_main_menu: (a: number, b: number, c: number) => void;
    readonly keymap_register_actions: (a: number, b: number, c: number) => void;
    readonly keymap_resolve: (a: number, b: number, c: number) => void;
    readonly keymap_set_override: (a: number, b: number, c: number) => void;
    readonly keymap_stroke_of_event: (a: number, b: number, c: number) => void;
    readonly live_templates_applicable: (a: number, b: number, c: number) => void;
    readonly live_templates_catalog: (a: number, b: number, c: number) => void;
    readonly live_templates_expand: (a: number, b: number, c: number) => void;
    readonly live_templates_surround: (a: number, b: number, c: number) => void;
    readonly live_templates_surround_items: (a: number, b: number, c: number) => void;
    readonly live_templates_unwrap: (a: number, b: number, c: number) => void;
    readonly live_templates_variables: (a: number, b: number, c: number) => void;
    readonly local_history_call: (a: number, b: number, c: number) => void;
    readonly lua_file_structure: (a: number, b: number, c: number) => void;
    readonly lua_goto: (a: number, b: number, c: number) => void;
    readonly lua_hover: (a: number, b: number, c: number) => void;
    readonly lua_index_files: (a: number, b: number, c: number) => void;
    readonly lua_run_markers: (a: number, b: number, c: number) => void;
    readonly lua_search_symbols: (a: number, b: number, c: number) => void;
    readonly markdown_anchor_line: (a: number, b: number, c: number) => void;
    readonly markdown_document: (a: number, b: number, c: number) => void;
    readonly markdown_hover: (a: number, b: number, c: number) => void;
    readonly markdown_parse: (a: number, b: number, c: number) => void;
    readonly nav_bar_segments: (a: number, b: number, c: number) => void;
    readonly output_file_links: (a: number, b: number, c: number) => void;
    readonly php_interpreter_automatic_mappings: (a: number, b: number, c: number) => void;
    readonly php_interpreter_parse_info: (a: number, b: number, c: number) => void;
    readonly php_interpreter_wrap: (a: number, b: number, c: number) => void;
    readonly php_source_roots: (a: number, b: number, c: number) => void;
    readonly remote_check_host_key: (a: number, b: number, c: number) => void;
    readonly remote_ssh_hosts: (a: number, b: number, c: number) => void;
    readonly remote_validate_config: (a: number, b: number, c: number) => void;
    readonly replace_in_files: (a: number, b: number, c: number) => void;
    readonly replace_preview: (a: number, b: number, c: number) => void;
    readonly run_anything_push_history: (a: number, b: number, c: number) => void;
    readonly run_anything_resolve: (a: number, b: number, c: number) => void;
    readonly run_anything_suggest: (a: number, b: number, c: number) => void;
    readonly run_launch_plan: (a: number, b: number, c: number) => void;
    readonly run_output_drop: (a: number, b: number, c: number) => void;
    readonly run_output_export: (a: number, b: number, c: number) => void;
    readonly run_output_handle: (a: number, b: number, c: number) => void;
    readonly run_output_new: (a: number, b: number, c: number) => void;
    readonly save_actions_changed_lines: (a: number, b: number, c: number) => void;
    readonly save_actions_file_types: (a: number, b: number, c: number) => void;
    readonly save_actions_plan: (a: number, b: number, c: number) => void;
    readonly scopes_check: (a: number, b: number, c: number) => void;
    readonly scopes_composer_test_roots: (a: number, b: number, c: number) => void;
    readonly scopes_contains: (a: number, b: number, c: number) => void;
    readonly scopes_edit: (a: number, b: number, c: number) => void;
    readonly scopes_entries: (a: number, b: number, c: number) => void;
    readonly scopes_tree: (a: number, b: number, c: number) => void;
    readonly scratches_http_environments: (a: number, b: number, c: number) => void;
    readonly scratches_http_handler: (a: number, b: number, c: number) => void;
    readonly scratches_http_render: (a: number, b: number, c: number) => void;
    readonly scratches_http_request: (a: number, b: number, c: number) => void;
    readonly scratches_languages: (a: number, b: number, c: number) => void;
    readonly scratches_load: (a: number, b: number, c: number) => void;
    readonly scratches_markers: (a: number, b: number, c: number) => void;
    readonly scratches_new_name: (a: number, b: number, c: number) => void;
    readonly scratches_next_buffer: (a: number, b: number, c: number) => void;
    readonly scratches_run_command: (a: number, b: number, c: number) => void;
    readonly scratches_runner: (a: number, b: number, c: number) => void;
    readonly scratches_validate_name: (a: number, b: number, c: number) => void;
    readonly search_find_symbols: (a: number, b: number, c: number) => void;
    readonly search_match_files: (a: number, b: number, c: number) => void;
    readonly search_rank: (a: number, b: number, c: number) => void;
    readonly services_details: (a: number, b: number, c: number) => void;
    readonly services_gutter: (a: number, b: number, c: number) => void;
    readonly services_tree: (a: number, b: number, c: number) => void;
    readonly settings_search: (a: number, b: number, c: number) => void;
    readonly settings_validate: (a: number, b: number, c: number) => void;
    readonly spell_context_actions: (a: number, b: number, c: number) => void;
    readonly spell_dictionaries: (a: number) => void;
    readonly spell_inspect: (a: number, b: number, c: number) => void;
    readonly sql_change_statements: (a: number, b: number, c: number) => void;
    readonly sql_export: (a: number, b: number, c: number) => void;
    readonly sql_leading_keywords: (a: number, b: number, c: number) => void;
    readonly sql_split: (a: number, b: number, c: number) => void;
    readonly sql_statement_at: (a: number, b: number, c: number) => void;
    readonly ssr_templates: (a: number, b: number, c: number) => void;
    readonly ssr_variables: (a: number, b: number, c: number) => void;
    readonly stacktrace_analyze: (a: number, b: number, c: number) => void;
    readonly structure_group_by_regions: (a: number, b: number, c: number) => void;
    readonly templates_list: (a: number, b: number, c: number) => void;
    readonly templates_new_file: (a: number, b: number, c: number) => void;
    readonly templates_php_directory: (a: number, b: number, c: number) => void;
    readonly templates_php_namespace: (a: number, b: number, c: number) => void;
    readonly templates_validate: (a: number, b: number, c: number) => void;
    readonly terminal_link_target: (a: number, b: number, c: number) => void;
    readonly terminal_parse_osc: (a: number, b: number, c: number) => void;
    readonly test_command: (a: number, b: number, c: number) => void;
    readonly test_discover: (a: number, b: number, c: number) => void;
    readonly test_frameworks_effective: (a: number, b: number, c: number) => void;
    readonly test_history_import: (a: number, b: number, c: number) => void;
    readonly test_rerun_failed: (a: number, b: number, c: number) => void;
    readonly text_file_format: (a: number, b: number, c: number) => void;
    readonly text_inspect_commit_message: (a: number, b: number, c: number) => void;
    readonly text_prepare_save: (a: number, b: number, c: number) => void;
    readonly text_read: (a: number, b: number, c: number) => void;
    readonly text_write: (a: number, b: number, c: number) => void;
    readonly todo_scan: (a: number, b: number, c: number) => void;
    readonly vcs_change_hunks: (a: number, b: number, c: number) => void;
    readonly vcs_changelists: (a: number, b: number, c: number) => void;
    readonly vcs_log_action_states: (a: number, b: number, c: number) => void;
    readonly vcs_merge: (a: number, b: number, c: number) => void;
    readonly vcs_patch_preview: (a: number, b: number, c: number) => void;
    readonly vcs_rebase_autosquash: (a: number, b: number, c: number) => void;
    readonly vcs_rebase_todo: (a: number, b: number, c: number) => void;
    readonly vim_attach: (a: number, b: number, c: number) => void;
    readonly vim_configure: (a: number, b: number, c: number) => void;
    readonly vim_detach: (a: number, b: number, c: number) => void;
    readonly vim_focus: (a: number, b: number, c: number) => void;
    readonly vim_handlers: (a: number, b: number, c: number) => void;
    readonly vim_key: (a: number, b: number, c: number) => void;
    readonly vim_load_rc: (a: number, b: number, c: number) => void;
    readonly vim_set_clipboard: (a: number, b: number, c: number) => void;
    readonly vim_state: (a: number, b: number, c: number) => void;
    readonly vim_timeout: (a: number, b: number, c: number) => void;
    readonly web_components: (a: number, b: number, c: number) => void;
    readonly web_extract_component: (a: number, b: number, c: number) => void;
    readonly web_extract_component_prepare: (a: number, b: number, c: number) => void;
    readonly web_goto: (a: number, b: number, c: number) => void;
    readonly web_gutter: (a: number, b: number, c: number) => void;
    readonly web_npm_gutter: (a: number, b: number, c: number) => void;
    readonly web_npm_packages: (a: number, b: number, c: number) => void;
    readonly window_chrome: (a: number, b: number, c: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
