/**
 * Refactor This (Ctrl+Alt+Shift+T) like PHPStorm: a popup with only the refactorings that
 * apply at the caret or to the selection, numbered (1-9, 0) and filtered by typing. Rust
 * decides which ones apply (`code_refactor_this`: PHP's own analysis; the language
 * servers' refactorings for JS/TS, worded like PHPStorm); this module renders the list and
 * runs the chosen action through the keymap (or the server's code action).
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import type { ContextAction } from "@/ipc/codeintel";
import { refactorThisApi } from "@/ipc/refactor";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";
import { runContextAction } from "./intentions";

/** Every refactoring Refactor This knows, PHPStorm's order (files without an analysis of their own get the enabled ones). */
export const REFACTOR_THIS = [
  "RenameElement",
  "ChangeSignature",
  "Move",
  "CopyElement",
  "SafeDelete",
  "IntroduceVariable",
  "IntroduceConstant",
  "IntroduceField",
  "IntroduceParameter",
  "ExtractMethod",
  "Inline",
];

export interface RefactorThisItem {
  title: string;
  /** Right-aligned shortcut. */
  shortcut: string;
  /** A separator above it (language server refactorings after Forge's). */
  separator?: boolean;
  run: () => void;
}

export interface RefactorThisRequest {
  items: RefactorThisItem[];
  x: number;
  y: number;
  done: (item: RefactorThisItem | null) => void;
}

const [refactorThisPopup, setRefactorThisPopup] = createSignal<RefactorThisRequest | null>(null);
export { refactorThisPopup };

/** The number shown before a row (1-9, then 0), PHPStorm's mnemonics. */
export function mnemonic(index: number): string {
  return index < 9 ? String(index + 1) : index === 9 ? "0" : "";
}

/** The row a typed digit picks, or `null`. */
export function indexOfMnemonic(key: string, count: number): number | null {
  if (!/^[0-9]$/.test(key)) return null;
  const i = key === "0" ? 9 : Number(key) - 1;
  return i < count ? i : null;
}

/** Speed search: the typed characters appear in the title in order (case-insensitive). */
export function matches(title: string, query: string): boolean {
  const t = title.toLowerCase();
  let at = 0;
  for (const ch of query.toLowerCase()) {
    if (ch === " ") continue;
    const i = t.indexOf(ch, at);
    if (i < 0) return false;
    at = i + 1;
  }
  return true;
}

export interface RefactorThisActions {
  available: (id: string) => boolean;
  title: (id: string) => string;
  shortcut: (id: string) => string;
  run: (id: string) => void;
}

/** The popup's rows: Forge's refactorings (enabled ones only), then the language server's. */
export function itemsOf(
  ids: string[],
  lsp: ContextAction[],
  actions: RefactorThisActions,
  runLsp: (a: ContextAction) => void,
): RefactorThisItem[] {
  const own: RefactorThisItem[] = ids
    .filter(actions.available)
    .map((id) => ({ title: actions.title(id), shortcut: actions.shortcut(id), run: () => actions.run(id) }));
  const seen = new Set(own.map((i) => i.title.replace(/[.…]+$/, "").toLowerCase()));
  const server = lsp
    .filter((a) => !a.disabled && !seen.has(a.title.replace(/[.…]+$/, "").toLowerCase()))
    .map((a, i) => ({ title: a.title, shortcut: "", separator: i === 0 && own.length > 0, run: () => runLsp(a) }));
  return [...own, ...server];
}

/** Refactor This at the caret (or for the selection) of the editor. */
export async function refactorThisAt(path: string, view: EditorView, actions: RefactorThisActions): Promise<void> {
  const sel = view.state.selection.main;
  const doc = view.state.doc;
  const range = { start: toLsp(doc, sel.from), end: toLsp(doc, sel.to) };
  let ids: string[] = REFACTOR_THIS;
  let lsp: ContextAction[] = [];
  try {
    await syncNow(path);
    const answer = await refactorThisApi.at(path, range, doc.toString());
    if (answer.actions) ids = answer.actions;
    lsp = answer.lsp;
  } catch {
    // Without an answer every enabled refactoring is offered.
  }
  const items = itemsOf(ids, lsp, actions, (a) => void runContextAction(path, view, a));
  if (!items.length) {
    showHint(view, sel.head, "No refactorings available at the caret");
    return;
  }
  const coords = view.coordsAtPos(sel.head) ?? view.coordsAtPos(doc.lineAt(sel.head).from);
  const chosen = await new Promise<RefactorThisItem | null>((resolve) =>
    setRefactorThisPopup({
      items,
      x: Math.max(8, (coords?.left ?? 200) - 8),
      y: (coords?.bottom ?? 200) + 4,
      done: (item) => {
        setRefactorThisPopup(null);
        resolve(item);
      },
    }),
  );
  view.focus();
  chosen?.run();
}
