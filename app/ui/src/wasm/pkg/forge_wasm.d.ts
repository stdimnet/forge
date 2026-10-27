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
 * `{ devices, avds }` (outputs of `adb devices -l` and `emulator -list-avds`) →
 * `{ devices: Device[], avds: string[] }`.
 */
export function android_adb(request_json: string): string;

/**
 * `{ run: AppRun, windows? }` → `{ program, args }` (an Android App run).
 */
export function android_app_command(request_json: string): string;

/**
 * `{ text, query, mine?, sources?, ps?, limit? }` → `LogcatView` (`android_logcat_view`).
 */
export function android_logcat_view(request_json: string): string;

/**
 * `{ build, manifest? }` → `{ module: AndroidModule | null, launcher: string | null }`.
 */
export function android_module(request_json: string): string;

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
 * `{ text, offset }` → completion or null (`cmake_complete`).
 */
export function cmake_complete(request_json: string): string;

/**
 * `{ text }` → configure messages (`cmake_reload`'s `messages`).
 */
export function cmake_parse_log(request_json: string): string;

/**
 * `{ entries, files, root }` → `{ profiles, presetFiles, presetErrors }` (`cmake_profiles`
 * without the active one).
 */
export function cmake_profiles(request_json: string): string;

/**
 * `{ files, query }` → the answer of `phpforge_fw_cms::api::answer`.
 */
export function cms_query(request_json: string): string;

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
 * `{settings, dark, change}` → `SettingWrite[]`.
 */
export function colorscheme_apply(json: string): string;

/**
 * `{settings, name}` → the scheme as .icls text.
 */
export function colorscheme_export(json: string): string;

/**
 * `{settings, text}` → `SettingWrite[]` that add the imported scheme and select it.
 */
export function colorscheme_import(json: string): string;

/**
 * `{settings, dark}` → `SchemesSnapshot`.
 */
export function colorscheme_state(json: string): string;

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
 * `{ report }` → `{ path: hint }`: the Project view's hints of files and folders.
 */
export function coverage_hints(request_json: string): string;

/**
 * `{ report, project, title, generated, sources }` → the HTML coverage report.
 */
export function coverage_html(request_json: string): string;

/**
 * `{ report, path }` → `LineMark[]` (the editor's coverage gutter).
 */
export function coverage_marks(request_json: string): string;

/**
 * `{ reports }` → the suite fields of their merged report (Show Coverage Data with several
 * suites).
 */
export function coverage_merge(request_json: string): string;

/**
 * `{ text, root, cwd?, mappings, rules, testsXml? }` → `{ report, format, lines, branches,
 * methods, … }`: a coverage report with project paths.
 */
export function coverage_parse(request_json: string): string;

/**
 * `{ report, project }` → `CoverageNode[]` (the Coverage tool window).
 */
export function coverage_summary(request_json: string): string;

/**
 * `{ output, root, buildDir, files }` → build problems with project paths
 * (`cpp_build_problems`).
 */
export function cpp_build_problems(request_json: string): string;

/**
 * `{ text (perf script output), sampleIndex? }` → the Profiler's views.
 */
export function cpp_perf_view(request_json: string): string;

/**
 * `{ text, root, buildDir, files }` → sanitizer reports with project paths.
 */
export function cpp_sanitizer_reports(request_json: string): string;

/**
 * `{ text (Valgrind XML), root, files }` → the Valgrind report.
 */
export function cpp_valgrind_report(request_json: string): string;

/**
 * `{ text, offset, ports }` → completion or null (`cpp_vcpkg_complete`).
 */
export function cpp_vcpkg_complete(request_json: string): string;

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
 * `{ local: {path, dir, text, modified}[], remote: …, by }` → `DirCompare`.
 */
export function deploy_compare(request_json: string): string;

/**
 * Remote Host: `{ path, name, rename }` → `{ path }` of the new or renamed entry, or
 * `{ error }` for an invalid name.
 */
export function deploy_entry_path(request_json: string): string;

/**
 * Remote Host: `{ server, entries }` → the tree nodes (mappings, exclusions, order).
 */
export function deploy_remote_nodes(request_json: string): string;

/**
 * `{ server, paths, excludeNames }` → where every project path is deployed.
 */
export function deploy_resolve(request_json: string): string;

/**
 * `DeployServer` → the problems shown under the form.
 */
export function deploy_validate(server_json: string): string;

/**
 * `{ path, content }` → `number | null`.
 */
export function devcontainer_gutter(request_json: string): string;

/**
 * `docker ps` output (the format of `docker::list_args`) → `DevContainerInstance[]`.
 */
