---
title: "Shortcuts: Search"
description: "Search Everywhere, Go to Class/File/Symbol, find in the current file and Find in Files."
order: 40
tags: [shortcuts, keymap, search]
---

# Shortcuts: Search

Search Everywhere, Go to Class/File/Symbol, find in the current file and Find in Files. Windows/Linux shortcuts first, macOS second; both are PHPStorm's defaults and can be changed in Settings | Keymap.

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| [Search Everywhere](#search-everywhere) | `Double Shift` | `Double Shift` |
| [Go to Class](#go-to-class) | `Ctrl+N` | `Cmd+O` |
| [Go to File](#go-to-file) | `Ctrl+Shift+N` | `Cmd+Shift+O` |
| [Go to Symbol](#go-to-symbol) | `Ctrl+Alt+Shift+N` | `Cmd+Alt+O` |
| [Find Action](#find-action) | `Ctrl+Shift+A` | `Cmd+Shift+A` |
| [Find in File](#find-in-file) | `Ctrl+F` | `Cmd+F` |
| [Replace in File](#replace-in-file) | `Ctrl+R` | `Cmd+R` |
| [Find Next / Move to Next Occurrence](#find-next--move-to-next-occurrence) | `F3` | `Cmd+G` |
| [Find Previous / Move to Previous Occurrence](#find-previous--move-to-previous-occurrence) | `Shift+F3` | `Cmd+Shift+G` |
| [Find in Files…](#find-in-files) | `Ctrl+Shift+F` | `Cmd+Shift+F` |

## Search Everywhere

**Shortcut:** `Double Shift` · macOS `Double Shift` · action id `SearchEverywhere`

Searches classes, files, symbols, actions and text in one popup.

**Menu:** Navigate | Search Everywhere

![Search Everywhere](../../../screenshots/docs-search-everywhere.png)

More: [Search Everywhere and Find Action](../../user/search-everywhere.md)

## Go to Class

**Shortcut:** `Ctrl+N` · macOS `Cmd+O` · action id `GotoClass`

Finds a class, interface, trait or enum by name (camel humps work).

**Menu:** Navigate | Class…

![Go to Class](../../../screenshots/docs-goto-class.png)

More: [Search Everywhere and Find Action](../../user/search-everywhere.md)

## Go to File

**Shortcut:** `Ctrl+Shift+N` · macOS `Cmd+Shift+O` · action id `GotoFile`

Finds a project file by name or path.

**Menu:** Navigate | File…

![Go to File](../../../screenshots/docs-goto-file.png)

More: [Search Everywhere and Find Action](../../user/search-everywhere.md)

## Go to Symbol

**Shortcut:** `Ctrl+Alt+Shift+N` · macOS `Cmd+Alt+O` · action id `GotoSymbol`

Finds a function, method, property or constant by name.

**Menu:** Navigate | Symbol…

![Go to Symbol](../../../screenshots/docs-goto-symbol.png)

More: [Search Everywhere and Find Action](../../user/search-everywhere.md)

## Find Action

**Shortcut:** `Ctrl+Shift+A` · macOS `Cmd+Shift+A` · action id `GotoAction`

Finds any action by name and runs it (Find Action).

**Menu:** Navigate | Action…; Help | Find Action…

![Find Action](../../../screenshots/docs-find-action.png)

More: [Search Everywhere and Find Action](../../user/search-everywhere.md#find-action)

## Find in File

**Shortcut:** `Ctrl+F` · macOS `Cmd+F` · action id `Find`

Opens the find panel of the editor for the current file.

**Menu:** Edit | Find | Find…

![Find in File](../../../screenshots/docs-find-in-file.png)

More: [Editor](../../user/editor.md#find-and-replace-in-a-file)

## Replace in File

**Shortcut:** `Ctrl+R` · macOS `Cmd+R` · action id `Replace`

Opens the find panel with the replace field.

**Menu:** Edit | Find | Replace…

More: [Editor](../../user/editor.md#find-and-replace-in-a-file)

## Find Next / Move to Next Occurrence

**Shortcut:** `F3` · macOS `Cmd+G` · action id `FindNext`

Moves to the next match of the current search.

**Menu:** Edit | Find | Find Next / Move to Next Occurrence

More: [Editor](../../user/editor.md#find-and-replace-in-a-file)

## Find Previous / Move to Previous Occurrence

**Shortcut:** `Shift+F3` · macOS `Cmd+Shift+G` · action id `FindPrevious`

Moves to the previous match.

**Menu:** Edit | Find | Find Previous / Move to Previous Occurrence

More: [Editor](../../user/editor.md#find-and-replace-in-a-file)

## Find in Files

**Shortcut:** `Ctrl+Shift+F` · macOS `Cmd+Shift+F` · action id `FindInPath`

Searches the text of all project files, with match case, whole words, regular expressions, file masks and a folder scope.

**Menu:** Edit | Find | Find in Files…

![Find in Files…](../../../screenshots/docs-find-in-files.png)

More: [Search Everywhere and Find Action](../../user/search-everywhere.md#find-in-files)
