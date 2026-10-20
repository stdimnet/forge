---
title: Editor
description: Editing, completion, hover documentation, Parameter Info, Go to Declaration, problems, formatting, folding, find and replace, gutter icons and the editor context menu.
order: 20
tags: [editor, completion, hover, navigation, diagnostics, context menu]
---

# Editor

Forge's editor is built on CodeMirror 6. Language intelligence (completion, hover, diagnostics,
Go to Declaration, formatting) comes from [language servers](language-servers.md) and from
Forge's own indexes for PHP, Lua and the frameworks. Shortcuts below are the Windows/Linux
defaults with macOS in parentheses; the full list is in the
[keymap reference](../reference/keymap.md).

## Code completion

- Completion pops up while you type, from the language server(s) of the file and, in
  framework projects, from Forge's framework sources (Laravel routes and views, Symfony
  services, Twig, DQL, …). Each item has a kind icon (class, method, property, variable,
  keyword, …).
- **Basic Completion** `Ctrl+Space` (also `Shift+Space`) opens it explicitly. Then words
  that already occur in the document are offered as well; this works in every file, even
  without a language server.
- `Enter` or `Tab` accepts. Functions are inserted with parentheses and the caret between
  them; snippet placeholders from the server can be filled in with `Tab`.
- Brackets and quotes are closed automatically.

![Completion](../../screenshots/lsp-completion.png)

## Quick documentation on hover

Hold the mouse over a symbol to see its documentation: signature, description, parameters
and return value, deprecation notes and links, rendered PHPStorm-style. Problems at that
position are shown in the same popup.

For PHP, Forge merges the language server's answer with its own index (PHPDoc, attributes,
inherited documentation) and falls back to the index when no server runs. Lua uses LuaForge's
index until lua-language-server answers.

![Hover documentation](../../screenshots/lsp-hover.png)

> **Note:** the keyboard actions Quick Documentation (`Ctrl+Q`) and External Documentation
> (`Shift+F1`) are not available yet; use the mouse hover.

## Parameter Info

**Parameter Info** (`Ctrl+P`, macOS `Cmd+P`) shows the parameters of the call at the caret,
the current one in bold, with its documentation. It also opens automatically after you type
`(` and follows you as you type `,`.

![Parameter Info](../../screenshots/lsp-parameter-info.png)

## Go to Declaration

**Go to Declaration or Usages** (`Ctrl+B`, macOS `Cmd+B`) or `Ctrl+Click` (macOS
`Cmd+Click`) jumps to the declaration of the symbol at the caret. While `Ctrl` is held, the
symbol under the mouse is underlined like a link.

Forge first asks its framework providers, which know things a language server cannot
resolve: Laravel route names, views, config keys and Eloquent attributes; Symfony service ids,
routes and Twig templates; Doctrine entities and DQL fields; React/Vue/Angular components and
props; Lua modules. Then it asks the language server.

When there are several targets — from a framework provider (a view that exists in two places, a
component used in many templates) or from the language server — the **Choose Declaration**
popup lists them at the caret with file, line and the code of each target, as in PHPStorm. Pick
one with the arrow keys and `Enter`, or `Escape` to cancel. The same popup is used by line
markers with several targets, in every language pack (PHP and its frameworks, WebForge,
LuaForge).

![Choose Declaration in React](../../screenshots/editor-polish-choose-declaration-react.png)

![Choose Declaration in Lua](../../screenshots/editor-polish-choose-declaration-lua.png)

If nothing is found, a hint "Cannot find declaration to go to" appears at the caret.

**Back** / **Forward** (`Ctrl+Alt+Left` / `Ctrl+Alt+Right`, macOS `Cmd+[` / `Cmd+]`) return
to where you were.

> **Note:** Go to Implementation (`Ctrl+Alt+B`), Type Declaration, Super Method, Test and
> Find Usages (`Alt+F7`) are not available yet. In React and Vue projects, Go to Declaration on
> a component's own name lists its usages instead.

