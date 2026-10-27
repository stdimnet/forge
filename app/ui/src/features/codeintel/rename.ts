/**
 * Rename (Shift+F6) like PHPStorm. The in-place rename puts an edit box on the name and
 * edits its occurrences in the file live (per-keystroke editor work, so it lives here);
 * Enter (or Tab, or moving the caret out) renames everywhere, Escape cancels, Shift+F6
 * again opens the Rename dialog with more options. Rust answers what to rename
 * (`code_prepare_rename`: the language server, else the PHP index), computes the edit,
 * its conflicts and the class file rename (`code_rename`); `refactoring.ts` applies it as
 * one undoable step.
 *
 * Live edits are not undo steps: committing first reverts them and then applies the real
 * edit, so Ctrl+Z restores the old name in one go.
 */
import { createSignal } from "solid-js";
import {
  EditorSelection,
  EditorState,
  Prec,
  StateEffect,
  StateField,
  Transaction,
  type ChangeSet,
  type Extension,
  type Text,
} from "@codemirror/state";
import { Decoration, EditorView, keymap, showTooltip, ViewPlugin, type DecorationSet, type Tooltip } from "@codemirror/view";
import { closeCompletion, completionStatus } from "@codemirror/autocomplete";
import { lspApi } from "@/ipc/lsp";
import { codeApi, isLibraryPath, type PrepareRenameAnswer, type RenameAnswer } from "@/ipc/codeintel";
import { editor, isDirty } from "@/state/editor";
import { showBalloon } from "@/keymap";
import { shortcutLabel } from "@/keymap/store";
import { openSettings } from "@/features/settings/dialogState";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { fromLsp, h, toLsp } from "@/features/lsp/util";
import { applyWorkspaceEdit } from "./refactoring";

export const NOT_RENAMEABLE = "Cannot perform refactoring. Caret should be positioned at the symbol to be renamed";

interface Span {
  from: number;
  to: number;
}

export interface RenameSession {
  path: string;
  /** The occurrences in this file; `ranges[primary]` is the edit box. */
  ranges: Span[];
  primary: number;
  /** The old name (without `$`). */
  original: string;
  answer: PrepareRenameAnswer;
  /** PHPStorm's "Rename file" option for class files. */
  renameFile: boolean;
  /**
   * Introduce Variable: the refactoring is shown with the new name, edited in place; the
   * real (undoable) edit is made with the final name when the editing ends.
   */
  local?: LocalSession;
}

export interface LocalSession {
  /** The document before the preview. */
  before: Text;
  /** Every change since `before` (the preview and the typed name). */
  applied: ChangeSet;
  /** Name suggestions (Up / Down or a click picks one). */
  suggestions: string[];
  /** Hint under the name. */
  hint: string;
  /** Make the real edit with `name`; `revert` puts `before` back right before it applies. */
  finish: (name: string, revert: () => void) => Promise<boolean>;
}

const startRenameEffect = StateEffect.define<RenameSession>();
const endRenameEffect = StateEffect.define<null>();

/** Text of every range after the user typed in the edit box (and mirrored it). */
function mapSession(s: RenameSession, tr: Transaction): RenameSession {
  return {
    ...s,
    ranges: s.ranges.map((r) => ({ from: tr.changes.mapPos(r.from, -1), to: tr.changes.mapPos(r.to, 1) })),
    local: s.local && { ...s.local, applied: s.local.applied.compose(tr.changes) },
  };
}

export const renameField = StateField.define<RenameSession | null>({
  create: () => null,
  update(s, tr) {
    for (const e of tr.effects) {
      if (e.is(startRenameEffect)) return e.value;
      if (e.is(endRenameEffect)) return null;
    }
    return s && tr.docChanged ? mapSession(s, tr) : s;
  },
  provide: (f) => [
    EditorView.decorations.from(f, (s): DecorationSet => {
      if (!s) return Decoration.none;
      const marks = s.ranges.map((r, i) =>
        (i === s.primary ? primaryMark : occurrenceMark).range(r.from, Math.max(r.to, r.from)),
      );
      return Decoration.set(
        marks.filter((m) => m.from < m.to),
        true,
      );
    }),
    showTooltip.from(f, (s): Tooltip | null => (s ? renameTooltip(s) : null)),
  ],
});

const primaryMark = Decoration.mark({ class: "cm-rename-primary" });
const occurrenceMark = Decoration.mark({ class: "cm-rename-occurrence" });

