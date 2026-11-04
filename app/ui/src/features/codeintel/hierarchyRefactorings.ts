/**
 * Extract Interface, Extract Superclass, Pull Members Up and Push Members Down like PHPStorm.
 * Rust finds the class, its members (with what each of them uses), the hierarchy and every
 * edit, the new file included (`refactor_hierarchy_prepare`, `refactor_extract_*`,
 * `refactor_pull_up`, `refactor_push_down`); this module shows the dialogs
 * (`HierarchyDialogs.tsx`) and applies the answer as one undoable refactoring. Problems
 * ("uses a member that is not moved", visibility) go through Problems Detected.
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { isLibraryPath } from "@/ipc/codeintel";
import {
  hierarchyApi,
  type ClassMember,
  type ExtractInterfaceOptions,
  type ExtractSuperclassOptions,
  type HierarchyInfo,
  type HierarchyRefactoring,
  type PullUpOptions,
  type PushDownOptions,
  type RefactorAnswer,
} from "@/ipc/refactor";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";
import { applyAnswer, dirtyBuffers } from "./extract";
import { renameSessionOf } from "./rename";
import { t } from "@/i18n";

export const isPhp = (path: string) => /\.(php|phtml|inc)$/i.test(path) && !/\.blade\.php$/i.test(path);
const message = (e: unknown) => String(e instanceof Error ? e.message : e);

// ---------- Dialog requests ----------

interface Request<T> {
  info: HierarchyInfo;
  done: (value: T | null) => void;
}

export type ExtractInterfaceRequest = Request<ExtractInterfaceOptions & { preview: boolean }>;
export type ExtractSuperclassRequest = Request<ExtractSuperclassOptions & { preview: boolean }>;
export type PullUpRequest = Request<PullUpOptions & { preview: boolean }>;
export type PushDownRequest = Request<PushDownOptions & { preview: boolean }>;

export type HierarchyDialog =
  | { kind: "extractInterface"; req: ExtractInterfaceRequest }
  | { kind: "extractSuperclass"; req: ExtractSuperclassRequest }
  | { kind: "pullUp"; req: PullUpRequest }
  | { kind: "pushDown"; req: PushDownRequest };

const [hierarchyDialog, setHierarchyDialog] = createSignal<HierarchyDialog | null>(null);
export { hierarchyDialog };

type ReqOf<K extends HierarchyDialog["kind"]> = Extract<HierarchyDialog, { kind: K }>["req"];
type ValueOf<R> = R extends Request<infer T> ? T : never;

function ask<K extends HierarchyDialog["kind"]>(kind: K, info: HierarchyInfo): Promise<ValueOf<ReqOf<K>> | null> {
  return new Promise((resolve) => {
    const req = {
      info,
      done: (v: ValueOf<ReqOf<K>> | null) => {
        setHierarchyDialog(null);
        resolve(v);
      },
    };
    setHierarchyDialog({ kind, req } as unknown as HierarchyDialog);
  });
}

// ---------- The dialogs' model (pure, tested) ----------

/** What the member table of a dialog offers. */
export type TableMode = "interface" | "superclass" | "pullUpClass" | "pullUpInterface" | "pushDown";

/** The member can be chosen (Extract Interface: public methods and constants; interfaces have no properties). */
export function canChoose(m: ClassMember, mode: TableMode): boolean {
  switch (mode) {
    case "interface":
      return m.visibility === "public" && (m.kind === "constant" || (m.kind === "method" && !m.constructor));
    case "pullUpInterface":
      return m.kind === "constant" ? m.visibility === "public" : m.kind === "method" && m.visibility === "public" && !m.constructor;
    default:
      return true;
  }
}

/** The "Make abstract" / "Keep abstract" box of a member: `null` when it has none. */
export function abstractBox(m: ClassMember, mode: TableMode, sourceKind: string): { enabled: boolean; implied: boolean } | null {
  if (m.kind !== "method") return null;
  switch (mode) {
    case "superclass":
    case "pullUpClass":
      return m.constructor ? null : { enabled: true, implied: false };
    // Interfaces only declare methods.
    case "pullUpInterface":
      return { enabled: false, implied: true };
    case "pushDown":
      return sourceKind === "interface" || m.isAbstract || m.constructor ? null : { enabled: true, implied: false };
    default:
      return null;
  }
}

/** Members that are not chosen but used by a chosen one (PHPStorm highlights them). */
export function dependencies(members: ClassMember[], chosen: Set<string>): Set<string> {
  const out = new Set<string>();
  for (const m of members) if (chosen.has(m.id)) for (const u of m.uses) if (!chosen.has(u)) out.add(u);
  return out;
}

/** The members checked when the dialog opens: the member at the caret. */
export function initialChoice(info: HierarchyInfo, mode: TableMode): Set<string> {
  const at = info.members.find((m) => m.id === info.selected);
  return new Set(at && canChoose(at, mode) ? [at.id] : []);
}