export function devcontainer_parse_list(output: string): string;

/**
 * `{ text, folder, configPath }` → `{ config, source } | { error }` (the browser mock has no
 * local environment, so `${localEnv:…}` is empty).
 */
export function devcontainer_resolve(request_json: string): string;

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
 * `{ names, patterns }` → for each name whether "Ignored Files and Folders" hides it (the
 * filter of `fs_list_dir`).
 */
export function filetypes_ignored(request_json: string): string;

/**
 * Mirrors the `filetypes_list` IPC command: `DetectRequest` → `FileTypeRow[]`.
 */
export function filetypes_list(request_json: string): string;

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

/**
 * `{ configuration, context }` → `GoCommand` of a Go Build configuration.
 */
export function go_build_command(request_json: string): string;

/**
 * `{ path, content, line? }` → `go generate` arguments.
 */
export function go_generate_args(request_json: string): string;

/**
 * `{ content }` → `GenerateDirective[]`.
 */
export function go_generate_directives(request_json: string): string;

/**
 * `{ tool: "vet" | "golangci", output, root }` → `GoProblem[]`.
 */
export function go_lint_parse(request_json: string): string;

/**
 * `{ path, content, offset }` → completion context or null.
 */
export function go_mod_completion_context(request_json: string): string;

/**
 * `{ path, content }` → `{ kind: "mod" | "work", … }` (`go_mod_parse`).
 */
export function go_mod_parse(request_json: string): string;

/**
 * `{ action, arg?, dir?, context }` → `GoCommand` (`go mod tidy`, `go get …`).
 */
export function go_module_command(request_json: string): string;

/**
 * `{ text }` (Groovy source) → `Runnable` (`groovy_runnable`).
 */
export function groovy_runnable(request_json: string): string;

/**
 * `{ run: ScriptRun, runner }` → `{ program, args, env }` (a Groovy Script run).
 */
export function groovy_script_command(request_json: string): string;

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
 * `{ path, source, text?, diagnostics }` → the diagnostics the profile keeps, with their
 * severity and `inspection` / `inspectionName` / `severityName` set; suppression comments
 * in `text` hide theirs. Without a configured profile everything is kept.
 */
export function inspections_apply(request_json: string): string;

/**
 * `{ text, edits }` → the text with the edits applied (browser mode's Apply Fix).
 */
export function inspections_apply_edits(request_json: string): string;

/**
 * `{ registry, problems, texts }` → `CleanupPlan` (browser mode's Code Cleanup).
 */
export function inspections_cleanup_plan(request_json: string): string;

/**
 * `{ registry, profile, severities, scopes, context }`: the profile the editor's
 * diagnostics go through from now on → `true`.
 */
export function inspections_configure(request_json: string): string;

/**
 * `{ registry, profile, format }` → the profile as PHPStorm XML or JSON.
 */
export function inspections_export_profile(request_json: string): string;

/**
 * `{ report, format: "html" | "xml" | "json" }` → the exported text.
 */
export function inspections_export_report(request_json: string): string;

/**
 * `{ registry, query, limit }` → matching `InspectionSpec`s (Run Inspection by Name).
 */
export function inspections_find(request_json: string): string;

/**
 * `{ registry, text }` → `Profile` (XML or JSON).
 */
export function inspections_import_profile(request_json: string): string;

/**
 * `{ registry, state, op }` → the changed `ProfilesState` (or `{ error }`).
 */
export function inspections_profile_op(request_json: string): string;

/**
 * `{ inspections, mappings }` (contributions of the enabled plugins) → `Registry` with the
 * framework's own inspections.
 */
export function inspections_registry(request_json: string): string;

/**
 * `{ report, levels }` → `ResultNode[]` (Inspection Results tree).
 */
export function inspections_report_tree(request_json: string): string;

/**
 * Browser mode's Inspect Code: `{ registry, profiles, profile?, files, problems, scopes,
 * context, only?, scope }` → `Report` (duplicated code is found here; `problems` are the
 * other providers' results).
 */
export function inspections_run(request_json: string): string;

/**
 * `{ custom }` → every severity (built-in, then custom).
 */
export function inspections_severities(request_json: string): string;

/**
 * `{ path, text, line, character, id, bytes? }` → `SuppressAction[]` (changes in UTF-16
 * offsets for the editor, in bytes with `bytes: true` for the backend).
 */
export function inspections_suppress_actions(request_json: string): string;

/**
 * `{ registry, profile, query }` → `ViewGroup` (the Settings tree with the profile's state).
 */
