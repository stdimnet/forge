/**
 * The LightEdit window (`app/src-tauri/src/commands/light_edit.rs`, tab model in
 * `forge_window::light_edit`): files opened from outside a project ("Open With → Forge",
 * `forge -e file`) as tabs of one lightweight editor window. Tab ids are
 * `light-edit:<path with forward slashes>`; only files that are tabs can be read and written.
 */
import { invoke, listen } from "./index";

export interface LightEditFile {
  /** Tab id: `light-edit:` plus the path with `/` separators (the editor tab path). */
  id: string;
  /** The absolute path as the OS spells it. */
  path: string;
  name: string;
}

export interface LightEditSnapshot {
  files: LightEditFile[];
  active: string | null;
  /** The file the last open request asked for, with its position. */
  reveal: { id: string; line: number | null; column: number | null } | null;
}

export const LIGHT_EDIT_PREFIX = "light-edit:";
export const LIGHT_EDIT_CHANGED_EVENT = "light-edit://changed";

export const lightEditApi = {
  state: () => invoke<LightEditSnapshot>("light_edit_state"),
  /** File | Open File… in the LightEdit window (absolute paths from the system dialog). */
  open: (paths: string[]) => invoke<LightEditSnapshot>("light_edit_open", { paths }),
  close: (id: string) => invoke<LightEditSnapshot>("light_edit_close", { id }),
  activate: (id: string) => invoke<LightEditSnapshot>("light_edit_activate", { id }),
  /** The file's text with `\n` separators (Rust keeps its encoding and separator for saving). */
  read: (id: string) => invoke<string>("light_edit_read", { id }),
  write: (id: string, text: string) => invoke<void>("light_edit_write", { id, text }),
  /** Open a folder as a project in the project window (opened first if needed). */
  openProject: (path: string) => invoke<void>("light_edit_open_project", { path }),
  /** Files added by a later launch. */
  onChanged: (cb: (s: LightEditSnapshot) => void) => listen<LightEditSnapshot>(LIGHT_EDIT_CHANGED_EVENT, cb),
};
