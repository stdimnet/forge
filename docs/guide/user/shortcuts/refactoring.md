---
title: "Shortcuts: Refactoring"
description: "Refactorings and code generation."
order: 50
tags: [shortcuts, keymap, refactoring]
---

# Shortcuts: Refactoring

Refactorings and code generation. Windows/Linux shortcuts first, macOS second; both are PHPStorm's defaults and can be changed in Settings | Keymap.

> **Note:** the other refactorings of PHPStorm's Refactor menu (Change Signature `Ctrl+F6`,
> Extract/Introduce Variable, Constant, Field, Parameter and Method, Inline, Move, Copy File,
> Safe Delete, Pull Members Up / Push Members Down) are not implemented yet. Their shortcuts
> stay reserved and show "*Action* is not available yet"; see the
> [Refactor menu](../../reference/main-menu/refactor.md). The actions below are the
> refactorings and code transformations Forge has today.

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| [Rename…](#rename) | `Shift+F6` | `Shift+F6` |
| [Show Context Actions](#show-context-actions) | `Alt+Enter` | `Alt+Enter` |
| [Generate…](#generate) | `Alt+Insert` | `Cmd+N` |
| [Reformat Code](#reformat-code) | `Ctrl+Alt+L` | `Cmd+Alt+L` |

## Rename

**Shortcut:** `Shift+F6` · macOS `Shift+F6` · action id `RenameElement`

In the editor: renames the symbol at the caret in place, in every file of the project; `Enter`
applies, `Escape` cancels, `Shift+F6` again opens the Rename dialog. In the Project view:
renames the selected file or folder. One undo step (`Ctrl+Z`) undoes the whole rename.

**Menu:** Refactor | Rename…

![In-place rename](../../../screenshots/rename-in-place.png)

More: [Editor](../../user/editor.md#rename), [PHP](../../user/php.md#rename)

## Show Context Actions

**Shortcut:** `Alt+Enter` · macOS `Alt+Enter` · action id `ShowIntentionActions`

Opens the intention popup at the caret: quick fixes, intentions and refactorings of the
language servers and Forge's PHP intentions. `Right` opens an entry's sub-menu.

**Menu:** Code | Show Context Actions

![Context actions](../../../screenshots/context-actions.png)

More: [Editor](../../user/editor.md#show-context-actions)

## Generate

**Shortcut:** `Alt+Insert` · macOS `Cmd+N` · action id `Generate`

In PHP files: generates a constructor, getters, setters, overrides, `__toString()` or a PHPDoc block.

**Menu:** Code | Generate…

![Generate…](../../../screenshots/docs-generate.png)

More: [PHP](../../user/php.md#generate)

## Reformat Code

**Shortcut:** `Ctrl+Alt+L` · macOS `Cmd+Alt+L` · action id `ReformatCode`

Formats the file with the language server's formatter (StyLua for Lua).

**Menu:** Code | Reformat Code

More: [Editor](../../user/editor.md#reformat-code)