export function inspections_view(request_json: string): string;

/**
 * `{ path, text, offset, data }` → `CompletionOut | null` (`java_build_complete`).
 */
export function java_build_complete(request_json: string): string;

/**
 * `{ folder, files, prefer?, exec }` → `{ project, goals, program } | null` (`java_build_info`).
 */
export function java_build_info(request_json: string): string;

/**
 * `{ folder, files, prefer?, gradleModel?, mavenTree? }` → `BuildModel | null`
 * (`java_build_model`, `java_sync_project`).
 */
export function java_build_model(request_json: string): string;

/**
 * `{ path, text, versions }` → `OutdatedMarker[]` (`java_build_outdated`); `{ path, text }`
 * alone → the `group:artifact`s to look up (`coordinates`).
 */
export function java_build_outdated(request_json: string): string;

/**
 * `{ path, text, offset }` → `Request | null`: what Maven Central must answer.
 */
export function java_build_request(request_json: string): string;

/**
 * `{ configuration, build, coverage?, java?, debugPort? }` → `{ program, args, test? }`: the
 * command line of a Java configuration as the backend builds it (`run.rs`, `java.rs`);
 * JUnit / TestNG add their `TestCommand` (`test`).
 */
export function java_command_line(request_json: string): string;

/**
 * `{ path, text }` → `MainMethod[]` (`java_main_methods`).
 */
export function java_main_methods(request_json: string): string;

/**
 * `{ text, update? }` → `{ wrapper, text }`: `gradle-wrapper.properties` read (and updated).
 */
export function java_wrapper(request_json: string): string;

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
 * `{ frame, query }` → `Page`.
 */
export function jupyter_dataframe_query(request_json: string): string;

/**
 * `{ left, right }` → `NotebookDiff`.
 */
export function jupyter_diff(request_json: string): string;

/**
 * `{ text, root? }` → `NotebookView`.
 */
export function jupyter_open(request_json: string): string;

/**
 * `{ outputs, root? }` → `Rendered[]`.
 */
export function jupyter_render(request_json: string): string;

/**
 * `{ notebook, original? }` → the `.ipynb` text.
 */
export function jupyter_serialize(request_json: string): string;

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
 * `{button, clickCount, deltaY?, …modifiers, focus}` → the action id the sync resolver binds
 * to that mouse stroke (or null), without firing it: editor extensions that handle a click
 * themselves (Ctrl+Click to the declaration) ask this.
 */
export function keymap_mouse_action(json: string): string;

/**
 * DOM mouse / wheel event (`button`, `clickCount`, `deltaY`, modifiers) → canonical mouse
 * stroke ("Ctrl+Button1Click"), for "press a mouse shortcut" fields.
 */
export function keymap_mouse_stroke(event_json: string): string;

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
 * `{ configuration, build, coverage?, debugPort?, runner? }` → `{ program, args, env?, test? }`:
 * a Kotlin application, script or Kotest configuration as the backend builds it.
 */
export function kotlin_command_line(request_json: string): string;

/**
 * The debugger's answer to the coroutine dump expression → `CoroutineDump`
 * (`kotlin_coroutines`).
 */
export function kotlin_coroutines(value: string): string;

/**
 * `{ format, text, detekt? }` → `LintProblem[]` (canned reports of the mocks).
 */
export function kotlin_lint_report(request_json: string): string;

/**
 * `{ path, text }` → `MainFunction[]` (`kotlin_main_functions`).
 */
export function kotlin_main_functions(request_json: string): string;

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
 * The Invalidate Caches dialog.
 */
export function maintenance_invalidate_model(): string;

/**
 * `{ keys, files }` (user setting keys, existing config files) → the settings groups with
 * their counts.
 */
export function maintenance_settings_groups(request_json: string): string;

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
 * `{ configuration, node, dir, args }` → the arguments of a Node.js program under c8 or nyc
 * (Run with Coverage of a Node.js configuration).
 */
export function node_coverage_wrap(request_json: string): string;

/**
 * Mirrors the `output_file_links` command: `{ text, root }` → `FileLink[]`.
 */
export function output_file_links(request_json: string): string;

/**
 * `{ configuration, dir, files }` → `{ interpreterArgs, args, env, file, inDir, testsDir }`:
 * how a PHP test configuration runs with coverage.
 */
export function php_coverage_plan(request_json: string): string;

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
 * `{ pio, env, task, cwd }` → the task's command line.
 */
export function pio_command(request_json: string): string;

/**
 * `{ text }` → the parsed `platformio.ini`.
 */
export function pio_project(request_json: string): string;

