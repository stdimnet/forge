/**
 * Get from VCS (Git ▸ Clone…, VCS ▸ Get from Version Control…), worktrees and Commit Message
 * History (Ctrl+M in the commit message) as commands with PHPStorm action ids, and the host of
 * the message history popup.
 */
import { render } from "solid-js/web";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { registerMainMenuEntries } from "@/features/mainMenu";
import { action } from "@/features/mainMenu/model";
import { showToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { MessageHistoryHost } from "./ExtrasDialogs";
import { newWorktreeDialog, openCloneDialog, showMessageHistory } from "./extras";
import { setActiveGitTab } from "./history";
import { registerObjectSettingEditor } from "@/features/settings/SchemaPage";
import { SIGN_KEY, SigningRow } from "./SigningSettings";

registerKeymapActions([
  { id: "Git.Clone", title: "Clone…", group: "Main Menu/Git", shortcuts: {} },
  { id: "Vcs.Checkout", title: "Get from Version Control…", group: "Main Menu/VCS", shortcuts: {} },
  { id: "Git.Worktree.Create", title: "New Worktree…", group: "Main Menu/Git", shortcuts: {} },
  { id: "Git.Show.Worktrees", title: "Show Worktrees", group: "Main Menu/Git", shortcuts: {} },
]);

const commands: { id: string; title: string; run: () => void | Promise<void> }[] = [
  { id: "Git.Clone", title: "Clone…", run: () => openCloneDialog() },
  { id: "Vcs.Checkout", title: "Get from Version Control…", run: () => openCloneDialog() },
  { id: "Vcs.ShowMessageHistory", title: "Show Commit Message History", run: showMessageHistory },
  { id: "Git.Worktree.Create", title: "New Worktree…", run: () => newWorktreeDialog() },
  {
    id: "Git.Show.Worktrees",
    title: "Show Worktrees",
    run: () => {
      setActiveGitTab("worktrees");
      showToolWindow("git");
    },
  },
];
for (const c of commands) registerCommand({ ...c, category: "Git" });

// Settings | Version Control | Git: "Sign commits with GPG/SSH key" with Configure GPG Key….
registerObjectSettingEditor(SIGN_KEY, SigningRow);

registerMainMenuEntries("Git", [action("Git.Worktree.Create", "New _Worktree...")], { after: "Git.CreateNewBranch", when: () => !!workspace() });

if (typeof window !== "undefined") {
  const host = document.createElement("div");
  host.id = "git-message-history";
  document.body.append(host);
  render(MessageHistoryHost, host);
}
