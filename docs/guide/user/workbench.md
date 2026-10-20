---
title: Workbench and tool windows
description: The main window - main toolbar, tool window stripes, the Project view, editor tabs, Recent Files, the Switcher and the status bar.
order: 10
tags: [workbench, tool windows, project view, status bar, tabs]
---

# Workbench and tool windows

The workbench is the main window of an open project. It follows PHPStorm's New UI.

![The workbench](../../screenshots/workbench-dark.png)

| Area | Where | What it holds |
| --- | --- | --- |
| Main toolbar | top | ☰ main menu, project widget, Git branch widget, run widget, AI Chat, Search Everywhere, theme toggle, Settings; it is also the window's title bar |
| Tool window stripes | left, right and bottom edges | buttons that show and hide tool windows |
| Editor | center | tabs, breadcrumbs and the code editor |
| Status bar | bottom | path of the current file, background tasks, project information, caret position, file type, Git branch |

## Main toolbar

From left to right:

- **☰ Main Menu** (`Alt+\`), see [Main menu](#main-menu) below.

- **Project widget**: the badge and name of the open project; click it to open another
  project.
- **Branch widget**: the current Git branch; click it for the [Branches popup](git.md#branches).
- Toolbar items of plugins, for example Xdebug's **Start Listening for PHP Debug Connections**
  button (see [Xdebug debugging](debugging.md)).
- **Run widget**: the selected run configuration with **Run** and **Debug** buttons, see
  [Run/Debug configurations](run-debug.md).
- **AI Chat**, **Search Everywhere** (`Double Shift`), the **theme toggle** (dark/light) and
  **Settings** (`Ctrl+Alt+S`).

Empty parts of the toolbar move the window; a double click maximizes it. The window buttons
sit at the toolbar's ends, styled for your desktop (see
[Window controls](settings-and-themes.md#window-controls)).

## Main menu

The main menu has PHPStorm's menus: **File, Edit, View, Navigate, Code, Refactor, Run, Tools,
Git** (**VCS** in projects without Git), **Window** and **Help**. Every entry shows its shortcut;
actions Forge has not implemented yet stay visible but disabled. Plugins add their own entries,
for example Tools | Laravel (Run Artisan Command…, Show Routes), Tools | Symfony and
Run | Start Listening for PHP Debug Connections. File | Open Recent and View | Tool Windows are
filled with your recent projects and the available tool windows.

![Main menu](../../screenshots/main-menu-file.png)

Where the menu appears is chosen with Settings | Appearance & Behavior | Appearance | UI
Options | **Main menu** (`appearance.mainMenu`), as in PHPStorm's New UI:

| Value | Look |
| --- | --- |
| `hamburger` (default) — Hamburger menu | behind the ☰ button at the left of the main toolbar |
| `merged` — Merged with main toolbar | the menu names (File, Edit, …) directly in the main toolbar |
| `toolbar` — Show in separate toolbar | a classic menu bar above the main toolbar |

View | Appearance | **Show Main Menu in Separate Toolbar** switches to the last mode from the
menu itself; **Compact Mode** and **Tool Window Bars** are toggles in the same submenu.

![Merged with the main toolbar](../../screenshots/main-menu-merged.png)

Keyboard: `Alt+\` opens the menu; the underlined letter (mnemonic) picks an entry, alone or with
`Alt`; arrow keys move, `Right`/`Left` open and close submenus and switch between the menus of
the menu bar; `Escape` closes. **Help | About** shows the version; **Help | Keymap Reference**
opens Settings | Keymap.

![Main menu in a separate toolbar](../../screenshots/main-menu-bar.png)

## Tool windows

Tool windows are the panels along the edges. Click a stripe button, or press the tool window's
shortcut, to show it; pressing the shortcut again hides it, as in PHPStorm.

| Tool window | Stripe | Shortcut | Page |
| --- | --- | --- | --- |
| Project | left | `Alt+1` (macOS `Cmd+1`) | below |
| Commit | left | `Alt+0` (macOS `Cmd+0`) | [Git](git.md) |
| Pull Requests | left | — | [Code hosts](code-hosts.md) |
| Structure | left | `Alt+7` (macOS `Cmd+7`) | [PHP](php.md#structure), [LuaForge](luaforge.md) |
| npm | left | — | [WebForge](webforge.md) |
| Run | bottom | `Alt+4` (macOS `Cmd+4`) | [Run/Debug configurations](run-debug.md) |
| Debug | bottom | `Alt+5` (macOS `Cmd+5`) | [Xdebug debugging](debugging.md) |
| Services | bottom | `Alt+8` (macOS `Cmd+8`) | [Services](services.md) |
| Routes (Laravel) | bottom | — | [Laravel](laravel.md) |
| Symfony | bottom | — | [Symfony](symfony.md) |
| Terminal | bottom | `Alt+F12` | [Terminal](terminal.md) |
| Problems | bottom | `Alt+6` (macOS `Cmd+6`) | [Editor](editor.md#problems) |
| Git | bottom | `Alt+9` (macOS `Cmd+9`) | [Git](git.md#log) |
| AI Chat | right | — | [AI](ai.md) |
| Database | right | — | [Database tools](database.md) |
| Doctrine | right | — | [Doctrine](doctrine.md) |
| Notifications | right | — | placeholder, see note |

Framework tool windows (Routes, Symfony, Doctrine, npm) only appear in projects that use the
framework.

> **Note:** the Notifications tool window is a placeholder ("coming soon"). Bookmarks
> (`Alt+2`), Find (`Alt+3`) and TODO are not available yet.

### Managing tool windows

| Action | Shortcut (Windows/Linux) | macOS |
| --- | --- | --- |
| Hide the active tool window | `Shift+Escape` | `Shift+Escape` |
| Hide all tool windows (again: restore) | `Ctrl+Shift+F12` | `Cmd+Shift+F12` |
| Jump to the last tool window | `F12` | `F12` |
| Stretch the focused tool window | `Ctrl+Shift+Left/Right/Up/Down` | `Ctrl+Alt+Shift+Left/Right/Up/Down` |
| Close the active tab of a tool window (Run, Terminal) | `Ctrl+Shift+F4` | `Cmd+Shift+F4` |

**Hide Side Tool Windows** has no default shortcut; run it from Find Action. Drag the border
between a tool window and the editor to resize it.

The stripes can be hidden with Settings | Appearance & Behavior | Appearance | **Show tool
window bars**. **Compact mode** on the same page makes toolbars and tool window headers
smaller.

> **Note:** tool window sizes and which tool windows are open are not remembered between
> sessions yet. "Restore Default Layout" and "Maximize Tool Window" are not available yet.

## Project view

The Project tool window (`Alt+1`) shows the project folder as a tree.

- **Single click opens** a file and expands a folder by default. To work like PHPStorm (single
  click selects, double click opens), turn off Settings | Appearance & Behavior | Appearance |
  Project View | **Open files with single click** (`projectView.openFilesWithSingleClick`).
- **Keyboard**: `Up`/`Down` move the selection, `Right` expands, `Left` collapses or goes to
  the parent, `Home`/`End` jump to the first/last row, `Enter` or `F4` opens.
- Files and folders ignored by Git (`.gitignore`, `.git/info/exclude`, the global excludes
  file) are grayed out.
- **Scratches and Consoles** appear at the bottom of the tree, see
  [Scratches, consoles and HTTP client](scratches-and-http.md).

![Project view](../../screenshots/project-tree-double-click.png)

### Context menu

Right-click a file, a folder or the empty area below the tree:

| Entry | What it does |
| --- | --- |
| New > File / Directory | Creates a file or folder in the selected folder (or the file's folder). The name may contain `/` to create nested folders, e.g. `app/Models/Order.php`. A new file opens in the editor. |
| Copy Path/Reference > Absolute Path (`Ctrl+Shift+C`), Path From Project Root, File Name | Copies to the clipboard |
| Find in Files… | searches the folder (or the project), see [Find in Files](search-everywhere.md#find-in-files) |
| Rename… | Renames the file or folder. Unsaved changes in open tabs are saved first and the tabs follow the new name. |
| Delete… | Deletes after a confirmation; folders with all their content. This cannot be undone. |
| Reload from Disk | Re-reads the folder |

![Project view context menu](../../screenshots/file-menu-new-file.png)

## Editor tabs

- Each open file has a tab. The active one is highlighted; a modified file is marked until it
  is saved.
- Close a tab with its × button, a middle click, or `Ctrl+F4` (macOS `Cmd+W`).
- `Alt+Right` / `Alt+Left` (macOS `Ctrl+Right` / `Ctrl+Left`) select the next/previous tab.
- **Close All Tabs**, **Close Other Tabs**, **Close Unmodified Tabs** and **Reopen Closed Tab**
  are available through Find Action.
- `Ctrl+S` (macOS `Cmd+S`) saves all files (**Save All**).
- Some tabs are not files: merge requests, issues, database consoles and table editors open as
  tabs as well.

Breadcrumbs above the editor show the file's folder path.

> **Note:** pinned tabs and split editors are not available yet.

## Recent Files and Switcher

**Recent Files** (`Ctrl+E`, macOS `Cmd+E`) lists tool windows on the left and recently opened
files on the right. Type to filter (speed search). Press `Ctrl+E` again to show only
recently **edited** files; **Recently Changed Files** opens that view directly.

![Recent Files](../../screenshots/recent-files.png)

The **Switcher** (`Ctrl+Tab`, `Ctrl+Shift+Tab`) is the same popup for quick switching: keep
`Ctrl` pressed, press `Tab` to move, release to open.

**Back** and **Forward** (`Ctrl+Alt+Left` / `Ctrl+Alt+Right`, macOS `Cmd+[` / `Cmd+]`) move
through your navigation history.

## Status bar

![Status bar while indexing](../../screenshots/status-bar-indexing.png)

Left: the project name and the path of the current file. Right, depending on the project:

| Widget | Shows | Click |
| --- | --- | --- |
| Remote connection | host and latency of a remote project | connection popup, see [Remote development](remote-development.md) |
| Xdebug | listening state and port | [Xdebug debugging](debugging.md) |
| PHP indexing | "Indexing… 45%" while the PHP index is built | — |
| Language servers | "Indexing… TypeScript 45%" or the running servers | opens Settings | Languages & Frameworks | Language Servers |
| Docker | connection to the Docker daemon | [Services](services.md) |
| Project type | "Laravel 12", "Symfony 7.2 · dev", "React 19" | Settings | Plugins, or the framework's popup |
| PHP version | the PHP language level, e.g. "PHP 8.3" | PHP settings |
| CI status | pipeline status of the current branch | [Code hosts](code-hosts.md) |
| Caret | `line:column` | — |
| `LF`, `UTF-8`, `4 spaces` | line separator, encoding and indentation | — |
| File type | e.g. "PHP", "Blade", "TypeScript" | — |
| Branch | the current Git branch | — |

> **Note:** the line separator, encoding and indentation widgets are labels only; they
> cannot be changed from the status bar yet.
