---
title: Main menu reference
description: Every entry of Forge's main menu, menu by menu, with its shortcut, whether it is implemented and what it does.
order: 15
tags: [main menu, reference, menus]
---

# Main menu reference

Forge's main menu has PHPStorm's structure. Open it with the ☰ button or `Alt+\`, or show it as a
menu bar (Settings | Appearance & Behavior | Appearance | **Main menu**); see
[Workbench: Main menu](../../user/workbench.md#main-menu).

Entries Forge has not implemented yet are visible but **disabled**, so the menu doubles as a map
of what is there and what is still to come. Plugins add their own entries (Tools | Docker,
Tools | Laravel, Tools | Symfony, Run | Start Listening for PHP Debug Connections) while they are
active.

| Menu | Contents |
| --- | --- |
| [File](file.md) | projects, new files and scratches, settings, Save All |
| [Edit](edit.md) | undo, clipboard, find and replace, Find in Files, selection, line editing |
| [View](view.md) | tool windows, appearance, Parameter Info, Recent Files |
| [Navigate](navigate.md) | back/forward, Search Everywhere, Go to Class/File/Symbol/Line, declarations, problems |
| [Code](code.md) | completion, Generate, folding, comments, Reformat Code, moving lines |
| [Refactor](refactor.md) | refactorings (not implemented yet) |
| [Run](run.md) | run configurations, debugging actions, breakpoints, Xdebug listener |
| [Tools](tools.md) | terminal, Docker, Laravel, Symfony |
| [Git](git.md) | commit, push, update, branches, log, blame |
| [Window](window.md) | active tool window, editor tabs |
| [Help](help.md) | Find Action, Keymap Reference, About |

The menu structure, labels, mnemonics and enabled states are computed in Rust
(`crates/forge-keymap/src/menu.rs`); an entry is enabled when a command implements its action.
These pages were generated from the menu of the browser-mode Laravel demo project.
