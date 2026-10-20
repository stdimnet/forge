/**
 * Context menus: the editor menu (right-click, Menu key, Shift+F10) and the clipboard
 * actions it needs. Other features add entries with `registerContextMenu("editor", …)`.
 */
import { EditorSelection } from "@codemirror/state";
import { EditorView, ViewPlugin } from "@codemirror/view";
import { registerEditorExtension } from "@/editor/extensions";
import { actionById, showBalloon } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { activeView } from "@/features/navigation/editorCommands";
import { action, menuEntries, registerContextMenu, separator, showContextMenu, submenu } from "./state";
import "./contextMenu.css";

function command(id: string, run: () => void | Promise<void>): void {
  registerCommand({ id, title: actionById(id)?.title ?? id, run });
}

function selectedText(view: EditorView): string {
  const { state } = view;
  return state.selection.ranges
    .filter((r) => !r.empty)
    .map((r) => state.sliceDoc(r.from, r.to))
    .join(state.lineBreak);
}

/** Without a selection Copy and Cut take the whole line, like PHPStorm. */
function lineRangeIfEmpty(view: EditorView) {
  const { state } = view;
  if (state.selection.ranges.some((r) => !r.empty)) return null;
  const line = state.doc.lineAt(state.selection.main.head);
  return { from: line.from, to: Math.min(line.to + 1, state.doc.length) };
}

async function copy(cut: boolean): Promise<void> {
  const view = activeView();
  if (!view) return;
  const wholeLine = lineRangeIfEmpty(view);
  const text = wholeLine ? view.state.sliceDoc(wholeLine.from, wholeLine.to) : selectedText(view);
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    showBalloon("Clipboard is not available", "Use the keyboard shortcut instead.");
    return;
  }
  if (cut) {
    const changes = wholeLine ?? view.state.selection.ranges.filter((r) => !r.empty).map((r) => ({ from: r.from, to: r.to }));
    view.dispatch({ changes, userEvent: "delete.cut" });
  }
  view.focus();
}

async function paste(): Promise<void> {
  const view = activeView();
  if (!view) return;
  let text: string;
  try {
    text = await navigator.clipboard.readText();
  } catch {
    showBalloon("Clipboard is not available", "Use the keyboard shortcut instead.");
    return;
  }
  if (text) view.dispatch(view.state.replaceSelection(text), { scrollIntoView: true, userEvent: "input.paste" });
  view.focus();
}

command("$Cut", () => copy(true));
command("$Copy", () => copy(false));
command("$Paste", paste);
command("PasteSimple", paste);
command("$SelectAll", () => {
  const view = activeView();
  if (!view) return;
  view.dispatch({ selection: EditorSelection.single(0, view.state.doc.length) });
  view.focus();
});

registerContextMenu("editor", [
  action("ShowIntentionActions"),
  action("PasteMultiple"),
  separator,
  action("$Cut"),
  action("$Copy"),
  submenu("Copy / Paste Special", [action("CopyPaths"), action("CopyReference"), action("PasteSimple")]),
  action("$Paste"),
  separator,
  action("$SelectAll"),
  action("EditorToggleColumnMode"),
  separator,
  action("FindUsages"),
  submenu("Refactor", [action("RenameElement"), action("Refactorings.QuickListPopupAction")]),
  submenu("Go To", [action("GotoDeclaration"), action("GotoImplementation"), action("GotoLine")]),
  action("Generate"),
  separator,
  action("ReformatCode"),
  action("OptimizeImports"),
  action("CommentByLineComment"),
  separator,
  submenu("Git", [action("Annotate")]),
]);

/** Open the editor menu at a screen position and hand the focus back to the editor afterwards. */
function showEditorMenu(view: EditorView, x: number, y: number): void {
  showContextMenu(menuEntries("editor"), x, y, () => view.focus());
}

registerEditorExtension(() =>
  ViewPlugin.define((view) => {
    const onMenu = (event: MouseEvent) => {
      // The gutter has its own menu (Annotate with Git Blame).
      if ((event.target as HTMLElement).closest(".cm-gutters")) return;
      event.preventDefault();
      // Right-click outside the selection moves the caret there first, like PHPStorm.
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
      if (pos != null && !view.state.selection.ranges.some((r) => pos >= r.from && pos <= r.to && !r.empty)) {
        view.dispatch({ selection: { anchor: pos } });
      }
      view.focus();
      showEditorMenu(view, event.clientX, event.clientY);
    };
    view.dom.addEventListener("contextmenu", onMenu);
    return { destroy: () => view.dom.removeEventListener("contextmenu", onMenu) };
  }),
);

// Menu key / Shift+F10: below the caret.
command("ShowPopupMenu", () => {
  const view = activeView();
  if (!view) return;
  const at = view.coordsAtPos(view.state.selection.main.head);
  if (at) showEditorMenu(view, at.left, at.bottom + 2);
});
