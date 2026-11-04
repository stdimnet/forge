/**
 * Local History actions: Show History (file, folder, project), Show History for Selection,
 * Put Label…, Recent Changes, and what a revision can do (Revert, Show Diff, Create Patch).
 * The history views open as editor tabs, like PHPStorm's Local History window.
 */
import { createSignal } from "solid-js";
import { localHistoryApi, type LhRevertTarget, type LhRevision, type StateKind } from "@/ipc/localHistory";
import { changesApi } from "@/features/git/changesApi";
import { chooseSavePath } from "@/features/git/changesDialogs";
import { setDiffRequest } from "@/features/git/overlays";
import { reloadOpenFiles } from "@/features/git/store";
import { vcsTargets } from "@/features/git/context";
import { activeView } from "@/features/navigation/editorCommands";
import { askChoice, askConfirm, askName } from "@/features/contextMenu/dialogs";
import { refreshDirs } from "@/features/project/ProjectView";
import { readText } from "@/features/compare/state";
import { activeTab, saveAll } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { isTauri } from "@/ipc";
import { showBalloon } from "@/keymap";
import { t as tr } from "@/i18n";

/** What a Local History tab shows. */
export type HistoryTarget =
  | { kind: "file"; path: string }
  | { kind: "folder"; path: string }
  | { kind: "selection"; path: string; start: number; end: number }
  | { kind: "change"; from: number; to: number; name: string; time: number };

export const nameOf = (path: string) => path.split("/").pop() || path;
const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");
const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);

function notifyError(title: string, e: unknown): void {
  showBalloon(title, errorText(e), 8000, undefined, { type: "error", group: "Local History" });
}

function notifyInfo(title: string, detail?: string): void {
  showBalloon(title, detail, 4000, undefined, { group: "Local History" });
}

/** Revision list and views refresh when this changes (`local-history://changed`). */
const [historyRevision, setHistoryRevision] = createSignal(0);
export { historyRevision };
void localHistoryApi.onChanged(() => setHistoryRevision((n) => n + 1));

// ---- formatting -----------------------------------------------------------------------------

const pad = (n: number) => String(n).padStart(2, "0");

/** "Today 14:03", "Yesterday 09:12", else "12.10.26, 14:03" (like the Git log). */
export function historyDate(ms: number, now = new Date()): string {
  const d = new Date(ms);
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86400000);
  if (diff === 0) return `Today ${time}`;
  if (diff === 1) return `Yesterday ${time}`;
  return `${pad(d.getDate())}.${pad(d.getMonth() + 1)}.${String(d.getFullYear()).slice(2)}, ${time}`;
}

/** Title of a revision side in a diff. */
export function revisionTitle(r: { time: number; name: string; kind: string }, state: StateKind): string {
  const when = historyDate(r.time);
  const what = r.kind === "label" ? `Label "${r.name}"` : r.name;
  const base = what ? `${when} · before ${what}`.replace("· before Label", "· at label") : when;
  if (state === "absent") return `${base} (file did not exist)`;
  if (state === "unknown") return `${base} (content not recorded)`;
  return base;
}

/** The text of a file at a revision; `""` when it did not exist or is not recorded. */
export async function revisionText(path: string, rev: number): Promise<{ state: StateKind; text: string }> {
  const c = await localHistoryApi.content(path, rev);
  if (c.state === "current") {
    const now = await readText(path);
    return { state: now === null ? "absent" : "file", text: now ?? "" };
  }
  return { state: c.state, text: c.text ?? "" };
}

/** The file now: the editor's text when it is open, else the file on disk. */
export async function currentText(path: string): Promise<{ exists: boolean; text: string }> {
  const now = await readText(path);
  return { exists: now !== null, text: now ?? "" };
}

// ---- opening the views ----------------------------------------------------------------------

let openTab: (target: HistoryTarget) => void = () => undefined;

/** Installed by the view module (keeps this module free of JSX). */
export function setHistoryTabOpener(open: (target: HistoryTarget) => void): void {
  openTab = open;
}

