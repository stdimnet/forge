---
title: Git
description: Git integration - the Commit tool window, AI commit messages, diff viewer, change markers, blame, branches, push, update, merge conflicts, the Git log, file and selection history, and comparing files, folders and the clipboard.
order: 120
tags: [git, vcs, commit, diff, blame, branches, merge, conflicts]
---

# Git

The Git plugin (`forge.git`) drives the `git` command line (no libgit2) and works in any project
that is a Git repository (also when the project is a sub-folder of the repository). The
current branch is shown in the main toolbar and the status bar.

## Commit tool window

**Commit…** (`Ctrl+K`, macOS `Cmd+K`) opens the **Commit** tool window (`Alt+0`, left stripe).

![Commit tool window](../../screenshots/vcs-commit.png)

- **Changes** and **Unversioned Files** are listed as trees grouped by folder. Tick the files
  to include. Double-click a file (or **Show Diff**, `Ctrl+D`) to see its diff.
- Toolbar: **Refresh** (`Ctrl+F5`), **Rollback…** (`Ctrl+Alt+Z`, discards the changes of the
  selected files after a confirmation), **Show Diff**, **Expand All**, **Collapse All**.
- Write the **Commit Message**. **Amend** amends the last commit (its message is loaded).
  **Sign-off** adds a `Signed-off-by` line (`git commit --signoff`; the same option as
  Settings | Version Control | Git | Sign-off commits).
- **Commit** (`Ctrl+Enter` in the message) or **Commit and Push…**.

![Commit message inspections](../../screenshots/vcs-commit-inspections.png)

