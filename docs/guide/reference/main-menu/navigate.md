---
title: "Navigate menu"
description: "Every entry of the Navigate menu with its shortcut, availability and what it does."
order: 40
tags: [main menu, reference, navigate]
---

# Navigate menu

Back and forward, Search Everywhere, Go to Class/File/Symbol/Line, declarations and problems. Open it with ☰ (`Alt+\`) or from the menu bar, see [Main menu](../../user/workbench.md#main-menu).

![Navigate menu](../../../screenshots/docs-menu-navigate.png)

**Available**: *yes* — implemented; *not yet* — shown disabled until Forge implements it; *dynamic* — filled at runtime. Shortcuts are the Windows/Linux and macOS defaults.

| Entry | Windows / Linux | macOS | Available | What it does |
| --- | --- | --- | --- | --- |
| Back | `Ctrl+Alt+Left` | `Cmd+[`, `Cmd+Alt+Left` | yes | Goes back to the previous caret location in your navigation history. [More](../../user/editor.md#go-to-declaration) |
| Forward | `Ctrl+Alt+Right` | `Cmd+]`, `Cmd+Alt+Right` | yes | Goes forward in the navigation history. [More](../../user/editor.md#go-to-declaration) |
| Last Edit Location | `Ctrl+Shift+Backspace` | `Cmd+Shift+Backspace` | not yet | Not available yet; the entry is disabled. |
| Next Edit Location | — | — | not yet | Not available yet; the entry is disabled. |
| **Bookmarks** | | | | submenu |
| Bookmarks › Toggle Bookmark | `F11` | `F3` | yes | Adds or removes a bookmark on the caret line. [More](../../user/workbench.md#bookmarks) |
| Bookmarks › Toggle Bookmark with Mnemonic | `Ctrl+F11` | `Alt+F3` | yes | Adds a bookmark with a digit or letter you pick in a popup, or changes or removes it. [More](../../user/workbench.md#bookmarks) ([screenshot](../../../screenshots/bookmark-mnemonic.png)) |
| Bookmarks › Show All Bookmarks | `Shift+F11` | `Cmd+F3` | yes | Opens the Bookmarks tool window. [More](../../user/workbench.md#bookmarks) ([screenshot](../../../screenshots/bookmarks.png)) |
| Bookmarks › Edit Bookmark Description… | — | — | yes | Edits the description of the bookmark on the caret line. [More](../../user/workbench.md#bookmarks) |
| Bookmarks › Next Bookmark | — | — | yes | Moves the caret to the next bookmark of the file. [More](../../user/workbench.md#bookmarks) |
| Bookmarks › Previous Bookmark | — | — | yes | Moves the caret to the previous bookmark of the file. [More](../../user/workbench.md#bookmarks) |
| Search Everywhere | `Double Shift` | `Double Shift` | yes | Searches classes, files, symbols, actions and text in one popup. [More](../../user/search-everywhere.md) ([screenshot](../../../screenshots/docs-search-everywhere.png)) |
| Class… | `Ctrl+N` | `Cmd+O` | yes | Finds a class, interface, trait or enum by name (camel humps work). [More](../../user/search-everywhere.md) ([screenshot](../../../screenshots/docs-goto-class.png)) |
| File… | `Ctrl+Shift+N` | `Cmd+Shift+O` | yes | Finds a project file by name or path. [More](../../user/search-everywhere.md) ([screenshot](../../../screenshots/docs-goto-file.png)) |
| Symbol… | `Ctrl+Alt+Shift+N` | `Cmd+Alt+O` | yes | Finds a function, method, property or constant by name. [More](../../user/search-everywhere.md) ([screenshot](../../../screenshots/docs-goto-symbol.png)) |
| Action… | `Ctrl+Shift+A` | `Cmd+Shift+A` | yes | Finds any action by name and runs it (Find Action). [More](../../user/search-everywhere.md#find-action) ([screenshot](../../../screenshots/docs-find-action.png)) |
| Line/Column… | `Ctrl+G` | `Cmd+L` | yes | Moves the caret to a line (and column) of the current file. [More](../../user/editor.md#editing-actions) ([screenshot](../../../screenshots/docs-goto-line.png)) |
| Select In… | `Alt+F1` | `Alt+F1` | not yet | Not available yet; the entry is disabled. |
| Navigation Bar | `Alt+Home` | `Cmd+Up` | not yet | Not available yet; the entry is disabled. |
| Declaration or Usages | `Ctrl+B` | `Cmd+B` | yes | Jumps to the declaration of the symbol at the caret; on a declaration it shows the usages; several targets open the Choose Declaration popup. [More](../../user/editor.md#go-to-declaration) ([screenshot](../../../screenshots/php-nav-stub.png)) |
| Implementation(s) | `Ctrl+Alt+B` | `Cmd+Alt+B` | yes | Lists the implementations of the class, interface or method at the caret. [More](../../user/editor.md#implementation-type-declaration-super-method) ([screenshot](../../../screenshots/php-nav-gutter.png)) |
| Type Declaration | `Ctrl+Shift+B` | `Cmd+Shift+B` | yes | Jumps to the class of the variable, property or method return type at the caret. [More](../../user/editor.md#implementation-type-declaration-super-method) |
| Super Method | `Ctrl+U` | `Cmd+U` | yes | Jumps to the method the current method overrides or implements. [More](../../user/editor.md#implementation-type-declaration-super-method) |
| Test | `Ctrl+Shift+T` | `Cmd+Shift+T` | not yet | Not available yet; the entry is disabled. |
| Related Symbol… | `Ctrl+Alt+Home` | `Ctrl+Cmd+Up` | not yet | Not available yet; the entry is disabled. |
| File Structure | `Ctrl+F12` | `Cmd+F12` | not yet | Not available yet; the entry is disabled. |
| Type Hierarchy | `Ctrl+H` | `Ctrl+H` | not yet | Not available yet; the entry is disabled. |
| Method Hierarchy | `Ctrl+Shift+H` | `Cmd+Shift+H` | not yet | Not available yet; the entry is disabled. |
| Call Hierarchy | `Ctrl+Alt+H` | `Ctrl+Alt+H` | not yet | Not available yet; the entry is disabled. |
| Next Highlighted Error | `F2` | `F2` | yes | Moves the caret to the next problem in the file. [More](../../user/editor.md#problems) |
| Previous Highlighted Error | `Shift+F2` | `Shift+F2` | yes | Moves the caret to the previous problem in the file. [More](../../user/editor.md#problems) |
| Next Occurrence | `Ctrl+Alt+Down` | `Cmd+Alt+Down` | yes | Opens the next result of the Find tool window's active tab. [More](../../user/workbench.md#find-tool-window) |
| Previous Occurrence | `Ctrl+Alt+Up` | `Cmd+Alt+Up` | yes | Opens the previous result of the Find tool window's active tab. [More](../../user/workbench.md#find-tool-window) |
| Next Method | `Alt+Down` | `Ctrl+Down` | not yet | Not available yet; the entry is disabled. |
| Previous Method | `Alt+Up` | `Ctrl+Up` | not yet | Not available yet; the entry is disabled. |
| Next Change | `Ctrl+Alt+Shift+Down` | `Ctrl+Alt+Shift+Down` | not yet | Not available yet; the entry is disabled. |
| Previous Change | `Ctrl+Alt+Shift+Up` | `Ctrl+Alt+Shift+Up` | not yet | Not available yet; the entry is disabled. |