## Problems

Diagnostics from language servers (and Forge's own syntax checks for YAML, JSON, INI, dotenv,
Bash, Python, Postfix and similar files) appear as:

- squiggles: red for errors, yellow for warnings, a weaker underline for weak warnings; unused
  code is grayed, deprecated usages are struck through;
- markers in the gutter;
- the **inspection widget** in the top-right corner of the editor: a ✓ when the file is clean,
  otherwise the number of errors and warnings. Click it to open the Problems tool window.

**Next Highlighted Error** `F2` and **Previous Highlighted Error** `Shift+F2` move the caret
between problems.

The **Problems** tool window (`Alt+6`, macOS `Cmd+6`) has two tabs: **Current File** and
**Project Errors**, each a file → problem tree. Click a problem to open it.

![Problems tool window](../../screenshots/lsp-problems.png)

> **Note:** quick-fixes and Show Context Actions (`Alt+Enter`) are not available yet.

## Reformat Code

**Reformat Code** (`Ctrl+Alt+L`, macOS `Cmd+Alt+L`) formats the file with the language
server's document formatting. Lua files use StyLua when it is installed (see
[LuaForge](luaforge.md)).

## Editing actions

| Action | Windows/Linux | macOS |
| --- | --- | --- |
| Duplicate Line or Selection | `Ctrl+D` | `Cmd+D` |
| Delete Line | `Ctrl+Y` | `Cmd+Backspace` |
| Comment with Line Comment | `Ctrl+/` | `Cmd+/` |
| Comment with Block Comment | `Ctrl+Shift+/` | `Cmd+Alt+/` |
| Move Line Up / Down | `Alt+Shift+Up` / `Down` | same |
| Move Statement Up / Down (moves the line) | `Ctrl+Shift+Up` / `Down` | `Cmd+Shift+Up` / `Down` |
| Extend / Shrink Selection | `Ctrl+W` / `Ctrl+Shift+W` | `Alt+Up` / `Alt+Down` |
| Add Selection for Next Occurrence | `Alt+J` | `Ctrl+G` |
| Unselect Occurrence | `Alt+Shift+J` | `Ctrl+Shift+G` |
| Select All Occurrences | `Ctrl+Alt+Shift+J` | `Ctrl+Cmd+G` |
| Join Lines | `Ctrl+Shift+J` | `Ctrl+Shift+J` |
| Toggle Case | `Ctrl+Shift+U` | `Cmd+Shift+U` |
| Split Line | `Ctrl+Enter` | `Cmd+Enter` |
| Start New Line | `Shift+Enter` | `Shift+Enter` |
| Start New Line Before Current | `Ctrl+Alt+Enter` | `Cmd+Alt+Enter` |
| Auto-Indent Lines | `Ctrl+Alt+I` | `Ctrl+Alt+I` |
| Move Caret to Matching Brace | `Ctrl+Shift+M` | `Ctrl+M` |
| Scroll Up / Down | `Ctrl+Up` / `Ctrl+Down` | — |
| Go to Line/Column | `Ctrl+G` | `Cmd+L` |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Shift+Z` | `Cmd+Z` / `Cmd+Shift+Z` |
| Copy Absolute Path | `Ctrl+Shift+C` | `Cmd+Shift+C` |
| Copy Reference (`path:line`) | `Ctrl+Alt+Shift+C` | `Cmd+Alt+Shift+C` |

Without a selection, **Copy** and **Cut** take the whole line, as in PHPStorm. `Alt`+drag
makes a rectangular (column) selection.

## Folding

Click the fold markers in the gutter, or use:

| Action | Windows/Linux | macOS |
| --- | --- | --- |
| Expand / Collapse | `Ctrl+=` / `Ctrl+-` | `Cmd+=` / `Cmd+-` |
| Expand / Collapse Recursively | `Ctrl+Alt+=` / `Ctrl+Alt+-` | `Cmd+Alt+=` / `Cmd+Alt+-` |
| Expand All / Collapse All | `Ctrl+Shift+=` / `Ctrl+Shift+-` | `Cmd+Shift+=` / `Cmd+Shift+-` |

## Find and replace in a file

**Find** (`Ctrl+F`, macOS `Cmd+F`) and **Replace** (`Ctrl+R`, macOS `Cmd+R`) open the search
panel of the editor (CodeMirror's search panel) with match case, regular expression and
whole word options. **Find Next**
`F3` and **Find Previous** `Shift+F3` (macOS `Cmd+G` / `Cmd+Shift+G`) move between matches.
Other occurrences of the selected text are highlighted automatically.

To search the whole project, use [Find in Files](search-everywhere.md#find-in-files)
(`Ctrl+Shift+F`, macOS `Cmd+Shift+F`).

> **Note:** Replace in Files (`Ctrl+Shift+R`) and the Find tool window are not available yet.

## Gutter

Left of the text, from outside in: line numbers, feature icons, Git change markers, fold
markers.

- **Git change markers** show added, modified and deleted lines compared with `HEAD`. Click
  one to see the change, roll it back or open the diff. See [Git](git.md#in-the-editor).
- **Line markers** of plugins: routes on controller actions, "go to template", entity ⇄
  repository, Angular component ⇄ template, compose services, and more. Click to navigate.
- **Run icons** ▶ next to PHPUnit tests, `package.json` scripts, Lua main chunks, scratch
  files, `.http` requests, compose services and Dockerfiles.
- **Breakpoints**: click a line number in a PHP file to toggle a breakpoint (drawn over the
  number, as in the New UI), or press `Ctrl+F8` (macOS `Cmd+F8`). Right-click it to edit its
  condition. See [Xdebug debugging](debugging.md).
- **Annotate with Git Blame**: right-click the gutter. See [Git](git.md#blame).

## Context menu

Right-click in the editor (or press the `Menu` key on Windows/Linux keyboards) to open the
editor context menu:

![Editor context menu](../../screenshots/context-menu-editor.png)

| Entry | Status |
| --- | --- |
| Show Context Actions | not available yet |
| Paste from History… | not available yet |
| Cut, Copy, Paste | works |
| Copy / Paste Special > Copy Absolute Path, Copy Reference, Paste as Plain Text | works |
| Select All | works |
| Column Selection Mode | not available yet (use `Alt`+drag) |
| Find Usages | not available yet |
| Refactor > Rename…, Refactor This… | not available yet |
| Go To > Declaration or Usages, Line/Column | works |
| Go To > Implementation(s) | not available yet |
| Generate… | works in PHP files, see [PHP](php.md#generate) |
| Reformat Code | works |
| Optimize Imports | not available yet |
| Comment with Line Comment | works |
| Git > Annotate with Git Blame | works |

Right-clicking outside the current selection moves the caret there first, like PHPStorm.

## Files changed on disk

When a file changes on disk (a checkout, a pull, a rollback, an AI agent's edit, a change in
another editor), the open tab is reloaded with the new content. The reload is not an undo
step: `Ctrl+Z` does not go back across it.

Line endings are preserved: a file with Windows line endings (CRLF) stays CRLF when you save
it.

## Soft wraps and appearance

Settings | Editor | General controls soft wrapping (`editor.softWraps` with the patterns in
`editor.softWrapFiles`, by default `*.md; *.txt; *.rst; *.adoc`), whitespace, line numbers,
the caret row highlight, caret blinking and smooth scrolling. Settings | Editor | Font sets the
font, size, line height and ligatures. See the [settings reference](../reference/settings.md).

> **Note:** "Remove trailing spaces on save", "Ensure line break at end of file", the
> Code Style options and Inlay Hints are listed in Settings but not applied yet.
