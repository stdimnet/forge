/**
 * Move (F6), Copy (F5), Safe Delete (Alt+Delete), Change Signature (Ctrl+F6) and Make
 * Static like PHPStorm. Rust finds the usages and computes every edit (`refactor_move_*`,
 * `refactor_safe_delete_*`, `refactor_change_signature*`, `refactor_make_static*`); this
 * module asks the questions in PHPStorm's order and applies the answer as one undoable
 * refactoring (`refactoring.ts`, file moves and copies included).
 *
 * In the editor the PHP element at the caret is refactored (a class, static members, a
 * method, ...); other files and the Project view work on files and folders.
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { isLibraryPath } from "@/ipc/codeintel";
import {
  moveApi,
  type ChangeSignatureOptions,
  type ClassTarget,
  type FunctionTarget,
  type MakeStaticInfo,
  type MakeStaticOptions,
  type MembersTarget,
  type RefactorAnswer,
  type SafeDeleteAnswer,
  type SignatureInfo,
} from "@/ipc/refactor";
import { templatesApi } from "@/ipc/templates";
import { workspaceApi, type FileEntry } from "@/ipc/workspace";
import { closeTab, editor, openFile } from "@/state/editor";
import { deletePath } from "@/state/pathOps";
import { refreshDirs } from "@/features/project/ProjectView";
import { showBalloon } from "@/keymap";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";
import { applyAnswer, dirtyBuffers } from "./extract";
import { renameSessionOf } from "./rename";
import { openInFindWindow } from "./usages";
import { type FileOp } from "./refactoring";
import { t as tr } from "@/i18n";

export const isPhp = (path: string) => /\.(php|phtml|inc)$/i.test(path) && !/\.blade\.php$/i.test(path);
const message = (e: unknown) => String(e instanceof Error ? e.message : e);
export const parentOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
export const baseName = (p: string) => p.split("/").pop() ?? p;
const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);

// ---------- Dialog requests ----------

interface Request<T> {
  done: (value: T | null) => void;
}

export interface MoveClassRequest extends Request<{ namespace: string; directory: string; searchInComments: boolean; preview: boolean }> {
  target: ClassTarget;
}
export interface CopyClassRequest extends Request<{ name: string; namespace: string; directory: string }> {
  target: ClassTarget;
}
export interface MoveMembersRequest extends Request<{ members: string[]; target: string; preview: boolean }> {
  target: MembersTarget;
}
export interface MoveFunctionRequest extends Request<{ file: string; preview: boolean }> {
  target: FunctionTarget;
}
export interface MoveFilesRequest extends Request<{ directory: string; searchForReferences: boolean; searchInComments: boolean }> {
  /** "file 'User.php'", "directory 'Models'", "2 items". */
  what: string;
  directory: string;
  php: boolean;
}
export interface CopyFilesRequest extends Request<{ name: string; directory: string }> {
  what: string;
  name: string;
  directory: string;
}
export interface SafeDeleteRequest extends Request<{ searchInComments: boolean }> {
  /** `class "User"`, `file "User.php"`. */
  what: string;
}
export interface UsagesDetectedRequest extends Request<"view" | "delete"> {
  message: string;
}
export interface ChangeSignatureRequest extends Request<ChangeSignatureOptions & { preview: boolean }> {
  info: SignatureInfo;
}
export interface MakeStaticRequest extends Request<MakeStaticOptions & { preview: boolean }> {
  info: MakeStaticInfo;
}

export type MoveDialog =
  | { kind: "moveClass"; req: MoveClassRequest }
  | { kind: "copyClass"; req: CopyClassRequest }
  | { kind: "moveMembers"; req: MoveMembersRequest }
  | { kind: "moveFunction"; req: MoveFunctionRequest }
  | { kind: "moveFiles"; req: MoveFilesRequest }
  | { kind: "copyFiles"; req: CopyFilesRequest }
  | { kind: "safeDelete"; req: SafeDeleteRequest }
  | { kind: "usagesDetected"; req: UsagesDetectedRequest }
  | { kind: "changeSignature"; req: ChangeSignatureRequest }
  | { kind: "makeStatic"; req: MakeStaticRequest };

