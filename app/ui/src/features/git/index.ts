/**
 * Git integration: Commit (Alt+0) and Git (Alt+9) tool windows, branches popup, diff
 * viewer, blame annotations, gutter change markers and VCS colors for tree and tabs.
 */
import { render } from "solid-js/web";
import { GitCommitHorizontal, Layers } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { registerEditorExtension } from "@/editor/extensions";
import { registerContextMenuProvider } from "@/features/contextMenu/state";
import type { FileEntry } from "@/ipc/workspace";
import { showToolWindow } from "@/state/layout";
import { CommitTabs, CommitToolWindow } from "./StashShelfPanels";
import { GitLogPanel } from "./GitLogPanel";
import { GitOverlays } from "./GitOverlays";
import { gitEditorExtensions, showChangeMarker } from "./editorGit";
import { editor } from "@/state/editor";
import { viewFor } from "@/editor/views";
import { callWasmSync, forgeWasm } from "@/wasm";
import { branchesOpen, setBranchesOpen } from "./overlays";
import { repoAvailable, startGitStore, status } from "./store";
import { fetchAll, newBranchDialog, openCommitWindow, push, updateProject } from "./actions";
import { gitSubmenu } from "./fileMenu";
import { activeMerge, resolveConflicts, watchConflicts } from "./conflicts";
import { conflictBanner } from "./conflictBanner";
import {
  addTargets,
  annotateTarget,
  commitTargets,
  compareWithBranch,
  compareWithRevision,
  ignoreTargets,
  manageRemotes,
  newTagDialog,
  rollbackTargets,
  showSelectionHistory,
  showTargetHistory,
  showTargetsDiff,
  stashDialog,
  unstashDialog,
} from "./fileActions";
import { registerCompareHandler } from "@/features/compare/state";
import "./revisionFiles";
import "./opsIndex";
import "./changesIndex";
import "./logIndex";
import "./git.css";

registerToolWindow({
  id: "commit",
  title: "Commit",
  icon: GitCommitHorizontal,
  anchor: "left",
  order: 10,
  shortcut: "Alt+0",
  component: CommitToolWindow,
  headerContent: CommitTabs,
});

registerToolWindow({
  id: "git",
  title: "Git",
  icon: Layers,
  anchor: "bottom",
  order: 60,
  shortcut: "Alt+9",
  component: GitLogPanel,
});

registerEditorExtension(({ path }) => gitEditorExtensions(path));
registerEditorExtension(({ path }) => conflictBanner(path));

const commands: { id: string; title: string; run: () => void | Promise<void> }[] = [
  { id: "git.commit", title: "Commit…", run: openCommitWindow },
  { id: "git.branches", title: "Branches…", run: () => setBranchesOpen(!branchesOpen()) },
  { id: "git.annotate", title: "Annotate with Git Blame", run: annotateTarget },
  { id: "git.showDiff", title: "Show Diff", run: showTargetsDiff },
  // File actions of the Project view Git submenu; they act on `vcsTargets()`.
  { id: "CheckinFiles", title: "Commit File…", run: commitTargets },
  { id: "Git.Add", title: "Add", run: addTargets },
  { id: "Git.Ignore.File", title: "Add to .gitignore", run: ignoreTargets },
  { id: "Compare.Selected", title: "Compare with Revision…", run: compareWithRevision },
  { id: "Git.CompareWithBranch", title: "Compare with Branch…", run: compareWithBranch },
  { id: "Vcs.ShowTabbedFileHistory", title: "Show History", run: showTargetHistory },
  { id: "Vcs.ShowHistoryForBlock", title: "Show History for Selection", run: showSelectionHistory },
  { id: "ChangesView.Rollback", title: "Rollback…", run: rollbackTargets },
  { id: "Git.Tag", title: "New Tag…", run: newTagDialog },
  { id: "Git.Stash", title: "Stash Changes…", run: stashDialog },
  { id: "Git.Unstash", title: "Unstash Changes…", run: unstashDialog },
  { id: "Git.Configure.Remotes", title: "Manage Remotes…", run: manageRemotes },
  { id: "git.push", title: "Push…", run: push },
  { id: "git.update", title: "Update Project…", run: updateProject },
  { id: "git.fetch", title: "Fetch", run: fetchAll },
  { id: "git.newBranch", title: "New Branch…", run: () => newBranchDialog(null) },
  { id: "git.log", title: "Show Git Log", run: () => showToolWindow("git") },
];
for (const c of commands) registerCommand({ ...c, category: "Git" });

// Ctrl+D in the Commit window: the diff of the selected change.
registerCompareHandler(".commit-panel", showTargetsDiff);
// Merge conflicts: Git ▸ Resolve Conflicts… and the merge window's actions ("diff" focus context).
registerCommand({ id: "Git.ResolveConflicts", title: "Resolve Conflicts…", category: "Git", run: resolveConflicts });
const mergeCommands: { id: string; title: string; run: () => void }[] = [
  { id: "NextDiff", title: "Next Difference", run: () => activeMerge()?.step(true) },
  { id: "PreviousDiff", title: "Previous Difference", run: () => activeMerge()?.step(false) },
  { id: "Diff.ApplyNonConflicts.Left", title: "Apply Non-Conflicting Changes from the Left Side", run: () => activeMerge()?.applyAll("left") },
  { id: "Diff.ApplyNonConflicts", title: "Apply All Non-Conflicting Changes", run: () => activeMerge()?.applyAll("all") },
  { id: "Diff.ApplyNonConflicts.Right", title: "Apply Non-Conflicting Changes from the Right Side", run: () => activeMerge()?.applyAll("right") },
  { id: "Diff.MagicResolveConflicts", title: "Resolve Simple Conflicts", run: () => activeMerge()?.magic() },
];
for (const c of mergeCommands) registerCommand({ ...c, category: "Diff" });

/** Next Change / Previous Change: the hunk order comes from `forge_editing::change_target`. */
async function gotoChange(forward: boolean): Promise<void> {
  const path = editor.active;
  const view = path ? viewFor(path) : undefined;
  if (!path || !view) return;
  const w = await forgeWasm();
  showChangeMarker(view, path, (ranges, line) => callWasmSync<number | null>(w.editor_change_target, { ranges, line, forward }));
}
registerCommand({ id: "VcsShowNextChangeMarker", title: "Next Change", category: "Navigate", run: () => gotoChange(true) });
registerCommand({ id: "VcsShowPrevChangeMarker", title: "Previous Change", category: "Navigate", run: () => gotoChange(false) });

// Project view: the Git submenu, only when the project is a Git repository.
registerContextMenuProvider<FileEntry | null>("project", (entry) => {
  const st = status();
  if (!repoAvailable() || !st) return [];
  const target = entry ? { path: entry.path, dir: entry.kind === "dir", ignored: !!entry.ignored } : { path: "", dir: true };
  return [gitSubmenu(target, st.entries)];
});

if (typeof window !== "undefined") {
  const host = document.createElement("div");
  host.id = "git-overlays";
  document.body.append(host);
  render(GitOverlays, host);
  startGitStore();
  watchConflicts();
}

export { status as gitStatus };
