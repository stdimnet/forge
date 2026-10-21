---
title: "Shortcuts: Refactoring"
description: "Refactorings and code generation."
order: 50
tags: [shortcuts, keymap, refactoring]
---

# Shortcuts: Refactoring

Refactorings and code generation. Windows/Linux shortcuts first, macOS second; both are PHPStorm's defaults and can be changed in Settings | Keymap.

> **Note:** the refactorings of PHPStorm's Refactor menu (Rename `Shift+F6`, Change Signature
> `Ctrl+F6`, Extract/Introduce Variable, Constant, Field, Parameter and Method, Inline, Move,
> Copy File, Safe Delete, Pull Members Up / Push Members Down) are not implemented yet. Their
> shortcuts stay reserved and show "*Action* is not available yet"; see the
> [Refactor menu](../../reference/main-menu/refactor.md). Renaming files and folders works from
> the Project view's context menu. The actions below are the code transformations Forge has
> today.

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| [Generate…](#generate) | `Alt+Insert` | `Cmd+N` |
| [Reformat Code](#reformat-code) | `Ctrl+Alt+L` | `Cmd+Alt+L` |

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
