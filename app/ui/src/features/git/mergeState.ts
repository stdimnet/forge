/**
 * State of the Result editor in the merge window. The chunks come from the three-way merge
 * in Rust (`forge_vcs::merge`); here we only track where each chunk sits in the editable
 * result (mapped through every edit) and which sides were applied or ignored, so that
 * undo/redo restore both text and state (`invertedEffects`).
 */
import { RangeSetBuilder, StateEffect, StateField, type EditorState, type Extension, type TransactionSpec } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";
import { invertedEffects } from "@codemirror/commands";
import type { ChunkKind, MergeChunk } from "./api";

export type Side = "left" | "right";

/** A chunk in the result document. */
export interface ResultChunk {
  from: number;
  to: number;
  /** The left / right side is handled: applied, ignored, or it has no change. */
  left: boolean;
  right: boolean;
  /** One side of a conflict was applied: applying the other appends it. */
  applied: boolean;
}

export const isResolved = (c: ResultChunk) => c.left && c.right;

/** Start offsets of the lines of `text` (lines keep their `\n`), plus the end. */
export function lineOffsets(text: string): number[] {
  const out = [0];
  for (let i = 0; i < text.length; i++) if (text[i] === "\n") out.push(i + 1);
  if (out[out.length - 1] !== text.length) out.push(text.length);
  return out;
}

/** Initial result state: the result starts as the base text. */
export function initialChunks(chunks: MergeChunk[], base: string): ResultChunk[] {
  const at = lineOffsets(base);
  return chunks.map((c) => ({
    from: at[c.base.start],
    to: at[c.base.end],
    left: c.kind === "right",
    right: c.kind === "left",
    applied: false,
  }));
}

interface FlagChange {
  index: number;
  left: boolean;
  right: boolean;
  applied: boolean;
}

const setFlags = StateEffect.define<FlagChange[]>();

/** Field factory: one per merge window, initialized with that merge's chunks. */
function chunkField(initial: ResultChunk[]) {
  return StateField.define<ResultChunk[]>({
    create: () => initial,
    update(value, tr) {
      let next = value;
      if (tr.docChanged) {
        next = value.map((c) => ({ ...c, from: tr.changes.mapPos(c.from, -1), to: tr.changes.mapPos(c.to, 1) }));
      }
      for (const e of tr.effects) {
        if (!e.is(setFlags)) continue;
        if (next === value) next = [...value];
        for (const f of e.value) next[f.index] = { ...next[f.index], left: f.left, right: f.right, applied: f.applied };
      }
      return next;
    },
  });
}

/** What the result side of a chunk will become: decides its colour. */
export function chunkClass(kind: ChunkKind, base: string, side: string): "conflict" | "inserted" | "deleted" | "modified" {
  if (kind === "conflict") return "conflict";
  if (!base) return "inserted";
  if (!side) return "deleted";
  return "modified";
}

/** Line decorations for `ranges` (`[from, to, class]`); empty ranges mark the line below. */
export function lineMarks(state: EditorState, ranges: [number, number, string][]): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = state.doc;
  let last = -1;
  for (const [from, to, cls] of ranges) {
    if (from === to) {
      const line = doc.lineAt(Math.min(from, doc.length));
      if (line.from <= last) continue;
      const mark = from === doc.length && from > line.from ? "cm-merge-empty-after" : "cm-merge-empty";
      builder.add(line.from, line.from, Decoration.line({ class: `${mark} cm-merge-${cls}` }));
      last = line.from;
      continue;
    }
    let pos = from;
    while (pos < to && pos <= doc.length) {
      const line = doc.lineAt(pos);
      if (line.from > last) builder.add(line.from, line.from, Decoration.line({ class: `cm-merge-line cm-merge-${cls}` }));
      last = line.from;
      if (line.to >= doc.length) break;
      pos = line.to + 1;
    }
  }
  return builder.finish();
}

export interface MergeResultState {
  extension: Extension;
  field: StateField<ResultChunk[]>;
}