/** PHPStorm-style suggestions for the new type's name. */
export function suggestedName(info: HierarchyInfo, kind: "interface" | "superclass"): string {
  return kind === "interface" ? `${info.name}Interface` : `Abstract${info.name}`;
}

const NAME = /^[A-Za-z_\u0080-￿][\w\u0080-￿]*$/;

/** The dialog's validation message for a new type name, or `null`. */
export function validateTypeName(name: string, info: HierarchyInfo): string | null {
  const n = name.trim();
  if (!NAME.test(n)) return n ? `'${n}' is not a valid class name` : t("codeintel.hierarchyRefactorings.specifyTheName");
  if (n.toLowerCase() === info.name.toLowerCase()) return t("codeintel.hierarchyRefactorings.theNameShouldDifferFrom", { name: info.name });
  return null;
}

// ---------- Running ----------

const caret = (view: EditorView) => toLsp(view.state.doc, view.state.selection.main.head);
const contentFor = (path: string, editorPath: string, view: EditorView) => (path === editorPath ? view.state.doc.toString() : undefined);

const TITLES: Record<HierarchyRefactoring, string> = {
  extractInterface: "Extract Interface",
  extractSuperclass: "Extract Superclass",
  pullUp: "Pull Members Up",
  pushDown: "Push Members Down",
};

async function prepare(kind: HierarchyRefactoring, path: string, view: EditorView): Promise<HierarchyInfo | null> {
  const pos = view.state.selection.main.head;
  if (isLibraryPath(path) || view.state.readOnly) {
    showHint(view, pos, "Cannot perform refactoring in a read-only file");
    return null;
  }
  if (renameSessionOf(view)) return null;
  if (!isPhp(path)) {
    showHint(view, pos, `${TITLES[kind]} is available for PHP classes`);
    return null;
  }
  await syncNow(path);
  try {
    return await hierarchyApi.prepare(path, caret(view), view.state.doc.toString(), kind, dirtyBuffers());
  } catch (e) {
    showHint(view, pos, message(e));
    return null;
  }
}

async function run(title: string, view: EditorView, call: () => Promise<RefactorAnswer>, preview: boolean): Promise<void> {
  let answer: RefactorAnswer;
  try {
    answer = await call();
  } catch (e) {
    showHint(view, view.state.selection.main.head, message(e));
    return;
  }
  await applyAnswer(title, answer, { preview });
}

/** Extract Interface (Refactor | Extract/Introduce | Interface…). */
export async function extractInterfaceAt(path: string, view: EditorView): Promise<void> {
  const info = await prepare("extractInterface", path, view);
  if (!info) return;
  const choice = await ask("extractInterface", info);
  view.focus();
  if (!choice) return;
  const { preview, ...options } = choice;
  await run(
    t("codeintel.hierarchyRefactorings.extractInterfaceName", { name: options.name }),
    view,
    () => hierarchyApi.extractInterface(info, options, contentFor(info.path, path, view), dirtyBuffers()),
    preview,
  );
}

/** Extract Superclass (Refactor | Extract/Introduce | Superclass…). */
export async function extractSuperclassAt(path: string, view: EditorView): Promise<void> {
  const info = await prepare("extractSuperclass", path, view);
  if (!info) return;
  const choice = await ask("extractSuperclass", info);
  view.focus();
  if (!choice) return;
  const { preview, ...options } = choice;
  await run(
    t("codeintel.hierarchyRefactorings.extractSuperclassName", { name: options.name }),
    view,
    () => hierarchyApi.extractSuperclass(info, options, contentFor(info.path, path, view), dirtyBuffers()),
    preview,
  );
}

/** Pull Members Up (Refactor | Pull Members Up…). */
export async function pullUpAt(path: string, view: EditorView): Promise<void> {
  const info = await prepare("pullUp", path, view);
  if (!info) return;
  if (!info.members.length) {
    showHint(view, view.state.selection.main.head, `${info.name} has no members to pull up`);
    return;
  }
  const choice = await ask("pullUp", info);
  view.focus();
  if (!choice) return;
  const { preview, ...options } = choice;
  await run("Pull Members Up", view, () => hierarchyApi.pullUp(info, options, contentFor(info.path, path, view), dirtyBuffers()), preview);
}

/** Push Members Down (Refactor | Push Members Down…). */
export async function pushDownAt(path: string, view: EditorView): Promise<void> {
  const info = await prepare("pushDown", path, view);
  if (!info) return;
  if (!info.targets.length) {
    showHint(view, view.state.selection.main.head, `${info.kind === "interface" ? t("codeintel.hierarchyRefactorings.interface") : t("codeintel.hierarchyRefactorings.class")} ${info.name} has no subclasses`);
    return;
  }
  const choice = await ask("pushDown", info);
  view.focus();
  if (!choice) return;
  const { preview, ...options } = choice;
  await run("Push Members Down", view, () => hierarchyApi.pushDown(info, options, contentFor(info.path, path, view), dirtyBuffers()), preview);
}