export function tabIdOf(t: HistoryTarget): string {
  switch (t.kind) {
    case "file":
    case "folder":
      return `forge:local-history:${t.path}`;
    case "selection":
      return `forge:local-history-selection:${t.path}:${t.start}-${t.end}`;
    case "change":
      return `forge:recent-change:${t.from}-${t.to}`;
  }
}

export function tabTitleOf(t: HistoryTarget): string {
  const project = workspace()?.name ?? "Project";
  switch (t.kind) {
    case "file":
    case "folder":
      return `Local History: ${t.path ? nameOf(t.path) : project}`;
    case "selection":
      return `History for Selection: ${nameOf(t.path)}`;
    case "change":
      return `Recent Change: ${t.name}`;
  }
}

export function openHistoryTab(target: HistoryTarget): void {
  openTab(target);
}

/** Show History (LocalHistory.ShowHistory) of the selected file or folder, or the editor's file. */
export function showHistoryOfTarget(): void {
  const [target] = vcsTargets();
  if (!target) {
    if (workspace()) openHistoryTab({ kind: "folder", path: "" });
    return;
  }
  openHistoryTab(target.dir ? { kind: "folder", path: target.path } : { kind: "file", path: target.path });
}

/** Show History for Selection (LocalHistory.ShowSelectionHistory): the selected lines of the editor. */
export function showSelectionHistory(): void {
  const tab = activeTab();
  const view = activeView();
  if (!tab || !view || tab.view || tab.path.includes("://")) return;
  const sel = view.state.selection.main;
  const first = view.state.doc.lineAt(sel.from).number;
  const lastLine = view.state.doc.lineAt(sel.to);
  // A selection ending at the start of a line does not include that line.
  const last = sel.to > sel.from && lastLine.from === sel.to ? Math.max(first, lastLine.number - 1) : lastLine.number;
  openHistoryTab({ kind: "selection", path: tab.path, start: first, end: last });
}

/** Recent Changes popup state. */
const [recentChangesOpen, setRecentChangesOpen] = createSignal(false);
export { recentChangesOpen, setRecentChangesOpen };

// ---- labels and revisions -------------------------------------------------------------------

/** Put Label… (LocalHistory.PutLabel): a named point in time for the whole project. */
export async function putLabel(): Promise<void> {
  if (!workspace()) return;
  const name = await askName({
    title: "Put Label",
    label: "Label name:",
    initial: "",
    confirm: "OK",
    validate: (n) => (n.trim() ? null : "Enter a label name"),
  });
  if (!name?.trim()) return;
  try {
    // Unsaved edits belong before the label.
    await saveAll("plain");
    await localHistoryApi.putLabel(name.trim());
    notifyInfo(tr("tools.localHistory.labelWasPut", { name: name.trim() }));
  } catch (e) {
    notifyError(tr("tools.localHistory.putLabelFailed"), e);
  }
}

/** The name the revert is recorded under. */
function revertName(r: { time: number; name: string; kind: string }): string {
  const when = historyDate(r.time);
  if (r.kind === "label") return `Revert to label "${r.name}"`;
  return `Revert to ${when}${r.name ? ` (before ${r.name})` : ""}`;
}

async function runRevert(target: LhRevertTarget, name: string): Promise<boolean> {
  try {
    await saveAll("plain");
    const result = await localHistoryApi.revert(target, name);
    if (result.changed.length) {
      await reloadOpenFiles(result.changed);
      refreshDirs([...new Set(result.changed.map(parentOf))]);
    }
    if (result.failed.length) notifyError(tr("tools.localHistory.revertFailedForSomeFiles"), result.failed.join(", "));
    else if (result.skipped.length) notifyInfo(tr("tools.localHistory.someFilesWereNotReverted"), tr("tools.localHistory.theirContentWasNotRecorded", { skipped: result.skipped.join(", ") }));
    else if (!result.changed.length) notifyInfo(tr("tools.localHistory.nothingToRevert"), tr("tools.localHistory.theFilesAreAlreadyAs"));
    else notifyInfo(name, tr("tools.localHistory.filesReverted", { count: result.changed.length }));
    return true;
  } catch (e) {
    notifyError(tr("tools.localHistory.revertFailed"), e);
    return false;
  }
}

