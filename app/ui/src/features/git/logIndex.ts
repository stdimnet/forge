/**
 * Git Log actions as commands and keymap actions (PHPStorm action ids): they act on the
 * selection of the Log tab, like the context menu. The interactive rebase dialog moves
 * commits with Alt+Up / Alt+Down.
 */
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { checkoutRevision, copyRevisionNumber } from "./history";
import { resetDialog } from "./gitDialogs";
import {
  cherryPick,
  createPatch,
  dropCommits,
  editMessage,
  fixupInto,
  goToRelative,
  interactiveRebase,
  revertCommits,
  runLogAction,
  showRepositoryAt,
  squashDialog,
  undoCommit,
} from "./logActions";
import { rebaseDialogActions } from "./LogDialogs";

registerKeymapActions([
  // No default key: F2 in tool windows edits data grid cells.
  { id: "Git.Reword.Commit", title: "Edit Commit Message…", group: "Version Control Systems/Log", context: "toolwindow", shortcuts: {} },
  { id: "Git.Interactive.Rebase.MoveUp", title: "Move Commit Up", group: "Version Control Systems/Interactive Rebase", context: "toolwindow", shortcuts: { $default: ["Alt+Up"] } },
  { id: "Git.Interactive.Rebase.MoveDown", title: "Move Commit Down", group: "Version Control Systems/Interactive Rebase", context: "toolwindow", shortcuts: { $default: ["Alt+Down"] } },
]);

const first = (shas: string[]) => shas[0];

const commands: { id: string; title: string; run: () => void | Promise<void> }[] = [
  { id: "Vcs.CopyRevisionNumberAction", title: "Copy Revision Number", run: () => runLogAction(null, (s) => copyRevisionNumber(s.join(" "))) },
  { id: "Vcs.Log.CreatePatch", title: "Create Patch…", run: () => runLogAction(null, createPatch) },
  { id: "Vcs.CherryPick", title: "Cherry-Pick", run: () => runLogAction("cherryPick", cherryPick) },
  { id: "Git.CheckoutRevision", title: "Checkout Revision", run: () => runLogAction(null, (s) => checkoutRevision(first(s))) },
  { id: "Git.BrowseRepoAtRevision", title: "Show Repository at Revision", run: () => runLogAction(null, (s) => showRepositoryAt(first(s))) },
  { id: "Git.Reset.In.Log", title: "Reset Current Branch to Here…", run: () => runLogAction(null, (s) => resetDialog(first(s))) },
  { id: "Git.Revert.In.Log", title: "Revert Commit", run: () => runLogAction("revert", revertCommits) },
  { id: "Git.Uncommit", title: "Undo Commit…", run: () => runLogAction("undoCommit", (s) => undoCommit(first(s))) },
  { id: "Git.Reword.Commit", title: "Edit Commit Message…", run: () => runLogAction("editMessage", (s, st) => editMessage(first(s), st)) },
  { id: "Git.Fixup.To.Commit", title: "Fixup…", run: () => runLogAction("fixup", (s) => fixupInto(first(s), "fixup")) },
  { id: "Git.Squash.Into.Commit", title: "Squash Into…", run: () => runLogAction("squashInto", (s) => fixupInto(first(s), "squash")) },
  { id: "Git.Squash.Commits", title: "Squash Commits…", run: () => runLogAction("squashCommits", squashDialog) },
  { id: "Git.Drop.Commits", title: "Drop Commits", run: () => runLogAction("dropCommits", dropCommits) },
  { id: "Git.Interactive.Rebase", title: "Interactively Rebase from Here…", run: () => runLogAction("interactiveRebase", (s, st) => interactiveRebase(first(s), st)) },
  { id: "Vcs.Log.GoToChild", title: "Go to Child Commit", run: () => runLogAction(null, (s) => goToRelative(first(s), false)) },
  { id: "Vcs.Log.GoToParent", title: "Go to Parent Commit", run: () => runLogAction(null, (s) => goToRelative(first(s), true)) },
];
for (const c of commands) registerCommand({ ...c, category: "Git" });

registerCommand({ id: "Git.Interactive.Rebase.MoveUp", title: "Move Commit Up", category: "Git", run: () => rebaseDialogActions()?.move(-1) });
registerCommand({ id: "Git.Interactive.Rebase.MoveDown", title: "Move Commit Down", category: "Git", run: () => rebaseDialogActions()?.move(1) });
