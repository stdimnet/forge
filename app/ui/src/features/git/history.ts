/**
 * Show History and Show History for Selection: the history tabs of the Git tool window and
 * the actions of a revision in them (Show Diff, Compare with Local, Get, Annotate Revision,
 * Checkout Revision, Copy Revision Number).
 */
import { createSignal } from "solid-js";
import { openFile, renameTab } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { workspaceApi } from "@/ipc/workspace";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { readText } from "@/features/compare/state";
import { gitApi, type ChangedFile } from "./api";
import { checkout } from "./actions";
import { notify, reloadOpenFiles, repositoryChanged } from "./store";
import { setDiffRequest, toggleAnnotate } from "./overlays";
import { revisionPath } from "./revisionFiles";
import { t as tr } from "@/i18n";
import { writeClipboardText } from "@/features/clipboard/system";

export const shortSha = (sha: string) => sha.slice(0, 8);
const nameOf = (path: string) => path.split("/").pop() || path;

/** A history tab: a file or folder (Show History) or lines of a file (Show History for Selection). */
export interface HistoryTab {
  id: string;
  kind: "file" | "selection";
  /** Workspace-relative; `""` is the project root. */
  path: string;
  dir: boolean;
  /** Selection history: 1-based lines as in HEAD. */
  start?: number;
  end?: number;
}

const [historyTabs, setHistoryTabs] = createSignal<HistoryTab[]>([]);
/** `"log"` or the id of a history tab. */
const [activeGitTab, setActiveGitTab] = createSignal<string>("log");
export { historyTabs, activeGitTab, setActiveGitTab };

export function historyTabTitle(tab: HistoryTab, projectName = "Project"): string {
  const name = tab.path ? nameOf(tab.path) : projectName;
  return tab.kind === "selection" ? tr("git.history.historyForSelectionName", { name }) : tr("git.history.historyName", { name });
}

function openTab(tab: HistoryTab): void {
  setHistoryTabs((tabs) => (tabs.some((t) => t.id === tab.id) ? tabs.map((t) => (t.id === tab.id ? tab : t)) : [...tabs, tab]));
  setActiveGitTab(tab.id);
  showToolWindow("git");
}

/** Show History (Vcs.ShowTabbedFileHistory) of a file or folder. */
export function openFileHistory(path: string, dir: boolean): void {
  openTab({ id: `history:${path}`, kind: "file", path, dir });
}

/** Show History for Selection (Vcs.ShowHistoryForBlock): lines `start..end` (1-based). */
export function openSelectionHistory(path: string, start: number, end: number): void {
  openTab({ id: `selection:${path}:${start}-${end}`, kind: "selection", path, dir: false, start, end });
}

export function closeHistoryTab(id: string): void {
  const tabs = historyTabs();
  const at = tabs.findIndex((t) => t.id === id);
  setHistoryTabs(tabs.filter((t) => t.id !== id));
  if (activeGitTab() === id) setActiveGitTab(tabs[at - 1]?.id ?? "log");
}

// ---- revision actions ---------------------------------------------------------------------

/** A file in a revision: the commit, its first parent and what the commit did to the file. */
export interface RevisionFile {
  sha: string;
  parent: string | undefined;
  file: ChangedFile;
}

/** Path of the file before the commit (renames), `null` when the commit added it. */
const beforePath = (r: RevisionFile) => (r.file.status === "A" || !r.parent ? null : (r.file.origPath ?? r.file.path));

/** The file before and after the commit. */
export async function revisionTexts(r: RevisionFile): Promise<{ before: string; after: string }> {
  const from = beforePath(r);
  const [before, after] = await Promise.all([
    from && r.parent ? gitApi.show(r.parent, from).catch(() => null) : Promise.resolve(null),
    r.file.status === "D" ? Promise.resolve(null) : gitApi.show(r.sha, r.file.path).catch(() => null),
  ]);
  return { before: before ?? "", after: after ?? "" };
}

/** Show Diff (Ctrl+D): the file's change in the commit. */
export async function showRevisionChange(r: RevisionFile): Promise<void> {
  const { before, after } = await revisionTexts(r);
  const from = beforePath(r);
  setDiffRequest({
    path: r.file.path,
    left: { title: from && r.parent ? `${shortSha(r.parent)}${from !== r.file.path ? ` (${from})` : ""}` : tr("git.history.emptyFileAdded"), content: before },
    right: { title: r.file.status === "D" ? `${shortSha(r.sha)} (file deleted)` : shortSha(r.sha), content: after },
    editable: false,
  });
}

/** Compare with Local: the revision against the working copy of `local`. */
export async function compareRevisionWithLocal(r: RevisionFile, local: string): Promise<void> {
  const [then, now] = await Promise.all([gitApi.show(r.sha, r.file.path).catch(() => null), readText(local)]);
  setDiffRequest({
    path: local,
    left: { title: then === null ? `${shortSha(r.sha)} (file does not exist)` : `${shortSha(r.sha)} (Read-only)`, content: then ?? "" },
    right: { title: now === null ? tr("git.history.deleted") : tr("git.history.yourVersion"), content: now ?? "" },
    editable: now !== null,
  });
}

/** Get: replace the working copy of `local` with the file as it was in the revision. */
export async function getRevision(r: RevisionFile, local: string): Promise<void> {
  const text = await gitApi.show(r.sha, r.file.path).catch(() => null);
  if (text === null) return notify("info", tr("tools.git.doesNotExistInRevision", { value: nameOf(local), value2: shortSha(r.sha) }));
  const ok = await askConfirm({
    title: tr("git.history.getRevision"),
    message: tr("git.history.replaceTheContentOfLocal", { local, sha: shortSha(r.sha) }),
    confirm: tr("git.history.get"),
  });
  if (!ok) return;
  try {
    await workspaceApi.writeFile(local, text);
  } catch (e) {
    return notify("error", tr("git.history.getFailed"), String(e instanceof Error ? e.message : e));
  }
  await reloadOpenFiles([local]);
  await repositoryChanged();
  notify("success", tr("git.history.gotRevisionShaOfName", { sha: shortSha(r.sha), name: nameOf(local) }));
}

/** Annotate Revision: the file as it was in the revision, read-only, with its blame. */
export async function annotateRevision(r: RevisionFile): Promise<void> {
  const sha = r.file.status === "D" && r.parent ? r.parent : r.sha;
  const path = revisionPath(sha, r.file.status === "D" ? (beforePath(r) ?? r.file.path) : r.file.path);
  toggleAnnotate(path, true);
  try {
    await openFile(path);
  } catch (e) {
    return notify("error", tr("git.history.annotateFailed"), String(e instanceof Error ? e.message : e));
  }
  renameTab(path, `${nameOf(r.file.path)} (${shortSha(sha)})`);
}

/** Checkout Revision: a detached HEAD at the commit. */
export const checkoutRevision = (sha: string) => checkout(sha);

/** Copy Revision Number: the full hash. */
export async function copyRevisionNumber(sha: string): Promise<void> {
  try {
    await writeClipboardText(sha);
    notify("info", tr("git.history.copiedSha", { sha: shortSha(sha) }));
  } catch {
    notify("error", tr("git.history.clipboardIsNotAvailable"));
  }
}