const [moveDialog, setMoveDialog] = createSignal<MoveDialog | null>(null);
export { moveDialog };

type ReqOf<K extends MoveDialog["kind"]> = Extract<MoveDialog, { kind: K }>["req"];
type ValueOf<R> = R extends Request<infer T> ? T : never;

/** Show a dialog; resolves to its result, `null` when cancelled. */
export function ask<K extends MoveDialog["kind"]>(kind: K, req: Omit<ReqOf<K>, "done">): Promise<ValueOf<ReqOf<K>> | null> {
  return new Promise((resolve) => {
    const full = {
      ...req,
      done: (v: ValueOf<ReqOf<K>> | null) => {
        setMoveDialog(null);
        resolve(v);
      },
    };
    setMoveDialog({ kind, req: full } as unknown as MoveDialog);
  });
}

// ---------- Helpers ----------

function fail(view: EditorView | null, title: string, e: unknown): void {
  if (view) showHint(view, view.state.selection.main.head, message(e));
  else showBalloon(`${title} failed`, message(e), 6000);
}

function blocked(path: string, view: EditorView): boolean {
  if (isLibraryPath(path) || view.state.readOnly) {
    showHint(view, view.state.selection.main.head, "Cannot perform refactoring in a read-only file");
    return true;
  }
  return !!renameSessionOf(view);
}

const caret = (view: EditorView) => toLsp(view.state.doc, view.state.selection.main.head);

/** The editor's text when `path` is the editor's file (the backend reads other files). */
const contentFor = (path: string, editorPath: string, view: EditorView | null) =>
  view && path === editorPath ? view.state.doc.toString() : undefined;

/** The PSR-4 folder of a namespace (`null` without a rule). */
export async function directoryFor(namespace: string): Promise<string | null> {
  return templatesApi.phpDirectory(namespace).catch(() => null);
}

/** The PSR-4 namespace of a folder. */
export async function namespaceFor(dir: string): Promise<string | null> {
  return templatesApi.phpNamespace(dir).catch(() => null);
}

async function entryOf(path: string): Promise<FileEntry | null> {
  return workspaceApi
    .listDir(parentOf(path))
    .then((entries) => entries.find((e) => e.path === path) ?? null)
    .catch(() => null);
}

/** The class of a PHP file that declares exactly one (Move / Copy Class from the Project view). */
async function soleClassOf(path: string): Promise<ClassTarget | null> {
  if (!isPhp(path)) return null;
  const content = await workspaceApi.readFile(path).catch(() => null);
  if (content == null) return null;
  const found = await moveApi.prepare(path, { line: 0, character: 0 }, content, true).catch(() => null);
  return found?.class?.sole ? found.class : null;
}

// ---------- Move ----------

/** Move (F6) in the editor: static members, a class, or the file of a non-PHP editor. */
export async function moveAt(path: string, view: EditorView): Promise<void> {
  if (blocked(path, view)) return;
  if (!isPhp(path)) {
    await moveEntries([path]);
    return;
  }
  await syncNow(path);
  let found;
  try {
    found = await moveApi.prepare(path, caret(view), view.state.doc.toString(), false, dirtyBuffers());
  } catch (e) {
    return fail(view, "Move", e);
  }
  if (found.members) await moveMembers(found.members, path, view);
  else if (found.function) await moveFunction(found.function, path, view);
  else if (found.class) await moveClass(found.class, path, view);
}

async function moveClass(target: ClassTarget, editorPath: string | null, view: EditorView | null): Promise<void> {
  const choice = await ask("moveClass", { target });
  view?.focus();
  if (!choice) return;
  let answer: RefactorAnswer;
  try {
    answer = await moveApi.moveClass(
      target,
      { ...choice, content: view && editorPath ? contentFor(target.path, editorPath, view) : undefined },
      dirtyBuffers(),
    );
  } catch (e) {
    return fail(view, "Move", e);
  }
  await applyAnswer(tr("codeintel.moveRefactorings.moveClassName", { name: target.name }), answer, { preview: choice.preview });
}

