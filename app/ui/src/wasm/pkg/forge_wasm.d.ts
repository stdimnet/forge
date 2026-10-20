/* tslint:disable */
/* eslint-disable */

/**
 * Version of the bridge, used by the UI to check that the bundle is current.
 */
export function bridge_version(): string;

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
