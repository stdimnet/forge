/**
 * Changelist and patch actions of the Commit tool window and Git ▸ Patch, like PHPStorm:
 * New / Edit / Delete Changelist, Set Active Changelist, Move to Another Changelist (F6),
 * Create Patch… and Apply Patch… / Apply Patch from Clipboard…. The dialogs render in
 * `ChangesDialogViews.tsx`; the logic lives in Rust (`forge_vcs::{changelists, patch}`).
 */
import { isTauri } from "@/ipc";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { getSetting } from "@/state/settings";
import { changesApi, type ChangelistView, type PatchPreviewFile } from "./changesApi";
import { changelistOp, changelists, setInclusion } from "./changelists";
import { setGitDialog } from "./gitDialogs";
import { logApi } from "./logApi";
import { notify, reloadOpenFiles, repositoryChanged } from "./store";
import { conflictedPaths, resolveConflicts } from "./conflicts";
import { setDiffRequest } from "./overlays";
import { t } from "@/i18n";

const plural = (n: number, what: string) => `${n} ${what}${n === 1 ? "" : "s"}`;
const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);

// ---- changelists ------------------------------------------------------------------------

/** New Changelist… (`ChangesView.NewChangeList`). */
export function newChangelistDialog(movePaths: string[] = []): void {
  setGitDialog({ kind: "changelist", edit: null, movePaths });
}

/** Edit Changelist… (`ChangesView.Rename`). */
export function editChangelistDialog(list: ChangelistView): void {
  setGitDialog({ kind: "changelist", edit: list, movePaths: [] });
}

export async function saveChangelist(edit: ChangelistView | null, name: string, comment: string, active: boolean, movePaths: string[]): Promise<boolean> {
  if (edit) return changelistOp("Could not edit the changelist", () => changesApi.editChangelist(edit.id, name, comment, active));
  const ok = await changelistOp("Could not create the changelist", () => changesApi.createChangelist(name, comment, active));
  if (ok && movePaths.length) await changelistOp("Could not move the changes", () => changesApi.move(movePaths, name, null, true));
  return ok;
}

/** Set Active Changelist (`ChangesView.SetDefault`). */
export function setActiveChangelist(list: ChangelistView): Promise<boolean> {
  return changelistOp("Could not set the active changelist", () => changesApi.setActive(list.id));
}

/** Delete Changelist (`ChangesView.RemoveChangeList`): its changes move to the active list. */
export async function deleteChangelist(list: ChangelistView): Promise<void> {
  if (list.active) {
    notify("info", t("git.changesDialogs.theActiveChangelistCannotBe"), "Set another changelist active first.");
    return;
  }
  if (list.files.length) {
    const ok = await askConfirm({
      title: t("git.changesDialogs.deleteChangelist"),
      message: `Delete changelist "${list.name}"? Its ${plural(list.files.length, "file")} will be moved to the active changelist.`,
      confirm: t("git.changesDialogs.delete"),
    });
    if (!ok) return;
  }
  await changelistOp("Could not delete the changelist", () => changesApi.deleteChangelist(list.id));
}

/**
 * Move to Another Changelist… (F6, `ChangesView.Move`): whole files, or with `hunks` single
 * change hunks of `paths[0]` (from the gutter popup or the commit diff).
 */
export function moveToChangelistDialog(paths: string[], opts: { hunks?: string[]; from?: string } = {}): void {
  if (!paths.length) return notify("info", t("git.changesDialogs.selectChangesToMoveTo"));
  setGitDialog({ kind: "moveChangelist", paths, hunks: opts.hunks ?? null, from: opts.from ?? null });
}

/** Move into an existing list (`target` id) or a new one (`newName`). */
export async function moveToChangelist(paths: string[], hunks: string[] | null, target: { id?: string; newName?: string; active?: boolean }): Promise<boolean> {
  let id = target.id;
  if (target.newName) {
    const ok = await changelistOp("Could not create the changelist", () => changesApi.createChangelist(target.newName!, "", !!target.active));
    if (!ok) return false;
    id = changelists().find((l) => l.name === target.newName!.trim())?.id;
  }
  if (!id) return false;
  const ok = await changelistOp("Could not move the changes", () => changesApi.move(paths, id!, hunks));
  if (ok) followInclusion(paths, hunks, id);
  return ok;
}

/** Moved hunks are included when they land in the active list, else excluded (PHPStorm). */
function followInclusion(paths: string[], hunks: string[] | null, listId: string): void {
  if (!hunks || !paths[0]) return;
  const active = changelists().find((l) => l.active)?.id === listId;
  setInclusion(paths[0], hunks, active);
}

/** Drag and drop in the Commit tool window. */
export function dropOnChangelist(listId: string, items: { path: string; hunks: string[] | null }[]): void {
  void (async () => {
    for (const i of items) {
      if (await changelistOp("Could not move the changes", () => changesApi.move([i.path], listId, i.hunks))) followInclusion([i.path], i.hunks, listId);
    }
  })();
}

/** Put applied or unshelved files into a changelist named `name` ("Create changelists automatically"). */
export async function intoNewChangelist(name: string, paths: string[]): Promise<void> {
  if (!paths.length || !getSetting<boolean>("vcs.changelists.createAutomatically", true)) return;
  await repositoryChanged();
  await changelistOp("Could not create the changelist", () => changesApi.move(paths, name, null, true));
}

// ---- Create Patch -------------------------------------------------------------------------