While you type, the message is checked like in PHPStorm (Settings | Version Control | Git |
Commit Message Inspections): the subject line is at most `vcs.git.commit.subjectMaxLength`
characters (72), a blank line separates subject and body
(`vcs.git.commit.blankLineAfterSubject`) and body lines stay within
`vcs.git.commit.bodyRightMargin` columns (72). Problems are listed below the message; they do
not block the commit. Lines starting with `#` are ignored. Typos and grammar problems in the
message are underlined and listed too, with their fixes on click (see
[Spelling and grammar](spelling-and-grammar.md#commit-messages)).

Before committing files with Windows line separators (CRLF), Forge asks whether to **Commit As
Is** (`vcs.git.commit.warnOnCrlf`), unless git converts them itself (`core.autocrlf` is `true`
or `input`).

> **Note:** "Commit Message History" (`Ctrl+M`) and the **Commit Options** gear are not
> available yet.

### AI commit messages

Click **Generate Commit Message with AI** (the sparkle icon above the message). Forge sends the
diff of the **ticked** files (new files included), the file list and the branch name to your
default AI provider and fills in the message: an imperative subject of at most 72 characters,
optionally a blank line and a short body. Large diffs are shortened so every file keeps a share
(about 14,000 characters in total).

Configure the provider in Settings | Tools | AI Providers; you can pick a separate model for
commit messages there. See [AI](ai.md#providers).

## Diff viewer

![Diff viewer](../../screenshots/vcs-diff.png)

| Action | Key |
| --- | --- |
| Next / Previous Difference | `F7` / `Shift+F7` |
| Compare Next / Previous File | `Alt+Right` / `Alt+Left` |
| Jump to Source | `F4` |
| Close | `Escape` |

The toolbar also has viewer, whitespace and highlighting options and **Collapse Unchanged
Fragments**. Each changed block has a "Revert" arrow that replaces your side with the left
side.

## In the editor

### Change markers

Lines that differ from `HEAD` are marked in the gutter: added, modified and deleted. Click a
marker to see the old text and choose **Rollback** (`Ctrl+Alt+Z`) or **Show Diff** (`Ctrl+D`).
**Next Change** / **Previous Change** (`Ctrl+Alt+Shift+Down` / `Ctrl+Alt+Shift+Up`, Navigate
menu) move the caret to the next or previous changed block, wrapping around at the end of the
file, and open the same popup.

![Next Change popup](../../screenshots/next-change.png)

### Blame

Right-click the gutter and choose **Annotate with Git Blame** (also in the editor context menu
under Git, and in Find Action). The gutter then shows author and date per line, colored by
age; click an entry for the commit details. **Close Annotations** hides it.

![Blame](../../screenshots/vcs-blame.png)

The caret line also shows an inline blame hint at its end: "Marlon, 2 days ago • message".

## Branches

Click the branch widget in the toolbar (or **Branches…** in Find Action) to open **Git
Branches**:

![Branches](../../screenshots/vcs-branches.png)

- At the top: **Update Project…** (`Ctrl+T`), **Commit…** (`Ctrl+K`), **Push…**
  (`Ctrl+Shift+K`), **New Branch…** and **Checkout Tag or Revision…**.
- **Local** and **Remote** branches with a search field ("Search for branches and actions").
- A branch's submenu: for the current branch **New Branch from '…'…**, **Push…** and
  **Update**; for other branches **Checkout**, **New Branch from '…'…**, **Rebase 'current'
  onto '…'**, **Merge '…' into 'current'** and, for local branches, **Delete** (with "Force
  delete" for unmerged branches).

**New Branch…** asks for the name and whether to check it out.

## Push, update and fetch

| Action | Shortcut | What it does |
| --- | --- | --- |
| Push… | `Ctrl+Shift+K` (macOS `Cmd+Shift+K`) | pushes the current branch |
| Update Project… | `Ctrl+T` (macOS `Cmd+T`) | pulls the current branch with the update method |
| Fetch | Find Action | fetches all remotes |

Results appear as balloons ("Pushed", "Project updated", "All files are up to date", or the
error). Git runs with `ssh -o BatchMode=yes`, so SSH remotes need a key in your ssh-agent (or
without passphrase); Forge cannot answer password prompts.

![Git settings](../../screenshots/settings-git.png)

Settings | Version Control | Git:

| Option | Setting | Default |
| --- | --- | --- |
| Path to Git executable | `vcs.git.path` | `git` (the one on `PATH`; `FORGE_GIT` also overrides it). Remote projects always use the git of their host. |
| Update method: Merge, Rebase or Branch default (`pull.rebase` of the branch) | `vcs.git.updateMethod` | Merge |
| Fetch remotes automatically, every n minutes | `vcs.git.autoFetch`, `vcs.git.autoFetchInterval` | on, 10 |

Automatic fetching runs in the background while the branch has an upstream; failures
(offline, missing credentials) are ignored.

## Merge conflicts

When a merge, rebase, update (pull), cherry-pick or unstash stops with conflicts, Forge opens
the **Conflicts** dialog. Later you get it with **Git ▸ Resolve Conflicts…** (Find Action:
`Git.ResolveConflicts`), the **Resolve** link of the **Merge conflicts** balloon, or the
**Resolve** link of the **Merge Conflicts** node in the Commit tool window.

![Conflicts dialog](../../screenshots/merge-conflicts-dialog.png)

- The header says what is going on ("Merging branch feature into branch main").
- **Yours** and **Theirs** show how each side changed a file: Modified, Added or Deleted. In a
  rebase, *yours* is the commit being replayed and *theirs* the branch you rebase onto, like
  in PHPStorm.
- **Accept Yours** / **Accept Theirs** take that side's version of the selected files (a side
  that deleted the file deletes it) and mark them resolved (`git add` / `git rm`).
- **Merge…** (or a double-click) opens the merge window. For a file deleted on one side it asks
  **Keep File** or **Accept Deletion**; for a binary file, which version to keep.
- **Group files by directory** shows the files under their folders.

Until they are resolved, conflicted files are red in the Project view (and their folders), the
editor tabs and the **Merge Conflicts** node of the Commit tool window. An editor of such a file
shows the banner **File has merge conflicts** with a **Resolve…** link to the merge window.

![Conflicts in the Commit tool window and the editor](../../screenshots/merge-conflicts-commit.png)

### Merge window

**Merge Revisions for …** shows your version (**Changes from main**) on the left, the editable
**Result** in the middle and the other side (**Changes from feature**) on the right. The result
starts with the common base version; the three panes scroll together and bands connect the
changes.

![Merge window](../../screenshots/merge-window.png)

- Colours: blue for modified, green for inserted, grey for deleted lines, red for conflicts.
  Handled changes lose their colour.
- `»` / `«` next to a change applies it to the result; `×` ignores it. After you applied one
  side of a conflict, the arrow of the other side appends its text after the first.
- Toolbar: **Previous / Next Difference** (`Shift+F7` / `F7`), **Apply Non-Conflicting
  Changes from the Left Side**, **Apply All Non-Conflicting Changes**, **Apply Non-Conflicting
  Changes from the Right Side**, **Resolve Simple Conflicts** (the magic wand) and **Compare
  Contents** (left, base, right and result side by side in the diff viewer). The counter on the
  right shows the unresolved changes: "3 changes. 2 conflicts".
- **Resolve Simple Conflicts** merges conflicts where both sides changed *different* words of
  the same lines (`$price * $qty` → `$netPrice * $qty` and `$price * $quantity` become
  `$netPrice * $quantity`). Conflicts where both sides changed the same words stay for you.
- Edit the result freely; `Ctrl+Z` also undoes applied changes.
- **Accept Left** / **Accept Right** take one side for the whole file. **Apply** writes the
  result and marks the file resolved (`git add`); with unresolved changes left it asks first
  ("There are unresolved changes"). **Cancel** (`Escape`) asks before it discards your edits.

![Merge window after applying changes](../../screenshots/merge-window-resolved.png)

Files with `\r\n` line separators keep them. In remote projects the stage versions are read
and the result is written on the remote host. When the last conflict is resolved, Forge
reminds you to commit (merge, cherry-pick) or continue the rebase.

## Log

The **Git** tool window (`Alt+9`, bottom) has a **Log** tab with the commit graph and a
**Console** tab with the git commands Forge ran.

![Git log](../../screenshots/vcs-log.png)

- Filter by text or hash, branch, user, date and paths; **Show all branches**.
- Select a commit to see its changed files; open their diffs from there.
- **Fetch All Remotes** and **Refresh** (`Ctrl+F5`) in the toolbar.

## File and folder history

**Show History** (Git ▸ Current File, the editor's and the Project view's Git submenu) opens a
**History: <name>** tab in the Git tool window, next to **Log**. For a file it lists every
commit that changed it and follows renames (the old path is shown next to the message); for a
folder, every commit that changed something below it.

![File history](../../screenshots/file-history.png)

- The table has the commit message, author, date and hash. Below it are the full message and,
  for a folder, the files the commit changed (click one to preview it).
- The right side previews the file's change in the selected commit.
- Toolbar and right-click menu:

| Action | What it does |
| --- | --- |
| Show Diff (`Ctrl+D`, double-click) | the file's change in the commit in the diff viewer |
| Compare with Local | the revision against your working copy (editable) |
| Get | replaces your working copy with the revision, after a confirmation |
| Annotate Revision | opens the file as it was in the revision, read-only, with its blame |
| Checkout Revision | checks out the commit (detached HEAD) |
| Copy Revision Number | copies the full hash |

![History context menu](../../screenshots/file-history-menu.png)

Close a history tab with its **×** or a middle click.

![Folder history](../../screenshots/folder-history.png)

### History for selection

Select lines in the editor and choose **Show History for Selection** (Git ▸ Current File or the
editor's Git submenu). The **History for Selection** tab lists the commits that changed those
lines (`git log -L`, line numbers as in `HEAD`); the preview shows the block before and after
each commit, so you can follow how it evolved.

![History for selection](../../screenshots/selection-history.png)

### Annotate Revision

**Annotate Revision** opens the file of that revision in a read-only editor tab named
`User.php (49e9d6ef)` with the blame of the revision in the gutter.

![Annotate Revision](../../screenshots/annotate-revision.png)

## Compare with Revision and Branch

- **Compare with Revision…** lists the revisions of the file (renames followed) and compares
  the picked one with your version.
- **Compare with Branch…** picks a local or remote branch or a tag. For a file it opens the
  diff; for a folder (Project view) it opens an editor tab with the files that differ between
  the branch and the working tree. Double-click a file (or `Ctrl+D`) for its diff.
- **Show Diff** is PHPStorm's Compare with the Same Repository Version: `HEAD` against your
  version.

## Compare files and folders

In the Project view, select two files or two folders (`Ctrl`+click the second one) and press
`Ctrl+D`, or right-click and choose **Compare Files** / **Compare Directories** (below Reload
from Disk). With one entry selected the item is **Compare With…** and asks for the second file
or folder in a **Select Path** dialog.

![Select Path](../../screenshots/compare-files.png)

Two files open in the diff viewer; the right side can be edited and is saved when you close
it. Two folders open a folder diff in an editor tab:

![Compare Directories](../../screenshots/compare-directories.png)

- Every file and folder of both sides with size and date, and an indicator in the middle: `→`
  only on the left, `≠` different, `=` equal, `←` only on the right. Folders can be collapsed.
- The filter buttons in the toolbar show or hide each kind, with their counts.
- Double-click a file (or `Ctrl+D`, **Compare Contents**) to open its diff.
- **Copy to Right** / **Copy to Left** copy the selected file or folder to the other side
  (folders are merged; replacing a different file asks first).
- `.git`, `.hg` and `.svn` folders are not compared.

The folders are read where the project is, so this works in remote projects too.

![Compare Directories, light theme](../../screenshots/compare-directories-light.png)

## Compare with Clipboard

**Compare with Clipboard** (View menu, editor context menu) compares the clipboard with the
selected text, or with the whole file when nothing is selected; the whole file can then be
edited in the diff viewer.

![Compare with Clipboard](../../screenshots/compare-with-clipboard.png)

## Code review

Pull and merge requests, issues and CI pipelines of GitHub, GitLab and Forgejo/Gitea are in
[Code hosts](code-hosts.md).

## Not available yet

Get from VCS (clone), cherry-pick, interactive rebase and Local History are not implemented
yet.
