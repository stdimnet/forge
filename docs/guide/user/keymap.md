---
title: Keymap
description: Forge's PHPStorm-compatible keymap - the default keymaps for Windows/Linux and macOS, changing shortcuts, chords and double taps, conflicts and focus contexts.
order: 240
tags: [keymap, shortcuts, phpstorm]
---

# Keymap

Forge ships **PHPStorm's default keymaps** 1:1, for Windows/Linux and for macOS, keyed by
PHPStorm's action ids. Shortcuts you know from PHPStorm work the same, and actions that Forge has
not implemented yet stay bound and show "*Action* is not available yet" instead of doing
something unexpected.

The complete list with both platforms is in the [keymap reference](../reference/keymap.md).

## The most important shortcuts

| Action | Windows/Linux | macOS |
| --- | --- | --- |
| Search Everywhere | `Double Shift` | `Double Shift` |
| Find Action | `Ctrl+Shift+A` | `Cmd+Shift+A` |
| Go to Class / File / Symbol | `Ctrl+N` / `Ctrl+Shift+N` / `Ctrl+Alt+Shift+N` | `Cmd+O` / `Cmd+Shift+O` / `Cmd+Alt+O` |
| Recent Files | `Ctrl+E` | `Cmd+E` |
| Go to Declaration | `Ctrl+B`, `Ctrl+Click` | `Cmd+B`, `Cmd+Click` |
| Generate | `Alt+Insert` | `Cmd+N` |
| Reformat Code | `Ctrl+Alt+L` | `Cmd+Alt+L` |
| Settings | `Ctrl+Alt+S` | `Cmd+,` |
| Commit | `Ctrl+K` | `Cmd+K` |
| Run / Debug | `Shift+F10` / `Shift+F9` | `Ctrl+R` / `Ctrl+D` |
| Project / Terminal tool window | `Alt+1` / `Alt+F12` | `Cmd+1` / `Alt+F12` |
| New Scratch File | `Ctrl+Alt+Shift+Insert` | `Cmd+Shift+N` |
| Run Artisan Command (Laravel) | `Ctrl+Shift+X` | `Cmd+Shift+X` |

## Settings | Keymap

![Keymap settings](../../screenshots/keymap-settings.png)

- **Keymap**: *Platform default* (`auto`), *Windows / Linux* or *macOS* (`keymap.active`). You
  can use the macOS keymap on Linux and vice versa.
- The tree groups actions like PHPStorm: Main Menu (File, Edit, View, Navigate, Code,
  Refactor, Run, Git, Window, …), Editor Actions, Debugger Actions, Other and Plugins.
  Changed actions are highlighted.
- **Search actions** filters by name; **Find Actions by Shortcut** (the keyboard icon) filters
  by pressing the keys.

### Changing a shortcut

Select an action and press `Enter`, double-click it or use **Edit Shortcuts**:

- **Add Keyboard Shortcut** opens a dialog: press the keys (**First stroke**), optionally a
  **Second stroke** for a chord such as `Ctrl+K Ctrl+C`. The dialog lists conflicts ("The
  shortcut is already assigned to other actions") before you confirm; **Remove Others** takes
  the shortcut away from them.
- **Remove *shortcut*** removes one shortcut.
- **Reset Shortcuts** restores the defaults of the action; the page can also reset all changes.

![Conflict](../../screenshots/keymap-shortcut-conflict.png)

Changes are stored in the settings (`keymap.overrides.default` / `keymap.overrides.macOS`, one
entry per action: the action id followed by its shortcuts, separated by tabs), so they live in
`settings.toml` and not in browser storage.

> **Note:** mouse shortcuts ("Add Mouse Shortcut") and importing PHPStorm keymap files are not
> available yet.

## How shortcuts are written

| Notation | Meaning |
| --- | --- |
| `Ctrl+Shift+A` | keys pressed together |
| `Ctrl+K Ctrl+C` | a chord: the first stroke, then the second |
| `Double Shift` | press `Shift` twice quickly |
| `Cmd`, `Opt`/`Alt` | macOS modifiers |

Shortcuts use the physical key position, so they work with any keyboard layout (on a German
keyboard `Ctrl+Z` is the key labelled Z).

## Focus contexts

The same keys can mean different things depending on what has focus: the **editor**, a text
**input**, a **tool window** (for example `Alt+Insert` adds a row in a database grid but opens
Generate in the editor), or the **terminal**. In the terminal, plain `Ctrl`+letter,
`Alt`+letter and `Escape` go to the shell; function keys, `Alt`+digit, `Ctrl+Shift` and
`Ctrl+Alt` combinations still reach the IDE.

## Plugin actions

Plugins add their own actions under **Plugins** (and some under Main Menu), for example:

| Action | Default shortcut |
| --- | --- |
| Laravel: Run Artisan Command… | `Ctrl+Shift+X` (macOS `Cmd+Shift+X`) |
| Terminal: New Terminal Tab | `Ctrl+Shift+T` (macOS `Cmd+T`), in the terminal |
| Database: Execute | `Ctrl+Enter` (macOS `Cmd+Enter`), in a console |
| Database: Submit / Add Row / Delete Rows / Revert Selected | `Ctrl+Enter` / `Alt+Insert` / `Ctrl+Y` / `Ctrl+Alt+Z` in a data grid |
| Database: Jump to Query Console… | `Ctrl+Shift+F10` in the Database tool window |
| Symfony: Run Console Command…, Symfony Routes | none |
| Doctrine Entities, npm tool window, Reload npm Scripts | none |

All of them can be rebound like PHPStorm actions.
