/**
 * Navigate | File Structure (Ctrl+F12): the popup with the current file's structure. Pressing
 * the shortcut again while it is open toggles "Show Inherited".
 */
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { languageIdFor } from "@/editor/languages";
import { activeView } from "@/features/lsp/navigation";
import { showHint } from "@/features/lsp/hint";
import { hasFileStructure } from "@/features/php/StructureView";
import { scopesAt, structureOf } from "@/features/codeStructure/model";
import { showBalloon } from "@/keymap";
import { fileStructureSession, showFileStructure, type FileStructureSession } from "./FileStructurePopup";
import { t } from "@/i18n";
import "./fileStructure.css";

/** The popup's input for the active editor: its text, the caret line and the scopes around it. */
export function currentSession(): FileStructureSession | null {
  const tab = activeTab();
  if (!tab) return null;
  const a = activeView();
  if (!a || a.path !== tab.path) return { path: tab.path, content: tab.content, enclosing: [] };
  const { state } = a.view;
  const head = state.selection.main.head;
  const line = state.doc.lineAt(head);
  const scopes = scopesAt(structureOf(state.doc, languageIdFor(tab.path)), line.number - 1, head - line.from);
  return {
    path: tab.path,
    content: state.doc.toString(),
    caretLine: line.number - 1,
    enclosing: scopes.map((s) => ({ line: s.line, openLine: s.openLine })),
  };
}

registerCommand({
  id: "FileStructurePopup",
  title: "File Structure",
  category: "Navigate",
  run: () => {
    if (fileStructureSession()) {
      showFileStructure(fileStructureSession()!);
      return;
    }
    const s = currentSession();
    if (!s) return;
    if (!hasFileStructure(s.path)) {
      const a = activeView();
      if (a) showHint(a.view, a.view.state.selection.main.head, "No structure for this file");
      else showBalloon(t("tools.fileStructure.noStructureForThisFile"));
      return;
    }
    showFileStructure(s);
  },
});