async function moveMembers(target: MembersTarget, editorPath: string, view: EditorView): Promise<void> {
  if (!target.members.length) {
    showHint(view, view.state.selection.main.head, tr("codeintel.moveRefactorings.classClassnameHasNoStatic", { className: target.className }));
    return;
  }
  const choice = await ask("moveMembers", { target });
  view.focus();
  if (!choice) return;
  let answer: RefactorAnswer;
  try {
    answer = await moveApi.moveMembers(target, choice.members, choice.target, contentFor(target.path, editorPath, view), dirtyBuffers());
  } catch (e) {
    return fail(view, "Move Members", e);
  }
  await applyAnswer("Move Members", answer, { preview: choice.preview });
}

async function moveFunction(target: FunctionTarget, editorPath: string, view: EditorView): Promise<void> {
  const choice = await ask("moveFunction", { target });
  view.focus();
  if (!choice) return;
  let answer: RefactorAnswer;
  try {
    answer = await moveApi.moveFunction(target, choice.file, contentFor(target.path, editorPath, view), dirtyBuffers());
  } catch (e) {
    return fail(view, "Move", e);
  }
  await applyAnswer(tr("codeintel.moveRefactorings.moveFunctionName", { name: target.name }), answer, { preview: choice.preview });
}

/** "file 'User.php'", "directory 'Models'", "3 items". */
export function describeEntries(entries: { path: string; kind: "file" | "dir" }[]): string {
  if (entries.length !== 1) return tr("codeintel.entries.items", { count: entries.length });
  const e = entries[0];
  return tr("codeintel.entries.one", { kind: e.kind, name: baseName(e.path) });
}

/**
 * Move files and folders (Project view F6, drag and drop): a PHP class file uses Move
 * Class; otherwise the Move dialog with the target folder. PHP files that follow PSR-4 get
 * the namespace of their new folder.
 */
export async function moveEntries(paths: string[], targetDir?: string): Promise<void> {
  const entries = (await Promise.all(paths.map(entryOf))).filter((e): e is FileEntry => !!e);
  if (!entries.length) return;
  if (entries.length === 1 && entries[0].kind === "file" && targetDir === undefined) {
    const target = await soleClassOf(entries[0].path);
    if (target) {
      await moveClass(target, null, null);
      return;
    }
  }
  const choice = await ask("moveFiles", {
    what: describeEntries(entries),
    directory: targetDir ?? parentOf(entries[0].path),
    php: entries.some((e) => e.kind === "dir" || isPhp(e.path)),
  });
  if (!choice) return;
  const dir = choice.directory.replace(/^\/+|\/+$/g, "");
  const moves = entries.map((e) => ({ from: e.path, to: join(dir, e.name) })).filter((m) => m.from !== m.to);
  if (!moves.length) return;
  for (const m of moves) {
    if (await entryOf(m.to)) {
      showBalloon(tr("codeintel.moveRefactorings.moveFailed"), tr("tools.codeintel.alreadyExists", { to: m.to }), 6000);
      return;
    }
  }
  try {
    const answer = await moveApi.moveFiles(moves, choice.searchForReferences, choice.searchInComments, dirtyBuffers());
    const fileOps: FileOp[] = moves.map((m) => ({ kind: "move", ...m }));
    await applyAnswer(tr("codeintel.moveRefactorings.moveDescribeentries", { describeEntries: describeEntries(entries) }), answer.answer, { fileOps });
  } catch (e) {
    fail(null, "Move", e);
  }
}

// ---------- Copy ----------