/** Result editor extension: chunk tracking, undoable flags and chunk colours. */
export function mergeResultState(chunks: MergeChunk[], base: string): MergeResultState {
  const field = chunkField(initialChunks(chunks, base));
  const inverse = invertedEffects.of((tr) => {
    const out: StateEffect<unknown>[] = [];
    for (const e of tr.effects) {
      if (!e.is(setFlags)) continue;
      const prev = tr.startState.field(field);
      out.push(setFlags.of(e.value.map((f) => ({ index: f.index, left: prev[f.index].left, right: prev[f.index].right, applied: prev[f.index].applied }))));
    }
    return out;
  });
  const marks = EditorView.decorations.compute([field], (state) =>
    lineMarks(
      state,
      state
        .field(field)
        .map((c, i): [number, number, string] | null =>
          isResolved(c) ? null : [c.from, c.to, chunkClass(chunks[i].kind, chunks[i].baseText, chunks[i].kind === "right" ? chunks[i].rightText : chunks[i].leftText)],
        )
        .filter((x): x is [number, number, string] => x !== null),
    ),
  );
  return { extension: [field, inverse, marks], field };
}

// ---- operations (each returns a transaction spec, or null when there is nothing to do) ---

type Op = { index: number; side: Side };

function build(state: EditorState, field: StateField<ResultChunk[]>, chunks: MergeChunk[], ops: Op[], userEvent: string): TransactionSpec | null {
  const list = state.field(field);
  const changes: { from: number; to?: number; insert: string }[] = [];
  const flags: FlagChange[] = [];
  for (const { index, side } of ops) {
    const cur = list[index];
    const ch = chunks[index];
    if (!cur || isResolved(cur) || cur[side]) continue;
    const text = side === "left" ? ch.leftText : ch.rightText;
    if (ch.kind === "conflict") {
      const other: Side = side === "left" ? "right" : "left";
      // PHPStorm: with the other side already applied, this one is appended after it.
      if (cur.applied && cur[other]) changes.push({ from: cur.to, insert: text });
      else changes.push({ from: cur.from, to: cur.to, insert: text });
      flags.push({ index, left: cur.left || side === "left", right: cur.right || side === "right", applied: true });
    } else {
      changes.push({ from: cur.from, to: cur.to, insert: text });
      flags.push({ index, left: true, right: true, applied: true });
    }
  }
  if (!flags.length) return null;
  return { changes, effects: setFlags.of(flags), userEvent };
}

/** » / « on a chunk: apply that side's change (Accept Left / Accept Right of a chunk). */
export function applySide(state: EditorState, field: StateField<ResultChunk[]>, chunks: MergeChunk[], index: number, side: Side): TransactionSpec | null {
  return build(state, field, chunks, [{ index, side }], "merge.apply");
}

/** × on a chunk: ignore that side's change (a non-conflicting change is then done). */
export function ignoreSide(state: EditorState, field: StateField<ResultChunk[]>, chunks: MergeChunk[], index: number, side: Side): TransactionSpec | null {
  const cur = state.field(field)[index];
  if (!cur || isResolved(cur) || cur[side]) return null;
  const conflict = chunks[index].kind === "conflict";
  const flag = conflict
    ? { index, left: cur.left || side === "left", right: cur.right || side === "right", applied: cur.applied }
    : { index, left: true, right: true, applied: cur.applied };
  return { effects: setFlags.of([flag]), userEvent: "merge.ignore" };
}

/** "Apply Non-Conflicting Changes" from the left side, the right side or all. */
export function applyNonConflicting(state: EditorState, field: StateField<ResultChunk[]>, chunks: MergeChunk[], which: Side | "all"): TransactionSpec | null {
  const ops: Op[] = [];
  state.field(field).forEach((c, index) => {
    const kind = chunks[index].kind;
    if (kind === "conflict" || isResolved(c) || c.applied) return;
    if (kind === "both" || (kind === "left" && which !== "right")) ops.push({ index, side: "left" });
    else if (kind === "right" && which !== "left") ops.push({ index, side: "right" });
  });
  return build(state, field, chunks, ops, "merge.apply");
}

