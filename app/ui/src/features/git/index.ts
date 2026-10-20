/**
 * Git integration: Commit (Alt+0) and Git (Alt+9) tool windows, branches popup, diff
 * viewer, blame annotations, gutter change markers and VCS colors for tree and tabs.
 */
import { render } from "solid-js/web";
import { GitCommitHorizontal, Layers } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerCommand, runCommand } from "@/state/commands";
import { registerEditorExtension } from "@/editor/extensions";
import { activeTab } from "@/state/editor";
import { layout, showToolWindow, toggleToolWindow } from "@/state/layout";
import { CommitPanel } from "./CommitPanel";
import { GitLogPanel } from "./GitLogPanel";
import { GitOverlays } from "./GitOverlays";
import { gitEditorExtensions } from "./editorGit";
import { branchesOpen, setBranchesOpen, toggleAnnotate } from "./overlays";
import { startGitStore, status } from "./store";
import { fetchAll, newBranchDialog, openCommitWindow, push, showDiff, updateProject } from "./actions";
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
  {
    id: "git.annotate",
    title: "Annotate with Git Blame",
    run: () => {
      const tab = activeTab();
      if (tab) toggleAnnotate(tab.path);
    },
  },
  {
    id: "git.showDiff",
    title: "Show Diff",
    run: () => {
      const tab = activeTab();
      if (tab) return showDiff(tab.path);
    },
  },
  { id: "git.push", title: "Push…", run: push },
  { id: "git.update", title: "Update Project…", run: updateProject },
  { id: "git.fetch", title: "Fetch", run: fetchAll },
  { id: "git.newBranch", title: "New Branch…", run: () => newBranchDialog(null) },
  { id: "git.log", title: "Show Git Log", run: () => showToolWindow("bottom", "git") },
];
for (const c of commands) registerCommand({ ...c, category: "Git" });

/** PHPStorm keymap for VCS actions. */
function onKeyDown(e: KeyboardEvent): void {
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  let id: string | null = null;
  if (mod && !e.altKey && !e.shiftKey && key === "k") id = "git.commit";
  else if (mod && e.shiftKey && !e.altKey && key === "k") id = "git.push";
  else if (mod && !e.shiftKey && !e.altKey && key === "t") id = "git.update";
  else if (mod && e.altKey && !e.shiftKey && key === "n") id = "git.newBranch";
  else if (e.altKey && !mod && (e.code === "Digit0" || e.code === "Digit9")) {
    e.preventDefault();
    const [anchor, tw] = e.code === "Digit0" ? (["left", "commit"] as const) : (["bottom", "git"] as const);
    if (layout.active[anchor] === tw) toggleToolWindow(anchor, tw);
    else if (tw === "commit") openCommitWindow();
    else showToolWindow(anchor, tw);
    return;
  }
  if (id) {
    e.preventDefault();
    void runCommand(id);
  }
}

if (typeof window !== "undefined") {
  window.addEventListener("keydown", onKeyDown);
  const host = document.createElement("div");
  host.id = "git-overlays";
  document.body.append(host);
  render(GitOverlays, host);
  startGitStore();
}

export { status as gitStatus };
