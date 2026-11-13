/**
 * The Git submenu of the Project view context menu, in PHPStorm's order and wording. Items
 * run PHPStorm actions with the clicked file or folder as their context and are greyed out
 * when the file's Git status does not allow them.
 */
import { item, separator, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { runAction } from "@/keymap";
import type { StatusEntry } from "./api";
import { targetState, withVcsTargets, type VcsTarget } from "./context";
import { t } from "@/i18n";

export function gitSubmenu(target: VcsTarget, entries: StatusEntry[]): MenuEntry {
  const s = targetState([target], entries);
  const repoHasChanges = entries.some((e) => e.worktree !== "untracked" && e.worktree !== "ignored");
  const entry = (label: string, action: string, enabled: boolean, shortcutAction?: string): MenuEntry =>
    item(label, () => void withVcsTargets([target], () => runAction(action)), action, { disabled: !enabled, shortcutAction });
  return submenu("Git", [
    entry(target.dir ? t("git.fileMenu.commitDirectory") : t("git.fileMenu.commitFile"), "CheckinFiles", s.changed.length + s.unversioned.length > 0, "CheckinProject"),
    entry("Add", "Git.Add", s.unversioned.length > 0),
    entry("Add to .gitignore", "Git.Ignore.File", s.allUnversioned),
    separator,
    entry("Annotate with Git Blame", "Annotate", s.fileTracked),
    entry("Show Diff", "Compare.SameVersion", s.changed.length > 0, "CompareTwoFiles"),
    entry("Compare with Revision…", "Compare.Selected", s.fileTracked),
    entry("Compare with Branch…", "Git.CompareWithBranch", target.dir ? !s.ignored : s.fileTracked),
    entry("Show History", "Vcs.ShowTabbedFileHistory", target.dir ? !s.ignored : s.fileTracked),
    separator,
    entry("Rollback…", "ChangesView.Rollback", s.changed.length > 0),
    separator,
    entry("Push…", "Vcs.Push", true),
    entry("Pull…", "Git.Pull", true),
    entry("Fetch", "Git.Fetch", true),
    separator,
    entry("Branches…", "Git.Branches", true),
    entry("New Branch…", "Git.CreateNewBranch", true),
    entry("New Tag…", "Git.Tag", true),
    separator,
    entry("Stash Changes…", "Git.Stash", repoHasChanges),
    entry("Unstash Changes…", "Git.Unstash", true),
    separator,
    entry("Manage Remotes…", "Git.Configure.Remotes", true),
  ]);
}
