/**
 * Applying workspace edits (rename, quick fixes, server commands) like PHPStorm: Rust plans
 * the edit (`code_plan_edit`): open editors apply their files' text edits themselves, as
 * one undoable step each; closed files, file renames and deletions go through the project
 * backend (`code_apply_edit`, so remote projects work). Open files that are renamed are
 * saved, written by the backend and reopened at their new path.
 *
 * The whole refactoring is one undo step: every affected editor gets a history entry
 * tagged with the refactoring (`refactoringStep`). Undo (Ctrl+Z) in any of them undoes the
 * others too (when they were not edited since), reverts the backend files
 * (`code_revert_edit`) and moves renamed tabs back; Redo does the reverse.
 */
import { StateEffect, type Extension, type Text } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { invertedEffects, isolateHistory, redo, undo } from "@codemirror/commands";
import { listen } from "@/ipc";
import { codeApi, isLibraryPath, type AppliedFile, type FileRename, type WorkspaceEdit } from "@/ipc/codeintel";
import { workspaceApi } from "@/ipc/workspace";
import { editor, closeTab, isDirty, openFile, saveFile } from "@/state/editor";
import { showBalloon } from "@/keymap";
import { refreshDirs } from "@/features/project/ProjectView";
import { viewFor } from "@/features/lsp/navigation";
import { fromLsp } from "@/features/lsp/util";

/** Marks the history entry of a refactoring in an editor; `undo` on the inverse entry. */
const refactoringStep = StateEffect.define<{ id: number; undo: boolean }>();

interface EditorPart {
  path: string;
  /** The document before / after the refactoring, to check it was not edited since. */
  before: Text;
  after: Text;
}

/**
 * A file or folder that moves or is copied as part of a refactoring (Project view Move and
 * Copy). It happens before the edit, whose paths are the ones after it.
 */
export interface FileOp {
  kind: "move" | "copy";
  from: string;
  to: string;
}

interface Refactoring {
  id: number;
  label: string;
  editors: EditorPart[];
  backend: WorkspaceEdit;
  applied: AppliedFile[];
  reopen: FileRename[];
  close: string[];
  fileOps: FileOp[];
  /** Tabs that moved with `fileOps`. */
  movedTabs: FileRename[];
  undone: boolean;
}

const refactorings = new Map<number, Refactoring>();
let seq = 0;
/** Set while this module drives undo / redo in the other editors. */
let driving = false;

const parentOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");

/** Editors that are open (with a CodeMirror view) for a workspace file. */
function openEditors(): string[] {
  return editor.tabs.map((t) => t.path).filter((p) => !isLibraryPath(p) && viewFor(p));
}

async function waitForView(path: string): Promise<EditorView | undefined> {
  for (let i = 0; i < 80; i++) {
    const v = viewFor(path);
    if (v) return v;
    await new Promise((r) => setTimeout(r, 25));
  }
  return undefined;
}

/** Move tabs (renamed files) and keep the active tab where it was. */
async function moveTabs(moves: FileRename[]): Promise<void> {
  if (!moves.length) return;
  const active = editor.active;
  for (const m of moves) {
    if (!editor.tabs.some((t) => t.path === m.from)) continue;
    closeTab(m.from);
    await openFile(m.to).catch(() => undefined);
  }
  const back = moves.find((m) => m.from === active)?.to ?? active;
  if (back && editor.tabs.some((t) => t.path === back)) {
    await openFile(back);
    // The editor that had the focus may be gone (its file moved).
    (await waitForView(back))?.focus();
  }
}

function refreshTree(edit: WorkspaceEdit): void {
  const dirs = new Set<string>();
  for (const c of edit.changes) {
    if (c.kind === "rename") {
      dirs.add(parentOf(c.from));
      dirs.add(parentOf(c.to));
    } else if (c.kind !== "edit") dirs.add(parentOf(c.path));
  }
  refreshDirs([...dirs]);
}

export interface ApplyOptions {
  /** "Rename", "Import class" (undo labels, messages). */
  label: string;
  /** Runs before anything changes (the in-place rename reverts its live edits). */
  beforeEditors?: () => void;
  /** Files and folders to move or copy first (the edit refers to their new paths). */
  fileOps?: FileOp[];
}

const under = (path: string, dir: string) => path === dir || path.startsWith(`${dir}/`);

