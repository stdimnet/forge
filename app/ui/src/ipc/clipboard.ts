/**
 * Clipboard history for Paste from History (`crates/forge-editing/src/clipboard.rs`,
 * `app/src-tauri/src/commands/clipboard.rs`): the texts copied or cut in Forge, newest
 * first, kept in memory for the session. Browser mode runs the same Rust code as
 * WebAssembly.
 */
import { invoke } from "./index";

export interface ClipEntry {
  /** The copied text, `\n` line breaks. */
  text: string;
  /** The text of each caret when it was copied with several carets. */
  carets?: string[];
}

/** An entry as Paste from History lists it. */
export interface ClipItem extends ClipEntry {
  /** Position in the history (0 = newest); the id for `remove` and `promote`. */
  index: number;
  /** First non-blank line, trimmed and shortened. */
  preview: string;
  lines: number;
  chars: number;
}

export const clipboardApi = {
  /** Remember a copied text; `max` is the configured history size. */
  add: (entry: ClipEntry, max?: number) => invoke<boolean>("clipboard_history_add", { entry, max }),
  list: (query?: string, max?: number) => invoke<ClipItem[]>("clipboard_history_list", { query, max }),
  remove: (index: number) => invoke<boolean>("clipboard_history_remove", { index }),
  /** Move a pasted entry to the top. */
  promote: (index: number) => invoke<ClipEntry | null>("clipboard_history_promote", { index }),
};
