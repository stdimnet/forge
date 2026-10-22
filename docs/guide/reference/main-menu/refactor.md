---
title: "Refactor menu"
description: "Every entry of the Refactor menu with its shortcut, availability and what it does."
order: 60
tags: [main menu, reference, refactor]
---

# Refactor menu

Refactorings. Refactor This, Rename, Rename File, Extract/Introduce Variable, Constant, Field, Parameter and Method, and Inline work; the menu shows PHPStorm's other entries disabled. Open it with ☰ (`Alt+\`) or from the menu bar, see [Main menu](../../user/workbench.md#main-menu).

![Refactor menu](../../../screenshots/docs-menu-refactor.png)

**Available**: *yes* — implemented; *not yet* — shown disabled until Forge implements it; *dynamic* — filled at runtime. Shortcuts are the Windows/Linux and macOS defaults.

| Entry | Windows / Linux | macOS | Available | What it does |
| --- | --- | --- | --- | --- |
| Refactor This… | `Ctrl+Alt+Shift+T` | `Ctrl+T` | yes | A popup with the refactorings and their shortcuts. [More](../../user/shortcuts/refactoring.md#refactor-this) |
| Rename… | `Shift+F6` | `Shift+F6` | yes | Renames the symbol at the caret in place across the project (the file or folder in the Project view). [More](../../user/editor.md#rename) ([screenshot](../../../screenshots/rename-in-place.png)) |
| Rename File… | — | — | yes | Renames the file of the current editor. [More](../../user/editor.md#rename) |
| Change Signature… | `Ctrl+F6` | `Cmd+F6` | not yet | Not available yet; the entry is disabled. |
| **Extract/Introduce** | | | | submenu |
| Extract/Introduce › Variable… | `Ctrl+Alt+V` | `Cmd+Alt+V` | yes | Puts the expression into a new local variable, named in place. [More](../../user/php.md#extract-introduce-and-inline) ([screenshot](../../../screenshots/introduce-variable-in-place.png)) |
| Extract/Introduce › Constant… | `Ctrl+Alt+C` | `Cmd+Alt+C` | yes | Turns a constant expression into a class constant or a global `const`. [More](../../user/php.md#extract-introduce-and-inline) ([screenshot](../../../screenshots/introduce-constant-dialog.png)) |
| Extract/Introduce › Field… | `Ctrl+Alt+F` | `Cmd+Alt+F` | yes | Moves the expression into a new property. [More](../../user/php.md#extract-introduce-and-inline) |
| Extract/Introduce › Parameter… | `Ctrl+Alt+P` | `Cmd+Alt+P` | yes | Adds a parameter for the expression and passes it at every call. [More](../../user/php.md#extract-introduce-and-inline) |
| Extract/Introduce › Method… | `Ctrl+Alt+M` | `Cmd+Alt+M` | yes | Moves the selected code into a new method or function. [More](../../user/php.md#extract-introduce-and-inline) ([screenshot](../../../screenshots/extract-method-dialog.png)) |
| Extract/Introduce › Interface… | — | — | not yet | Not available yet; the entry is disabled. |
| Extract/Introduce › Superclass… | — | — | not yet | Not available yet; the entry is disabled. |
| Inline… | `Ctrl+Alt+N` | `Cmd+Alt+N` | yes | Inlines a variable, method, function or constant. [More](../../user/php.md#extract-introduce-and-inline) ([screenshot](../../../screenshots/inline-method-dialog.png)) |
| Move… | `F6` | `F6` | not yet | Not available yet; the entry is disabled. |
| Copy File… | `F5` | `F5` | not yet | Not available yet; the entry is disabled. |
| Safe Delete… | `Alt+Delete` | `Cmd+Delete` | not yet | Not available yet; the entry is disabled. |
| Pull Members Up… | — | — | not yet | Not available yet; the entry is disabled. |
| Push Members Down… | — | — | not yet | Not available yet; the entry is disabled. |