/** "Resolve Simple Conflicts" (magic wand): untouched conflicts with a word-level merge. */
export function resolveSimpleConflicts(state: EditorState, field: StateField<ResultChunk[]>, chunks: MergeChunk[]): TransactionSpec | null {
  const changes: { from: number; to: number; insert: string }[] = [];
  const flags: FlagChange[] = [];
  state.field(field).forEach((c, index) => {
    const ch = chunks[index];
    if (ch.kind !== "conflict" || ch.simple === null || c.left || c.right) return;
    if (state.sliceDoc(c.from, c.to) !== ch.baseText) return;
    changes.push({ from: c.from, to: c.to, insert: ch.simple });
    flags.push({ index, left: true, right: true, applied: true });
  });
  if (!flags.length) return null;
  return { changes, effects: setFlags.of(flags), userEvent: "merge.resolveSimple" };
}

export interface MergeCounts {
  /** Unresolved non-conflicting changes. */
  changes: number;
  /** Unresolved conflicts. */
  conflicts: number;
  /** Unresolved conflicts that "Resolve Simple Conflicts" can merge. */
  simple: number;
  /** Unresolved non-conflicting changes per side ("both" counts on both). */
  left: number;
  right: number;
}

export function mergeCounts(list: ResultChunk[], chunks: MergeChunk[]): MergeCounts {
  const out: MergeCounts = { changes: 0, conflicts: 0, simple: 0, left: 0, right: 0 };
  list.forEach((c, i) => {
    if (isResolved(c)) return;
    const kind = chunks[i].kind;
    if (kind === "conflict") {
      out.conflicts++;
      if (chunks[i].simple !== null && !c.left && !c.right) out.simple++;
      return;
    }
    out.changes++;
    if (c.applied) return;
    if (kind !== "right") out.left++;
    if (kind !== "left") out.right++;
  });
  return out;
}

/** "N changes. M conflicts" like PHPStorm's merge window. */
export function countsLabel(c: MergeCounts): string {
  if (c.changes + c.conflicts === 0) return "All changes have been processed.";
  const n = (k: number, word: string) => `${k} ${word}${k === 1 ? "" : "s"}`;
  return `${n(c.changes, "change")}. ${n(c.conflicts, "conflict")}`;
}

/** The next (or previous) unresolved chunk from `pos`, wrapping around; -1 when none. */
export function nextChunk(list: ResultChunk[], pos: number, forward: boolean): number {
  const open = list.map((c, i) => [c, i] as const).filter(([c]) => !isResolved(c));
  if (!open.length) return -1;
  if (forward) return (open.find(([c]) => c.from > pos) ?? open[0])[1];
  return ([...open].reverse().find(([c]) => c.from < pos) ?? open[open.length - 1])[1];
}

/** 0-based lines `[start, end)` that the result range `from..to` covers. */
export function resultLines(state: EditorState, from: number, to: number): [number, number] {
  const start = state.doc.lineAt(from).number - 1;
  if (from === to) return [start, start];
  const end = state.doc.lineAt(to);
  return [start, end.from === to ? end.number - 1 : end.number];
}

/**
 * Map a 0-based line of one pane to the other through the chunks (`[fromStart, fromEnd,
 * toStart, toEnd]` line ranges): equal text between chunks maps one to one, inside a chunk
 * proportionally. Used to keep the three panes scrolled together.
 */
export function mapLine(line: number, anchors: [number, number, number, number][]): number {
  let delta = 0;
  for (const [fs, fe, ts, te] of anchors) {
    if (line < fs) break;
    if (line < fe) return ts + Math.round(((line - fs) / Math.max(1, fe - fs)) * (te - ts));
    delta = te - fe;
  }
  return line + delta;
}
