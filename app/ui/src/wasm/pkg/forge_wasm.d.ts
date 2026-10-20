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
    readonly dql_complete: (a: number, b: number, c: number) => void;
    readonly dql_diagnostics: (a: number, b: number, c: number) => void;
    readonly dql_goto: (a: number, b: number, c: number) => void;
    readonly dql_highlight: (a: number, b: number, c: number) => void;
    readonly markdown_hover: (a: number, b: number, c: number) => void;
    readonly markdown_parse: (a: number, b: number, c: number) => void;
    readonly output_file_links: (a: number, b: number, c: number) => void;
    readonly run_output_drop: (a: number, b: number, c: number) => void;
    readonly run_output_handle: (a: number, b: number, c: number) => void;
    readonly run_output_new: (a: number, b: number, c: number) => void;
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
