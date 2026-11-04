/**
 * Git actions on files and folders (Project view Git submenu, Git > Current File) and the
 * repository dialogs of that menu: New Tag, Stash, Unstash and Manage Remotes. File actions
 * act on `vcsTargets()`, so the same command serves menus and shortcuts.
 */
import { openFile } from "@/state/editor";
import { gitApi } from "./api";
import { checkOnly } from "./CommitPanel";
import { diffSiblings, targetState, vcsTargets, type VcsTarget } from "./context";
import { historyApi } from "./historyApi";
import { openFileHistory, openSelectionHistory } from "./history";
import { openBranchChanges } from "./BranchChanges";
import { activeView } from "@/features/navigation/editorCommands";
import { activeTab } from "@/state/editor";
import { logDate } from "./format";
import { notify, runGit, status } from "./store";
import { setChooserDialog, toggleAnnotate } from "./overlays";
import { confirmRollback, openCommitWindow, showDiff, showRevisionDiff } from "./actions";
import { t as tr } from "@/i18n";

const stateOf = (targets: VcsTarget[]) => targetState(targets, status()?.entries ?? []);

/** Commit File… / Commit Directory… (CheckinFiles): the Commit window with just these changes. */
export function commitTargets(): void {
  const targets = vcsTargets();
  const s = stateOf(targets);
  openCommitWindow();
  if (targets.length) checkOnly([...s.changed, ...s.unversioned]);
}

/** Add (Git.Add, Ctrl+Alt+A): stage the unversioned files below the targets. */
export async function addTargets(): Promise<void> {
  const { unversioned } = stateOf(vcsTargets());
  if (!unversioned.length) return notify("info", tr("git.fileActions.noUnversionedFilesToAdd"));
  await runGit("Adding files to Git failed", () => gitApi.stage(unversioned));
}

/** Add to .gitignore (Git.Ignore.File): unversioned files and folders only. */
export async function ignoreTargets(): Promise<void> {
  const targets = vcsTargets();
  if (!stateOf(targets).allUnversioned) return notify("info", tr("git.fileActions.onlyUnversionedFilesCanBe"));
  const patterns = await runGit("Adding to .gitignore failed", () => gitApi.addToGitignore(targets.map((t) => t.path)));
  if (patterns) notify("success", tr("git.fileActions.addedToGitignore"), patterns.join("\n"));
}

/** Annotate with Git Blame: one tracked file, opened in the editor. */
export async function annotateTarget(): Promise<void> {
  const { file, fileTracked } = stateOf(vcsTargets());
  if (!file || !fileTracked) return;
  await openFile(file);
  toggleAnnotate(file);
}

/** Show Diff (Compare.SameVersion, Ctrl+D): HEAD vs working copy of the changed files. */
export async function showTargetsDiff(): Promise<void> {
  const s = stateOf(vcsTargets());
  const files = [...s.changed, ...s.unversioned];
  if (!files.length) return notify("info", tr("git.fileActions.noChangesToShow"));
  await showDiff(files[0], diffSiblings(files));
}

/** Compare with Revision… (Compare.Selected): pick a commit that touched the file (renames followed). */
export async function compareWithRevision(): Promise<void> {
  const { file, fileTracked } = stateOf(vcsTargets());
  if (!file || !fileTracked) return;
  const revs = (await runGit("Could not load the file history", () => historyApi.fileHistory(file, false, { limit: 200 }))) ?? [];
  const pathAt = new Map(revs.map((r) => [r.sha, r.files[0]?.path ?? file]));
  setChooserDialog({
    title: `Compare ${file.split("/").pop()} with Revision`,
    empty: "No revisions of this file",
    okLabel: tr("git.fileActions.compare"),
    items: revs.map((c) => ({ id: c.sha, label: c.subject, detail: `${c.sha.slice(0, 8)} · ${c.author} · ${logDate(c.time)}` })),
    onOk: (item) => showRevisionDiff(file, item.id, item.id.slice(0, 8), pathAt.get(item.id)),
  });
}

/**
 * Compare with Branch… (Git.CompareWithBranch): a file on another branch vs the working copy,
 * or for a folder the files that differ between the branch and the working tree.
 */
export async function compareWithBranch(): Promise<void> {
  const [target] = vcsTargets();
  const { file, fileTracked, ignored } = stateOf(vcsTargets());
  const folder = target?.dir && !ignored ? target.path : null;
  if (folder === null && (!file || !fileTracked)) return;
  const name = folder === null ? file!.split("/").pop() : folder.split("/").pop() || "Project";
  const branches = await runGit("Could not load branches", () => gitApi.branches());
  if (!branches) return;
  const items = [
    ...branches.local.filter((b) => !b.current).map((b) => ({ id: b.name, label: b.name, detail: tr("git.fileActions.local") })),
    ...branches.remote.map((b) => ({ id: b.name, label: b.name, detail: tr("git.fileActions.remote") })),
    ...branches.tags.map((b) => ({ id: b.name, label: b.name, detail: tr("git.fileActions.tag") })),
  ];
  setChooserDialog({
    title: tr("git.fileActions.compareNameWithBranch", { name }),
    empty: "No other branches",
    okLabel: tr("git.fileActions.compare"),
    items,
    onOk: (item) => (folder === null ? showRevisionDiff(file!, item.id, item.label) : openBranchChanges(item.id, folder)),
  });
}

/** Show History (Vcs.ShowTabbedFileHistory): a "History: <name>" tab in the Git tool window. */
export function showTargetHistory(): void {
  const [target] = vcsTargets();
  if (!target || target.ignored) return;
  const { fileTracked } = stateOf([target]);
  if (!target.dir && !fileTracked) return notify("info", tr("git.fileActions.theFileIsNotUnder"));
  openFileHistory(target.path, target.dir);
}

/** Show History for Selection (Vcs.ShowHistoryForBlock): the history of the selected lines. */
export function showSelectionHistory(): void {
  const tab = activeTab();
  const view = activeView();
  if (!tab || !view || tab.view || tab.path.includes("://")) return;
  if (!stateOf([{ path: tab.path, dir: false }]).fileTracked) return notify("info", tr("git.fileActions.theFileIsNotUnder"));
  const sel = view.state.selection.main;
  const first = view.state.doc.lineAt(sel.from).number;
  // A selection ending at the start of a line does not include that line.
  const lastLine = view.state.doc.lineAt(sel.to);
  const last = sel.to > sel.from && lastLine.from === sel.to ? Math.max(first, lastLine.number - 1) : lastLine.number;
  openSelectionHistory(tab.path, first, last);
}

/** Rollback… (ChangesView.Rollback, Ctrl+Alt+Z): discard the tracked changes below the targets. */
export function rollbackTargets(): void {
  const { changed } = stateOf(vcsTargets());
  if (!changed.length) return notify("info", tr("git.fileActions.noChangesToRollBack"));
  confirmRollback(changed);
}

// ---- repository dialogs -----------------------------------------------------------------

// New Tag, Stash, Unstash and Manage Remotes are the Git menu dialogs (`gitDialogs.ts`).
export { manageRemotes, newTagDialog, stashDialog, unstashDialog } from "./gitDialogs";
