/**
 * Scratches and Consoles (Rust: `crates/forge-scratches`, `commands/scratches.rs`). Scratch
 * files are global, stored in `<config dir>/forge/scratches/`; the UI addresses them by name.
 */
import type { RunStarted } from "./run";
import { invoke, listen } from "./index";

/** How a scratch runs. */
export type ScratchRunner = "php" | "node" | "typeScript" | "python" | "shell" | "lua" | "http" | "sql";

export interface ScratchFile {
  name: string;
  /** Absolute path on disk. */
  path: string;
  size: number;
  /** Milliseconds since the Unix epoch. */
  modified: number;
  runner?: ScratchRunner;
  /** Data source attached to a SQL scratch. */
  dataSource?: string;
}

export interface ScratchLanguage {
  /** File type id (`php`, `http`) or `text`. */
  id: string;
  name: string;
  extension: string;
  /** Matched character positions in `name`. */
  positions: number[];
}

export interface LanguageRequest {
  query: string;
  /** File type of the active editor, listed first. */
  current?: string;
  /** Recently used languages, most recent first. */
  recent?: string[];
}

export interface ScratchMarker {
  /** 0-based line. */
  line: number;
  kind: "run" | "http";
  title: string;
  /** Name of the run's tab, e.g. `rest-api | Show order`. */
  runName: string;
}

/** Environments of `http-client.env.json` / `http-client.private.env.json`. */
export interface HttpEnvironments {
  names: string[];
  /** Used while none is selected (`dev`, else the first). */
  default?: string;
}

export const scratchesApi = {
  list: () => invoke<ScratchFile[]>("scratches_list"),
  languages: (request: LanguageRequest) => invoke<ScratchLanguage[]>("scratches_languages", { request }),
  create: (language: string, content?: string) => invoke<ScratchFile>("scratches_create", { language, content }),
  createBuffer: () => invoke<ScratchFile>("scratches_create_buffer"),
  read: (name: string) => invoke<string>("scratches_read", { name }),
  write: (name: string, contents: string) => invoke<void>("scratches_write", { name, contents }),
  rename: (name: string, newName: string) => invoke<ScratchFile>("scratches_rename", { name, newName }),
  delete: (name: string) => invoke<void>("scratches_delete", { name }),
  setDataSource: (name: string, dataSourceId: string | null) =>
    invoke<ScratchFile>("scratches_set_data_source", { name, dataSourceId }),
  markers: (name: string, content: string) => invoke<ScratchMarker[]>("scratches_markers", { name, content }),
  /**
   * Send the request at `line` (0-based) and stream the response (and the report of its
   * response handler) to `run://<runId>`. `source` is the editor path of the `.http` file
   * (`scratch://<name>` or a project path); `< ./body.json` files are relative to it.
   */
  /** `debug` adds the Xdebug session cookie. */
  httpRun: (runId: string, content: string, line: number, env?: string, source?: string, debug = false) =>
    invoke<RunStarted>("scratches_http_run", { runId, content, line, env, source, debug }),
  httpEnvironments: () => invoke<HttpEnvironments>("scratches_http_environments"),
  onChanged: (cb: () => void) => listen<null>("scratches://changed", () => cb()),
};