/** Run the file operations (or undo them, in reverse order); returns the tabs that moved. */
async function runFileOps(ops: FileOp[], reverse: boolean): Promise<FileRename[]> {
  const moved: FileRename[] = [];
  const list = reverse ? [...ops].reverse() : ops;
  for (const op of list) {
    if (op.kind === "copy") {
      if (reverse) await workspaceApi.delete(op.to);
      else await workspaceApi.copy(op.from, op.to);
      continue;
    }
    const [from, to] = reverse ? [op.to, op.from] : [op.from, op.to];
    const tabs = editor.tabs.filter((t) => under(t.path, from));
    // Unsaved edits go with the file.
    for (const t of tabs.filter(isDirty)) await saveFile(t.path);
    await workspaceApi.rename(from, to);
    for (const t of tabs) moved.push({ from: t.path, to: to + t.path.slice(from.length) });
  }
  if (moved.length) for (const m of moved) closeTab(m.from);
  refreshDirs([...new Set(ops.flatMap((o) => [parentOf(o.from), parentOf(o.to)]))]);
  return moved;
}

export interface ApplyResult {
  id: number;
  fileCount: number;
}

/**
 * Apply `edit` across the project as one undoable refactoring. Throws (and changes
 * nothing) when the backend cannot apply its part, e.g. a file changed in the meantime.
 */
export async function applyWorkspaceEdit(edit: WorkspaceEdit, opts: ApplyOptions): Promise<ApplyResult> {
  const fileOps = opts.fileOps ?? [];
  const movedTabs = fileOps.length ? await runFileOps(fileOps, false) : [];
  const plan = await codeApi.planEdit(edit, openEditors());
  // Before saving: a file the backend rewrites must be saved as the edit expects it.
  opts.beforeEditors?.();
  for (const p of plan.saveFirst) {
    const tab = editor.tabs.find((t) => t.path === p);
    if (tab && isDirty(tab)) await saveFile(p);
  }
  const hasBackend = plan.backend.changes.length > 0;
  let applied: AppliedFile[] = [];
  try {
    applied = hasBackend ? await codeApi.applyEdit(plan.backend) : [];
  } catch (e) {
    // Put moved files back, so nothing changed.
    if (fileOps.length) await runFileOps(fileOps, true).catch(() => undefined);
    await moveTabs(movedTabs.map((m) => ({ from: m.to, to: m.from })));
    throw e;
  }
  const id = ++seq;
  const r: Refactoring = {
    id,
    label: opts.label,
    editors: [],
    backend: plan.backend,
    applied,
    reopen: plan.reopen,
    close: plan.close,
    fileOps,
    movedTabs,
    undone: false,
  };
  refactorings.set(id, r);
  for (const e of plan.editors) {
    const view = viewFor(e.path);
    if (!view) continue;
    const doc = view.state.doc;
    const before = doc;
    view.dispatch({
      changes: e.edits.map((te) => ({ from: fromLsp(doc, te.range.start), to: fromLsp(doc, te.range.end), insert: te.newText })),
      effects: refactoringStep.of({ id, undo: false }),
      annotations: isolateHistory.of("full"),
      userEvent: "refactor",
    });
    r.editors.push({ path: e.path, before, after: view.state.doc });
  }
  for (const p of plan.close) closeTab(p);
  await moveTabs(plan.reopen);
  for (const m of movedTabs) await openFile(m.to).catch(() => undefined);
  // Reopened files start a new history: give them the refactoring's entry, so Ctrl+Z there
  // undoes the refactoring too.
  for (const m of [...plan.reopen, ...movedTabs]) {
    const view = await waitForView(m.to);
    if (!view) continue;
    view.dispatch({ effects: refactoringStep.of({ id, undo: false }), annotations: isolateHistory.of("full") });
    r.editors.push({ path: m.to, before: view.state.doc, after: view.state.doc });
  }
  if (hasBackend) refreshTree(plan.backend);
  return { id, fileCount: plan.fileCount };
}

/**
 * Give an editor opened after a refactoring (Create method in a file that was closed) the
 * refactoring's history entry, so Ctrl+Z there undoes it too.
 */
