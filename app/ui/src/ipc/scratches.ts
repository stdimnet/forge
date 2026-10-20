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
  /** Send the request at `line` (0-based) and stream the response to `run://<runId>`. */
  httpRun: (runId: string, content: string, line: number, env?: string) =>
    invoke<RunStarted>("scratches_http_run", { runId, content, line, env }),
  onChanged: (cb: () => void) => listen<null>("scratches://changed", () => cb()),
};
