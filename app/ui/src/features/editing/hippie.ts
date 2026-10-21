/**
 * Cyclic Expand Word (`Alt+/`, `HippieCompletion`; `Alt+Shift+/`, `HippieBackwardCompletion`):
 * replaces the word prefix at the caret with the nearest word that starts with it. Pressing
 * the shortcut again right away cycles through the other words (the backward action goes
 * the other way). Rust (`forge-editing`) finds and orders the words.
 */
import { EditorSelection, type Text } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { editingApi } from "@/ipc/editing";

interface Cycle {
  /** The document after our last expansion; any other edit starts a new cycle. */
  doc: Text;
  from: number;
  prefix: string;
  candidates: string[];
  /** Index of the inserted candidate. */
  index: number;
}

const cycles = new WeakMap<EditorView, Cycle>();

function insert(view: EditorView, cycle: Cycle, to: number): void {
  const word = cycle.candidates[cycle.index];
  view.dispatch({
    changes: { from: cycle.from, to, insert: word },
    selection: EditorSelection.cursor(cycle.from + word.length),
    scrollIntoView: true,
    userEvent: "input.complete",
  });
  cycle.doc = view.state.doc;
  cycles.set(view, cycle);
}

/**
 * Expand (or cycle) the word at the caret. `others` are the texts of the other open files,
 * searched after the current one.
 */
export async function expandWord(
  view: EditorView,
  opts: { backward: boolean; language?: string; others: () => string[] },
): Promise<void> {
  const { state } = view;
  const head = state.selection.main.head;
  const current = cycles.get(view);
  if (current && current.doc === state.doc && head === current.from + current.candidates[current.index].length) {
    const n = current.candidates.length;
    current.index = (current.index + (opts.backward ? n - 1 : 1)) % n;
    insert(view, current, head);
    return;
  }
  cycles.delete(view);
  const result = await editingApi
    .expandWord({
      text: state.doc.toString(),
      offset: head,
      others: opts.others(),
      backward: opts.backward,
      language: opts.language,
    })
    .catch(() => null);
  if (!result?.candidates.length || view.state.doc !== state.doc) return;
  insert(view, { doc: state.doc, from: result.from, prefix: result.prefix, candidates: result.candidates, index: 0 }, head);
}
