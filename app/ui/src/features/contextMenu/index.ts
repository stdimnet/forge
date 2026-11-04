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
import { copiedFrom, recordCopy } from "@/features/clipboard/clipboard";
import { pasteClipboard } from "@/features/clipboard";
import { action, menuEntries, providedEntries, registerContextMenu, separator, showContextMenu, submenu } from "./state";
import "./contextMenu.css";
import { t } from "@/i18n";

function command(id: string, run: () => void | Promise<void>): void {
  registerCommand({ id, title: actionById(id)?.title ?? id, run });
}

async function copy(cut: boolean): Promise<void> {
  const view = activeView();
  if (!view) return;
  // The selections, or the caret lines without one (PHPStorm copies the whole line).
  const copied = copiedFrom(view.state);
  if (!copied.text) return;
  try {
    await navigator.clipboard.writeText(copied.linewise ? copied.text + view.state.lineBreak : copied.text);
  } catch {
    showBalloon(t("editor.clipboardUnavailable"), t("editor.useShortcut"));
    return;
  }
  recordCopy(copied.text, copied.linewise ? [] : copied.carets);
  if (cut) view.dispatch({ changes: copied.ranges, userEvent: "delete.cut" });
  view.focus();
}

command("$Cut", () => copy(true));
command("$Copy", () => copy(false));
command("$Paste", pasteClipboard);
// Forge's paste does not reformat or add imports, so plain text is the same paste.
command("PasteSimple", pasteClipboard);
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
  submenu(t("editor.menu.copyPasteSpecial"), [action("CopyPaths"), action("CopyReference"), action("PasteSimple")]),
  action("$Paste"),
  separator,
  action("$SelectAll"),
  action("EditorToggleColumnMode"),
  separator,
  action("FindUsages"),
  submenu(t("editor.menu.refactor"), [
    action("Refactorings.QuickListPopupAction"),
    separator,
    action("RenameElement"),
    action("ChangeSignature"),
    submenu(t("editor.menu.extractIntroduce"), [
      action("IntroduceVariable", t("editor.menu.variable")),
      action("IntroduceConstant", t("editor.menu.constant")),
      action("IntroduceField", t("editor.menu.field")),
      action("IntroduceParameter", t("editor.menu.parameter")),
      separator,
      action("ExtractMethod", t("editor.menu.method")),
    ]),
    action("Inline"),
    separator,
    action("Move"),
    action("CopyElement", t("editor.menu.copy")),
    action("SafeDelete"),
    separator,
    action("MakeStatic"),
  ]),
  submenu(t("editor.menu.goTo"), [action("GotoDeclaration"), action("GotoImplementation"), action("GotoLine")]),
  action("Generate"),
  separator,
  action("ReformatCode"),
  action("OptimizeImports"),
  action("CommentByLineComment"),
  separator,
  submenu(t("editor.menu.git"), [
    action("Annotate"),
    action("Compare.SameVersion"),
    action("Compare.Selected"),
    action("Git.CompareWithBranch"),
    action("Vcs.ShowTabbedFileHistory"),
    action("Vcs.ShowHistoryForBlock"),
  ]),
  submenu(t("editor.menu.localHistory"), [
    action("LocalHistory.ShowHistory", t("editor.menu.showHistory")),
    action("LocalHistory.ShowSelectionHistory", t("editor.menu.showSelectionHistory")),
    separator,
    action("LocalHistory.PutLabel", t("editor.menu.putLabel")),
  ]),
  action("CompareClipboardWithSelection"),
]);

/** Open the editor menu at a screen position and hand the focus back to the editor afterwards. */
function showEditorMenu(view: EditorView, x: number, y: number): void {
  // Entries that depend on the moment (External Tools) come from providers.
  showContextMenu([...menuEntries("editor"), ...providedEntries("editor", view)], x, y, () => view.focus());
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
