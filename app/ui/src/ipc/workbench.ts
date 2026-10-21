/**
 * TODO scanning (Rust: `forge-find::todo`, walked through the project's backend so remote
 * projects are scanned on their host) and bookmarks (Rust: `forge-bookmarks`, stored per
 * project in Forge's state dir). Columns are UTF-16, lines 0-based.
 */
import { invoke } from "./index";

export interface TodoPattern {
  pattern: string;
  caseSensitive?: boolean;
}

export interface TodoRequest {
  /** Empty: PHPStorm's defaults (`\btodo\b.*`, `\bfixme\b.*`). */
  patterns?: TodoPattern[];
  /** Only these files (live updates); omitted = the whole project. */
  paths?: string[];
  /** Unsaved editor contents by path; they replace the files on disk. */
  buffers?: Record<string, string>;
  limit?: number;
}

export interface TodoItem {
  line: number;
  column: number;
  length: number;
  text: string;
  /** Index of the matching pattern. */
  pattern: number;
}

export interface TodoFile {
  path: string;
  items: TodoItem[];
}

export interface TodoResult {
  files: TodoFile[];
  count: number;
  truncated: boolean;
  error?: string;
}

export interface Bookmark {
  id: number;
  path: string;
  /** 0-based line. */
  line: number;
  /** `0`–`9` or `A`–`Z`. */
  mnemonic?: string;
  description?: string;
}

export interface BookmarkList {
  name: string;
  bookmarks: Bookmark[];
}

export interface Bookmarks {
  lists: BookmarkList[];
  defaultList: string;
  nextId: number;
}

/** A document change in line terms, in the coordinates before the change. */
export interface LineEdit {
  line: number;
  removed: number;
  added: number;
  atLineStart?: boolean;
}

export type BookmarkOp =
  | { op: "toggle"; path: string; line: number }
  | { op: "setMnemonic"; path: string; line: number; mnemonic: string | null }
  | { op: "remove"; id: number }
  | { op: "clear"; list: string | null }
  | { op: "describe"; id: number; description: string }
  | { op: "moveToList"; id: number; list: string }
  | { op: "addList"; name: string }
  | { op: "renameList"; from: string; to: string }
  | { op: "removeList"; name: string }
  | { op: "setDefaultList"; name: string }
  | { op: "applyEdits"; path: string; edits: LineEdit[] }
  | { op: "renamePath"; from: string; to: string }
  | { op: "removePath"; path: string };

export const todoApi = {
  scan: (request: TodoRequest) => invoke<TodoResult>("todo_scan", { request }),
};

export const bookmarksApi = {
  get: () => invoke<Bookmarks>("bookmarks_get"),
  apply: (ops: BookmarkOp[]) => invoke<Bookmarks>("bookmarks_apply", { ops }),
};
