/**
 * No unsaved edit is lost when a project or window closes. PHPStorm saves every modified file
 * when a project closes, when the IDE exits and when a file's last tab closes (an autosave,
 * whatever Settings | Appearance & Behavior | System Settings | Autosave says); Forge does the
 * same (`saveAllForClose`, `saveClosedDocument` in `state/editor.ts`). Only when a file cannot
 * be written does Forge ask: keep the project open (default) or close and discard the edits.
 */
import { askConfirm } from "@/features/contextMenu/dialogs";
import { showBalloon } from "@/keymap";
import { onCloseSaveFailed, saveAllForClose } from "@/state/editor";
import { onProject } from "@/state/projectLifecycle";
import { t as tr } from "@/i18n";

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/**
 * Save every modified file before the project or window goes. `true` when nothing unsaved is
 * left or the user chose to discard what could not be saved.
 */
export async function saveBeforeLeaving(what: "project" | "window"): Promise<boolean> {
  const failed = await saveAllForClose();
  if (!failed.length) return true;
  const names = failed.map((f) => `${f.path}: ${message(f.error)}`).join("\n");
  return askConfirm({
    title: tr("tools.system.cannotSaveFiles"),
    message: tr(what === "project" ? "tools.system.unsavedCloseProject" : "tools.system.unsavedCloseWindow", {
      count: failed.length,
      files: names,
    }),
    confirm: tr("tools.system.discardAndClose"),
  });
}

// Before anything else of the project closes (terminals, language servers).
onProject("beforeClose", () => saveBeforeLeaving("project"), { order: -100, name: "editor documents" });

// A modified file whose tab closed could not be saved: its tab is back.
onCloseSaveFailed((path, error) => showBalloon(tr("tools.system.cannotSaveFile", { path }), message(error), 8000));
