/**
 * Multiple carets and Column Selection Mode on top of CodeMirror, as in PHPStorm:
 *
 * - Clone Caret Above / Below (double Ctrl held + Up/Down): a caret on the line above or
 *   below every caret, at the caret's column; the opposite direction removes the carets the
 *   last clones added.
 * - Add Carets to Ends of Selected Lines (Alt+Shift+G).
 * - Alt+Shift+Click adds a caret, or removes the one clicked; Alt+Shift+drag adds a selection.
 * - Alt+drag and middle-button drag select a rectangle; in Column Selection Mode
 *   (Alt+Shift+Insert) a plain drag and Shift+arrow keys do.
 *
 * Only per-keystroke caret work lives here; shortcuts come from the keymap.
 */
import {
  EditorSelection,
  EditorState,
  Prec,
  StateEffect,
  StateField,
  countColumn,
  findColumn,
  type Extension,
  type SelectionRange,
} from "@codemirror/state";
import { EditorView, keymap, rectangularSelection, type Command, type MouseSelectionStyle } from "@codemirror/view";

// ---------------------------------------------------------------- Clone Caret Above/Below

type Direction = -1 | 1;

/** The last clone run per editor, so the opposite direction can take its carets back. */
interface CloneStep {
  direction: Direction;
  /** Selection after the step. */
  after: EditorSelection;
  /** Heads of the carets the step added. */
  added: number[];
}
const steps = new WeakMap<EditorView, CloneStep[]>();

/** Visual column of `pos` (tabs expanded). */
function columnOf(state: EditorState, pos: number): number {
  const line = state.doc.lineAt(pos);
  return countColumn(line.text.slice(0, pos - line.from), state.tabSize);
}

/** Position on line `lineNo` closest to visual column `col`. */
function posAtColumn(state: EditorState, lineNo: number, col: number): number {
  const line = state.doc.line(lineNo);
  return line.from + findColumn(line.text, col, state.tabSize);
}

/** Index of the caret furthest in `direction` in sorted ranges (the main one, scrolled to). */
const mainOf = (count: number, direction: Direction) => (direction < 0 ? 0 : count - 1);

/**
 * New selection after cloning every caret one line up (`-1`) or down (`1`), or after taking
 * back the last clone in the other direction. `null` when nothing changes.
 */
export function cloneCarets(
  state: EditorState,
  direction: Direction,
  history: CloneStep[] = [],
): { selection: EditorSelection; history: CloneStep[] } | null {
  const sel = state.selection;
  const ours = history.length > 0 && history[history.length - 1].after.eq(sel);
  const last = ours ? history[history.length - 1] : undefined;
  if (last && last.direction !== direction) {
    const kept = sel.ranges.filter((r) => !last.added.includes(r.head));
    const selection = EditorSelection.create(kept, mainOf(kept.length, last.direction));
    const rest = history.slice(0, -1).map((s, i, all) => (i === all.length - 1 ? { ...s, after: selection } : s));
    return { selection, history: rest };
  }
  const occupied = new Set(sel.ranges.map((r) => state.doc.lineAt(r.head).number));
  const added: SelectionRange[] = [];
  for (const r of sel.ranges) {
    const lineNo = state.doc.lineAt(r.head).number + direction;
    if (lineNo < 1 || lineNo > state.doc.lines || occupied.has(lineNo)) continue;
    const col = r.goalColumn ?? columnOf(state, r.head);
    added.push(EditorSelection.cursor(posAtColumn(state, lineNo, col), 0, undefined, col));
    occupied.add(lineNo);
  }
  if (!added.length) return null;
  const all = [...sel.ranges, ...added];
  // The first added caret is the topmost new one, the last added the bottommost.
  const selection = EditorSelection.create(all, direction < 0 ? sel.ranges.length : all.length - 1);
  const step: CloneStep = { direction, after: selection, added: added.map((r) => r.head) };
  return { selection, history: [...(ours ? history : []), step] };
}

