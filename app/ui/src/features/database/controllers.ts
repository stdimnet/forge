/**
 * Open consoles and data editors register a controller under their tab path so keymap
 * actions (Execute, Submit, Add Row, ...) can reach the one that has focus.
 */
import { editor } from "@/state/editor";

export interface ConsoleController {
  execute: () => Promise<void>;
  cancel: () => void;
  focus: () => void;
  /** Browse Query History (SQL consoles). */
  history?: () => void;
}

export interface DataEditorController {
  submit: () => Promise<void>;
  revert: () => void;
  addRow: () => void;
  deleteRows: () => void;
  reload: () => Promise<void>;
}

export const consoles = new Map<string, ConsoleController>();
export const dataEditors = new Map<string, DataEditorController>();

/** Tab path of the view containing the focused element (`data-db-path`), if any. */
function focusedPath(): string | null {
  const el = document.activeElement as HTMLElement | null;
  return el?.closest<HTMLElement>("[data-db-path]")?.dataset.dbPath ?? null;
}

export const focusedConsole = (): ConsoleController | undefined => {
  const p = focusedPath();
  return p ? consoles.get(p) : undefined;
};

/** The data editor with focus, else the one in the active tab. */
export const activeDataEditor = (): DataEditorController | undefined => {
  const p = focusedPath();
  return (p ? dataEditors.get(p) : undefined) ?? (editor.active ? dataEditors.get(editor.active) : undefined);
};

export const activeConsole = (): ConsoleController | undefined =>
  focusedConsole() ?? (editor.active ? consoles.get(editor.active) : undefined);
