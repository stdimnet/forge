/**
 * Tracks the CodeMirror view of every open file so features can jump to a location:
 * open the file, move the caret and scroll it to the middle of the editor.
 */
import { ViewPlugin, EditorView } from "@codemirror/view";
import { activeTab, openFile } from "@/state/editor";
import type { LspPosition } from "@/ipc/lsp";
import { fromLsp } from "./util";
import { centerOnPos, viewFor as sharedViewFor } from "@/editor/views";

const views = new Map<string, EditorView>();

export function trackView(path: string) {
  return ViewPlugin.define((view) => {
    views.set(path, view);
    return {
      destroy() {
        if (views.get(path) === view) views.delete(path);
      },
    };
  });
}

export const viewFor = (path: string) => sharedViewFor(path);

/** The editor view of the active tab. */
export function activeView(): { path: string; view: EditorView } | undefined {
  const path = activeTab()?.path;
  const view = path ? sharedViewFor(path) : undefined;
  return path && view ? { path, view } : undefined;
}

const nextFrame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

/** Open `path` and put the caret at `pos` (an LSP position). */
export async function revealLocation(path: string, pos: LspPosition, end?: LspPosition): Promise<boolean> {
  await openFile(path);
  for (let i = 0; i < 60; i++) {
    const view = sharedViewFor(path);
    if (view && view.dom.isConnected && view.dom.offsetParent !== null) {
      const anchor = fromLsp(view.state.doc, pos);
      const head = end ? fromLsp(view.state.doc, end) : anchor;
      view.dispatch({ selection: { anchor, head } });
      centerOnPos(view, anchor);
      view.focus();
      return true;
    }
    await nextFrame();
  }
  return false;
}
