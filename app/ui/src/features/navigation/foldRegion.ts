/**
 * Code | Folding | Expand / Collapse (Ctrl+= / Ctrl+-) and their "Recursively" variants
 * (Ctrl+Alt+= / Ctrl+Alt+-), like PHPStorm: Collapse folds the innermost region around the
 * caret from anywhere inside it, not only from the line it starts on (#218).
 *
 * Fold ranges come from CodeMirror's `foldable` (fold services and syntax tree fold props), so
 * every language with folding works the same way.
 */
import { EditorSelection, type EditorState, type StateEffect } from "@codemirror/state";
import type { Command, EditorView } from "@codemirror/view";
import { foldable, foldedRanges, foldEffect, unfoldEffect } from "@codemirror/language";

export interface FoldRange {
  from: number;
  to: number;
}

/** Lines scanned upwards for an enclosing region (keeps huge files responsive). */
const MAX_SCAN_LINES = 5000;

const isFolded = (state: EditorState, r: FoldRange) => {
  let found = false;
  foldedRanges(state).between(r.from, r.to, (from, to) => {
    if (from === r.from && to === r.to) found = true;
  });
  return found;
};

const foldOnLine = (state: EditorState, lineNo: number): FoldRange | null => {
  const line = state.doc.line(lineNo);
  return foldable(state, line.from, line.to);
};

/**
 * The innermost foldable region around `pos`, skipping regions that are already folded (so
 * pressing Collapse again folds the parent, like PHPStorm). A region "contains" the caret from
 * the start of the line it begins on to its end; with the caret on a declaration whose opening
 * brace is on the next line (PSR-12 methods and classes), that brace's region counts too.
 */
export function enclosingFoldRange(state: EditorState, pos: number, opts: { includeFolded?: boolean } = {}): FoldRange | null {
  const usable = (r: FoldRange | null): r is FoldRange => !!r && r.to > r.from && (opts.includeFolded || !isFolded(state, r));
  const caretLine = state.doc.lineAt(pos);
  const own = foldOnLine(state, caretLine.number);
  if (usable(own) && pos <= own.to) return own;
  if (caretLine.number < state.doc.lines) {
    const next = state.doc.line(caretLine.number + 1);
    if (next.text.trimStart().startsWith("{")) {
      const r = foldable(state, next.from, next.to);
      if (usable(r)) return r;
    }
  }
  const stop = Math.max(1, caretLine.number - MAX_SCAN_LINES);
  for (let n = caretLine.number - 1; n >= stop; n--) {
    const r = foldOnLine(state, n);
    if (usable(r) && r.to >= pos) return r;
  }
  return null;
}

/** Every foldable region inside `outer` (not `outer` itself), outermost first. */
export function nestedFoldRanges(state: EditorState, outer: FoldRange): FoldRange[] {
  const out: FoldRange[] = [];
  const first = state.doc.lineAt(outer.from).number + 1;
  const last = state.doc.lineAt(outer.to).number;
  for (let n = first; n <= last; n++) {
    const r = foldOnLine(state, n);
    if (r && r.to > r.from && r.from >= outer.from && r.to <= outer.to && !(r.from === outer.from && r.to === outer.to)) out.push(r);
  }
  return out;
}

/** Folded ranges that overlap `[from, to]`. */
function foldedIn(state: EditorState, from: number, to: number): FoldRange[] {
  const out: FoldRange[] = [];
  foldedRanges(state).between(from, to, (a, b) => {
    if (b >= from && a <= to) out.push({ from: a, to: b });
  });
  return out;
}

/** Fold `ranges`; a caret inside the outermost one moves to its start so it stays visible. */
function fold(view: EditorView, outer: FoldRange, ranges: FoldRange[]): boolean {
  // The fold state needs its ranges in document order (outer before inner at the same start).
  const effects: StateEffect<unknown>[] = ranges
    .filter((r) => !isFolded(view.state, r))
    .sort((a, b) => a.from - b.from || b.to - a.to)
    .map((r) => foldEffect.of(r));
  if (!effects.length) return false;
  const head = view.state.selection.main.head;
  const selection = head > outer.from && head <= outer.to ? EditorSelection.cursor(outer.from) : undefined;
  view.dispatch({ effects, selection });
  return true;
}

/** Collapse (CollapseRegion): fold the innermost unfolded region around the caret. */
export const collapseRegion: Command = (view) => {
  const r = enclosingFoldRange(view.state, view.state.selection.main.head);
  return r ? fold(view, r, [r]) : false;
};

/** Collapse Recursively (CollapseRegionRecursively): fold that region and every region in it. */
export const collapseRegionRecursively: Command = (view) => {
  const r = enclosingFoldRange(view.state, view.state.selection.main.head);
  return r ? fold(view, r, [r, ...nestedFoldRanges(view.state, r)]) : false;
};

/** The folded ranges on the caret's line (the fold placeholder the caret sits next to). */
function foldsAtCaret(state: EditorState): FoldRange[] {
  const line = state.doc.lineAt(state.selection.main.head);
  return foldedIn(state, line.from, line.to).filter((f) => f.from >= line.from && f.from <= line.to);
}

/** Expand (ExpandRegion): unfold the fold at the caret, or the folded region around it. */
export const expandRegion: Command = (view) => {
  const state = view.state;
  let targets = foldsAtCaret(state);
  if (!targets.length) {
    const r = enclosingFoldRange(state, state.selection.main.head, { includeFolded: true });
    targets = r && isFolded(state, r) ? [r] : [];
  }
  if (!targets.length) return false;
  view.dispatch({ effects: targets.map((r) => unfoldEffect.of(r)) });
  return true;
};

/** Expand Recursively (ExpandRegionRecursively): unfold the region at the caret and every fold inside it. */
export const expandRegionRecursively: Command = (view) => {
  const state = view.state;
  const atCaret = foldsAtCaret(state);
  const outer =
    atCaret.sort((a, b) => b.to - b.from - (a.to - a.from))[0] ??
    enclosingFoldRange(state, state.selection.main.head, { includeFolded: true });
  if (!outer) return false;
  const targets = foldedIn(state, outer.from, outer.to).filter((f) => f.from >= outer.from && f.to <= outer.to);
  if (!targets.length) return false;
  view.dispatch({ effects: targets.map((r) => unfoldEffect.of(r)) });
  return true;
};