/**
 * `{ base64?, sampleIndex? }` → `ProfileView` (`pprof_open`).
 */
export function pprof_view(request_json: string): string;

/**
 * `{ config }` → `FolderRow[]` (Settings | Directories).
 */
export function project_structure_describe(request_json: string): string;

/**
 * `{ config, paths: [path, isDir][] }` → `boolean[]` (excluded or hidden).
 */
export function project_structure_excluded(request_json: string): string;

/**
 * `{ config, path, action }` → the new `projectStructure.folders`.
 */
export function project_structure_mark(request_json: string): string;

/**
 * `{ config, path, isDir }` → `FolderState`.
 */
export function project_structure_state(request_json: string): string;

/**
 * `{ entries, listing, options }` → `TreeRow[]`.
 */
export function project_view_arrange(request_json: string): string;

/**
 * PHPStorm's predefined File Nesting rules.
 */
export function project_view_nesting_defaults(): string;

/**
 * `{ target, text, requirement }` -> the new text.
 */
export function python_add_requirement(request_json: string): string;

/**
 * `{ path, content, offset }` -> completion context or null.
 */
export function python_completion_context(request_json: string): string;

/**
 * `{ rows }` → `ProfileView`: a demo snapshot written as a real `.pstats` file and read
 * back by the parser the desktop app uses.
 */
export function python_demo_profile(request_json: string): string;

/**
 * `{ path, content }` -> the file's declared dependencies.
 */
export function python_dependencies(request_json: string): string;

/**
 * `{ request, files }` -> the steps that create a new environment.
 */
export function python_environment_plan(request_json: string): string;

/**
 * `{ path, content, installed?, imports }` -> `Problem[]`.
 */
export function python_inspect(request_json: string): string;

/**
 * An interpreter -> its status bar label.
 */
export function python_interpreter_label(request_json: string): string;

/**
 * `{ kind, uvLock, action, packages, python }` -> `{ manager, program, args }`.
 */
export function python_packages_command(request_json: string): string;

/**
 * `{ configuration, failed, files }` → `Configuration | null`.
 */
export function python_rerun_failed(request_json: string): string;

/**
 * `{ content }` -> run markers.
 */
export function python_run_markers(request_json: string): string;

/**
 * `{ configuration, cwd, reportDir, coverageFile?, reporter, toxProgram, head }` →
 * `PythonCommand`.
 */
export function python_test_command(request_json: string): string;

/**
 * `{ path, text, runner, files }` → `TestFile` (`test_discover` for Python files).
 */
export function python_test_discover(request_json: string): string;

/**
 * `{ files }` → `ProjectTesting`.
 */
export function python_testing_detect(request_json: string): string;

/**
 * `{ files }` → `ToxEnv[]`.
 */
export function python_tox_environments(request_json: string): string;

/**
 * `{ files, options, path, content, offset }` -> `Completions | null`.
 */
export function python_web_complete(request_json: string): string;

/**
 * `{ files, options, path, content, offset }` -> `Location[]`.
 */
export function python_web_goto(request_json: string): string;

/**
 * `{ files, options }` -> Django's commands plus the project's `management/commands`.
 */
export function python_web_manage_commands(request_json: string): string;

/**
 * `{ files, options, path, content }` -> `FileMarks`.
 */
export function python_web_marks(request_json: string): string;

/**
 * `{ files, options }` -> `WebProject`.
 */
export function python_web_project(request_json: string): string;

/**
 * `{ files, options, configuration }` -> `PythonRun`.
 */
export function python_web_run_info(request_json: string): string;

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
 * `{ target, invocation, root, compose?, docker }` → `{ program, args, cwd, pathMappings,
 * debug }`: a command line on a run target.
 */
export function run_target_wrap(request_json: string): string;

/**
 * `{ action, package?, target?, context }` → `CargoCommand` (`cargo_command`).
 */
export function rust_action_command(request_json: string): string;

/**
 * `{ configuration, context }` → `CargoCommand` of a "Cargo" configuration.
 */
export function rust_command_line(request_json: string): string;

/**
 * `{ path, text, context? }` → `TestFile` (`test_discover` for Rust files).
 */
export function rust_discover(request_json: string): string;

/**
 * `{ json, root }` → `CargoWorkspace` (`cargo_workspace`).
 */
export function rust_parse_metadata(request_json: string): string;

/**
 * `{ json, workspace }` → `RustProblem[]` (`rust_clippy`).
 */
export function rust_problems(request_json: string): string;

/**
 * `{ path, text, context? }` → `RunTarget[]` (`rust_file_targets`).
 */