/** Create Patch… from local changes (`ChangesView.CreatePatch`). */
export function createPatchDialog(paths: string[], name = "Changes"): void {
  if (!paths.length) return notify("info", t("git.changesDialogs.selectChangesToCreateA"));
  setGitDialog({ kind: "createPatch", paths, shas: [], name });
}

/** Create Patch… from commits of the Git log. */
export function createCommitsPatchDialog(shas: string[], name: string): void {
  setGitDialog({ kind: "createPatch", paths: [], shas, name });
}

/** Save or copy the patch. `file` null copies it to the clipboard. */
export async function createPatch(opts: { paths: string[]; shas: string[]; file: string | null; reverse: boolean }): Promise<boolean> {
  try {
    const patch = opts.shas.length ? await logApi.patch(opts.shas) : await changesApi.createPatch(opts.paths, opts.reverse);
    if (!patch.trim()) {
      notify("info", t("git.changesDialogs.noChangesToCreateA"));
      return false;
    }
    if (opts.file === null) {
      await navigator.clipboard.writeText(patch);
      notify("success", t("git.changesDialogs.patchCopiedToClipboard"));
    } else {
      await changesApi.writePatch(opts.file, patch);
      notify("success", t("git.changesDialogs.patchCreated"), opts.file);
      await repositoryChanged();
    }
    return true;
  } catch (e) {
    notify("error", t("git.changesDialogs.createPatchFailed"), errorText(e), 10000);
    return false;
  }
}

/** Browse… for the patch file to save (desktop: native dialog). */
export async function chooseSavePath(initial: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  return (await save({ defaultPath: initial, filters: [{ name: "Patch files", extensions: ["patch", "diff"] }] })) ?? null;
}

// ---- Apply Patch --------------------------------------------------------------------------

/** Browser mode: read a file the user picks. */
function pickFileText(): Promise<{ name: string; text: string } | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".patch,.diff,.txt";
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return resolve(null);
      void file.text().then((text) => resolve({ name: file.name, text }), () => resolve(null));
    };
    input.click();
  });
}

const baseName = (p: string) => p.slice(Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\")) + 1).replace(/\.(patch|diff)$/i, "");

/** Open the preview of a patch text. */
export async function previewPatch(patch: string, name: string): Promise<void> {
  try {
    const files = await changesApi.patchPreview(patch, "");
    if (!files.length) return notify("error", t("git.changesDialogs.applyPatch"), "The text is not a patch: no file changes found.");
    setGitDialog({ kind: "applyPatch", patch, name, files });
  } catch (e) {
    notify("error", t("git.changesDialogs.applyPatchFailed"), errorText(e), 10000);
  }
}

/** Apply Patch… (`ChangesView.ApplyPatch`): pick a patch file, then preview it. */
export async function applyPatchDialog(): Promise<void> {
  try {
    if (isTauri()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false, filters: [{ name: "Patch files", extensions: ["patch", "diff", "txt"] }] });
      if (typeof path !== "string") return;
      await previewPatch(await changesApi.readPatch(path), baseName(path));
      return;
    }
    const picked = await pickFileText();
    if (picked) await previewPatch(picked.text, baseName(picked.name));
  } catch (e) {
    notify("error", t("git.changesDialogs.applyPatchFailed"), errorText(e), 10000);
  }
}

/** Apply Patch from Clipboard… (`ChangesView.ApplyPatchFromClipboard`). */
export async function applyPatchFromClipboard(): Promise<void> {
  const text = await navigator.clipboard?.readText().catch(() => "");
  if (!text?.trim()) return notify("info", t("git.changesDialogs.theClipboardHoldsNoPatch"));
  await previewPatch(text, "Patch from clipboard");
}

/** Show Diff of one file of the Apply Patch preview. */
export function showPatchFileDiff(f: PatchPreviewFile): void {
  setDiffRequest({
    path: f.path,
    left: { title: f.before === null ? t("git.changesDialogs.emptyNewFile") : t("git.changesDialogs.currentVersion"), content: f.before ?? "" },
    right: { title: f.after === null ? (f.status === "deleted" ? t("git.changesDialogs.deleted") : t("git.changesDialogs.doesNotApplyCleanly")) : t("git.changesDialogs.patchedVersion"), content: f.after ?? f.before ?? "" },
    editable: false,
  });
}

/** Apply the checked files of the preview; conflicts open the resolver. */
export async function applyPatch(patch: string, base: string, paths: string[] | null, changelist: { id?: string; newName?: string }): Promise<boolean> {
  let res;
  try {
    res = await changesApi.applyPatch(patch, base, paths);
  } catch (e) {
    notify("error", t("git.changesDialogs.applyPatchFailed"), errorText(e), 10000);
    await repositoryChanged();
    return false;
  }
  await repositoryChanged();
  await reloadOpenFiles();
  const touched = [...res.applied, ...res.conflicts];
  if (touched.length && changelist.newName) {
    await changelistOp("Could not create the changelist", () => changesApi.move(touched, changelist.newName!, null, true));
  } else if (touched.length && changelist.id) {
    await changelistOp("Could not move the changes", () => changesApi.move(touched, changelist.id!));
  }
  for (const p of res.applied) setInclusion(p, null, true);
  if (res.failed.length) {
    notify("error", `Patch could not be applied to ${plural(res.failed.length, "file")}`, res.failed.map(([p, m]) => `${p}: ${m}`).join("\n"), 15000);
  }
  if (res.applied.length) notify("success", t("git.changesDialogs.patchApplied"), plural(res.applied.length, "file"));
  if (res.conflicts.length || conflictedPaths().length) resolveConflicts();
  return !res.failed.length;
}
