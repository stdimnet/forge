---
title: Editor
description: Editing, completion, hover documentation, Parameter Info, Go to Declaration, Find and Show Usages, problems, formatting, folding, find and replace, gutter icons and the editor context menu.
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

**Go to Declaration or Usages** (`Ctrl+B`, macOS `Cmd+B`), `Ctrl+Click` (macOS `Cmd+Click`)
or a middle click jumps to the declaration of the symbol at the caret. While `Ctrl` is held,
the symbol under the mouse is underlined like a link, with a small tooltip that shows where it
is declared (in PHP: the namespace or class and the declaration).

Forge first asks its framework providers, which know things a language server cannot
resolve: Laravel route names, views, config keys and Eloquent attributes; Symfony service ids,
routes and Twig templates; Doctrine entities and DQL fields; React/Vue/Angular components and
props; Lua modules. Then it asks the language server and, in PHP files, Forge's PHP index and
the PHP stubs (see [PHP navigation](php.md#navigation)).

On a declaration itself (a class, method or function name), Go to Declaration shows its
**usages** instead, as in PHPStorm; a single usage opens directly.

When there are several targets — from a framework provider (a view that exists in two places, a
component used in many templates) or from the language server — the **Choose Declaration**
popup lists them at the caret with file, line and the code of each target (or the symbol name
and its namespace), as in PHPStorm. Pick one with the arrow keys and `Enter`, or `Escape` to
cancel. The same popup is used by line markers with several targets, in every language pack
(PHP and its frameworks, WebForge, LuaForge).

![Choose Declaration in React](../../screenshots/editor-polish-choose-declaration-react.png)

![Choose Declaration in Lua](../../screenshots/editor-polish-choose-declaration-lua.png)

If nothing is found, a hint "Cannot find declaration to go to" appears at the caret.

Declarations outside the project (PHP's own classes in the PHP stubs, files a language server
points to outside the project) open in a **read-only** tab with a "Read-only library file"
banner and breadcrumbs such as `PHP Runtime › Core › Core_c.php › Throwable`.

**Back** / **Forward** (`Ctrl+Alt+Left` / `Ctrl+Alt+Right`, macOS `Cmd+[` / `Cmd+]`) return
to where you were.

### Implementation, Type Declaration, Super Method

| Action | Shortcut | macOS |
| --- | --- | --- |
| Implementation(s) | `Ctrl+Alt+B`, `Ctrl+Alt+Click` | `Cmd+Alt+B`, `Cmd+Alt+Click` |
| Type Declaration | `Ctrl+Shift+B` | `Cmd+Shift+B` |
| Super Method | `Ctrl+U` | `Cmd+U` |

They ask the language servers (`textDocument/implementation`, `typeDefinition`) and fall back
to the PHP index in PHP files. When a server lacks the feature (Go to Implementation in
Intelephense without a licence) and the index finds nothing, the hint says so.

> **Note:** Go to Test (`Ctrl+Shift+T`) is not available yet. In React and Vue projects, Go to
> Declaration on a component's own name lists its usages through the framework provider.

## Find Usages and Show Usages

**Show Usages** (`Ctrl+Alt+F7`, macOS `Cmd+Alt+F7`) opens a popup at the caret with every
usage of the symbol: its code line with the reference highlighted, the file and the line,
grouped by usage type (import, new instance, method call, static access, extends/implements,
type declaration, PHPDoc, …). `Up`/`Down` select, `Enter` opens, `Escape` closes, the pin
button opens the result in the Find tool window.

**Find Usages** (`Alt+F7`) shows the usages in the **Find** tool window (`Alt+3`, macOS
`Cmd+3`), one tab per search, as a tree by usage type and file; **Group by Usage Type**,
**Expand All** and **Collapse All** are on its toolbar. The Find stripe button appears once
there is a result.

Usages come from the language servers (`textDocument/references`); in PHP files without a
server answer, from the PHP index. Unsaved changes in open editors are taken into account.

![Show Usages](../../screenshots/php-nav-show-usages.png)

![Find tool window](../../screenshots/php-nav-find-window.png)

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

## Rename

**Rename…** (`Shift+F6`) renames the symbol at the caret everywhere, like PHPStorm's in-place
rename:

1. The name gets an edit box and every occurrence in the file is highlighted. Type the new name;
   the occurrences change as you type.
2. **Enter** (or `Tab`, or moving the caret out of the box) renames it in the whole project,
   including files that are not open. **Escape** puts the old name back.
3. `Shift+F6` again opens the **Rename** dialog: the new name, **Rename file** for classes and
   **Preview**, which lists every change before anything is written.

![In-place rename](../../screenshots/rename-in-place.png)

When the rename would break something, the **Problems Detected** dialog lists the problems
first (a class or member with the new name already exists, a file with the new name is in the
way) together with every occurrence; **Continue** renames anyway, **Cancel** keeps the old name.

![Problems Detected](../../screenshots/rename-refactoring-preview.png)

![Rename dialog](../../screenshots/rename-dialog.png)

- The language server computes the rename where it can (TypeScript, Vue, Lua, Intelephense
  Premium, …); PHP files fall back to Forge's PHP index, see [PHP](php.md#rename).
- Renaming a class whose file is named after it (`Order.php` declaring `class Order`) also
  renames the file, like PHPStorm; untick **Rename file** in the hint below the edit box or in
  the dialog to keep the name. Open tabs follow the file.
- An invalid name ("'2fa' is not a valid identifier") keeps the edit box open.
- **Undo:** the whole rename is one step. `Ctrl+Z` (macOS `Cmd+Z`) in any editor the rename
  changed undoes it in every file: open editors, files that were not open (written back
  through the project, also on remote hosts) and a renamed file. `Ctrl+Shift+Z` redoes it.
  Files edited after the rename are left alone and a balloon names them. Redo is not
  available in a tab that undo reopened under its old name.
- Open files keep the change as unsaved edits; files that were not open are saved directly.
- In the **Project** view, `Shift+F6` renames the selected file or folder.
  **Refactor | Rename File…** renames the file of the current editor.

## Show Context Actions

**Show Context Actions** (`Alt+Enter`) opens PHPStorm's intention popup at the caret:

- **Quick fixes** (red bulb) from the language servers, for example "Remove unused import" or
  "Add method 'markAsPaid()' to class 'Order'";
- **Intentions** (yellow bulb) from the language servers and Forge's own PHP intentions;
- refactorings and source actions, and links like **Generate…** (`Alt+Insert`) last.

![Context actions](../../screenshots/context-actions.png)

Use `Up` / `Down` and `Enter`, or click. An entry with `›` has more options ("Fix all '…'
problems in file"): open them with `Right`, go back with `Left`. Typing filters the list.
`Escape` closes it. A chosen fix is applied as one undo step, also when it changes other files.

When actions are available at the caret line, a **light bulb** appears at the start of the line
after a short pause (red when a quick fix is available); click it to open the popup.

![Light bulb](../../screenshots/light-bulb.png)

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

> **Note:** Replace in Files (`Ctrl+Shift+R`) is not available yet. The Find tool window
> shows [usages](#find-usages-and-show-usages) and the Find in Files results you open there
> (see [Find tool window](workbench.md#find-tool-window)).

## Gutter

Left of the text, from outside in: line numbers, feature icons, Git change markers, fold
markers.

- **Git change markers** show added, modified and deleted lines compared with `HEAD`. Click
  one to see the change, roll it back or open the diff. See [Git](git.md#in-the-editor).
- **Line markers** of plugins: routes on controller actions, "go to template", entity ⇄
  repository, Angular component ⇄ template, compose services, and more. Click to navigate.
- **Implemented / overridden markers** in PHP: ↓ on classes with subclasses, interfaces and
  methods with implementations or overrides, ↑ on methods that implement or override one. See
  [PHP](php.md#gutter-icons).
- **Run icons** ▶ next to PHPUnit tests, `package.json` scripts, Lua main chunks, scratch
  files, `.http` requests, compose services and Dockerfiles.
- **Breakpoints**: click a line number in a PHP file to toggle a breakpoint (drawn over the
  number, as in the New UI), or press `Ctrl+F8` (macOS `Cmd+F8`). Right-click it to edit its
  condition. See [Xdebug debugging](debugging.md).
- **Bookmarks**: a bookmark icon, or the bookmark's mnemonic, on bookmarked lines (`F11`,
  `Ctrl+F11`). See [Bookmarks](workbench.md#bookmarks).
- **Annotate with Git Blame**: right-click the gutter. See [Git](git.md#blame).

## Context menu

Right-click in the editor (or press the `Menu` key on Windows/Linux keyboards) to open the
editor context menu:

![Editor context menu](../../screenshots/context-menu-editor.png)

| Entry | Status |
| --- | --- |
| Show Context Actions | works, see [Show Context Actions](#show-context-actions) |
| Paste from History… | not available yet |
| Cut, Copy, Paste | works |
| Copy / Paste Special > Copy Absolute Path, Copy Reference, Paste as Plain Text | works |
| Select All | works |
| Column Selection Mode | not available yet (use `Alt`+drag) |
| Find Usages | works |
| Refactor > Rename… | works, see [Rename](#rename) |
| Refactor > Refactor This… | not available yet |
| Go To > Declaration or Usages, Line/Column | works |
| Go To > Implementation(s) | works |
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