/** Copy (F5) in the editor: Copy Class on a PHP class, else the editor's file. */
export async function copyAt(path: string, view: EditorView): Promise<void> {
  if (isLibraryPath(path) || renameSessionOf(view)) return;
  if (!isPhp(path)) {
    await copyEntries([path]);
    return;
  }
  await syncNow(path);
  let found;
  try {
    found = await moveApi.prepare(path, caret(view), view.state.doc.toString(), true, dirtyBuffers());
  } catch (e) {
    return fail(view, "Copy", e);
  }
  if (found.class) await copyClass(found.class, path, view);
}

async function copyClass(target: ClassTarget, editorPath: string | null, view: EditorView | null): Promise<void> {
  const choice = await ask("copyClass", { target });
  view?.focus();
  if (!choice) return;
  let answer: RefactorAnswer;
  try {
    answer = await moveApi.copyClass(
      target,
      { ...choice, content: view && editorPath ? contentFor(target.path, editorPath, view) : undefined },
      dirtyBuffers(),
    );
  } catch (e) {
    return fail(view, "Copy", e);
  }
  if (await applyAnswer(tr("codeintel.moveRefactorings.copyClassName", { name: target.name }), answer)) {
    const created = answer.edit.changes.find((c) => c.kind === "create");
    if (created && created.kind === "create") await openFile(created.path).catch(() => undefined);
  }
}

/** Copy files and folders (Project view F5): a PHP class file uses Copy Class. */
export async function copyEntries(paths: string[], targetDir?: string): Promise<void> {
  const entries = (await Promise.all(paths.map(entryOf))).filter((e): e is FileEntry => !!e);
  if (entries.length !== 1) return;
  const entry = entries[0];
  if (entry.kind === "file" && targetDir === undefined) {
    const target = await soleClassOf(entry.path);
    if (target) {
      await copyClass(target, null, null);
      return;
    }
  }
  const choice = await ask("copyFiles", {
    what: describeEntries(entries),
    name: entry.name,
    directory: targetDir ?? parentOf(entry.path),
  });
  if (!choice) return;
  const to = join(choice.directory.replace(/^\/+|\/+$/g, ""), choice.name.trim());
  if (await entryOf(to)) {
    showBalloon(tr("codeintel.moveRefactorings.copyFailed"), tr("tools.codeintel.alreadyExists", { to }), 6000);
    return;
  }
  const empty: RefactorAnswer = { edit: { changes: [] }, preview: [], conflicts: [], nameRanges: [], skipped: 0 };
  if (await applyAnswer(tr("codeintel.moveRefactorings.copyDescribeentries", { describeEntries: describeEntries(entries) }), empty, { fileOps: [{ kind: "copy", from: entry.path, to }] })) {
    if (entry.kind === "file") await openFile(to).catch(() => undefined);
  }
}

// ---------- Safe Delete ----------

/**
 * Usages Detected: View Usages (the Find tool window, with Delete Anyway below the usages)
 * / Delete Anyway / Cancel. `true` to delete now.
 */
async function confirmUsages(answer: SafeDeleteAnswer, deleteAnyway: () => Promise<void>): Promise<boolean> {
  if (!answer.count) return true;
  const choice = await ask("usagesDetected", { message: answer.message });
  if (choice === "view") {
    openInFindWindow(answer.usages, {
      kind: "usages",
      title: tr("codeintel.moveRefactorings.safeDeleteName", { name: answer.name }),
      header: tr("codeintel.moveRefactorings.usagesOfKindNameThat", { kind: answer.kind, name: answer.name }),
      newTab: true,
      actions: [{ label: tr("codeintel.moveRefactorings.deleteAnyway"), primary: true, run: () => deleteAnyway() }],
    });
    return false;
  }
  return choice === "delete";
}

