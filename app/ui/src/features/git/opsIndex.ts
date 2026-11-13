/**
 * Git menu dialogs and the shelf: commands (PHPStorm action ids), the Git menu entries of an
 * operation in progress, Ctrl+D in the Stash and Shelf tabs and the dialog host.
 */
import { render } from "solid-js/web";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { registerMainMenuEntries } from "@/features/mainMenu";
import { action, separator } from "@/features/mainMenu/model";
import { registerCompareHandler } from "@/features/compare/state";
import { showToolWindow } from "@/state/layout";
import { GitDialogs } from "./GitDialogHost";
import { mergeDialog, pullDialog, rebaseDialog, resetDialog, shelveDialog } from "./gitDialogs";
import { pullIntoDialog, updateBranchDialog } from "./actions";
import { merging, operationAction, rebasing, watchOperation } from "./operation";
import { diffSelectedShelved, diffSelectedStash, showShelfTab, unshelveSelected } from "./StashShelfPanels";

// Branch actions of the branches popup, for Find Action and the keymap (no default shortcut).
registerKeymapActions([
  { id: "Git.Branch.Update", title: "Update Branch…", group: "Main Menu/Git", shortcuts: {} },
  { id: "Git.Branch.Pull.WithMerge", title: "Pull into Current Using Merge…", group: "Main Menu/Git", shortcuts: {} },
  { id: "Git.Branch.Pull.WithRebase", title: "Pull into Current Using Rebase…", group: "Main Menu/Git", shortcuts: {} },
]);

const commands: { id: string; title: string; run: () => void | Promise<void> }[] = [
  { id: "Git.Pull", title: "Pull…", run: pullDialog },
  { id: "Git.Branch.Update", title: "Update Branch…", run: updateBranchDialog },
  { id: "Git.Branch.Pull.WithMerge", title: "Pull into Current Using Merge…", run: () => pullIntoDialog(false) },
  { id: "Git.Branch.Pull.WithRebase", title: "Pull into Current Using Rebase…", run: () => pullIntoDialog(true) },
  { id: "Git.Merge", title: "Merge…", run: mergeDialog },
  { id: "Git.Rebase", title: "Rebase…", run: rebaseDialog },
  { id: "Git.Reset", title: "Reset HEAD…", run: () => resetDialog() },
  { id: "Git.Rebase.Continue", title: "Continue Rebase", run: () => operationAction("continue") },
  { id: "Git.Rebase.Skip", title: "Skip Commit", run: () => operationAction("skip") },
  { id: "Git.Rebase.Abort", title: "Abort Rebase", run: () => operationAction("abort") },
  { id: "Git.Merge.Abort", title: "Abort Merge", run: () => operationAction("abort") },
  { id: "ChangesView.Shelve", title: "Shelve Changes…", run: () => shelveDialog() },
  { id: "Vcs.Show.Shelf", title: "Show Shelf", run: () => showShelfTab(showToolWindow) },
  { id: "ShelveChanges.UnshelveWithDialog", title: "Unshelve…", run: unshelveSelected },
];
for (const c of commands) registerCommand({ ...c, category: "Git" });

// While a rebase or merge stops, the Git menu offers to continue or abort it, like PHPStorm.
registerMainMenuEntries(
  "Git",
  [separator, action("Git.Rebase.Continue", "_Continue Rebase"), action("Git.Rebase.Skip", "S_kip Commit"), action("Git.Rebase.Abort", "_Abort Rebase")],
  { after: "Git.ResolveConflicts", when: rebasing },
);
registerMainMenuEntries("Git", [separator, action("Git.Merge.Abort", "_Abort Merge")], { after: "Git.ResolveConflicts", when: merging });

registerCompareHandler(".stash-panel", diffSelectedStash);
registerCompareHandler(".shelf-panel", diffSelectedShelved);

if (typeof window !== "undefined") {
  // Before the other git overlays, so the diff viewer opened from a dialog shows on top.
  const host = document.createElement("div");
  host.id = "git-dialogs";
  document.body.append(host);
  render(GitDialogs, host);
  watchOperation();
}
