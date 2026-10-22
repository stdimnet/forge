---
title: "Git menu"
description: "Every entry of the Git menu with its shortcut, availability and what it does."
order: 90
tags: [main menu, reference, git]
---

# Git menu

Commit, push, update, branches, the log and file actions. In projects without Git the menu is called VCS. Open it with ☰ (`Alt+\`) or from the menu bar, see [Main menu](../../user/workbench.md#main-menu).

![Git menu](../../../screenshots/docs-menu-git.png)

**Available**: *yes* — implemented; *not yet* — shown disabled until Forge implements it; *dynamic* — filled at runtime. Shortcuts are the Windows/Linux and macOS defaults.

| Entry | Windows / Linux | macOS | Available | What it does |
| --- | --- | --- | --- | --- |
| Commit… | `Ctrl+K` | `Cmd+K` | yes | Opens the Commit tool window to commit the selected changes. [More](../../user/git.md#commit-tool-window) ([screenshot](../../../screenshots/docs-commit.png)) |
| Push… | `Ctrl+Shift+K` | `Cmd+Shift+K` | yes | Pushes the current branch. [More](../../user/git.md#push-update-and-fetch) |
| Update Project… | `Ctrl+T` | `Cmd+T` | yes | Pulls the current branch. [More](../../user/git.md#push-update-and-fetch) |
| Pull… | — | — | yes | Opens the Pull dialog: remote, branches and options (--rebase, --ff-only, --no-ff, …). [More](../../user/git.md#pull-merge-and-rebase) ([screenshot](../../../screenshots/git-pull-dialog.png)) |
| Fetch | — | — | yes | Fetches all remotes. [More](../../user/git.md#push-update-and-fetch) |
| Merge… | — | — | yes | Opens the Merge dialog: branches and options (--no-ff, --squash, -m, …). [More](../../user/git.md#pull-merge-and-rebase) ([screenshot](../../../screenshots/git-merge-dialog.png)) |
| Rebase… | — | — | yes | Opens the Rebase dialog: upstream or --onto base and options. While a rebase stops, Continue Rebase, Skip Commit and Abort Rebase follow. [More](../../user/git.md#pull-merge-and-rebase) ([screenshot](../../../screenshots/git-rebase-dialog.png)) |
| Continue Rebase / Skip Commit / Abort Rebase | — | — | dynamic | Shown while a rebase is stopped; Abort Merge while a merge is. [More](../../user/git.md#pull-merge-and-rebase) ([screenshot](../../../screenshots/git-rebasing.png)) |
| Resolve Conflicts… | — | — | yes | Opens the Conflicts dialog for the files with merge conflicts. [More](../../user/git.md#merge-conflicts) ([screenshot](../../../screenshots/merge-conflicts-dialog.png)) |
| Branches… | — | — | yes | Opens the Git Branches popup: checkout, new branch, merge, rebase, push, delete. [More](../../user/git.md#branches) ([screenshot](../../../screenshots/docs-branches.png)) |
| New Branch… | — | — | yes | Creates a branch from the current one and optionally checks it out. [More](../../user/git.md#branches) |
| New Tag… | — | — | yes | Creates a tag on a commit, optionally annotated or forced. [More](../../user/git.md#reset-head-tags-and-remotes) ([screenshot](../../../screenshots/git-tag-dialog.png)) |
| Reset HEAD… | — | — | yes | Resets the current branch to a commit: Soft, Mixed, Hard or Keep. [More](../../user/git.md#reset-head-tags-and-remotes) ([screenshot](../../../screenshots/git-reset-dialog.png)) |
| Show Git Log | — | — | yes | Opens the Log tab of the Git tool window. [More](../../user/git.md#log) ([screenshot](../../../screenshots/docs-git-log.png)) |
| **Uncommitted Changes** | | | | submenu |
| Uncommitted Changes › Shelve Changes… | — | — | yes | Shelves the selected changes in Forge's per-project shelf and rolls them back. [More](../../user/git.md#shelf) ([screenshot](../../../screenshots/git-shelve-dialog.png)) |
| Uncommitted Changes › Show Shelf | — | — | yes | Opens the Shelf tab of the Commit tool window. [More](../../user/git.md#shelf) ([screenshot](../../../screenshots/git-shelf-tab.png)) |
| Uncommitted Changes › Stash Changes… | — | — | yes | Stashes the local changes with a message, optionally keeping the index. [More](../../user/git.md#stash) ([screenshot](../../../screenshots/git-stash-dialog.png)) |
| Uncommitted Changes › Unstash Changes… | — | — | yes | Applies, pops, drops or branches a stash; View shows its files. [More](../../user/git.md#stash) ([screenshot](../../../screenshots/git-unstash-dialog.png)) |
| Uncommitted Changes › Rollback… | `Ctrl+Alt+Z` | `Cmd+Alt+Z` | not yet | Not available yet; the entry is disabled. |
| Uncommitted Changes › Show Local Changes | — | — | not yet | Not available yet; the entry is disabled. |
| **Current File** | | | | submenu |
| Current File › Commit File… | — | — | not yet | Not available yet; the entry is disabled. |
| Current File › Add | — | — | not yet | Not available yet; the entry is disabled. |
| Current File › Annotate with Git Blame | — | — | yes | Shows Git blame (author and date per line) in the editor gutter. [More](../../user/git.md#blame) ([screenshot](../../../screenshots/vcs-blame.png)) |
| Current File › Show Diff | — | — | yes | Compares the file with the repository version (Compare with the Same Repository Version). [More](../../user/git.md#diff-viewer) |
| Current File › Compare with Revision… | — | — | yes | Picks a revision of the file and compares it with your version. [More](../../user/git.md#compare-with-revision-and-branch) |
| Current File › Compare with Branch… | — | — | yes | Compares the file with another branch; for a folder, lists the files that differ. [More](../../user/git.md#compare-with-revision-and-branch) |
| Current File › Show History | — | — | yes | Opens a History tab of the file in the Git tool window. [More](../../user/git.md#file-and-folder-history) ([screenshot](../../../screenshots/file-history.png)) |
| Current File › Show History for Selection | — | — | yes | Shows how the selected lines changed, revision by revision. [More](../../user/git.md#history-for-selection) ([screenshot](../../../screenshots/selection-history.png)) |
| Current File › Rollback Lines | `Ctrl+Alt+Z` | `Cmd+Alt+Z` | not yet | Not available yet; the entry is disabled. |
| Manage Remotes… | — | — | yes | Adds, edits and removes remotes. [More](../../user/git.md#reset-head-tags-and-remotes) ([screenshot](../../../screenshots/git-remotes-dialog.png)) |
| Clone… | — | — | not yet | Not available yet; the entry is disabled. |
| VCS Operations… | `` Alt+` `` | `Ctrl+V` | not yet | Not available yet; the entry is disabled. |

In a project without a Git repository the menu is called **VCS** and offers the version control entries that do not need a repository.
