/* tslint:disable */
/* eslint-disable */

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
 * `ComposeCommand` → arguments after `docker`.
 */
export function containers_compose_args(request_json: string): string;

/**
 * `{ path, content, env?, project? }` → `ComposeFile`.
 */
export function containers_compose_parse(request_json: string): string;

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
 * `DetectRequest` → `FileTypeInfo[]`.
 */
export function filetypes_detect(request_json: string): string;

/**
 * Register the file types of plugins (`PluginInfo[]` JSON) on top of the bundled ones.
 * Returns the number of known file types.
 */
export function filetypes_load(plugins_json: string): string;

/**
 * `FilterRequest` → `FilterHit[]`.
 */
export function fuzzy_filter(request_json: string): string;

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
 * Resolve one `KeyInput` with the sync resolver → `Resolution`.
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
 * `{ markdown }` → `HoverDoc`.
 */
export function markdown_hover(request_json: string): string;

/**
 * `{ text, flavor? }` → `Block[]`.
 */
export function markdown_parse(request_json: string): string;

/**
 * Mirrors the `output_file_links` command: `{ text, root }` → `FileLink[]`.
 */
export function output_file_links(request_json: string): string;

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

export function run_output_drop(request_json: string): string;

/**
 * `{ handle, input: RunInput }` → `OutputEvent[]`.
 */
export function run_output_handle(request_json: string): string;

/**
 * `RunOutputOptions` → handle (a number) for [`run_output_handle`].
 */
export function run_output_new(options_json: string): string;

/**
 * `{ request?: HttpRequest, response?: HttpResponse, error?: string }` → console text.
 */
export function scratches_http_render(request_json: string): string;

/**
 * `{ content, line, env? }` → the resolved request at `line` (`HttpRequest`) or `null`.
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

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly bridge_version: (a: number) => void;
    readonly chat_apply: (a: number, b: number, c: number) => void;
    readonly chat_begin_turn: (a: number, b: number, c: number) => void;
    readonly chat_drop: (a: number, b: number, c: number) => void;
    readonly chat_new: (a: number) => void;
    readonly chat_set_permission: (a: number, b: number, c: number) => void;
    readonly containers_compose_args: (a: number, b: number, c: number) => void;
    readonly containers_compose_parse: (a: number, b: number, c: number) => void;
    readonly containers_docker_snapshot: (a: number, b: number, c: number) => void;
    readonly containers_dockerfile_plan: (a: number, b: number, c: number) => void;
    readonly containers_kubeconfig_parse: (a: number, b: number, c: number) => void;
    readonly containers_pull_new: (a: number, b: number, c: number) => void;
    readonly containers_pull_push: (a: number, b: number, c: number) => void;
    readonly containers_terminal_command: (a: number, b: number, c: number) => void;
    readonly containers_workloads_parse: (a: number, b: number, c: number) => void;
    readonly dql_complete: (a: number, b: number, c: number) => void;
    readonly dql_diagnostics: (a: number, b: number, c: number) => void;
    readonly dql_goto: (a: number, b: number, c: number) => void;
    readonly dql_highlight: (a: number, b: number, c: number) => void;
    readonly filetypes_detect: (a: number, b: number, c: number) => void;
    readonly filetypes_load: (a: number, b: number, c: number) => void;
    readonly fuzzy_filter: (a: number, b: number, c: number) => void;
    readonly keymap_actions: (a: number, b: number, c: number) => void;
    readonly keymap_conflicts: (a: number, b: number, c: number) => void;
    readonly keymap_find_by_shortcut: (a: number, b: number, c: number) => void;
    readonly keymap_format: (a: number, b: number, c: number) => void;
    readonly keymap_handle_key: (a: number, b: number, c: number) => void;
    readonly keymap_legacy_changes: (a: number, b: number, c: number) => void;
    readonly keymap_load_bindings: (a: number, b: number, c: number) => void;
    readonly keymap_register_actions: (a: number, b: number, c: number) => void;
    readonly keymap_resolve: (a: number, b: number, c: number) => void;
    readonly keymap_set_override: (a: number, b: number, c: number) => void;
    readonly keymap_stroke_of_event: (a: number, b: number, c: number) => void;
    readonly lua_file_structure: (a: number, b: number, c: number) => void;
    readonly lua_goto: (a: number, b: number, c: number) => void;
    readonly lua_hover: (a: number, b: number, c: number) => void;
    readonly lua_index_files: (a: number, b: number, c: number) => void;
    readonly lua_run_markers: (a: number, b: number, c: number) => void;
    readonly lua_search_symbols: (a: number, b: number, c: number) => void;
    readonly markdown_hover: (a: number, b: number, c: number) => void;
    readonly markdown_parse: (a: number, b: number, c: number) => void;
    readonly output_file_links: (a: number, b: number, c: number) => void;
    readonly remote_check_host_key: (a: number, b: number, c: number) => void;
    readonly remote_ssh_hosts: (a: number, b: number, c: number) => void;
    readonly remote_validate_config: (a: number, b: number, c: number) => void;
    readonly run_output_drop: (a: number, b: number, c: number) => void;
    readonly run_output_handle: (a: number, b: number, c: number) => void;
    readonly run_output_new: (a: number, b: number, c: number) => void;
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
    readonly search_match_files: (a: number, b: number, c: number) => void;
    readonly search_rank: (a: number, b: number, c: number) => void;
    readonly services_details: (a: number, b: number, c: number) => void;
    readonly services_gutter: (a: number, b: number, c: number) => void;
    readonly services_tree: (a: number, b: number, c: number) => void;
    readonly settings_search: (a: number, b: number, c: number) => void;
    readonly settings_validate: (a: number, b: number, c: number) => void;
    readonly sql_change_statements: (a: number, b: number, c: number) => void;
    readonly sql_export: (a: number, b: number, c: number) => void;
    readonly sql_leading_keywords: (a: number, b: number, c: number) => void;
    readonly sql_split: (a: number, b: number, c: number) => void;
    readonly sql_statement_at: (a: number, b: number, c: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
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