/** Safe Delete (Alt+Delete) of the element at the caret (a non-PHP editor: its file). */
export async function safeDeleteAt(path: string, view: EditorView): Promise<void> {
  if (blocked(path, view)) return;
  if (!isPhp(path)) {
    await safeDeleteEntries([path]);
    return;
  }
  await syncNow(path);
  const content = view.state.doc.toString();
  let answer: SafeDeleteAnswer;
  try {
    // What is at the caret first, so the dialog can name it.
    answer = await moveApi.safeDelete(path, caret(view), content, false, dirtyBuffers());
  } catch (e) {
    return fail(view, "Safe Delete", e);
  }
  const choice = await ask("safeDelete", { what: `${answer.kind} "${answer.name}"` });
  view.focus();
  if (!choice) return;
  if (choice.searchInComments) {
    try {
      answer = await moveApi.safeDelete(path, caret(view), content, true, dirtyBuffers());
    } catch (e) {
      return fail(view, "Safe Delete", e);
    }
  }
  const run = async () => void (await applyAnswer(tr("codeintel.moveRefactorings.safeDeleteName", { name: answer.name }), answer.answer));
  if (await confirmUsages(answer, run)) await run();
}

/** Safe Delete of files and folders (Project view Alt+Delete). */
export async function safeDeleteEntries(paths: string[]): Promise<void> {
  const entries = (await Promise.all(paths.map(entryOf))).filter((e): e is FileEntry => !!e);
  if (!entries.length) return;
  const what =
    entries.length === 1 ? `${entries[0].kind === "dir" ? "directory" : "file"} "${entries[0].name}"` : `${entries.length} items`;
  const choice = await ask("safeDelete", { what });
  if (!choice) return;
  let answer: SafeDeleteAnswer;
  try {
    answer = await moveApi.safeDeleteFiles(
      entries.map((e) => e.path),
      choice.searchInComments,
      dirtyBuffers(),
    );
  } catch (e) {
    return fail(null, "Safe Delete", e);
  }
  const run = () => deleteFiles(answer);
  if (await confirmUsages(answer, run)) await run();
}

async function deleteFiles(answer: SafeDeleteAnswer): Promise<void> {
  if (answer.answer.edit.changes.length) await applyAnswer(tr("codeintel.moveRefactorings.safeDeleteName", { name: answer.name }), answer.answer);
  for (const p of answer.paths) {
    try {
      await deletePath(p);
    } catch (e) {
      showBalloon(tr("codeintel.moveRefactorings.safeDeleteFailed"), message(e), 6000);
    }
  }
  if (!answer.paths.length) return;
  for (const t of editor.tabs.filter((t) => answer.paths.some((p) => t.path === p || t.path.startsWith(`${p}/`)))) closeTab(t.path);
  refreshDirs([...new Set(answer.paths.map(parentOf))]);
}

// ---------- Change Signature ----------

export async function changeSignatureAt(path: string, view: EditorView): Promise<void> {
  if (blocked(path, view)) return;
  if (!isPhp(path)) {
    showHint(view, view.state.selection.main.head, "Change Signature is available for PHP methods and functions");
    return;
  }
  await syncNow(path);
  let info: SignatureInfo;
  try {
    info = await moveApi.signature(path, caret(view), view.state.doc.toString(), dirtyBuffers());
  } catch (e) {
    return fail(view, "Change Signature", e);
  }
  const choice = await ask("changeSignature", { info });
  view.focus();
  if (!choice) return;
  const { preview, ...options } = choice;
  let answer: RefactorAnswer;
  try {
    answer = await moveApi.changeSignature(info, options, contentFor(info.path, path, view), dirtyBuffers());
  } catch (e) {
    return fail(view, "Change Signature", e);
  }
  await applyAnswer("Change Signature", answer, { preview });
}

// ---------- Make Static ----------

