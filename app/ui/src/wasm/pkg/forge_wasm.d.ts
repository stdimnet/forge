/* tslint:disable */
/* eslint-disable */

/**
 * Version of the bridge, used by the UI to check that the bundle is current.
 */
export function bridge_version(): string;

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
 * `{ query, paths, limit }` → `FileMatch[]`: "Go to File" over a list of paths (the
 * browser-mode demo project instead of a file walk).
 */
export function search_match_files(request_json: string): string;

/**
 * `RankRequest` → `RankResponse`.
 */
export function search_rank(request_json: string): string;

/**
 * `SearchRequest` → `SearchResponse`.
 */
export function settings_search(request_json: string): string;

/**
 * `{ spec, value }` → the problem with the value, or `null` when it is valid.
 */
export function settings_validate(request_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly bridge_version: (a: number) => void;
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
    readonly search_match_files: (a: number, b: number, c: number) => void;
    readonly search_rank: (a: number, b: number, c: number) => void;
    readonly settings_search: (a: number, b: number, c: number) => void;
    readonly settings_validate: (a: number, b: number, c: number) => void;
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
