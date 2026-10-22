/**
 * Extract / Introduce and Inline like PHPStorm. Rust analyses the code and computes every
 * edit (`refactor_*` commands); this module asks the questions in PHPStorm's order and applies
 * the result as one undoable refactoring (`refactoring.ts`):
 *
 * - Introduce Variable (Ctrl+Alt+V): the "Expressions" popup when the caret is in several,
 *   "Multiple occurrences found", then the variable with a suggested name edited in place
 *   (Up / Down pick other suggestions, Enter finishes).
 * - Introduce Constant / Field / Parameter (Ctrl+Alt+C / F / P): the same choosers, then a
 *   dialog with the name, visibility, initialization and "Replace all occurrences".
 * - Extract Method (Ctrl+Alt+M): the Extract Method dialog (name, visibility, parameters,
 *   static, duplicates, signature preview).
 * - Inline (Ctrl+Alt+N): the Inline dialog for variables, methods, functions and constants.
 *
 * Conflicts go through the Problems Detected dialog. Files that are not PHP use the language
 * server's refactorings (typescript-language-server: "Extract to function in module scope"...),
 * chosen in the same popup.
 */
import { createSignal } from "solid-js";
import { ChangeSet, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";
import { isLibraryPath, type ContextAction, type RenameAnswer } from "@/ipc/codeintel";
import {
  refactorApi,
  type ExpressionChoice,
  type ExtractMethodInfo,
  type ExtractMethodOptions,
  type FieldInit,
  type InlineInfo,
  type InlineMode,
  type IntroduceInfo,
  type IntroduceKind,
  type IntroduceOptions,
  type RefactorAnswer,
  type Visibility,
} from "@/ipc/refactor";
import type { LspRange } from "@/ipc/lsp";
import { editor, isDirty } from "@/state/editor";
import { showBalloon } from "@/keymap";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { fromLsp, toLsp } from "@/features/lsp/util";
import { applyWorkspaceEdit, type FileOp } from "./refactoring";
import { renameSessionOf, showRefactoringPreview, startLocalSession } from "./rename";
import { runContextAction } from "./intentions";

const isPhp = (path: string) => /\.(php|phtml|inc)$/i.test(path) && !/\.blade\.php$/i.test(path);

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Unsaved editor contents, so Rust refactors what is on screen. */
export function dirtyBuffers(): Record<string, string> {
  return Object.fromEntries(editor.tabs.filter((t) => isDirty(t) && !isLibraryPath(t.path)).map((t) => [t.path, t.content]));
}

// ---------- Highlighting the expression a popup row stands for ----------

const setHighlight = StateEffect.define<{ from: number; to: number } | null>();
const highlightMark = Decoration.mark({ class: "cm-refactor-highlight" });

const highlightField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    for (const e of tr.effects) {
      if (e.is(setHighlight))
        return e.value && e.value.to > e.value.from ? Decoration.set([highlightMark.range(e.value.from, e.value.to)]) : Decoration.none;
    }
    return marks.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** Editor extension: the expression highlighted by the choosers. */
export function refactorHighlight(): Extension {
  return highlightField;
}

function highlight(view: EditorView, range: LspRange | null): void {
  if (view.state.field(highlightField, false) === undefined) return;
  const doc = view.state.doc;
  view.dispatch({
    effects: setHighlight.of(range ? { from: fromLsp(doc, range.start), to: fromLsp(doc, range.end) } : null),
  });
}

// ---------- Chooser popup ("Expressions", "Multiple occurrences found", ...) ----------

export interface ChooserItem {
  label: string;
  /** Right-aligned text (a shortcut). */
  detail?: string;
  /** Highlighted in the editor while the row is selected. */
  range?: LspRange;
}

export interface ChooserRequest {
  title: string;
  items: ChooserItem[];
  x: number;
  y: number;
  /** The row was selected (highlight its expression). */
  select: (index: number) => void;
  done: (index: number | null) => void;
}

const [chooser, setChooser] = createSignal<ChooserRequest | null>(null);
export { chooser };

/** A list popup below the caret; resolves to the chosen index or `null`. */
export function choose(view: EditorView, title: string, items: ChooserItem[]): Promise<number | null> {
  const pos = view.state.selection.main.head;
  const coords = view.coordsAtPos(pos) ?? view.coordsAtPos(view.state.doc.lineAt(pos).from);
  return new Promise((resolve) => {
    setChooser({
      title,
      items,
      x: Math.max(8, (coords?.left ?? 200) - 8),
      y: (coords?.bottom ?? 200) + 4,
      select: (i) => highlight(view, items[i]?.range ?? null),
      done: (i) => {
        setChooser(null);
        highlight(view, null);
        view.focus();
        resolve(i);
      },
    });
  });
}

// ---------- Dialogs ----------

export interface IntroduceDialogRequest {
  kind: Exclude<IntroduceKind, "variable">;
  info: IntroduceInfo;
  done: (options: (IntroduceOptions & { preview: boolean }) | null) => void;
}

export interface ExtractMethodDialogRequest {
  info: ExtractMethodInfo;
  done: (options: (ExtractMethodOptions & { preview: boolean }) | null) => void;
}

export interface InlineDialogRequest {
  info: InlineInfo;
  done: (result: { mode: InlineMode; preview: boolean } | null) => void;
}

const [introduceDialog, setIntroduceDialog] = createSignal<IntroduceDialogRequest | null>(null);
const [extractMethodDialog, setExtractMethodDialog] = createSignal<ExtractMethodDialogRequest | null>(null);
const [inlineDialog, setInlineDialog] = createSignal<InlineDialogRequest | null>(null);
export { extractMethodDialog, inlineDialog, introduceDialog };

function ask<T, R extends { done: (v: T) => void }>(set: (r: R | null) => void, req: Omit<R, "done">): Promise<T> {
  return new Promise((resolve) =>
    set({
      ...req,
      done: (v: T) => {
        set(null);
        resolve(v);
      },
    } as R),
  );
}

// ---------- Applying ----------

const TITLES: Record<IntroduceKind, string> = {
  variable: "Introduce Variable",
  constant: "Introduce Constant",
  field: "Introduce Field",
  parameter: "Introduce Parameter",
};

/** Problems Detected / Refactoring Preview, then the edit as one undoable step. */
export async function applyAnswer(
  title: string,
  answer: RefactorAnswer,
  opts: { preview?: boolean; beforeEditors?: () => void; fileOps?: FileOp[] } = {},
): Promise<boolean> {
  if (answer.conflicts.length || opts.preview) {
    const asRename: RenameAnswer = {
      ...answer,
      source: "index",
      missing: null,
      fileRename: null,
    };
    const ok = await showRefactoringPreview(title, asRename);
    if (!ok) return false;
  }
  if (!answer.edit.changes.length && !opts.fileOps?.length) return false;
  try {
    await applyWorkspaceEdit(answer.edit, {
      label: title,
      beforeEditors: opts.beforeEditors,
      fileOps: opts.fileOps,
    });
  } catch (e) {
    showBalloon(`${title} failed`, message(e), 6000);
    return false;
  }
  return true;
}

function selectionRange(view: EditorView): LspRange {
  const sel = view.state.selection.main;
  const doc = view.state.doc;
  return { start: toLsp(doc, sel.from), end: toLsp(doc, sel.to) };
}

function readOnly(path: string, view: EditorView): boolean {
  if (isLibraryPath(path) || view.state.readOnly) {
    showHint(view, view.state.selection.main.head, "Cannot perform refactoring in a read-only file");
    return true;
  }
  return false;
}

/** The changes a workspace edit makes to the editor's own file. */
function ownChanges(view: EditorView, path: string, answer: RefactorAnswer): ChangeSet | null {
  const doc = view.state.doc;
  const specs = answer.edit.changes.flatMap((c) =>
    c.kind === "edit" && c.path === path
      ? c.edits.map((e) => ({
          from: fromLsp(doc, e.range.start),
          to: fromLsp(doc, e.range.end),
          insert: e.newText,
        }))
      : [],
  );
  if (answer.edit.changes.some((c) => c.kind !== "edit" || c.path !== path)) return null;
  return specs.length ? ChangeSet.of(specs, doc.length, view.state.lineBreak) : null;
}

// ---------- Language servers (JS / TS) ----------

const LSP_TITLES: Record<string, string> = {
  IntroduceVariable: "Introduce Variable",
  IntroduceConstant: "Introduce Constant",
  IntroduceField: "Introduce Field",
  IntroduceParameter: "Introduce Parameter",
  ExtractMethod: "Extract Method",
  Inline: "Inline",
};

async function lspRefactoring(refactoring: string, path: string, view: EditorView): Promise<void> {
  const pos = view.state.selection.main.head;
  let actions: ContextAction[];
  try {
    await syncNow(path);
    actions = await refactorApi.lspActions(path, selectionRange(view), refactoring);
  } catch (e) {
    showHint(view, pos, message(e));
    return;
  }
  const enabled = actions.filter((a) => !a.disabled);
  if (!enabled.length) {
    const why = actions.find((a) => a.disabled)?.disabled;
    showHint(view, pos, why ?? `${LSP_TITLES[refactoring] ?? refactoring} is not available here`);
    return;
  }
  const picked =
    enabled.length === 1
      ? 0
      : await choose(
          view,
          LSP_TITLES[refactoring] ?? refactoring,
          enabled.map((a) => ({ label: a.title })),
        );
  if (picked == null) return;
  await runContextAction(path, view, enabled[picked]);
}

// ---------- Introduce Variable / Constant / Field / Parameter ----------

const LSP_ACTION: Record<IntroduceKind, string> = {
  variable: "IntroduceVariable",
  constant: "IntroduceConstant",
  field: "IntroduceField",
  parameter: "IntroduceParameter",
};

/** The expression to introduce: the selection, or one chosen in the "Expressions" popup. */
async function pickExpression(kind: IntroduceKind, path: string, view: EditorView): Promise<IntroduceInfo | null> {
  const pos = view.state.selection.main.head;
  const content = view.state.doc.toString();
  let prepared;
  try {
    prepared = await refactorApi.introducePrepare(path, kind, selectionRange(view), content);
    if (!prepared.info && prepared.expressions.length) {
      const items = prepared.expressions.map((e: ExpressionChoice) => ({
        label: e.text,
        range: e.range,
      }));
      const i = await choose(view, "Expressions", items);
      if (i == null) return null;
      prepared = await refactorApi.introducePrepare(path, kind, prepared.expressions[i].range, content);
    }
  } catch (e) {
    showHint(view, pos, message(e));
    return null;
  }
  return prepared.info ?? null;
}

/** "Multiple occurrences found": `true` to replace all, `null` when cancelled. */
async function askReplaceAll(view: EditorView, info: IntroduceInfo): Promise<boolean | null> {
  if (info.occurrences < 2) return false;
  const i = await choose(view, "Multiple occurrences found", [
    { label: "Replace this occurrence only", range: info.range },
    { label: `Replace all ${info.occurrences} occurrences` },
  ]);
  return i == null ? null : i === 1;
}

export async function introduce(kind: IntroduceKind, path: string, view: EditorView): Promise<void> {
  if (readOnly(path, view) || renameSessionOf(view)) return;
  if (!isPhp(path)) {
    await lspRefactoring(LSP_ACTION[kind], path, view);
    return;
  }
  const info = await pickExpression(kind, path, view);
  if (!info) return;
  if (kind === "variable") {
    await introduceVariable(path, view, info);
    return;
  }
  const options = await ask<(IntroduceOptions & { preview: boolean }) | null, IntroduceDialogRequest>(setIntroduceDialog, { kind, info });
  view.focus();
  if (!options) return;
  const content = view.state.doc.toString();
  let answer: RefactorAnswer;
  try {
    answer = await refactorApi.introduce(path, kind, info.range, content, options, dirtyBuffers());
  } catch (e) {
    showHint(view, view.state.selection.main.head, message(e));
    return;
  }
  await applyAnswer(TITLES[kind], answer, { preview: options.preview });
}

async function introduceVariable(path: string, view: EditorView, info: IntroduceInfo): Promise<void> {
  const replaceAll = await askReplaceAll(view, info);
  if (replaceAll == null) return;
  const content = view.state.doc.toString();
  const name = info.names[0] ?? "value";
  let preview: RefactorAnswer;
  try {
    preview = await refactorApi.introduce(path, "variable", info.range, content, { name, replaceAll });
  } catch (e) {
    showHint(view, view.state.selection.main.head, message(e));
    return;
  }
  if (view.state.doc.toString() !== content) return;
  const changes = ownChanges(view, path, preview);
  if (!changes) {
    await applyAnswer(TITLES.variable, preview);
    return;
  }
  const after = changes.apply(view.state.doc);
  const ranges = preview.nameRanges.map((r) => ({
    from: fromLsp(after, r.start),
    to: fromLsp(after, r.end),
  }));
  startLocalSession(path, view, changes, ranges, {
    suggestions: info.names,
    hint: "Choose a name with Up / Down or type one, then press Enter",
    finish: async (typed, revert) => {
      let answer: RefactorAnswer;
      try {
        answer = await refactorApi.introduce(path, "variable", info.range, content, { name: typed, replaceAll });
      } catch (e) {
        showHint(view, view.state.selection.main.head, message(e));
        return false;
      }
      const applied = await applyAnswer(TITLES.variable, answer, {
        beforeEditors: revert,
      });
      if (applied) {
        // The caret goes to the end of the new variable's name, like PHPStorm.
        const doc = view.state.doc;
        const at = answer.nameRanges.map((r) => fromLsp(doc, r.end)).find((p) => p >= 0);
        if (at != null) view.dispatch({ selection: { anchor: at } });
      }
      return true;
    },
  });
}

// ---------- Extract Method ----------

export async function extractMethod(path: string, view: EditorView): Promise<void> {
  if (readOnly(path, view) || renameSessionOf(view)) return;
  if (!isPhp(path)) {
    await lspRefactoring("ExtractMethod", path, view);
    return;
  }
  const pos = view.state.selection.main.head;
  const content = view.state.doc.toString();
  let info: ExtractMethodInfo;
  try {
    info = await refactorApi.extractMethodPrepare(path, selectionRange(view), content);
  } catch (e) {
    showHint(view, pos, message(e));
    return;
  }
  highlight(view, info.range);
  const options = await ask<(ExtractMethodOptions & { preview: boolean }) | null, ExtractMethodDialogRequest>(setExtractMethodDialog, {
    info,
  });
  highlight(view, null);
  view.focus();
  if (!options) return;
  let answer: RefactorAnswer;
  try {
    answer = await refactorApi.extractMethod(path, info.range, content, options);
  } catch (e) {
    showHint(view, pos, message(e));
    return;
  }
  await applyAnswer(info.title, answer, { preview: options.preview });
}

// ---------- Inline ----------

export async function inline(path: string, view: EditorView): Promise<void> {
  if (readOnly(path, view) || renameSessionOf(view)) return;
  if (!isPhp(path)) {
    await lspRefactoring("Inline", path, view);
    return;
  }
  const pos = view.state.selection.main.head;
  const doc = view.state.doc;
  const content = doc.toString();
  const at = toLsp(doc, pos);
  const buffers = dirtyBuffers();
  let info: InlineInfo;
  try {
    info = await refactorApi.inlinePrepare(path, at, content, buffers);
  } catch (e) {
    showHint(view, pos, message(e));
    return;
  }
  // A variable used once is inlined right away, like PHPStorm.
  let mode: InlineMode = "all";
  let preview = false;
  if (!(info.kind === "variable" && info.occurrences === 1)) {
    const result = await ask<{ mode: InlineMode; preview: boolean } | null, InlineDialogRequest>(setInlineDialog, { info });
    view.focus();
    if (!result) return;
    mode = result.mode;
    preview = result.preview;
  }
  let answer: RefactorAnswer;
  try {
    answer = await refactorApi.inline(path, at, content, mode, buffers);
  } catch (e) {
    showHint(view, pos, message(e));
    return;
  }
  await applyAnswer(`${info.title} ${info.name}`, answer, { preview });
}

/** Visibility choices of the dialogs, PHPStorm's order. */
export const VISIBILITIES: Visibility[] = ["public", "protected", "private"];

export const FIELD_INIT_LABELS: Record<FieldInit, string> = {
  currentMethod: "Current method",
  fieldDeclaration: "Field declaration",
  constructor: "Class constructor",
};

// ---------- Refactor This ----------

/** The refactorings of the Refactor This popup (Ctrl+Alt+Shift+T), PHPStorm's order. */
export const REFACTOR_THIS = [
  "RenameElement",
  "ChangeSignature",
  "Move",
  "CopyElement",
  "SafeDelete",
  "MakeStatic",
  "IntroduceVariable",
  "IntroduceConstant",
  "IntroduceField",
  "IntroduceParameter",
  "ExtractMethod",
  "Inline",
];

/** Refactor This: the refactorings that are available, with their shortcuts. */
export async function refactorThis(
  view: EditorView,
  available: (id: string) => boolean,
  title: (id: string) => string,
  shortcut: (id: string) => string,
  run: (id: string) => void,
): Promise<void> {
  const ids = REFACTOR_THIS.filter(available);
  const i = await choose(
    view,
    "Refactor This",
    ids.map((id) => ({ label: title(id), detail: shortcut(id) })),
  );
  if (i != null) run(ids[i]);
}
