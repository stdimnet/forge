/** Replace in Files for the Find window: preview the changed lines, replace, undo. */
import { createSignal } from "solid-js";
import { findApi, type FindRequest, type ReplacePreviewFile } from "@/ipc/find";
import { findForm, runFindWindow } from "./window";

export const [replaceWith, setReplaceWith] = createSignal("");
/** The lines a replace would change; null while no preview is open. */
export const [replacePreview, setReplacePreview] = createSignal<ReplacePreviewFile[] | null>(null);
export const [replaceBusy, setReplaceBusy] = createSignal(false);
export const [replaceMessage, setReplaceMessage] = createSignal<string | null>(null);
export const [canUndoReplace, setCanUndoReplace] = createSignal(false);

const UNLIMITED = 100_000_000;

const request = (): FindRequest => ({ ...findForm, context: 0, limit: UNLIMITED });

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

export const previewTotals = (files: ReplacePreviewFile[]) => ({
  files: files.length,
  matches: files.reduce((n, f) => n + f.matches, 0),
});

export async function openReplacePreview(): Promise<void> {
  if (!findForm.query) return;
  setReplaceBusy(true);
  setReplaceMessage(null);
  try {
    const files = await findApi.replacePreview(request(), replaceWith());
    setReplacePreview(files);
    if (!files.length) setReplaceMessage("Nothing to replace.");
  } catch (e) {
    setReplacePreview(null);
    setReplaceMessage(message(e));
  } finally {
    setReplaceBusy(false);
  }
}

export const closeReplacePreview = () => setReplacePreview(null);

export async function applyReplace(): Promise<void> {
  setReplaceBusy(true);
  try {
    const done = await findApi.replaceAll(request(), replaceWith());
    setReplacePreview(null);
    setCanUndoReplace(done.files > 0);
    const skipped = done.skipped.length ? `, ${done.skipped.length} skipped` : "";
    setReplaceMessage(`Replaced ${done.replacements} matches in ${done.files} files${skipped}.`);
    await runFindWindow();
  } catch (e) {
    setReplaceMessage(message(e));
  } finally {
    setReplaceBusy(false);
  }
}

export async function undoReplace(): Promise<void> {
  setReplaceBusy(true);
  try {
    const undone = await findApi.undoReplace();
    setCanUndoReplace(false);
    const left = undone.left.length ? `; ${undone.left.length} changed since and were left alone` : "";
    setReplaceMessage(`Restored ${undone.restored} files${left}.`);
    await runFindWindow();
  } catch (e) {
    setReplaceMessage(message(e));
  } finally {
    setReplaceBusy(false);
  }
}
