/**
 * Compare Files (`CompareTwoFiles`, Ctrl+D) in each tool window: views register what Ctrl+D
 * compares while they have the focus (the Commit window shows the diff of the selected
 * change, a history tab the diff of the selected revision); the Project view compares the
 * selected files or folders. Also the helpers that open the diff viewer for two texts.
 */
import { createSignal } from "solid-js";
import { editor } from "@/state/editor";
import { workspaceApi } from "@/ipc/workspace";
import { setDiffRequest } from "@/features/git/overlays";

interface CompareHandler {
  /** The handler runs when the focused element is inside an element matching this. */
  selector: string;
  run: () => void | Promise<void>;
}

const handlers: CompareHandler[] = [];

export function registerCompareHandler(selector: string, run: () => void | Promise<void>): void {
  handlers.push({ selector, run });
}

/** The handler for the focused element, if any. */
export function compareHandlerForFocus(): CompareHandler | undefined {
  const focus = typeof document === "undefined" ? null : document.activeElement;
  return focus ? handlers.find((h) => focus.closest(h.selector)) : undefined;
}

/** A project file's text as the editor has it (unsaved edits included), `null` if unreadable. */
export async function readText(path: string): Promise<string | null> {
  const tab = editor.tabs.find((t) => t.path === path && !t.view);
  if (tab) return tab.content;
  try {
    return await workspaceApi.readFile(path);
  } catch {
    return null;
  }
}

const isProjectFile = (path: string) => !path.includes("://");

/** Compare Files: two project files side by side; the right one can be edited. */
export async function compareFiles(left: string, right: string): Promise<void> {
  const [a, b] = await Promise.all([readText(left), readText(right)]);
  setDiffRequest({
    path: right,
    left: { title: a === null ? `${left} (does not exist)` : left, content: a ?? "" },
    right: { title: b === null ? `${right} (does not exist)` : right, content: b ?? "" },
    editable: b !== null && isProjectFile(right),
  });
}

// ---- Select Path dialog -------------------------------------------------------------------

export interface SelectPathRequest {
  title: string;
  /** Shown above the tree. */
  description: string;
  /** Pick a folder instead of a file. */
  dirs: boolean;
  /** Initially selected path. */
  initial?: string;
  onOk: (path: string) => void | Promise<void>;
}

const [selectPathRequest, setSelectPathRequest] = createSignal<SelectPathRequest | null>(null);
export { selectPathRequest, setSelectPathRequest };