export async function attachToRefactoring(id: number, path: string): Promise<void> {
  const r = refactorings.get(id);
  if (!r || r.editors.some((e) => e.path === path)) return;
  const view = await waitForView(path);
  if (!view) return;
  view.dispatch({ effects: refactoringStep.of({ id, undo: false }), annotations: isolateHistory.of("full") });
  r.editors.push({ path, before: view.state.doc, after: view.state.doc });
}

function driveOthers(r: Refactoring, source: EditorView, dir: "undo" | "redo"): string[] {
  const skipped: string[] = [];
  driving = true;
  try {
    for (const part of r.editors) {
      const view = viewFor(part.path);
      if (!view || view === source) continue;
      const expected = dir === "undo" ? part.after : part.before;
      if (!view.state.doc.eq(expected)) {
        skipped.push(part.path);
        continue;
      }
      (dir === "undo" ? undo : redo)(view);
    }
  } finally {
    driving = false;
  }
  return skipped;
}

async function undoRefactoring(id: number, source: EditorView): Promise<void> {
  const r = refactorings.get(id);
  if (!r || r.undone) return;
  r.undone = true;
  const skipped = driveOthers(r, source, "undo");
  if (r.applied.length) {
    try {
      await codeApi.revertEdit(r.applied);
    } catch (e) {
      skipped.push(String(e instanceof Error ? e.message : e));
    }
  }
  await moveTabs(r.reopen.map((m) => ({ from: m.to, to: m.from })));
  for (const p of r.close) await openFile(p).catch(() => undefined);
  if (r.fileOps.length) {
    try {
      await runFileOps(r.fileOps, true);
      for (const m of r.movedTabs) await openFile(m.from).catch(() => undefined);
    } catch (e) {
      skipped.push(String(e instanceof Error ? e.message : e));
    }
  }
  if (r.applied.length) refreshTree(r.backend);
  if (skipped.length) showBalloon(`${r.label} was undone only partly`, `Changed since: ${skipped.join(", ")}`, 6000);
}

async function redoRefactoring(id: number, source: EditorView): Promise<void> {
  const r = refactorings.get(id);
  if (!r || !r.undone) return;
  r.undone = false;
  const skipped = driveOthers(r, source, "redo");
  if (r.fileOps.length) {
    try {
      await runFileOps(r.fileOps, false);
      for (const m of r.movedTabs) await openFile(m.to).catch(() => undefined);
    } catch (e) {
      skipped.push(String(e instanceof Error ? e.message : e));
    }
  }
  if (r.backend.changes.length) {
    try {
      r.applied = await codeApi.applyEdit(r.backend);
    } catch (e) {
      skipped.push(String(e instanceof Error ? e.message : e));
    }
  }
  for (const p of r.close) closeTab(p);
  await moveTabs(r.reopen);
  if (r.backend.changes.length) refreshTree(r.backend);
  if (skipped.length) showBalloon(`${r.label} was redone only partly`, `Changed since: ${skipped.join(", ")}`, 6000);
}

/** Editor extension: refactoring entries in the undo history of every file editor. */
export function refactoringHistory(): Extension {
  return [
    invertedEffects.of((tr) =>
      tr.effects.filter((e) => e.is(refactoringStep)).map((e) => refactoringStep.of({ id: e.value.id, undo: !e.value.undo })),
    ),
    EditorView.updateListener.of((u) => {
      if (driving) return;
      for (const tr of u.transactions) {
        if (!tr.isUserEvent("undo") && !tr.isUserEvent("redo")) continue;
        for (const e of tr.effects) {
          if (!e.is(refactoringStep)) continue;
          // The undo entry carries `undo: true`; redoing it carries `undo: false` again.
          void (e.value.undo ? undoRefactoring(e.value.id, u.view) : redoRefactoring(e.value.id, u.view));
        }
      }
    }),
  ];
}

/** Edits a language server applies itself (while running a code action's command). */
export function listenForServerEdits(): void {
  void listen<{ label?: string | null; edit: WorkspaceEdit }>("lsp://apply-edit", (e) => {
    applyWorkspaceEdit(e.edit, { label: e.label ?? "Edit" }).catch((err) =>
      showBalloon(`${e.label ?? "Edit"} failed`, String(err instanceof Error ? err.message : err)),
    );
  });
}

/** For tests: the refactorings applied in this session. */
export const appliedRefactorings = () => [...refactorings.values()];