const fileName = (p: string) => p.split("/").pop() ?? p;

/** Replace the name being edited (a picked suggestion); the mirror updates the others. */
function setName(view: EditorView, name: string): void {
  const s = renameSessionOf(view);
  if (!s) return;
  const p = s.ranges[s.primary];
  view.dispatch({ changes: { from: p.from, to: p.to, insert: name }, selection: { anchor: p.from + name.length }, userEvent: "input.complete" });
}

/** Up / Down: the next or previous suggested name. */
function cycleName(view: EditorView, delta: number): boolean {
  const s = renameSessionOf(view);
  const list = s?.local?.suggestions ?? [];
  if (!s || list.length < 2) return false;
  const p = s.ranges[s.primary];
  const cur = list.indexOf(view.state.sliceDoc(p.from, p.to));
  setName(view, list[(cur + delta + list.length) % list.length]);
  return true;
}

function localTooltip(s: RenameSession, local: LocalSession): Tooltip {
  const primary = s.ranges[s.primary];
  return {
    pos: primary.from,
    above: false,
    create: (view) => {
      const dom = h("div", "cm-rename-hint cm-introduce-hint");
      dom.addEventListener("mousedown", (e) => e.preventDefault());
      const list = h("div", "cm-introduce-names");
      const rows = local.suggestions.map((name) => {
        const row = h("div", "cm-introduce-name", `$${name}`);
        row.addEventListener("click", () => setName(view, name));
        list.append(row);
        return { name, row };
      });
      const current = () => {
        const cur = view.state.field(renameField, false);
        if (!cur) return "";
        const p = cur.ranges[cur.primary];
        return view.state.sliceDoc(p.from, p.to);
      };
      const mark = () => rows.forEach((r) => r.row.classList.toggle("selected", r.name === current()));
      mark();
      if (rows.length > 1) dom.append(list);
      dom.append(h("div", "cm-rename-hint-text", local.hint));
      return { dom, update: mark };
    },
  };
}

function renameTooltip(s: RenameSession): Tooltip {
  if (s.local) return localTooltip(s, s.local);
  const primary = s.ranges[s.primary];
  return {
    pos: primary.from,
    above: false,
    create: (view) => {
      const dom = h("div", "cm-rename-hint");
      // Clicks in the hint must not move the caret out of the edit box.
      dom.addEventListener("mousedown", (e) => e.preventDefault());
      if (s.answer.classFile) {
        const box = h("input") as HTMLInputElement;
        box.type = "checkbox";
        box.checked = s.renameFile;
        box.addEventListener("change", () => {
          const cur = view.state.field(renameField, false);
          if (cur) cur.renameFile = box.checked;
        });
        dom.append(h("label", "cm-rename-hint-option", box, `Rename file '${fileName(s.answer.classFile)}'`));
      }
      const key = shortcutLabel("RenameElement") || "Shift+F6";
      dom.append(h("div", "cm-rename-hint-text", `Press ${key} to show dialog with more options`));
      return { dom };
    },
  };
}

const internal = Transaction.addToHistory.of(false);

/**
 * Keystrokes in the edit box are mirrored to every occurrence in the same transaction;
 * none of it goes into the undo history.
 */
const mirror = EditorState.transactionFilter.of((tr) => {
  const s = tr.startState.field(renameField, false);
  if (!s || !tr.docChanged || tr.annotation(Transaction.addToHistory) === false) return tr;
  if (tr.isUserEvent("undo") || tr.isUserEvent("redo")) {
    // Undo while renaming cancels the rename first (the next Ctrl+Z undoes as usual).
    return [{ effects: cancelRequest.of(null) }];
  }
  const p = s.ranges[s.primary];
  let inside = true;
  tr.changes.iterChangedRanges((fromA, toA) => {
    if (fromA < p.from || toA > p.to) inside = false;
  });
  if (!inside) return [tr, { effects: commitRequest.of(null) }];
  const mapped = mapSession(s, tr);
  const np = mapped.ranges[s.primary];
  const text = tr.newDoc.sliceString(np.from, np.to);
  const changes = mapped.ranges
    .filter((_, i) => i !== s.primary)
    .map((r) => ({ from: r.from, to: r.to, insert: text }))
    .filter((c) => tr.newDoc.sliceString(c.from, c.to) !== text);
  return [tr, { changes, sequential: true, annotations: internal }];
});

