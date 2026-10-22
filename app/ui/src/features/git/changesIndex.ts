/**
 * Changelists and patches: commands (PHPStorm action ids, Move to Another Changelist on F6
 * in the Commit tool window) for the Commit tool window, Git ▸ Patch and Find Action.
 */
import { registerCommand } from "@/state/commands";
import { runAction } from "@/keymap/dispatch";
import { changelists } from "./changelists";
import {
  applyPatchDialog,
  applyPatchFromClipboard,
  createPatchDialog,
  deleteChangelist,
  editChangelistDialog,
  newChangelistDialog,
  setActiveChangelist,
} from "./changesDialogs";
import { moveSelectionToChangelist } from "./CommitPanel";
import { status } from "./store";
import { vcsTargets } from "./context";

const inCommitPanel = () => !!document.activeElement?.closest(".commit-panel");

/** The changelist of the Commit window's selection, else the active one. */
const currentList = () => {
  const node = document.querySelector<HTMLElement>(".commit-panel .tree-row.selected[data-changelist]");
  return changelists().find((l) => l.id === node?.dataset.changelist) ?? changelists().find((l) => l.active);
};

const commands: { id: string; title: string; run: () => void | Promise<void> }[] = [
  {
    id: "ChangesView.Move",
    title: "Move to Another Changelist…",
    // F6 is also Move (refactoring) everywhere else.
    run: () => (inCommitPanel() ? void moveSelectionToChangelist() : runAction("Move")),
  },
  { id: "ChangesView.NewChangeList", title: "New Changelist…", run: () => newChangelistDialog() },
  {
    id: "ChangesView.Rename",
    title: "Edit Changelist…",
    run: () => {
      const l = currentList();
      if (l) editChangelistDialog(l);
    },
  },
  {
    id: "ChangesView.RemoveChangeList",
    title: "Delete Changelist",
    run: () => {
      const l = currentList();
      if (l) void deleteChangelist(l);
    },
  },
  {
    id: "ChangesView.SetDefault",
    title: "Set Active Changelist",
    run: () => {
      const l = currentList();
      if (l) void setActiveChangelist(l);
    },
  },
  {
    id: "ChangesView.CreatePatch",
    title: "Create Patch…",
    run: () => {
      // The selection's changes, else all local changes.
      const entries = (status()?.entries ?? []).filter((e) => !e.conflict && e.worktree !== "ignored");
      const targets = vcsTargets();
      const picked = targets.length
        ? entries.filter((e) => targets.some((t) => (t.dir ? t.path === "" || e.path.startsWith(`${t.path}/`) : e.path === t.path)))
        : entries.filter((e) => e.worktree !== "untracked");
      createPatchDialog(picked.map((e) => e.path), currentList()?.name ?? "Changes");
    },
  },
  { id: "ChangesView.ApplyPatch", title: "Apply Patch…", run: applyPatchDialog },
  { id: "ChangesView.ApplyPatchFromClipboard", title: "Apply Patch from Clipboard…", run: applyPatchFromClipboard },
];
for (const c of commands) registerCommand({ ...c, category: "Git" });
