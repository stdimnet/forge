/**
 * Tracks the CodeMirror view of each open PHP file (via an editor extension) so PHP
 * actions can read the caret, apply edits and navigate.
 */
import { EditorSelection, type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin } from "@codemirror/view";
import { activeTab, openFile } from "@/state/editor";
import type { TextEdit } from "./api";

const views = new Map<string, EditorView>();

export function viewTracker(path: string): Extension {
  return ViewPlugin.define((view) => {
    views.set(path, view);
    return {
      destroy() {
        if (views.get(path) === view) views.delete(path);
      },
    };
  });
}

export const viewFor = (path: string) => views.get(path);

export function activeView(): { path: string; view: EditorView } | undefined {
  const tab = activeTab();
  const view = tab && views.get(tab.path);
  return tab && view ? { path: tab.path, view } : undefined;
}

/** Apply backend edits as one undoable transaction and put the caret at the first insert. */
export function applyEdits(view: EditorView, edits: TextEdit[]): void {
  if (!edits.length) return;
  const changes = view.state.changes(edits.map((e) => ({ from: e.start, to: e.end, insert: e.newText })));
  const first = [...edits].sort((a, b) => a.start - b.start)[0];
  // Skip the leading blank lines and indentation so the caret lands on code.
  const lead = /^\s*/.exec(first.newText)?.[0].length ?? 0;
  const caret = Math.min(changes.mapPos(first.start, -1) + lead, changes.newLength);
  view.dispatch({
    changes,
    selection: EditorSelection.cursor(caret),
    effects: EditorView.scrollIntoView(caret, { y: "center" }),
    userEvent: "input.generate",
  });
  view.focus();
}

async function waitForView(path: string): Promise<EditorView | undefined> {
  for (let i = 0; i < 40; i++) {
    const v = views.get(path);
    if (v) return v;
    await new Promise((r) => setTimeout(r, 25));
  }
  return undefined;
}

/**
 * Open `path` and select the symbol: by UTF-16 offset when known, else by 0-based line
 * (and the first occurrence of `name` on that line).
 */
export async function revealSymbol(path: string, target: { offset?: number; line?: number; name?: string }): Promise<void> {
  await openFile(path);
  const view = await waitForView(path);
  if (!view) return;
  const doc = view.state.doc;
  let from: number;
  if (target.offset !== undefined && target.offset > 0) from = Math.min(target.offset, doc.length);
  else {
    const line = doc.line(Math.min(doc.lines, (target.line ?? 0) + 1));
    const col = target.name ? line.text.indexOf(target.name) : -1;
    from = line.from + Math.max(0, col);
  }
  const to = target.name && doc.sliceString(from, from + target.name.length) === target.name ? from + target.name.length : from;
  view.dispatch({ selection: EditorSelection.range(from, to), effects: EditorView.scrollIntoView(from, { y: "center" }) });
  view.focus();
}