/** Asks the view plugin to finish the rename after the current update. */
const commitRequest = StateEffect.define<null>();
const cancelRequest = StateEffect.define<null>();

const finisher = ViewPlugin.define((view) => ({
  update(u) {
    const s = u.startState.field(renameField, false);
    if (!s) return;
    for (const tr of u.transactions) {
      for (const e of tr.effects) {
        if (e.is(commitRequest)) setTimeout(() => void commitRename(view));
        if (e.is(cancelRequest)) setTimeout(() => cancelRename(view));
      }
    }
    const now = u.state.field(renameField, false);
    // No completion popup while renaming: Enter belongs to the rename.
    if (now && completionStatus(u.state) !== null) setTimeout(() => closeCompletion(view));
    if (now && u.selectionSet && !u.docChanged) {
      const p = now.ranges[now.primary];
      const head = u.state.selection.main.head;
      // Moving the caret out of the edit box finishes the rename, like PHPStorm.
      if (head < p.from || head > p.to) setTimeout(() => void commitRename(view));
    }
  },
}));

const renameKeys = Prec.highest(
  keymap.of([
    { key: "Enter", run: (v) => (v.state.field(renameField, false) ? (closeCompletion(v), void commitRename(v), true) : false) },
    { key: "Tab", run: (v) => (v.state.field(renameField, false) ? (void commitRename(v), true) : false) },
    { key: "Escape", run: (v) => (v.state.field(renameField, false) ? (cancelRename(v), true) : false) },
    { key: "ArrowDown", run: (v) => cycleName(v, 1) },
    { key: "ArrowUp", run: (v) => cycleName(v, -1) },
  ]),
);

/** Editor extension: the in-place rename. */
export function inPlaceRename(): Extension {
  return [renameField, mirror, finisher, renameKeys];
}

export const renameSessionOf = (view: EditorView) => view.state.field(renameField, false) ?? null;

/** The edit box's current text (the new name, without `$`). */
export function currentName(view: EditorView): string {
  const s = renameSessionOf(view);
  if (!s) return "";
  const p = s.ranges[s.primary];
  return view.state.sliceDoc(p.from, p.to);
}

/** The changes that put the old name back everywhere (live edits are not undo steps). */
function revertSpec(view: EditorView, s: RenameSession) {
  return s.ranges
    .map((r) => ({ from: r.from, to: r.to, insert: s.original }))
    .filter((c) => view.state.sliceDoc(c.from, c.to) !== c.insert);
}

/** Escape: put the old name back (Introduce Variable keeps the variable with the name). */
export function cancelRename(view: EditorView): void {
  const s = renameSessionOf(view);
  if (!s) return;
  if (s.local) {
    void commitRename(view);
    return;
  }
  view.dispatch({ changes: revertSpec(view, s), effects: endRenameEffect.of(null), annotations: internal });
}

/**
 * A feature that renames names of its own files in place (SQL aliases and CTEs): returns
 * true when it handled Shift+F6 at the caret.
 */
export type RenameProvider = (path: string, view: EditorView) => Promise<boolean>;
const renameProviders: RenameProvider[] = [];

export function registerRenameProvider(provider: RenameProvider): void {
  renameProviders.push(provider);
}

