/**
 * Multiple carets and Column Selection Mode (PHPStorm: Edit | Column Selection Mode,
 * Alt+Shift+Insert): Clone Caret Above / Below, Add Carets to Ends of Selected Lines, the
 * Alt+Shift+Click / Alt+drag mouse gestures and the status bar indicator. Occurrence
 * actions (Alt+J, Alt+Shift+J, Ctrl+Alt+Shift+J) are in `navigation/editorCommands`.
 */
import { createSignal } from "solid-js";
import { registerEditorExtension } from "@/editor/extensions";
import { activeView } from "@/editor/views";
import { registerMainMenuToggle } from "@/features/mainMenu/model";
import { registerStatusItem } from "@/features/statusItems";
import { actionById, shortcutLabel } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { addCaretsToLineEnds, caretExtensions, cloneCaret, isColumnMode, setColumnMode } from "./carets";
import "./carets.css";

registerEditorExtension(() => caretExtensions());

// Bumped when an editor's mode changes; the active editor is reactive already.
const [modeVersion, setModeVersion] = createSignal(0);

/** Whether the active editor is in Column Selection Mode (reactive). */
export function columnModeOn(): boolean {
  modeVersion();
  const view = activeView();
  return view ? isColumnMode(view.state) : false;
}

function toggleColumnMode(): void {
  const view = activeView();
  if (!view) return;
  setColumnMode(view, !isColumnMode(view.state));
  setModeVersion((v) => v + 1);
  view.focus();
}

function editorAction(id: string, run: () => void): void {
  registerCommand({ id, title: actionById(id)?.title ?? id, run });
}

const inEditor = (cmd: (view: NonNullable<ReturnType<typeof activeView>>) => boolean) => () => {
  const view = activeView();
  if (view) cmd(view);
};

editorAction("EditorCloneCaretAbove", inEditor(cloneCaret(-1)));
editorAction("EditorCloneCaretBelow", inEditor(cloneCaret(1)));
editorAction("EditorAddCaretPerSelectedLine", inEditor(addCaretsToLineEnds));
editorAction("EditorToggleColumnMode", toggleColumnMode);
registerMainMenuToggle("EditorToggleColumnMode", columnModeOn);

function ColumnModeStatus() {
  return (
    <button
      class="status-item column-mode-status"
      title={`Column Selection Mode is on. Click or press ${shortcutLabel("EditorToggleColumnMode")} to turn it off.`}
      onClick={toggleColumnMode}
    >
      Column
    </button>
  );
}

registerStatusItem({ id: "editor.columnMode", align: "right", order: 0, component: ColumnModeStatus, when: columnModeOn });
