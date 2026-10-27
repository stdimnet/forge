/** Pure helpers of the notebook editor: new cells, ids, the notebook model to save. */
import type { Cell, CellType, Notebook, Output, Rendered } from "@/ipc/jupyter";

/** A cell as the editor holds it: the model plus what is shown while it runs. */
export interface CellState extends Cell {
  rendered: Rendered[];
  /** Sent to the kernel, not finished yet. */
  running: boolean;
  /** The kernel started it (`execute_input`). */
  started: boolean;
  /** Result of its last run in this session. */
  status?: "ok" | "error" | "aborted";
  /** Markdown cells: showing the editor instead of the rendered text. */
  editing: boolean;
  /** `clear_output(wait=True)`: clear before the next output. */
  clearPending?: boolean;
  /** `display_id` of outputs that `update_display_data` may replace, by output index. */
  displayIds?: Record<number, string>;
}

/** A new cell id like Jupyter's (8 hex digits), unique among `taken`. */
export function newCellId(taken: Iterable<string>, random: () => number = Math.random): string {
  const used = new Set(taken);
  for (;;) {
    const id = Math.floor(random() * 0xffffffff)
      .toString(16)
      .padStart(8, "0");
    if (!used.has(id)) return id;
  }
}

export function newCell(cellType: CellType, taken: Iterable<string>, source = ""): CellState {
  return {
    id: newCellId(taken),
    cellType,
    source,
    metadata: {},
    executionCount: null,
    outputs: [],
    rendered: [],
    running: false,
    started: false,
    editing: cellType !== "code",
  };
}

export function toCellState(cell: Cell, rendered: Rendered[] = []): CellState {
  return { ...cell, rendered, running: false, started: false, editing: false };
}

/** The model the editor saves (without the run state). */
export function toNotebook(meta: Omit<Notebook, "cells">, cells: CellState[]): Notebook {
  return {
    ...meta,
    cells: cells.map((c) => {
      const cell: Cell = {
        id: c.id,
        cellType: c.cellType,
        source: c.source,
        metadata: c.metadata,
        executionCount: c.cellType === "code" ? c.executionCount : null,
        outputs: c.cellType === "code" ? c.outputs : [],
      };
      if (c.attachments !== undefined) cell.attachments = c.attachments;
      if (c.origin !== undefined) cell.origin = c.origin;
      return cell;
    }),
  };
}

/** Index after moving `from` by `delta`, or null when it would leave the list. */
export function moveTarget(from: number, delta: number, length: number): number | null {
  const to = from + delta;
  return to < 0 || to >= length || delta === 0 ? null : to;
}

/** Selection after removing cell `index` from `length` cells. */
export function selectionAfterRemove(index: number, length: number): number {
  return Math.max(0, Math.min(index, length - 2));
}

/** `[3]`, `[*]` while running, `[ ]` before the first run. */
export function countLabel(cell: Pick<CellState, "running" | "executionCount">): string {
  if (cell.running) return "[*]";
  return cell.executionCount == null ? "[ ]" : `[${cell.executionCount}]`;
}

/** Plain text of an output, for copying. */
export function outputText(o: Output): string {
  switch (o.outputType) {
    case "stream":
      return o.text;
    case "error":
      // eslint-disable-next-line no-control-regex
      return o.traceback.join("\n").replace(/\x1b\[[0-9;]*m/g, "");
    default: {
      const t = o.data["text/plain"];
      return typeof t === "string" ? t : "";
    }
  }
}

/** Kernel state as the toolbar shows it. */
export function kernelLabel(status: string): string {
  switch (status) {
    case "idle":
      return "Idle";
    case "busy":
      return "Busy";
    case "starting":
      return "Starting…";
    case "restarting":
      return "Restarting…";
    case "dead":
      return "Disconnected";
    case "failed":
      return "Failed to start";
    default:
      return "Not started";
  }
}
