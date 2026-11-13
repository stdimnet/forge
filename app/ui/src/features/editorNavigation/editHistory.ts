/**
 * Last Edit Location (Ctrl+Shift+Backspace, PHPStorm's `JumpToLastChange`) and Next Edit
 * Location (`JumpToNextChange`): the places where text was changed, across files. The model
 * (merging nearby edits, mapping places through later edits, Back/Forward position) is
 * `forge_editing::navigation::EditHistory` in Rust, called synchronously through
 * WebAssembly in the desktop app and in browser mode alike, because every document change
 * is reported to it. It is separate from Back/Forward (Ctrl+Alt+Left/Right) and also feeds
 * Recent Locations' "Show changed only".
 */
import { createSignal } from "solid-js";
import { EditorView } from "@codemirror/view";
import { Transaction } from "@codemirror/state";
import { registerCommand } from "@/state/commands";
import { registerEditorExtension } from "@/editor/extensions";
import { isSyncedTransaction } from "@/editor/docSync";
import { openFileAt } from "@/editor/views";
import { editor, isCustomTab, onPathMoved } from "@/state/editor";
import { actionById } from "@/keymap";
import { lineEditsOf } from "@/features/bookmarks/gutter";
import type { LineEdit } from "@/ipc/workbench";
import { forgeWasm, type ForgeWasm } from "@/wasm";
import { activeView } from "@/features/navigation/editorCommands";

/** A place where text was changed; `line` and `column` are 0-based. */
export interface EditPlace {
  path: string;
  line: number;
  column: number;
}

export type EditOp =
  | { op: "record"; path: string; line: number; column: number }
  | { op: "applyEdits"; path: string; edits: LineEdit[] }
  | { op: "renamePath"; from: string; to: string }
  | { op: "removePath"; path: string }
  | { op: "back" | "forward"; caret?: { path: string; line: number } | null }
  | { op: "clear" };

let wasm: ForgeWasm | null = null;
void forgeWasm().then(
  (w) => (wasm = w),
  () => undefined,
);

/** Bumped after every change, so lists of places re-read them. */
const [version, setVersion] = createSignal(0);

interface Answer {
  place: EditPlace | null;
  places?: EditPlace[];
}

function call(ops: EditOp[], list = false): Answer {
  if (!wasm || (!ops.length && !list)) return { place: null };
  try {
    const out = JSON.parse(wasm.edit_history(JSON.stringify({ ops, list }))) as Answer | { error: string };
    if ("error" in out) throw new Error(out.error);
    return out;
  } catch (e) {
    console.warn("edit history:", e);
    return { place: null };
  }
}

/** Apply operations to the edit history; returns the place of the last Back/Forward. */
export function applyEditOps(ops: EditOp[]): EditPlace | null {
  const { place } = call(ops);
  if (ops.some((o) => o.op !== "back" && o.op !== "forward")) setVersion((v) => v + 1);
  return place;
}

/** The places where text was changed, newest first (reactive). */
export function editPlaces(): EditPlace[] {
  version();
  return call([], true).places ?? [];
}

const isProjectFile = (path: string) => !isCustomTab(path) && !path.includes("://");

/** Whether a transaction is an edit of the user (typed, pasted, refactored), not a reload. */
function isUserEdit(tr: Transaction): boolean {
  return tr.docChanged && !isSyncedTransaction(tr) && tr.annotation(Transaction.addToHistory) !== false;
}

registerEditorExtension(({ path }) =>
  isProjectFile(path)
    ? EditorView.updateListener.of((u) => {
        if (!u.docChanged) return;
        const ops: EditOp[] = [];
        let edited = false;
        for (const tr of u.transactions) {
          // Other views of the same file report their own edits.
          if (!tr.docChanged || isSyncedTransaction(tr)) continue;
          let whole = false;
          tr.changes.iterChanges((fromA, toA) => (whole ||= fromA === 0 && toA === tr.startState.doc.length && toA > 0));
          const edits = whole ? [] : lineEditsOf(tr.startState.doc, tr.changes);
          if (edits.length) ops.push({ op: "applyEdits", path, edits });
          edited ||= isUserEdit(tr);
        }
        if (edited) {
          const head = u.state.selection.main.head;
          const line = u.state.doc.lineAt(head);
          ops.push({ op: "record", path, line: line.number - 1, column: head - line.from });
        }
        applyEditOps(ops);
      })
    : null,
);

onPathMoved((from, to) => applyEditOps([to === null ? { op: "removePath", path: from } : { op: "renamePath", from, to }]));

/** The caret of the active editor, to skip the place it is already at. */
function caret(): { path: string; line: number } | null {
  const path = editor.active;
  const view = activeView();
  if (!path || !view) return null;
  return { path, line: view.state.doc.lineAt(view.state.selection.main.head).number - 1 };
}

async function jump(op: "back" | "forward"): Promise<void> {
  const place = applyEditOps([{ op, caret: caret() }]);
  if (place) await openFileAt(place.path, place.line + 1, place.column + 1, { flash: false });
}

export const jumpToLastChange = () => jump("back");
export const jumpToNextChange = () => jump("forward");

registerCommand({
  id: "JumpToLastChange",
  title: actionById("JumpToLastChange")?.title ?? "Last Edit Location",
  category: "Navigate",
  run: jumpToLastChange,
});
registerCommand({
  id: "JumpToNextChange",
  title: actionById("JumpToNextChange")?.title ?? "Next Edit Location",
  category: "Navigate",
  run: jumpToNextChange,
});