export async function makeStaticAt(path: string, view: EditorView): Promise<void> {
  if (blocked(path, view)) return;
  if (!isPhp(path)) {
    showHint(view, view.state.selection.main.head, "Make Static is available for PHP methods");
    return;
  }
  await syncNow(path);
  let info: MakeStaticInfo;
  try {
    info = await moveApi.makeStaticInfo(path, caret(view), view.state.doc.toString(), dirtyBuffers());
  } catch (e) {
    return fail(view, "Make Static", e);
  }
  const choice = await ask("makeStatic", { info });
  view.focus();
  if (!choice) return;
  const { preview, ...options } = choice;
  let answer: RefactorAnswer;
  try {
    answer = await moveApi.makeStatic(info, options, contentFor(info.path, path, view), dirtyBuffers());
  } catch (e) {
    return fail(view, "Make Static", e);
  }
  await applyAnswer(tr("codeintel.moveRefactorings.makeStaticName", { name: info.name }), answer, { preview });
}

// ---------- Change Signature model (pure, tested) ----------

export interface ParamRow {
  original: number | null;
  name: string;
  type: string;
  defaultValue: string;
  callValue: string;
  byRef: boolean;
  variadic: boolean;
}

export function rowsOf(info: SignatureInfo): ParamRow[] {
  return info.parameters.map((p, i) => ({
    original: i,
    name: p.name,
    type: p.typeHint ?? "",
    defaultValue: p.defaultValue ?? "",
    callValue: "",
    byRef: !!p.byRef,
    variadic: !!p.variadic,
  }));
}

/** The dialog's preview: `public function name(int $a = 1): void`. */
export function signaturePreview(info: SignatureInfo, name: string, visibility: string | null, returnType: string, rows: ParamRow[]): string {
  const params = rows
    .map((r) => {
      const old = r.original != null ? info.parameters[r.original] : undefined;
      let s = "";
      if (old?.promotion) s += `${old.promotion} `;
      if (r.type.trim()) s += `${r.type.trim()} `;
      if (r.byRef) s += "&";
      if (r.variadic) s += "...";
      s += `$${r.name.trim().replace(/^\$/, "")}`;
      if (r.defaultValue.trim() && !r.variadic) s += ` = ${r.defaultValue.trim()}`;
      return s;
    })
    .join(", ");
  const head = [info.isAbstract ? "abstract" : "", info.kind === "method" ? (visibility ?? "public") : "", info.isStatic ? "static" : ""]
    .filter(Boolean)
    .join(" ");
  const ret = returnType.trim() ? `: ${returnType.trim()}` : "";
  return `${head ? `${head} ` : ""}function ${name.trim() || "…"}(${params})${ret}`;
}

export function optionsOf(name: string, visibility: string | null, returnType: string, rows: ParamRow[]): ChangeSignatureOptions {
  return {
    name: name.trim(),
    visibility: (visibility as ChangeSignatureOptions["visibility"]) ?? null,
    returnType: returnType.trim(),
    parameters: rows.map((r) => ({
      original: r.original,
      name: r.name.trim().replace(/^\$/, ""),
      typeHint: r.type.trim() || null,
      defaultValue: r.defaultValue.trim() || null,
      callValue: r.original == null ? r.callValue.trim() || null : null,
      byRef: r.byRef,
      variadic: r.variadic,
    })),
  };
}

const IDENT = /^[A-Za-z_\u0080-￿][\w\u0080-￿]*$/;

/** The dialog's validation message, or `null`. */
export function validateSignature(name: string, rows: ParamRow[]): string | null {
  if (!IDENT.test(name.trim())) return `'${name.trim()}' is not a valid identifier`;
  const seen = new Set<string>();
  for (const [i, r] of rows.entries()) {
    const n = r.name.trim().replace(/^\$/, "");
    if (!IDENT.test(n)) return `'${n}' is not a valid parameter name`;
    if (seen.has(n)) return `Duplicate parameter name: $${n}`;
    seen.add(n);
    if (r.variadic && i !== rows.length - 1) return "Only the last parameter can be variadic";
    if (r.original == null && !r.callValue.trim() && !r.defaultValue.trim() && !r.variadic)
      return `Specify the default value for call sites of the new parameter $${n}`;
  }
  return null;
}

/** Test hook: the open tabs' paths. */
export const openTabPaths = () => editor.tabs.map((t) => t.path);