export function rust_run_targets(request_json: string): string;

/**
 * `{ text }` → `ToolchainFile` of a `rust-toolchain.toml`.
 */
export function rust_toolchain_file(request_json: string): string;

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
 * `{ configuration, program }` → `{ program, args, test? }`: an sbt Task, ScalaTest or MUnit
 * configuration as the backend builds it.
 */
export function scala_command_line(request_json: string): string;

/**
 * `{ text }` (Scala source) → `MainObject[]` (`scala_main_objects`).
 */
export function scala_main_objects(request_json: string): string;

/**
 * `{ buildSbt, buildProperties?, pluginsSbt?, folder, syncOutput?, root }` → `SbtBuild`
 * (`scala_sbt_model`, `scala_sbt_import`).
 */
export function scala_sbt_build(request_json: string): string;

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
 * `KubeInput` → `KubeSelection | null`, mirroring `services_kube_selection`.
 */
export function services_kube_selection(kube_json: string): string;

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
 * `{ files, metadata, enums, path, content, offset }` plus `kind`: `complete` | `inspect`
 * | `hover` | `goto` (the `spring_config_*` commands).
 */
export function spring_config(kind: string, request_json: string): string;

/**
 * `{ apps }` → nodes; `{ apps, nodeId, runtime? }` → details (`spring_dashboard*`).
 */
export function spring_dashboard(request_json: string): string;

/**
 * `{ files, path, content }` → `Marker[]` (`spring_gutter`).
 */
export function spring_gutter(request_json: string): string;

/**
 * `{ endpoint, baseUrl }` → `.http` text (`spring_http_request`).
 */
export function spring_http_request(request_json: string): string;

/**
 * `{ options, vmOptions, arguments }` → `{ vmOptions, arguments }` of the Application run.
 */
export function spring_launch(request_json: string): string;

/**
 * Console lines → what the dashboard reads from them (`Observed`).
 */
export function spring_observe(lines_json: string): string;

/**
 * `{ files, metadata, enums, runtime? }` → the `spring_model` answer.
 */
export function spring_overview(request_json: string): string;

/**
 * `{ dialect, schema, table, pk, changes }` → the statements, or `{ error }`.
 */
export function sql_change_statements(request_json: string): string;

/**
 * Mirrors `db_export`: `{ format, dialect, table, columns, rows }` → the text.
 */
export function sql_export(request_json: string): string;

/**
 * Mirrors `db_guard`: `{ sql, dialect, readOnly, production, name }` → `{ blocked?, confirm? }`.
 */
export function sql_guard(request_json: string): string;

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
 * `{ text, peripheral, values }` → decoded registers, as if read from a target whose
 * registers hold `values` (else their reset values).
 */
export function svd_peripheral_values(request_json: string): string;

/**
 * `{ text }` → the SVD file's peripherals.
 */
export function svd_summary(request_json: string): string;

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
 * `{ configuration, spec, reportDir, coverageFile? }` → `TestCommand` of a manifest
 * framework.
 */
export function testing_command(request_json: string): string;

/**
 * `{ specs, files, texts }` → ids of the frameworks the project uses.
 */
export function testing_detect(request_json: string): string;

/**
 * `{ path, text, specs, detected? }` → `TestFile` (`test_discover`, every framework,
 * those the project uses first).
 */
export function testing_discover(request_json: string): string;

/**
 * `{ configuration, failed, specs }` → `Configuration | null` (`test_rerun_failed`).
 */
export function testing_rerun_failed(request_json: string): string;

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

/**
 * `{ files, query }` → the answer of `phpforge_fw_wordpress::api::answer`.
 */
export function wordpress_query(request_json: string): string;

/**
 * A Windows path → `LinuxPath | null`.
 */
export function wsl_map_path(path: string): string;

/**
 * `wsl.exe -l -v` output bytes → `Distro[]` (without Docker Desktop's distributions).
 */
export function wsl_parse_list(bytes: Uint8Array): string;

/**
 * `{ distro, path }` → the Windows view of a Linux path.
 */