export const cloneCaret =
  (direction: Direction): Command =>
  (view) => {
    const out = cloneCarets(view.state, direction, steps.get(view));
    if (!out) return false;
    steps.set(view, out.history);
    view.dispatch({ selection: out.selection, scrollIntoView: true, userEvent: "select.clone" });
    return true;
  };

// ---------------------------------------------------------------- Add Carets to Ends of Selected Lines

/** A caret at the end of every line touched by a selection; carets stay as they are. */
export function caretsAtLineEnds(state: EditorState): EditorSelection | null {
  const ranges: SelectionRange[] = [];
  let changed = false;
  for (const r of state.selection.ranges) {
    if (r.empty) {
      ranges.push(r);
      continue;
    }
    const first = state.doc.lineAt(r.from).number;
    // A selection ending at the start of a line does not include that line.
    let last = state.doc.lineAt(r.to).number;
    if (last > first && state.doc.line(last).from === r.to) last--;
    for (let n = first; n <= last; n++) ranges.push(EditorSelection.cursor(state.doc.line(n).to));
    changed = true;
  }
  return changed ? EditorSelection.create(ranges, ranges.length - 1) : null;
}

export const addCaretsToLineEnds: Command = (view) => {
  const selection = caretsAtLineEnds(view.state);
  if (!selection) return false;
  view.dispatch({ selection, scrollIntoView: true, userEvent: "select" });
  return true;
};

// ---------------------------------------------------------------- Select All Occurrences

/** The word at every empty caret (the start of Select All Occurrences without a selection). */
export function selectWordsAtCarets(state: EditorState): EditorSelection | null {
  const ranges = state.selection.ranges.map((r) => (r.empty ? (state.wordAt(r.head) ?? r) : r));
  if (ranges.every((r, i) => r.eq(state.selection.ranges[i]))) return null;
  return EditorSelection.create(ranges, state.selection.mainIndex);
}

// ---------------------------------------------------------------- Column Selection Mode

const toggleColumnModeEffect = StateEffect.define<boolean>();

/** Whether Column Selection Mode is on in an editor (per editor, like PHPStorm). */
export const columnModeField = StateField.define<boolean>({
  create: () => false,
  update(on, tr) {
    for (const e of tr.effects) if (e.is(toggleColumnModeEffect)) on = e.value;
    return on;
  },
});

export const isColumnMode = (state: EditorState): boolean => state.field(columnModeField, false) ?? false;

export function setColumnMode(view: EditorView, on: boolean): void {
  view.dispatch({ effects: toggleColumnModeEffect.of(on) });
}

/** Corner positions of a rectangular selection: lines and visual columns. */
interface Block {
  anchorLine: number;
  anchorCol: number;
  headLine: number;
  headCol: number;
}

const blocks = new WeakMap<EditorView, { block: Block; selection: EditorSelection }>();

/** One range per line of `block`; lines shorter than the block get a caret at their end. */
export function blockSelection(state: EditorState, block: Block): EditorSelection {
  const ranges: SelectionRange[] = [];
  const step = block.headLine >= block.anchorLine ? 1 : -1;
  let main = 0;
  for (let n = block.anchorLine; ; n += step) {
    const line = state.doc.line(n);
    const anchor = line.from + findColumn(line.text, block.anchorCol, state.tabSize);
    const head = line.from + findColumn(line.text, block.headCol, state.tabSize);
    if (n === block.headLine) main = ranges.length;
    ranges.push(EditorSelection.range(anchor, head));
    if (n === block.headLine) break;
  }
  return EditorSelection.create(ranges, main);
}

/** The block a Shift+arrow press in Column Selection Mode starts from. */
function currentBlock(view: EditorView): Block {
  const known = blocks.get(view);
  if (known && known.selection.eq(view.state.selection)) return known.block;
  const { state } = view;
  const main = state.selection.main;
  return {
    anchorLine: state.doc.lineAt(main.anchor).number,
    anchorCol: columnOf(state, main.anchor),
    headLine: state.doc.lineAt(main.head).number,
    headCol: main.goalColumn ?? columnOf(state, main.head),
  };
}

