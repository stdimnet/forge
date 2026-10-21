---
title: Search Everywhere and Find Action
description: Find files, classes, symbols and actions with Search Everywhere, Go to Class/File/Symbol and Find Action, and how fuzzy matching works.
order: 30
tags: [search, navigation, find action, go to file]
---

# Search Everywhere and Find Action

## Search Everywhere

Press `Shift` twice (**Double Shift**), or click the magnifier in the main toolbar.

![Search Everywhere](../../screenshots/search-everywhere.png)

The popup has tabs; `Tab` / `Shift+Tab` switch between them:

| Tab | Opens directly with | Finds | Source |
| --- | --- | --- | --- |
| All | `Double Shift` | the best few results of every tab | — |
| Classes | **Go to Class** `Ctrl+N` (macOS `Cmd+O`) | classes, interfaces, traits, enums, Lua classes | PHP index, LuaForge index, language servers (`workspace/symbol`) |
| Files | **Go to File** `Ctrl+Shift+N` (macOS `Cmd+Shift+O`) | project files; without a query, recent files | Forge's file search (respects `.gitignore`) |
| Symbols | **Go to Symbol** `Ctrl+Alt+Shift+N` (macOS `Cmd+Alt+O`) | functions, methods, constants, properties | PHP index, LuaForge index, language servers |
| Actions | **Find Action** `Ctrl+Shift+A` (macOS `Cmd+Shift+A`) | every IDE action and command | the keymap and registered commands |
| Text | — | lines of text in project files | Find in Files (`forge-find`) |

- `Up`/`Down`, `PageUp`/`PageDown` move the selection, `Enter` opens it, `Escape` closes.
- In the All tab, a "more" row under a group switches to that group's tab.
- Pressing a tab's shortcut again while the popup is open toggles the checkbox in the header:
  **Include non-project items** (most tabs) or **Include disabled actions** (Actions).

Results from language servers appear once a server for that language runs, so the Symbols
tab also finds TypeScript, Python, Go or Rust symbols in a polyglot project.

![Symbols from language servers](../../screenshots/search-everywhere-lsp-symbols.png)

### Fuzzy matching

Matching works like PHPStorm's:

- Characters of the query must appear in order, case-insensitively.
- Matches at word starts, after separators (`/ _ - . : $` and spaces) and on **camel humps**
  score higher, so `OrCo` finds `OrderController` first and `usrctl` finds `UserController`.
- For files you can type part of the path: `models/user` finds `app/Models/User.php`.
- For classes, symbols and actions a word must start where a hump or separator starts
  ("strict" matching), which keeps the lists short.

![Go to File with camel humps](../../screenshots/goto-file-camel-humps.png)

Ranking happens in Rust (`forge-fuzzy`), so the same rules apply in every list with a
speed search: Recent Files, the New Scratch File popup, choosers.

## Find Action

**Find Action** (`Ctrl+Shift+A`, macOS `Cmd+Shift+A`) is the Actions tab on its own. Type an
action's name, for example "reformat", "commit", "new scratch" or "close project".

![Find Action](../../screenshots/find-action.png)

- Each row shows the action, its menu group (e.g. "Code | Folding") and its shortcut.
- You can also type the action id (`ReformatCode`), which ranks below title matches.
- Actions Forge has not implemented yet are dimmed and listed last; running one shows
  "*Action* is not available yet". Turn on **Include disabled actions** to see them while
  searching.

Find Action is the fastest way to reach commands that have no shortcut and no menu yet, for
example **Close Project**, **Detect Project Type**, **Reindex PHP Files**, **Show Git Log**,
**Fetch** or **New SSH Connection…**.

## Recent Files

See [Workbench: Recent Files and Switcher](workbench.md#recent-files-and-switcher).

## Find in Files

**Find in Files** (`Ctrl+Shift+F`, macOS `Cmd+Shift+F`, also Edit | Find | Find in Files… and
the Project view's context menu) searches the text of every project file.

![Find in Files](../../screenshots/editor-polish-find-in-files.png)

- Type the text; results update as you type, one row per matching line with the file name and
  line number. The header counts matches and files.
- Options next to the field: **Cc** match case, **W** whole words, **.\*** regular expression.
- **File mask**: tick it and enter patterns such as `*.php` or `*.php, !*Test.php`
  (`!` excludes).
- Scope: **In Project** or **Directory** (a folder of the project; empty means the project
  root).
- `Up`/`Down` select a row; the preview below shows the file around the match. `Enter` opens
  the file at the match, `Escape` closes.
- **Open in Find Window** (`Ctrl+Enter`, macOS `Cmd+Enter`) closes the popup and keeps the
  results as a tab of the [Find tool window](workbench.md#find-tool-window) (`Alt+3`),
  grouped by file, with **Rerun**. `Ctrl+Alt+Down` / `Ctrl+Alt+Up` (Next/Previous Occurrence)
  then walk through them.

Ignored files (`.gitignore`), binary files and very large files are skipped. In remote projects
the search runs on the host.

The **Text** tab of Search Everywhere uses the same search (from two characters on).

![Find in Files results in the Find tool window](../../screenshots/find-window-text.png)

> **Note:** Replace in Files (`Ctrl+Shift+R`) is not available yet.
