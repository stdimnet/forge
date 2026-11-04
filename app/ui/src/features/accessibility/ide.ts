/**
 * Accessibility in IDE windows: the editor's accessible name, "Announce Caret Position" (line,
 * column and the problems at the caret) and, with "Support screen readers" on, announcements
 * of the active file and of problems when the caret moves onto them.
 */
import { createEffect, createRoot, on } from "solid-js";
import { EditorView } from "@codemirror/view";
import { registerEditorExtension } from "@/editor/extensions";
import { registerCommand } from "@/state/commands";
import { activeTab, editor } from "@/state/editor";
import { registerKeymapActions } from "@/keymap";
import { activeView } from "@/features/navigation/editorCommands";
import { diagnosticsAt } from "@/features/lsp/diagnostics";
import { registerMainMenuEntries, registerMainMenuToggle, toggle } from "@/features/mainMenu/model";
import { getSetting } from "@/state/settings";
import { t } from "@/i18n";
import { announce, announceForScreenReader, screenReaderMode } from "./announce";

/** What "Announce Caret Position" says for a caret: "Line 3, column 5. Error: …". */
export function caretAnnouncement(line: number, col: number, problems: { severity: string; message: string }[]): string {
  const parts = [t("a11y.caret.position", { line, col })];
  for (const p of problems.slice(0, 3)) parts.push(t("a11y.caret.problem", { severity: p.severity, message: p.message }));
  if (!problems.length) parts.push(t("a11y.caret.noProblems"));
  return parts.join(" ");
}

function caretInfo(view: EditorView) {
  const pos = view.state.selection.main.head;
  const line = view.state.doc.lineAt(pos);
  return { line: line.number, col: pos - line.from + 1, problems: diagnosticsAt(view.state, pos) };
}

/** The editor's accessible name ("Editor for User.php") and a focusable scroller for axe. */
registerEditorExtension(({ path }) =>
  EditorView.contentAttributes.of({ "aria-label": t("a11y.editor.label", { name: path.split("/").pop() ?? path }) }),
);

/** With screen reader support: problems at the caret are spoken when the caret reaches a new line. */
registerEditorExtension(() => {
  let lastLine = -1;
  let timer: ReturnType<typeof setTimeout> | undefined;
  return EditorView.updateListener.of((u) => {
    if (!u.selectionSet || !u.view.hasFocus || !screenReaderMode()) return;
    const { line, problems } = caretInfo(u.view);
    if (line === lastLine) return;
    lastLine = line;
    clearTimeout(timer);
    if (!problems.length) return;
    timer = setTimeout(() => {
      const text = problems
        .slice(0, 2)
        .map((p) => t("a11y.caret.problem", { severity: p.severity, message: p.message }))
        .join(" ");
      announce(text);
    }, 400);
  });
});

registerKeymapActions([
  {
    id: "Forge.AnnounceCaretPosition",
    title: "Announce Caret Position",
    group: "Main Menu/View",
    description: "Says the line and column of the caret and the problems there through the screen reader.",
    shortcuts: { $default: ["Alt+Shift+L"], macOS: ["Alt+Shift+L"] },
  },
]);
registerCommand({
  id: "Forge.AnnounceCaretPosition",
  title: "Announce Caret Position",
  category: "View",
  run: () => {
    const view = activeView();
    if (!view) return announce(t("a11y.caret.noEditor"));
    const { line, col, problems } = caretInfo(view);
    announce(caretAnnouncement(line, col, problems), "assertive");
  },
});

registerMainMenuEntries("View/Appearance", [toggle("Forge.TogglePresentationAssistant", "Presentation _Assistant")]);
registerMainMenuToggle("Forge.TogglePresentationAssistant", () => getSetting<boolean>("appearance.presentationAssistant.enabled", false) === true);

// The active file, when another tab becomes active.
createRoot(() =>
  createEffect(
    on(
      () => editor.active,
      () => {
        const tab = activeTab();
        if (tab) announceForScreenReader(t("a11y.editor.opened", { name: tab.name }));
      },
      { defer: true },
    ),
  ),
);