/** Start the in-place rename at the caret (Shift+F6), or open the dialog when it runs. */
export async function startRename(path: string, view: EditorView): Promise<void> {
  const running = renameSessionOf(view);
  if (running) {
    if (running.local) void commitRename(view);
    else openRenameDialog(view);
    return;
  }
  const pos = view.state.selection.main.head;
  if (isLibraryPath(path) || view.state.readOnly) {
    showHint(view, pos, "Cannot rename in a read-only file");
    return;
  }
  for (const provider of renameProviders) {
    if (await provider(path, view).catch(() => false)) return;
  }
  await syncNow(path);
  const doc = view.state.doc;
  let answer: PrepareRenameAnswer;
  try {
    answer = await codeApi.prepareRename(path, toLsp(doc, pos), doc.toString());
  } catch (e) {
    showHint(view, pos, String(e instanceof Error ? e.message : e));
    return;
  }
  if (answer.invalid || !answer.range) {
    showHint(view, pos, answer.message ?? answer.missing?.message ?? NOT_RENAMEABLE);
    return;
  }
  // The file changed while Rust was answering: start over with the next Shift+F6.
  if (!view.state.doc.eq(doc)) return;
  const toSpan = (r: { start: { line: number; character: number }; end: { line: number; character: number } }): Span => ({
    from: fromLsp(doc, r.start),
    to: fromLsp(doc, r.end),
  });
  const primarySpan = toSpan(answer.range);
  const ranges = answer.occurrences.map(toSpan);
  let primary = ranges.findIndex((r) => r.from === primarySpan.from && r.to === primarySpan.to);
  if (primary < 0) {
    ranges.push(primarySpan);
    ranges.sort((a, b) => a.from - b.from);
    primary = ranges.indexOf(primarySpan);
  }
  const session: RenameSession = {
    path,
    ranges,
    primary,
    original: doc.sliceString(primarySpan.from, primarySpan.to),
    answer,
    renameFile: true,
  };
  view.dispatch({
    effects: [startRenameEffect.of(session), EditorView.scrollIntoView(primarySpan.from, { y: "nearest" })],
    selection: EditorSelection.range(primarySpan.from, primarySpan.to),
  });
  view.focus();
}

/** Unsaved editor contents, so Rust renames what is on screen. */
function dirtyBuffers(): Record<string, string> {
  return Object.fromEntries(editor.tabs.filter((t) => isDirty(t) && !isLibraryPath(t.path)).map((t) => [t.path, t.content]));
}

let premiumHintShown = false;

/** Once per session: the index renamed because the server cannot (Intelephense Premium). */
export function premiumHint(answer: Pick<RenameAnswer, "source" | "missing">): boolean {
  if (premiumHintShown || answer.source !== "index" || !answer.missing?.premium) return false;
  premiumHintShown = true;
  showBalloon(
    "Renamed with the PHP index",
    "Intelephense renames only with a premium licence, so Forge used its own index of the project's classes, methods, functions and properties. Enter a licence key in Settings | PHP for the full rename.",
    0,
    [{ label: "Enter Licence Key…", run: () => openSettings("lang.php") }],
  );
  return true;
}

/** Test hook. */
export const resetPremiumHint = () => (premiumHintShown = false);

export interface PreviewRequest {
  title: string;
  answer: RenameAnswer;
  done: (ok: boolean) => void;
}

const [previewRequest, setPreviewRequest] = createSignal<PreviewRequest | null>(null);
export { previewRequest };

/** Refactoring Preview / Problems Detected: resolves `true` for "Do Refactor". */
export function showRefactoringPreview(title: string, answer: RenameAnswer): Promise<boolean> {
  return new Promise((resolve) =>
    setPreviewRequest({
      title,
      answer,
      done: (ok) => {
        setPreviewRequest(null);
        resolve(ok);
      },
    }),
  );
}

export interface RenameDialogRequest {
  label: string;
  name: string;
  classFile?: string | null;
  renameFile: boolean;
  done: (result: { name: string; renameFile: boolean; preview: boolean } | null) => void;
}

const [renameDialog, setRenameDialog] = createSignal<RenameDialogRequest | null>(null);
export { renameDialog };

/** Shift+F6 during the in-place rename: PHPStorm's Rename dialog. */
function openRenameDialog(view: EditorView): void {
  const s = renameSessionOf(view);
  if (!s) return;
  setRenameDialog({
    label: s.answer.label ?? `Rename '${s.original}' and its usages to:`,
    name: currentName(view),
    classFile: s.answer.classFile,
    renameFile: s.renameFile,
    done: (result) => {
      setRenameDialog(null);
      if (!result) {
        view.focus();
        return;
      }
      const cur = renameSessionOf(view);
      if (!cur) return;
      cur.renameFile = result.renameFile;
      void commitRename(view, { name: result.name, preview: result.preview });
    },
  });
}

let committing = false;

/**
 * Enter: rename everywhere. The edit is computed for the file as it was before the live
 * edits; conflicts (and "Preview") show the Refactoring Preview first.
 */