/** Move the head corner of the rectangle (Shift+arrows in Column Selection Mode). */
export function extendBlock(state: EditorState, block: Block, lines: number, cols: number): Block {
  return {
    ...block,
    headLine: Math.max(1, Math.min(state.doc.lines, block.headLine + lines)),
    headCol: Math.max(0, block.headCol + cols),
  };
}

const blockMove =
  (lines: number, cols: number): Command =>
  (view) => {
    if (!isColumnMode(view.state)) return false;
    const block = extendBlock(view.state, currentBlock(view), lines, cols);
    const selection = blockSelection(view.state, block);
    blocks.set(view, { block, selection });
    view.dispatch({ selection, scrollIntoView: true, userEvent: "select" });
    // The dispatched selection may be normalized; remember what the editor holds.
    blocks.set(view, { block, selection: view.state.selection });
    return true;
  };

/** Alt+Shift+Click adds a caret or removes the one under the pointer; a drag adds a selection. */
function addCaretStyle(view: EditorView, event: MouseEvent): MouseSelectionStyle | null {
  if (!(event.altKey && event.shiftKey && event.button === 0)) return null;
  let start = view.posAtCoords({ x: event.clientX, y: event.clientY }, false);
  let startSel = view.state.selection;
  return {
    update(update) {
      if (update.docChanged) {
        start = update.changes.mapPos(start);
        startSel = startSel.map(update.changes);
      }
      return false;
    },
    get(e) {
      const cur = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
      if (cur !== start) return startSel.addRange(EditorSelection.range(start, cur));
      return toggleCaretAt(startSel, cur);
    },
  };
}

/** Add a caret at `pos`, or remove the caret or selection there (never the last one). */
export function toggleCaretAt(sel: EditorSelection, pos: number): EditorSelection {
  const i = sel.ranges.findIndex((r) => r.from <= pos && r.to >= pos);
  if (i < 0) return sel.addRange(EditorSelection.cursor(pos));
  if (sel.ranges.length === 1) return sel;
  const ranges = sel.ranges.filter((_, j) => j !== i);
  const main = sel.mainIndex === i ? ranges.length - 1 : sel.mainIndex - (sel.mainIndex > i ? 1 : 0);
  return EditorSelection.create(ranges, main);
}

/** Whether a mouse press starts a rectangular selection. */
function startsRectangle(event: MouseEvent): boolean {
  if (event.button === 1) return true;
  if (event.button !== 0 || event.shiftKey) return false;
  const target = event.target instanceof HTMLElement ? event.target : null;
  const view = target ? EditorView.findFromDOM(target.closest(".cm-editor") as HTMLElement) : null;
  const column = view ? isColumnMode(view.state) : false;
  // In Column Selection Mode, Alt+drag makes a plain selection instead.
  return column ? !event.altKey : event.altKey;
}

/** Editor extension: column mode state, Shift+arrows in column mode and the mouse gestures. */
export function caretExtensions(): Extension {
  return [
    // CodeMirror keeps only the main range unless multiple selections are allowed.
    EditorState.allowMultipleSelections.of(true),
    columnModeField,
    EditorView.clickAddsSelectionRange.of((e) => e.altKey && e.shiftKey),
    EditorView.mouseSelectionStyle.of(addCaretStyle),
    rectangularSelection({ eventFilter: startsRectangle }),
    // Native caret keys change their meaning in Column Selection Mode (PHPStorm does the same).
    Prec.high(
      keymap.of([
        { key: "Shift-ArrowUp", run: blockMove(-1, 0) },
        { key: "Shift-ArrowDown", run: blockMove(1, 0) },
        { key: "Shift-ArrowLeft", run: blockMove(0, -1) },
        { key: "Shift-ArrowRight", run: blockMove(0, 1) },
      ]),
    ),
  ];
}
