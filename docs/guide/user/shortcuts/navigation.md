---
title: "Shortcuts: Navigation"
description: "Jumping to declarations, implementations, super methods, lines, problems, recent files and between editor tabs."
order: 30
tags: [shortcuts, keymap, navigation]
---

# Shortcuts: Navigation

Jumping to declarations, implementations, super methods, lines, problems, recent files and between editor tabs. Windows/Linux shortcuts first, macOS second; both are PHPStorm's defaults and can be changed in Settings | Keymap.

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| [Go to Declaration or Usages](#go-to-declaration-or-usages) | `Ctrl+B` | `Cmd+B` |
| [Go to Implementation(s)](#go-to-implementations) | `Ctrl+Alt+B` | `Cmd+Alt+B` |
| [Go to Type Declaration](#go-to-type-declaration) | `Ctrl+Shift+B` | `Cmd+Shift+B` |
| [Go to Super Method](#go-to-super-method) | `Ctrl+U` | `Cmd+U` |
| [Back](#back) | `Ctrl+Alt+Left` | `Cmd+[`, `Cmd+Alt+Left` |
| [Forward](#forward) | `Ctrl+Alt+Right` | `Cmd+]`, `Cmd+Alt+Right` |
| [Go to Line/Column](#go-to-linecolumn) | `Ctrl+G` | `Cmd+L` |
| [Move Caret to Matching Brace](#move-caret-to-matching-brace) | `Ctrl+Shift+M` | `Ctrl+M` |
| [Next Highlighted Error](#next-highlighted-error) | `F2` | `F2` |
| [Previous Highlighted Error](#previous-highlighted-error) | `Shift+F2` | `Shift+F2` |
| [Recent Files](#recent-files) | `Ctrl+E` | `Cmd+E` |
| [Recently Changed Files](#recently-changed-files) | — | — |
| [Switcher](#switcher) | `Ctrl+Tab`, `Ctrl+Shift+Tab` | `Ctrl+Tab`, `Ctrl+Shift+Tab` |
| [Select Next Tab](#select-next-tab) | `Alt+Right` | `Ctrl+Right`, `Cmd+Shift+]` |
| [Select Previous Tab](#select-previous-tab) | `Alt+Left` | `Ctrl+Left`, `Cmd+Shift+[` |
| [Close Tab](#close-tab) | `Ctrl+F4` | `Cmd+W` |
| [Close All Tabs](#close-all-tabs) | — | — |
| [Close Other Tabs](#close-other-tabs) | — | `Cmd+Alt+W` |
| [Close Unmodified Tabs](#close-unmodified-tabs) | — | — |
| [Reopen Closed Tab](#reopen-closed-tab) | — | — |

## Go to Declaration or Usages

**Shortcut:** `Ctrl+B` · macOS `Cmd+B` · action id `GotoDeclaration`

Jumps to the declaration of the symbol at the caret (also `Ctrl+Click` and middle click); on a declaration it shows its usages; several targets open the Choose Declaration popup. PHP's own classes and functions open read-only in the PHP stubs.

**Menu:** Navigate | Declaration or Usages

![Ctrl+hover on \Throwable](../../../screenshots/php-nav-ctrl-hover.png)

More: [Editor](../../user/editor.md#go-to-declaration), [PHP navigation](../../user/php.md#navigation)

## Go to Implementation(s)

**Shortcut:** `Ctrl+Alt+B` · macOS `Cmd+Alt+B` · action id `GotoImplementation`

Lists the classes implementing or extending the class or interface at the caret, or the methods overriding the method (also `Ctrl+Alt+Click`).

**Menu:** Navigate | Implementation(s)

More: [Editor](../../user/editor.md#implementation-type-declaration-super-method)

## Go to Type Declaration

**Shortcut:** `Ctrl+Shift+B` · macOS `Cmd+Shift+B` · action id `GotoTypeDeclaration`

Jumps to the class of the variable, property or method return type at the caret.

**Menu:** Navigate | Type Declaration

More: [Editor](../../user/editor.md#implementation-type-declaration-super-method)

## Go to Super Method

**Shortcut:** `Ctrl+U` · macOS `Cmd+U` · action id `GotoSuperMethod`

Jumps to the method the current method overrides or implements; on a class, to its parent class and interfaces.

**Menu:** Navigate | Super Method

More: [Editor](../../user/editor.md#implementation-type-declaration-super-method)

## Back

**Shortcut:** `Ctrl+Alt+Left` · macOS `Cmd+[`, `Cmd+Alt+Left` · action id `Back`

Goes back to the previous caret location in your navigation history.

**Menu:** Navigate | Back

More: [Editor](../../user/editor.md#go-to-declaration)

## Forward

**Shortcut:** `Ctrl+Alt+Right` · macOS `Cmd+]`, `Cmd+Alt+Right` · action id `Forward`

Goes forward in the navigation history.

**Menu:** Navigate | Forward

More: [Editor](../../user/editor.md#go-to-declaration)

## Go to Line/Column

**Shortcut:** `Ctrl+G` · macOS `Cmd+L` · action id `GotoLine`

Moves the caret to a line (and column) of the current file.

**Menu:** Navigate | Line/Column…

![Go to Line/Column](../../../screenshots/docs-goto-line.png)

More: [Editor](../../user/editor.md#editing-actions)

## Move Caret to Matching Brace

**Shortcut:** `Ctrl+Shift+M` · macOS `Ctrl+M` · action id `EditorMatchBrace`

Moves the caret to the matching bracket.

More: [Editor](../../user/editor.md#editing-actions)

## Next Highlighted Error

**Shortcut:** `F2` · macOS `F2` · action id `GotoNextError`

Moves the caret to the next problem in the file.

**Menu:** Navigate | Next Highlighted Error

More: [Editor](../../user/editor.md#problems)

## Previous Highlighted Error

**Shortcut:** `Shift+F2` · macOS `Shift+F2` · action id `GotoPreviousError`

Moves the caret to the previous problem in the file.

**Menu:** Navigate | Previous Highlighted Error

More: [Editor](../../user/editor.md#problems)

## Recent Files

**Shortcut:** `Ctrl+E` · macOS `Cmd+E` · action id `RecentFiles`

Lists recently opened files and the tool windows; press again for recently edited files only.

**Menu:** View | Recent Files

![Recent Files](../../../screenshots/docs-recent-files.png)

More: [Workbench and tool windows](../../user/workbench.md#recent-files-and-switcher)

## Recently Changed Files

**Shortcut:** — · macOS — · action id `RecentChangedFiles`

Lists recently edited files.

**Menu:** View | Recently Changed Files

More: [Workbench and tool windows](../../user/workbench.md#recent-files-and-switcher)

## Switcher

**Shortcut:** `Ctrl+Tab`, `Ctrl+Shift+Tab` · macOS `Ctrl+Tab`, `Ctrl+Shift+Tab` · action id `Switcher`

Switches between open files and tool windows while `Ctrl` is held.

![Switcher](../../../screenshots/docs-switcher.png)

More: [Workbench and tool windows](../../user/workbench.md#recent-files-and-switcher)

## Select Next Tab

**Shortcut:** `Alt+Right` · macOS `Ctrl+Right`, `Cmd+Shift+]` · action id `NextTab`

Selects the next editor tab.

**Menu:** Window | Editor Tabs | Select Next Tab

More: [Workbench and tool windows](../../user/workbench.md#editor-tabs)

## Select Previous Tab

**Shortcut:** `Alt+Left` · macOS `Ctrl+Left`, `Cmd+Shift+[` · action id `PreviousTab`

Selects the previous editor tab.

**Menu:** Window | Editor Tabs | Select Previous Tab

More: [Workbench and tool windows](../../user/workbench.md#editor-tabs)

## Close Tab

**Shortcut:** `Ctrl+F4` · macOS `Cmd+W` · action id `CloseContent`

Closes the current editor tab.

**Menu:** Window | Editor Tabs | Close Tab

More: [Workbench and tool windows](../../user/workbench.md#editor-tabs)

## Close All Tabs

**Shortcut:** — · macOS — · action id `CloseAllEditors`

Closes all editor tabs.

**Menu:** Window | Editor Tabs | Close All Tabs

More: [Workbench and tool windows](../../user/workbench.md#editor-tabs)

## Close Other Tabs

**Shortcut:** — · macOS `Cmd+Alt+W` · action id `CloseAllEditorsButActive`

Closes all editor tabs except the current one.

**Menu:** Window | Editor Tabs | Close Other Tabs

More: [Workbench and tool windows](../../user/workbench.md#editor-tabs)

## Close Unmodified Tabs

**Shortcut:** — · macOS — · action id `CloseAllUnmodifiedEditors`

Closes the editor tabs without unsaved changes.

**Menu:** Window | Editor Tabs | Close Unmodified Tabs

More: [Workbench and tool windows](../../user/workbench.md#editor-tabs)

## Reopen Closed Tab

**Shortcut:** — · macOS — · action id `ReopenClosedTab`

Reopens the editor tab closed last.

**Menu:** Window | Editor Tabs | Reopen Closed Tab

More: [Workbench and tool windows](../../user/workbench.md#editor-tabs)
