---
title: Git
description: Git integration - the Commit tool window, AI commit messages, diff viewer, change markers, blame, branches, push, update and the Git log.
order: 120
tags: [git, vcs, commit, diff, blame, branches]
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

## Log

The **Git** tool window (`Alt+9`, bottom) has a **Log** tab with the commit graph and a
**Console** tab with the git commands Forge ran.

![Git log](../../screenshots/vcs-log.png)

- Filter by text or hash, branch, user, date and paths; **Show all branches**.
- Select a commit to see its changed files; open their diffs from there.
- **Fetch All Remotes** and **Refresh** (`Ctrl+F5`) in the toolbar.

## Code review

Pull and merge requests, issues and CI pipelines of GitHub, GitLab and Forgejo/Gitea are in
[Code hosts](code-hosts.md).

## Not available yet

Get from VCS (clone), stash/unstash, cherry-pick, interactive rebase, file history, Local
History and merge conflict resolution are not implemented yet.