/** Revert a file, or a folder (asks first), to a revision. */
export async function revertTo(target: Extract<HistoryTarget, { kind: "file" | "folder" }>, rev: LhRevision): Promise<void> {
  if (target.kind === "folder") {
    const changes = await localHistoryApi.folderChanges(target.path, rev.id);
    if (!changes.length) return notifyInfo(tr("tools.localHistory.nothingToRevert"), tr("tools.localHistory.theFilesAreAlreadyAs"));
    const ok = await askConfirm({
      title: "Revert",
      message: `Revert ${changes.length} file${changes.length === 1 ? "" : "s"} in ${target.path || workspace()?.name || "the project"} to ${historyDate(rev.time)}? Files created since are deleted, deleted files are recreated.`,
      confirm: "Revert",
    });
    if (!ok) return;
    await runRevert({ kind: "folder", path: target.path, rev: rev.id }, revertName(rev));
    return;
  }
  await runRevert({ kind: "file", path: target.path, revPath: rev.path, rev: rev.id }, revertName(rev));
}

/** Revert a Recent Changes entry: its files get the content they had before it. */
export async function revertChange(t: { from: number; to: number; name: string; time: number; kind?: string }): Promise<void> {
  if (t.kind === "label") {
    const ok = await askConfirm({
      title: "Revert to Label",
      message: `Revert the whole project to label "${t.name}"? Files created since are deleted, deleted files are recreated.`,
      confirm: "Revert",
    });
    if (ok) await runRevert({ kind: "folder", path: "", rev: t.from }, `Revert to label "${t.name}"`);
    return;
  }
  await runRevert({ kind: "change", from: t.from, to: t.to }, `Revert of ${t.name}`);
}

/** Show Diff in the diff viewer: the file at the revision against the file now. */
export async function showRevisionDiff(path: string, revPath: string, r: LhRevision | { id: number; time: number; name: string; kind: string }): Promise<void> {
  const [then, now] = await Promise.all([revisionText(revPath, r.id), currentText(path)]);
  setDiffRequest({
    path,
    left: { title: revisionTitle(r, then.state), content: then.text },
    right: { title: now.exists ? "Current version" : "Current version (deleted)", content: now.text },
    editable: now.exists,
  });
}

/** Create Patch: the changes since the revision, saved to a file or copied. */
export async function createPatch(target: Extract<HistoryTarget, { kind: "file" | "folder" }>, rev: LhRevision): Promise<void> {
  let patch: string;
  try {
    patch = await localHistoryApi.patch(target.path, target.kind === "folder", rev.path, rev.id);
  } catch (e) {
    return notifyError(tr("tools.localHistory.createPatchFailed"), e);
  }
  if (!patch.trim()) return notifyInfo(tr("tools.localHistory.noChangesToCreateA"));
  const how = await askChoice({
    title: "Create Patch",
    message: `Patch of the changes since ${historyDate(rev.time)}${rev.name ? ` (${rev.name})` : ""}.`,
    choices: [
      { id: "clipboard", label: "Copy to Clipboard" },
      { id: "file", label: "Save to File…" },
    ],
  });
  if (!how) return;
  try {
    if (how === "clipboard") {
      await navigator.clipboard.writeText(patch);
      return notifyInfo(tr("tools.localHistory.patchCopiedToClipboard"));
    }
    const base = `${nameOf(target.path || workspace()?.name || "project")}-local-history.patch`;
    const file = isTauri()
      ? await chooseSavePath(base)
      : await askName({ title: "Save Patch", label: "File (project-relative):", initial: base, confirm: "Save" });
    if (!file) return;
    await changesApi.writePatch(file, patch);
    notifyInfo(tr("tools.localHistory.patchCreated"), file);
  } catch (e) {
    notifyError(tr("tools.localHistory.createPatchFailed"), e);
  }
}
