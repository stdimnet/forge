/**
 * Vim emulation (`crates/forge-vim`). The engine runs in-process as WebAssembly in the
 * desktop app and in browser mode (`features/vim/engine.ts`), because every editor key
 * press needs a synchronous answer. Only reading `~/.ideavimrc` goes to the backend.
 */
import { invoke } from "./index";

export interface VimRc {
  /** The file that was read; null when there is none. */
  path: string | null;
  /** Its text, with `source <file>` lines inlined. */
  text: string;
}

export const vimApi = {
  /** `~/.ideavimrc` (or `path`, the `vim.rcFile` setting). */
  readRc: (path?: string) => invoke<VimRc>("vim_read_rc", { path: path || null }),
};

/** A DOM key event as the engine reads it. */
export interface VimKeyEvent {
  key: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/** An edit in UTF-16 offsets; the edits of a response apply one after another. */
export interface VimChange {
  from: number;
  to: number;
  insert: string;
}

export interface VimRange {
  anchor: number;
  head: number;
}

/** What changed in the editor since the last call. */
export interface VimSync {
  text?: string;
  /** Batches of changes, each in the coordinates of the document before it. */
  changes?: VimChange[][];
  selection?: VimRange[];
  main?: number;
  /** Fully visible lines, 0-based. */
  viewport?: { top: number; bottom: number };
  indentUnit?: string;
}

export type VimEffect =
  | { kind: "undo"; count: number }
  | { kind: "redo"; count: number }
  | { kind: "action"; id: string }
  | { kind: "actionOnRange"; id: string; from: number; to: number; caret: number }
  | { kind: "open"; path: string }
  | { kind: "clipboard"; text: string }
  | { kind: "reveal"; pos: number; align: "start" | "center" | "end" | "nearest" }
  | { kind: "scrollTo"; line: number }
  | { kind: "reloadRc" }
  | { kind: "bell" };

export type VimMode = "normal" | "insert" | "replace" | "visual" | "visualLine" | "visualBlock";

export interface VimResponse {
  consumed: boolean;
  changes: VimChange[];
  selection: VimRange[] | null;
  main: number;
  /** Where the block caret is drawn. */
  cursor: number;
  mode: VimMode;
  /** Keys of an unfinished command ("2d", "\"a"). */
  showcmd: string;
  /** The command line being typed (":s/a/b"). */
  cmdline: string | null;
  message: { text: string; error: boolean } | null;
  effects: VimEffect[];
  /** Search matches to highlight: a regular expression source valid in JavaScript. */
  highlight: { source: string; ignoreCase: boolean } | null;
  recording: string | null;
  docLength: number;
  /** Queued keys wait for the effects; call the engine again without a key afterwards. */
  resume: boolean;
  /** A mapping may continue: call `timeout` after `timeoutlen` ms. */
  wait: boolean;
  number: boolean;
  relativenumber: boolean;
  scrolloff: number;
  timeoutlen: number;
}

export type VimHandler = "vim" | "ide";

/** A row of Settings | Editor | Vim | Shortcut Conflicts. */
export interface VimHandlerRow {
  shortcut: string;
  key: string;
  normal: VimHandler;
  insert: VimHandler;
  visual: VimHandler;
  normalAction: string | null;
  insertAction: string | null;
  visualAction: string | null;
  /** A `sethandler` line in ~/.ideavimrc decides it. */
  fromRc: boolean;
}

/** A stored override of the `vim.shortcutHandlers` setting. */
export interface VimHandlerOverride {
  shortcut: string;
  normal: string;
  insert: string;
  visual: string;
}

export interface VimRcReport {
  applied: number;
  problems: { line: number; text: string; message: string }[];
}
