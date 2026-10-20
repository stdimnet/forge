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
import { CommitPanel } from "./CommitPanel";
import { GitLogPanel } from "./GitLogPanel";
import { GitOverlays } from "./GitOverlays";
import { gitEditorExtensions } from "./editorGit";
import { branchesOpen, setBranchesOpen } from "./overlays";
import { repoAvailable, startGitStore, status } from "./store";
import { fetchAll, newBranchDialog, openCommitWindow, push, updateProject } from "./actions";
import { gitSubmenu } from "./fileMenu";
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
  showTargetHistory,
  showTargetsDiff,
  stashDialog,
  unstashDialog,
} from "./fileActions";
import "./git.css";

registerToolWindow({
  id: "commit",
  title: "Commit",
  icon: GitCommitHorizontal,
  anchor: "left",
  order: 10,
  shortcut: "Alt+0",
  component: CommitPanel,
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
  { id: "ChangesView.Rollback", title: "Rollback…", run: rollbackTargets },
  { id: "Git.Tag", title: "New Tag…", run: newTagDialog },
  { id: "Git.Stash", title: "Stash Changes…", run: stashDialog },
  { id: "Git.Unstash", title: "Unstash Changes…", run: unstashDialog },
  { id: "Git.Configure.Remotes", title: "Manage Remotes…", run: manageRemotes },
  { id: "git.push", title: "Push…", run: push },
  { id: "git.update", title: "Update Project…", run: updateProject },
  { id: "git.fetch", title: "Fetch", run: fetchAll },
  { id: "git.newBranch", title: "New Branch…", run: () => newBranchDialog(null) },
  { id: "git.log", title: "Show Git Log", run: () => showToolWindow("bottom", "git") },
];
for (const c of commands) registerCommand({ ...c, category: "Git" });

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
}

export { status as gitStatus };