export async function commitRename(view: EditorView, opts: { name?: string; preview?: boolean } = {}): Promise<void> {
  const s = renameSessionOf(view);
  if (!s || committing) return;
  if (s.local) {
    await commitLocal(view, s.local);
    return;
  }
  const p = s.ranges[s.primary];
  const typed = (opts.name ?? view.state.sliceDoc(p.from, p.to)).replace(/^\$/, "");
  if (typed === s.original && !opts.preview) {
    view.dispatch({ effects: endRenameEffect.of(null), annotations: internal });
    return;
  }
  committing = true;
  try {
    const revert = view.state.changes(revertSpec(view, s));
    const originalDoc = revert.apply(view.state.doc);
    const originalText = originalDoc.toString();
    const at = revert.mapPos(p.from, -1);
    await syncNow(s.path);
    // The server renames the file as it was before the live edits.
    if (s.answer.source === "lsp") await lspApi.didChange(s.path, originalText).catch(() => undefined);
    let answer: RenameAnswer;
    try {
      answer = await codeApi.rename(s.path, toLsp(originalDoc, at), (s.answer.sigil ? "$" : "") + typed, {
        buffers: { ...dirtyBuffers(), [s.path]: originalText },
        renameFile: s.renameFile,
      });
    } catch (e) {
      // An invalid name keeps the edit box open, like PHPStorm.
      showHint(view, p.from, String(e instanceof Error ? e.message : e));
      if (s.answer.source === "lsp") await lspApi.didChange(s.path, view.state.doc.toString()).catch(() => undefined);
      return;
    }
    if (answer.conflicts.length || opts.preview) {
      const ok = await showRefactoringPreview(`Rename ${s.original} to ${typed}`, answer);
      if (!ok) {
        cancelRename(view);
        return;
      }
    }
    if (!renameSessionOf(view)) return;
    const label = `Rename ${s.original} to ${typed}`;
    try {
      await applyWorkspaceEdit(answer.edit, {
        label,
        beforeEditors: () => {
          const cur = renameSessionOf(view);
          if (cur) view.dispatch({ changes: revertSpec(view, cur), effects: endRenameEffect.of(null), annotations: internal });
        },
      });
    } catch (e) {
      cancelRename(view);
      showBalloon(`${label} failed`, String(e instanceof Error ? e.message : e), 6000);
      return;
    }
    if (!premiumHint(answer) && answer.skipped > 0) {
      showBalloon(`${answer.skipped} dynamic usage${answer.skipped === 1 ? " was" : "s were"} not renamed`, "Find them with Find Usages (Alt+F7).");
    }
    if (view.dom.isConnected) view.focus();
  } finally {
    committing = false;
  }
}

/**
 * Start editing a new name in place over a preview of a refactoring (Introduce Variable):
 * `changes` turn the current document into the preview, `ranges` are the name's occurrences
 * in the preview.
 */
export function startLocalSession(
  path: string,
  view: EditorView,
  changes: ChangeSet,
  ranges: Span[],
  local: Omit<LocalSession, "before" | "applied">,
): void {
  const before = view.state.doc;
  view.dispatch({ changes, annotations: internal });
  const sorted = [...ranges].sort((a, b) => a.from - b.from);
  const head = view.state.selection.main.head;
  const near = sorted.reduce((best, r, i) => (Math.abs(r.from - head) < Math.abs(sorted[best].from - head) ? i : best), 0);
  const primarySpan = sorted[near];
  const session: RenameSession = {
    path,
    ranges: sorted,
    primary: near,
    original: view.state.sliceDoc(primarySpan.from, primarySpan.to),
    answer: { invalid: false, source: "index", occurrences: [], sigil: false },
    renameFile: false,
    local: { ...local, before, applied: changes },
  };
  view.dispatch({
    effects: [startRenameEffect.of(session), EditorView.scrollIntoView(primarySpan.from, { y: "nearest" })],
    selection: EditorSelection.range(primarySpan.from, primarySpan.to),
  });
  view.focus();
}

/** Finish an in-place Introduce Variable with the typed name. */
async function commitLocal(view: EditorView, local: LocalSession): Promise<void> {
  const name = currentName(view).replace(/^\$/, "");
  const revert = () => {
    const cur = renameSessionOf(view);
    if (!cur?.local) return;
    view.dispatch({ changes: cur.local.applied.invert(cur.local.before), effects: endRenameEffect.of(null), annotations: internal });
  };
  committing = true;
  try {
    // Not applied (an invalid name): the edit box stays, like PHPStorm.
    if (!(await local.finish(name, revert))) return;
    // The edit failed before it reverted the preview: put the document back.
    revert();
    if (view.dom.isConnected) view.focus();
  } finally {
    committing = false;
  }
}
