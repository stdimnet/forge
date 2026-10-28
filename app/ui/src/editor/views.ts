/**
 * Live CodeMirror views by file path, so features outside the editor (AI chat, ...) can
 * read the selection, insert text at the caret of the active editor, or open a file at a
 * position (`openFileAt`).
 *
 * With split editors one file can be shown by several views (one per editor group, all
 * editing the same document, see `docSync.ts`). `viewFor(path)` answers the view in the
 * active group, else the one focused last.
 */
import type { Text } from "@codemirror/state";
import { isolateHistory } from "@codemirror/commands";
import { EditorView } from "@codemirror/view";
import { editor, groupActive, openFile, setEditorBridge } from "@/state/editor";
import { flashLineAt } from "./flash";

interface Entry {
  view: EditorView;
  /** Editor group that shows the view (undefined for views outside groups). */
  group?: number;
  focused: number;
}

const views = new Map<string, Entry[]>();
let focusSeq = 0;

export function registerView(path: string, view: EditorView, group?: number): () => void {
  const entry: Entry = { view, group, focused: ++focusSeq };
  views.set(path, [...(views.get(path) ?? []), entry]);
  const onFocus = () => (entry.focused = ++focusSeq);
  view.dom.addEventListener("focusin", onFocus);
  return () => {
    view.dom.removeEventListener("focusin", onFocus);
    const rest = (views.get(path) ?? []).filter((e) => e !== entry);
    if (rest.length) views.set(path, rest);
    else views.delete(path);
  };
}

/**
 * The view of `path`: in `group` when given, else in the active editor group, else the
 * one focused last.
 */
export function viewFor(path: string, group?: number): EditorView | undefined {
  const list = views.get(path);
  if (!list?.length) return undefined;
  const g = group ?? editor.layout.activeGroup;
  const inGroup = list.find((e) => e.group === g);
  if (inGroup) return inGroup.view;
  if (group !== undefined) return undefined;
  return [...list].sort((a, b) => b.focused - a.focused)[0].view;
}

/** Every view of `path` (one per editor group showing it). */
export const viewsOf = (path: string): EditorView[] => (views.get(path) ?? []).map((e) => e.view);

export const activeView = (): EditorView | undefined => (editor.active ? viewFor(editor.active) : undefined);

/** The view of the active tab of a group. */
export function groupView(group: number): EditorView | undefined {
  const id = groupActive(group);
  return id ? viewFor(id, group) : undefined;
}

export interface EditorSelection {
  path: string;
  startLine: number;
  endLine: number;
  text: string;
}

/** Non-empty selection of the active editor, if any. */
export function activeSelection(): EditorSelection | undefined {
  const view = activeView();
  if (!view || !editor.active) return undefined;
  const { from, to } = view.state.selection.main;
  if (from === to) return undefined;
  return {
    path: editor.active,
    startLine: view.state.doc.lineAt(from).number,
    endLine: view.state.doc.lineAt(to).number,
    text: view.state.sliceDoc(from, to),
  };
}

/** Replace the selection of the active editor with `text`. Returns false without an editor. */
export function insertAtCaret(text: string): boolean {
  const view = activeView();
  if (!view) return false;
  view.dispatch(view.state.replaceSelection(text));
  view.focus();
  return true;
}

/** Document offset of a 1-based line and column, clamped to the document. */
export function offsetAt(doc: Text, line: number, column = 1): number {
  const l = doc.line(Math.min(Math.max(1, Math.floor(line) || 1), doc.lines));
  return l.from + Math.min(Math.max(0, (Math.floor(column) || 1) - 1), l.length);
}

const nextFrame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

/**
 * Scroll `pos` to the middle of the editor and keep it there while the layout settles:
 * block widgets far from the old position (rendered doc comments in Reader mode) are only
 * measured once they come into view, which moves the target after the first scroll. Stops
 * after half a second or as soon as the selection moves away from `pos`.
 */
export function centerOnPos(view: EditorView, pos: number): void {
  view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: "center" }) });
  let frames = 0;
  const recenter = () => {
    const sel = view.state.selection.main;
    if (++frames > 30 || !view.dom.isConnected || pos < sel.from || pos > sel.to) return;
    view.requestMeasure({
      read: (v) => {
        const at = v.coordsAtPos(pos);
        // Not rendered at all: the first scroll went to the wrong place.
        if (!at) return true;
        const box = v.scrollDOM.getBoundingClientRect();
        return Math.abs((at.top + at.bottom) / 2 - (box.top + box.bottom) / 2) > v.defaultLineHeight;
      },
      // Dispatching is not allowed while measuring: scroll in the next frame.
      write: (off) =>
        requestAnimationFrame(() => {
          if (off && view.dom.isConnected) view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: "center" }) });
          recenter();
        }),
    });
  };
  requestAnimationFrame(recenter);
}

/** The view of `path` once it is mounted and visible (new tabs need a few frames). */
export async function visibleView(path: string): Promise<EditorView | undefined> {
  for (let i = 0; i < 60; i++) {
    const view = viewFor(path);
    if (view?.dom.isConnected && view.dom.offsetParent !== null) return view;
    await nextFrame();
  }
  return viewFor(path);
}

/**
 * Open a workspace-relative file, put the caret at the 1-based `line`/`column`, scroll it
 * to the middle and (by default) flash the line, like PHPStorm does for navigation from
 * outside the IDE.
 */
export async function openFileAt(
  path: string,
  line?: number | null,
  column?: number | null,
  opts: { flash?: boolean; focus?: boolean } = {},
): Promise<boolean> {
  await openFile(path);
  const view = await visibleView(path);
  if (!view) return false;
  const pos = line ? offsetAt(view.state.doc, line, column ?? 1) : view.state.selection.main.head;
  view.dispatch({ selection: { anchor: pos } });
  centerOnPos(view, pos);
  if (opts.focus !== false) view.focus();
  if (line && opts.flash !== false) flashLineAt(view, pos);
  return true;
}

/**
 * Replace a document with `text` as an edit of only the lines that differ, so the caret,
 * folds and undo history stay (used by the on-save cleanup and the Actions on Save).
 */
export function applyLineChanges(view: EditorView, text: string): void {
  const doc = view.state.doc;
  const lines = text.split("\n");
  const changes: { from: number; to: number; insert: string }[] = [];
  const common = Math.min(doc.lines, lines.length);
  for (let i = 0; i < common; i++) {
    const line = doc.line(i + 1);
    if (line.text !== lines[i]) changes.push({ from: line.from, to: line.to, insert: lines[i] });
  }
  if (lines.length > doc.lines) changes.push({ from: doc.length, to: doc.length, insert: "\n" + lines.slice(doc.lines).join("\n") });
  else if (lines.length < doc.lines) changes.push({ from: doc.line(lines.length).to, to: doc.length, insert: "" });
  // One undo step of its own for everything saving changed (cleanup and Actions on Save).
  if (changes.length) view.dispatch({ changes, userEvent: "save.cleanup", annotations: isolateHistory.of("full") });
}

setEditorBridge({
  caretLines: (path) => {
    const view = viewFor(path);
    if (!view) return [];
    return [...new Set(view.state.selection.ranges.map((r) => view.state.doc.lineAt(r.head).number - 1))];
  },
  // Other views of the file follow through the shared document (docSync.ts).
  applyText: (path, text) => {
    const view = viewFor(path);
    if (!view) return false;
    applyLineChanges(view, text);
    return true;
  },
});
