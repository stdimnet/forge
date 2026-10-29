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
  /** Explain Plan / Explain Plan (Raw) of the statement at the caret (SQL consoles). */
  explain?: (raw: boolean) => Promise<void>;
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

/**
 * Result views with a data grid (table editors and console results) register under a
 * `data-grid-id` so Ctrl+B, Alt+F7, Shift+Enter and the Transpose / Aggregate / Chart
 * actions reach the grid that has focus.
 */
export interface GridController {
  /** Ctrl+B: the row the cell's foreign key references. */
  gotoReferenced?: () => void;
  /** Alt+F7: rows of other tables that reference this row. */
  findReferencing?: () => void;
  /** Shift+Enter: the value editor of the current cell. */
  editValue?: () => void;
  transpose?: () => void;
  aggregateView?: () => void;
  chart?: () => void;
  compare?: () => void;
}

export const gridControllers = new Map<string, GridController>();

let gridSeq = 0;
export const newGridId = () => `grid-${++gridSeq}`;

/** The grid controller around the focused element, if any. */
export function focusedGrid(): GridController | undefined {
  const el = document.activeElement as HTMLElement | null;
  const id = el?.closest<HTMLElement>("[data-grid-id]")?.dataset.gridId;
  return id ? gridControllers.get(id) : undefined;
}

/** The focused grid, else the one in the active editor tab. */
export function activeGrid(): GridController | undefined {
  const focused = focusedGrid();
  if (focused) return focused;
  const tab = editor.active ? document.querySelector<HTMLElement>(`[data-db-path="${CSS.escape(editor.active)}"] [data-grid-id]`) : null;
  const id = tab?.dataset.gridId;
  return id ? gridControllers.get(id) : undefined;
}