export function wsl_windows_path(request_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly agent_parse_lines: (a: number, b: number, c: number) => void;
    readonly agent_profiles: (a: number) => void;
    readonly android_adb: (a: number, b: number, c: number) => void;
    readonly android_app_command: (a: number, b: number, c: number) => void;
    readonly android_logcat_view: (a: number, b: number, c: number) => void;
    readonly android_module: (a: number, b: number, c: number) => void;
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
    readonly cmake_complete: (a: number, b: number, c: number) => void;
    readonly cmake_parse_log: (a: number, b: number, c: number) => void;
    readonly cmake_profiles: (a: number, b: number, c: number) => void;
    readonly cms_query: (a: number, b: number, c: number) => void;
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
    readonly colorscheme_apply: (a: number, b: number, c: number) => void;
    readonly colorscheme_export: (a: number, b: number, c: number) => void;
    readonly colorscheme_import: (a: number, b: number, c: number) => void;
    readonly colorscheme_state: (a: number, b: number, c: number) => void;
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
    readonly coverage_hints: (a: number, b: number, c: number) => void;
    readonly coverage_html: (a: number, b: number, c: number) => void;
    readonly coverage_marks: (a: number, b: number, c: number) => void;
    readonly coverage_merge: (a: number, b: number, c: number) => void;
    readonly coverage_parse: (a: number, b: number, c: number) => void;
    readonly coverage_summary: (a: number, b: number, c: number) => void;
    readonly cpp_build_problems: (a: number, b: number, c: number) => void;
    readonly cpp_perf_view: (a: number, b: number, c: number) => void;
    readonly cpp_sanitizer_reports: (a: number, b: number, c: number) => void;
    readonly cpp_valgrind_report: (a: number, b: number, c: number) => void;
    readonly cpp_vcpkg_complete: (a: number, b: number, c: number) => void;
    readonly csv_edit: (a: number, b: number, c: number) => void;
    readonly csv_parse: (a: number, b: number, c: number) => void;
    readonly csv_sort_order: (a: number, b: number, c: number) => void;
    readonly deploy_compare: (a: number, b: number, c: number) => void;
    readonly deploy_entry_path: (a: number, b: number, c: number) => void;
    readonly deploy_remote_nodes: (a: number, b: number, c: number) => void;
    readonly deploy_resolve: (a: number, b: number, c: number) => void;
    readonly deploy_validate: (a: number, b: number, c: number) => void;
    readonly devcontainer_gutter: (a: number, b: number, c: number) => void;
    readonly devcontainer_parse_list: (a: number, b: number, c: number) => void;
    readonly devcontainer_resolve: (a: number, b: number, c: number) => void;
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
    readonly filetypes_ignored: (a: number, b: number, c: number) => void;
    readonly filetypes_list: (a: number, b: number, c: number) => void;
    readonly filetypes_load: (a: number, b: number, c: number) => void;
    readonly filetypes_template_delimiters: (a: number) => void;
    readonly find_in_files: (a: number, b: number, c: number) => void;
    readonly find_page: (a: number, b: number, c: number) => void;
    readonly find_paged: (a: number, b: number, c: number) => void;
    readonly fuzzy_filter: (a: number, b: number, c: number) => void;
    readonly go_build_command: (a: number, b: number, c: number) => void;
    readonly go_generate_args: (a: number, b: number, c: number) => void;
    readonly go_generate_directives: (a: number, b: number, c: number) => void;
    readonly go_lint_parse: (a: number, b: number, c: number) => void;
    readonly go_mod_completion_context: (a: number, b: number, c: number) => void;
    readonly go_mod_parse: (a: number, b: number, c: number) => void;
    readonly go_module_command: (a: number, b: number, c: number) => void;
    readonly groovy_runnable: (a: number, b: number, c: number) => void;
    readonly groovy_script_command: (a: number, b: number, c: number) => void;
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
    readonly inspections_apply: (a: number, b: number, c: number) => void;
    readonly inspections_apply_edits: (a: number, b: number, c: number) => void;
    readonly inspections_cleanup_plan: (a: number, b: number, c: number) => void;
    readonly inspections_configure: (a: number, b: number, c: number) => void;
    readonly inspections_export_profile: (a: number, b: number, c: number) => void;
    readonly inspections_export_report: (a: number, b: number, c: number) => void;
    readonly inspections_find: (a: number, b: number, c: number) => void;
    readonly inspections_import_profile: (a: number, b: number, c: number) => void;
    readonly inspections_profile_op: (a: number, b: number, c: number) => void;
    readonly inspections_registry: (a: number, b: number, c: number) => void;
    readonly inspections_report_tree: (a: number, b: number, c: number) => void;
    readonly inspections_run: (a: number, b: number, c: number) => void;
    readonly inspections_severities: (a: number, b: number, c: number) => void;
    readonly inspections_suppress_actions: (a: number, b: number, c: number) => void;
    readonly inspections_view: (a: number, b: number, c: number) => void;
    readonly java_build_complete: (a: number, b: number, c: number) => void;
    readonly java_build_info: (a: number, b: number, c: number) => void;
    readonly java_build_model: (a: number, b: number, c: number) => void;
    readonly java_build_outdated: (a: number, b: number, c: number) => void;
    readonly java_build_request: (a: number, b: number, c: number) => void;
    readonly java_command_line: (a: number, b: number, c: number) => void;
    readonly java_main_methods: (a: number, b: number, c: number) => void;
    readonly java_wrapper: (a: number, b: number, c: number) => void;
    readonly json_schema_builtins: (a: number) => void;
    readonly json_schema_complete: (a: number, b: number, c: number) => void;
    readonly json_schema_goto: (a: number, b: number, c: number) => void;
    readonly json_schema_hover: (a: number, b: number, c: number) => void;
    readonly json_schema_read: (a: number, b: number, c: number) => void;
    readonly json_schema_validate: (a: number, b: number, c: number) => void;
    readonly jupyter_dataframe_query: (a: number, b: number, c: number) => void;
    readonly jupyter_diff: (a: number, b: number, c: number) => void;
    readonly jupyter_open: (a: number, b: number, c: number) => void;
    readonly jupyter_render: (a: number, b: number, c: number) => void;
    readonly jupyter_serialize: (a: number, b: number, c: number) => void;
    readonly keymap_actions: (a: number, b: number, c: number) => void;
    readonly keymap_conflicts: (a: number, b: number, c: number) => void;
    readonly keymap_find_by_shortcut: (a: number, b: number, c: number) => void;
    readonly keymap_format: (a: number, b: number, c: number) => void;
    readonly keymap_handle_key: (a: number, b: number, c: number) => void;
    readonly keymap_legacy_changes: (a: number, b: number, c: number) => void;
    readonly keymap_load_bindings: (a: number, b: number, c: number) => void;
    readonly keymap_main_menu: (a: number, b: number, c: number) => void;
    readonly keymap_mouse_action: (a: number, b: number, c: number) => void;
    readonly keymap_mouse_stroke: (a: number, b: number, c: number) => void;
    readonly keymap_register_actions: (a: number, b: number, c: number) => void;
    readonly keymap_resolve: (a: number, b: number, c: number) => void;
    readonly keymap_set_override: (a: number, b: number, c: number) => void;
    readonly keymap_stroke_of_event: (a: number, b: number, c: number) => void;
    readonly kotlin_command_line: (a: number, b: number, c: number) => void;
    readonly kotlin_coroutines: (a: number, b: number, c: number) => void;
    readonly kotlin_lint_report: (a: number, b: number, c: number) => void;
    readonly kotlin_main_functions: (a: number, b: number, c: number) => void;
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
    readonly maintenance_invalidate_model: (a: number) => void;
    readonly maintenance_settings_groups: (a: number, b: number, c: number) => void;
    readonly markdown_anchor_line: (a: number, b: number, c: number) => void;
    readonly markdown_document: (a: number, b: number, c: number) => void;
    readonly markdown_hover: (a: number, b: number, c: number) => void;
    readonly markdown_parse: (a: number, b: number, c: number) => void;
    readonly nav_bar_segments: (a: number, b: number, c: number) => void;
    readonly node_coverage_wrap: (a: number, b: number, c: number) => void;
    readonly output_file_links: (a: number, b: number, c: number) => void;
    readonly php_coverage_plan: (a: number, b: number, c: number) => void;
    readonly php_interpreter_automatic_mappings: (a: number, b: number, c: number) => void;
    readonly php_interpreter_parse_info: (a: number, b: number, c: number) => void;
    readonly php_interpreter_wrap: (a: number, b: number, c: number) => void;
    readonly php_source_roots: (a: number, b: number, c: number) => void;
    readonly pio_command: (a: number, b: number, c: number) => void;
    readonly pio_project: (a: number, b: number, c: number) => void;
    readonly pprof_view: (a: number, b: number, c: number) => void;
    readonly project_structure_describe: (a: number, b: number, c: number) => void;
    readonly project_structure_excluded: (a: number, b: number, c: number) => void;
    readonly project_structure_mark: (a: number, b: number, c: number) => void;
    readonly project_structure_state: (a: number, b: number, c: number) => void;
    readonly project_view_arrange: (a: number, b: number, c: number) => void;
    readonly project_view_nesting_defaults: (a: number) => void;
    readonly python_add_requirement: (a: number, b: number, c: number) => void;
    readonly python_completion_context: (a: number, b: number, c: number) => void;
    readonly python_demo_profile: (a: number, b: number, c: number) => void;
    readonly python_dependencies: (a: number, b: number, c: number) => void;
    readonly python_environment_plan: (a: number, b: number, c: number) => void;
    readonly python_inspect: (a: number, b: number, c: number) => void;
    readonly python_interpreter_label: (a: number, b: number, c: number) => void;
    readonly python_packages_command: (a: number, b: number, c: number) => void;
    readonly python_rerun_failed: (a: number, b: number, c: number) => void;
    readonly python_run_markers: (a: number, b: number, c: number) => void;
    readonly python_test_command: (a: number, b: number, c: number) => void;
    readonly python_test_discover: (a: number, b: number, c: number) => void;
    readonly python_testing_detect: (a: number, b: number, c: number) => void;
    readonly python_tox_environments: (a: number, b: number, c: number) => void;
    readonly python_web_complete: (a: number, b: number, c: number) => void;
    readonly python_web_goto: (a: number, b: number, c: number) => void;
    readonly python_web_manage_commands: (a: number, b: number, c: number) => void;
    readonly python_web_marks: (a: number, b: number, c: number) => void;
    readonly python_web_project: (a: number, b: number, c: number) => void;
    readonly python_web_run_info: (a: number, b: number, c: number) => void;
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
    readonly run_target_wrap: (a: number, b: number, c: number) => void;
    readonly rust_action_command: (a: number, b: number, c: number) => void;
    readonly rust_command_line: (a: number, b: number, c: number) => void;
    readonly rust_discover: (a: number, b: number, c: number) => void;
    readonly rust_parse_metadata: (a: number, b: number, c: number) => void;
    readonly rust_problems: (a: number, b: number, c: number) => void;
    readonly rust_run_targets: (a: number, b: number, c: number) => void;
    readonly rust_toolchain_file: (a: number, b: number, c: number) => void;
    readonly save_actions_changed_lines: (a: number, b: number, c: number) => void;
    readonly save_actions_file_types: (a: number, b: number, c: number) => void;
    readonly save_actions_plan: (a: number, b: number, c: number) => void;
    readonly scala_command_line: (a: number, b: number, c: number) => void;
    readonly scala_main_objects: (a: number, b: number, c: number) => void;
    readonly scala_sbt_build: (a: number, b: number, c: number) => void;
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
    readonly services_kube_selection: (a: number, b: number, c: number) => void;
    readonly services_tree: (a: number, b: number, c: number) => void;
    readonly settings_search: (a: number, b: number, c: number) => void;
    readonly settings_validate: (a: number, b: number, c: number) => void;
    readonly spell_context_actions: (a: number, b: number, c: number) => void;
    readonly spell_dictionaries: (a: number) => void;
    readonly spell_inspect: (a: number, b: number, c: number) => void;
    readonly spring_config: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly spring_dashboard: (a: number, b: number, c: number) => void;
    readonly spring_gutter: (a: number, b: number, c: number) => void;
    readonly spring_http_request: (a: number, b: number, c: number) => void;
    readonly spring_launch: (a: number, b: number, c: number) => void;
    readonly spring_observe: (a: number, b: number, c: number) => void;
    readonly spring_overview: (a: number, b: number, c: number) => void;
    readonly sql_change_statements: (a: number, b: number, c: number) => void;
    readonly sql_export: (a: number, b: number, c: number) => void;
    readonly sql_guard: (a: number, b: number, c: number) => void;
    readonly sql_leading_keywords: (a: number, b: number, c: number) => void;
    readonly sql_split: (a: number, b: number, c: number) => void;
    readonly sql_statement_at: (a: number, b: number, c: number) => void;
    readonly ssr_templates: (a: number, b: number, c: number) => void;
    readonly ssr_variables: (a: number, b: number, c: number) => void;
    readonly stacktrace_analyze: (a: number, b: number, c: number) => void;
    readonly structure_group_by_regions: (a: number, b: number, c: number) => void;
    readonly svd_peripheral_values: (a: number, b: number, c: number) => void;
    readonly svd_summary: (a: number, b: number, c: number) => void;
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
    readonly testing_command: (a: number, b: number, c: number) => void;
    readonly testing_detect: (a: number, b: number, c: number) => void;
    readonly testing_discover: (a: number, b: number, c: number) => void;
    readonly testing_rerun_failed: (a: number, b: number, c: number) => void;
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
    readonly wordpress_query: (a: number, b: number, c: number) => void;
    readonly wsl_map_path: (a: number, b: number, c: number) => void;
    readonly wsl_parse_list: (a: number, b: number, c: number) => void;
    readonly wsl_windows_path: (a: number, b: number, c: number) => void;
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
