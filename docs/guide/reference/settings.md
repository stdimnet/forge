---
title: Settings reference
description: Every setting key Forge's plugins declare, with type, default, scope and where it appears in the Settings dialog.
order: 20
tags: [settings, reference, configuration]
---

# Settings reference

All settings are declared by plugin manifests (`[[contributions.settings]]` in
`app/src-tauri/plugins/*.toml`) and shown on the Settings page whose id is their `group`. Values
are layered: default < user (`<config dir>/forge/settings.toml`) < Settings for New Projects
(`<config dir>/forge/new-project-settings.toml`, project-level settings only) < project
(`<project>/.forge/settings.toml`). See [Settings and themes](../user/settings-and-themes.md).

- **Type**: `bool`, `int`, `number`, `string`, `enum` (allowed values listed), `path`,
  `list` (a list of strings) or `objects` (a list of tables with string values, edited in a
  table). Ranges are given as (min–max).
- **Scope**: `user` settings are global; `user, project` settings can also be stored per
  project ("For current project" in the dialog).
-: the option is declared and shown, but no code reads it on this version.

Settings written by Forge itself and not declared in a manifest:

| Key | Meaning |
| --- | --- |
| `plugins.disabled` | list of disabled plugin ids (Settings \| Plugins) |
| `project.announcedFrameworks` | `<root>#<framework>` pairs whose "project detected" balloon was shown |
| `langservers.servers.<id>.enabled` | per-server toggle on the Language Servers page |
| `langservers.servers.<id>.path` | custom executable of a language server |
| `langservers.servers.<id>.settings` | extra JSON settings of a language server |
| `langservers.hints.dismissed` | servers whose install banner was turned off (also declared below) |
| `<language>.codeStyle.<option>`, `editor.codeStyle.<option>` | the code style options of the Default scheme (user settings) and the Project scheme (project settings), e.g. `php.codeStyle.indent`, `php.codeStyle.spaces.afterComma`, `editor.codeStyle.hardWrapAt`; the options are listed in [Editor](../user/editor.md#code-style) and defined in `crates/forge-codestyle` |
| `editor.codeStyle.scheme` | the scheme the project uses: `default`, `project` or an IDE scheme id |
| `editor.codeStyle.schemes.<id>.name`, `editor.codeStyle.schemes.<id>.values.<language>.<option>` | further IDE code style schemes (Duplicate, Copy to IDE, Import Scheme) |

### Appearance & Behavior | Appearance

Page id `appearance`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `appearance.theme` | enum: `dark`, `light`, `solarized-dark`, `solarized-light` | `dark` | user |  | Theme |
| `appearance.zoom` | enum: `50`, `67`, `75`, `80`, `90`, `100`, `110`, `125`, `150`, `175`, `200`, `250`, `300` | `100` | user |  | Zoom — Scales the whole IDE: menus, tool windows, dialogs and the editor (View \| Appearance \| Zoom IDE). |
| `appearance.uiFont.family` | enum: `Inter`, `system-ui`, `Segoe UI`, `Roboto` | `Inter` | user | Font | Font — Font used for menus, tool windows and dialogs. |
| `appearance.uiFont.size` | int (10–20) | `13` | user | Font | Size |
| `appearance.mainMenu` | enum: `hamburger`, `merged`, `toolbar` | `hamburger` | user | UI Options | Main menu — Hamburger menu: behind the button at the left of the main toolbar. Merged: the menu names in the main toolbar. Separate toolbar: a classic menu bar above the main toolbar. |
| `appearance.compactMode` | bool | `false` | user | UI Options | Compact mode — Reduces paddings and the height of toolbars and tool window headers. |
| `appearance.smoothScrolling` | bool | `true` | user | UI Options | Smooth scrolling — Animate scrolling in lists and trees. |
| `appearance.showSplash` | bool | `true` | user | UI Options | Show splash screen on startup — The Forge wordmark fills with molten metal while the IDE loads, and offers updates. |
| `appearance.windowControls` | enum: `auto`, `windows`, `macos`, `gnome`, `kde`, `linux`, `native` | `auto` | user | Window | Window controls — Look of the title bar buttons of the main window and dialogs. "Native" lets the operating system draw the title bar. |
| `appearance.showToolWindowBars` | bool | `true` | user | Tool Windows | Show tool window bars — The stripes with tool window buttons along the window edges. |
| `appearance.showToolWindowNumbers` | bool | `true` | user | Tool Windows | Show tool window numbers |
| `appearance.presentationFontSize` | int (8–96) | `24` | user | Presentation Mode | Font size — Editor font size in Presentation Mode (View \| Appearance \| Enter Presentation Mode). |
| `workbench.layouts` | objects (unique `name`) | `[]` | user | — | Layouts — Tool window layouts saved with Window \| Layouts \| Save Current Layout as New…; each entry has a name and the layout as JSON. |
| `appearance.navigationBar.location` | enum: `top`, `statusBar`, `hidden` | `top` | user | Navigation Bar | Navigation bar — Where the navigation bar (project, folders, file and members at the caret) is shown: Top: above the editor, with the path in the status bar too. In status bar: only in the status bar. Don't show: Jump to Navigation Bar (Alt+Home) shows it as a popup. View \| Appearance \| Navigation Bar. |
| `appearance.navigationBar.showMembers` | bool | `true` | user | Navigation Bar | Show members in the navigation bar — Add the class and function at the caret after the file name. View \| Appearance \| Members in Navigation Bar. |
| `projectView.openFilesWithSingleClick` | bool | `true` | user | Project View | Open files with single click — In the Project tool window a single click opens files and expands folders. Turn it off to select with a single click and open with a double click, like PHPStorm. |
| `projectView.autoscrollFromSource` | bool | `false` | user | Project View | Always select opened file — The Project tool window selects the file of the active editor whenever you switch editors (PHPStorm's Autoscroll from Source). Also in the tool window's Options menu. |
| `projectView.showExcludedFiles` | bool | `true` | user | Project View | Show excluded files — The Project tool window lists excluded folders and files (orange) instead of hiding them. Options menu (⋮) › Show Excluded Files. |
| `projectView.showMembers` | bool | `false` | user | Project View | Show members — PHP, JavaScript, TypeScript and Lua files expand to their classes, functions, methods and properties; a click on a member opens it. Options menu (⋮) › Show Members. |
| `projectView.compactMiddlePackages` | bool | `false` | user | Project View | Compact middle packages — Folders that contain only one folder are shown as one row (`src/Acme/Shop`). Options menu (⋮) › Compact Middle Packages. |
| `projectView.foldersAlwaysOnTop` | bool | `true` | user | Project View | Folders always on top — Folders come before files; turn it off to sort folders and files together. Options menu (⋮) › Folders Always on Top. |
| `projectView.sortByType` | bool | `false` | user | Project View | Sort by type — Files are sorted by their extension, then by name. Options menu (⋮) › Sort by Type. |
| `projectView.fileNesting.enabled` | bool | `true` | user | Project View | File nesting — Files with the same name as another file and a child suffix of a nesting rule are shown below it (`app.js` below `app.ts`). Options menu (⋮) › File Nesting… |
| `projectView.fileNesting.rules` | objects (unique `parent`) | `[]` | user | Project View | File nesting rules — Parent file suffix and child file suffixes (separated by `;`) of File Nesting. Empty uses PHPStorm's predefined rules. Edit them in the Project tool window: Options menu (⋮) › File Nesting… |
| `structureView.autoscrollToSource` | bool | `true` | user | Structure View | Autoscroll to source — A single click on a Structure tool window node moves the editor to it. Turn it off to select with a single click and navigate with a double click. |
| `structureView.autoscrollFromSource` | bool | `true` | user | Structure View | Autoscroll from source — The Structure tool window selects the member at the caret as you move through the editor. |

### Appearance & Behavior | Scopes

Page id `appearance.scopes` (custom page).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `scopes.shared` | objects (`name`, `pattern`) | `[]` | project |  | Shared scopes, stored in `.forge/settings.toml` |
| `scopes.local` | objects (`name`, `pattern`) | `[]` | user |  | Local scopes |

### Appearance & Behavior | File Colors

Page id `appearance.fileColors` (custom page).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `fileColors.enabled` | bool | `true` | user |  | Enable file colors |
| `fileColors.useInEditorTabs` | bool | `true` | user |  | Use in editor tabs |
| `fileColors.useInProjectView` | bool | `true` | user |  | Use in project view |
| `fileColors.local` | objects (`scope`, `color`) | Non-Project Files: Yellow, Tests: Green | user |  | Local colors |
| `fileColors.shared` | objects (`scope`, `color`) | `[]` | project |  | Shared colors |

### Appearance & Behavior | System Settings

Page id `system`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `system.reopenLastProject` | bool | `true` | user | Project | Reopen projects on startup — the last local project, when Forge starts without a project argument |
| `system.confirmExit` | bool | `true` | user | Project | Confirm before exiting the IDE |
| `system.openProjectIn` | enum: `newWindow`, `thisWindow`, `ask` | `ask` | user | Project | Open project in — Where File \| Open and the recent projects open a project while another one is open in this window. |
| `system.defaultProjectDirectory` | path | `~/Projects` | user | Project | Default project directory — New projects are created here unless you pick another location. |
| `system.autosave.onFrameDeactivation` | bool | `true` | user | Autosave | Save files if the IDE is switched to a different application |
| `system.autosave.onIdle` | bool | `false` | user | Autosave | Save files if the IDE is idle |
| `system.autosave.idleSeconds` | int (1–600) | `15` | user | Autosave | Idle time (seconds) |
| `system.syncExternalChanges` | bool | `true` | user | Synchronization | Synchronize external changes when switching to the IDE window |

### Appearance & Behavior | Notifications

Page id `notifications`. The display mode of a group is also set from the context menu of a
notification in the Notifications tool window.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `notifications.balloons` | bool | `true` | user |  | Display balloon notifications — Off: notifications only appear in the Notifications tool window. |
| `notifications.groups` | list | `[]` | user |  | Notification groups — How each group is shown, one entry per group: the group name, a tab, then `balloon`, `sticky` or `log`. |

### Appearance & Behavior | System Settings | Updates

Page id `system.updates`. The page also shows whether this build can update, the current
version, the last check, **Check Now** and a skipped version with **Offer Again** (stored in
`updates.json`, see [File locations](file-locations.md)).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `updates.checkOnStartup` | bool | `true` | user | Automatic Checks | Check for updates on startup — The splash screen checks at most once a day. Help \| Check for Updates… checks at any time. |
| `updates.channel` | enum: `stable`, `beta` | `stable` | user | Automatic Checks | Update channel — Beta also offers pre-releases. |
| `updates.endpoint` | string | `""` | user | Advanced | Update feed URL — The latest.json of the update feed. Empty: the GitHub releases of Forge for the selected channel. Updates must be signed with the key this build was made with. |

### Keymap

Page id `keymap`. The override lists and user keymaps are edited through the Keymap page, not
as text.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `keymap.active` | string | `auto` | user |  | Keymap — auto (the PHPStorm keymap of the platform), $default, macOS, a bundled keymap (VSCode, VSCode OSX, Eclipse, Eclipse (Mac OS X), Sublime Text, Sublime Text (Mac OS X), Emacs) or the name of a user keymap. |
| `keymap.overrides.default` | list | `[]` | user |  | Changed shortcuts (Windows / Linux) — One entry per action: the action id, then its shortcuts, separated by tabs. |
| `keymap.overrides.macOS` | list | `[]` | user |  | Changed shortcuts (macOS) — One entry per action: the action id, then its shortcuts, separated by tabs. |
| `keymap.overrides.bundled` | list | `[]` | user |  | Changed shortcuts (other bundled keymaps) — One entry per action: the keymap id, the action id, then its shortcuts, separated by tabs. |
| `keymap.custom` | objects (`name`) | `[]` | user |  | User keymaps — Keymaps made with Duplicate or Derive: name, parent keymap and the changed shortcuts (one action per line: the action id, then its shortcuts, separated by tabs). |

### Find Usages

Group `findUsages`, edited in the Find Usages dialog (`Ctrl+Alt+Shift+F7`), not on a Settings
page. Every Find Usages and Show Usages run uses them.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `findUsages.options.classUsages` | bool | `true` | user |  | Usages (classes, traits, enums) |
| `findUsages.options.classMethodUsages` | bool | `false` | user |  | Usages of methods (classes) |
| `findUsages.options.classFieldUsages` | bool | `false` | user |  | Usages of fields (classes) |
| `findUsages.options.classDerivedClasses` | bool | `false` | user |  | Derived classes |
| `findUsages.options.interfaceUsages` | bool | `true` | user |  | Usages (interfaces) |
| `findUsages.options.interfaceMethodUsages` | bool | `false` | user |  | Usages of methods (interfaces) |
| `findUsages.options.interfaceImplementingClasses` | bool | `false` | user |  | Implementing classes |
| `findUsages.options.interfaceDerivedInterfaces` | bool | `false` | user |  | Derived interfaces |
| `findUsages.options.methodUsages` | bool | `true` | user |  | Usages (methods) |
| `findUsages.options.methodOverridingMethods` | bool | `false` | user |  | Overriding methods |
| `findUsages.options.readAccess` | bool | `true` | user |  | Read access (properties, variables) |
| `findUsages.options.writeAccess` | bool | `true` | user |  | Write access (properties, variables) |
| `findUsages.options.usages` | bool | `true` | user |  | Usages (functions, constants, other symbols) |
| `findUsages.options.searchInCommentsAndStrings` | bool | `false` | user |  | Search in comments and strings |
| `findUsages.options.searchForTextOccurrences` | bool | `false` | user |  | Search for text occurrences |
| `findUsages.scope.kind` | enum: `projectFiles`, `projectAndLibraries`, `openFiles`, `currentFile`, `recentlyChangedFiles`, `directory`, `custom` | `projectFiles` | user |  | Scope |
| `findUsages.scope.directory` | string | `""` | user |  | Directory |
| `findUsages.scope.includeSubdirectories` | bool | `true` | user |  | Include subdirectories |
| `findUsages.scope.pattern` | string | `""` | user |  | Custom scope — A scope pattern (`file:app//*.php&&!test:*..*`) or file globs separated by ';', where '!' excludes (app/**/*.php; !app/Legacy/**). |
| `findUsages.scope.name` | string | `""` | user |  | Named scope — a scope of the Scopes page used as the custom scope; empty uses the pattern. |
| `findUsages.openInNewTab` | bool | `true` | user |  | Open in new tab |

### Editor | General

Page id `editor.general`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.smoothScrolling` | bool | `true` | user | Scrolling | Enable smooth scrolling |
| `editor.caret.blinking` | bool | `true` | user | Caret | Caret blinking |
| `editor.caret.blinkPeriod` | int (100–2000) | `500` | user | Caret | Blink period (ms) |
| `editor.softWraps` | bool | `false` | user | Soft Wraps | Soft-wrap files — Wrap long lines visually in files matching the patterns below. |
| `editor.softWrapFiles` | string | `*.md; *.txt; *.rst; *.adoc` | user | Soft Wraps | File patterns |
| `editor.showWhitespace` | bool | `false` | user | Appearance | Show whitespaces |
| `editor.showLineNumbers` | bool | `true` | user | Appearance | Show line numbers |
| `editor.highlightCurrentLine` | bool | `true` | user | Appearance | Highlight caret row |
| `editor.renderDocComments` | bool | `false` | user | Appearance | Render documentation comments — Show PHPDoc, JSDoc and LuaDoc comments as formatted text. Toggle one with Ctrl+Alt+Q or its gutter icon. |
| `editor.stickyLines.enabled` | bool | `true` | user | Sticky Lines | Show sticky lines while scrolling — The headers of the classes, functions, blocks, rules and tags around the top of the editor stay visible. Click one to jump to it. |
| `editor.stickyLines.maxLines` | int (1–20) | `5` | user | Sticky Lines | Limit sticky lines |
| `editor.stickyLines.disabledLanguages` | list | `[]` | user | Sticky Lines | Show sticky lines for — one checkbox per language; holds the file type ids switched off (`php`, `vue`, `lua`). |
| `editor.stripTrailingSpaces` | enum: `none`, `modified`, `all` | `modified` | user | On Save | Remove trailing spaces on |
| `editor.ensureNewlineAtEof` | bool | `true` | user | On Save | Ensure every saved file ends with a line break |
| `editor.clipboard.maxContents` | int (1–100) | `20` | user | Limits | Maximum number of contents to keep in clipboard |
| `editor.highlightUsagesAtCaret` | bool | `true` | user | Highlight on Caret Movement | Highlight usages of element at caret — Highlights the usages of the symbol at the caret in the file (reads and writes in different colors), also on the scrollbar. |
| `editor.colorPreview.gutter` | bool | `true` | user | Appearance | Show color preview in the gutter — Swatches next to colors in CSS, markup and scripts; click one to open the color picker. |
| `editor.colorPreview.inline` | bool | `false` | user | Appearance | Show color preview inline |
| `run.console.softWrap` | bool | `false` | user | Console | Use soft wraps in console |
| `run.console.cycleBufferKb` | int (100–100000) | `1024` | user | Console | Override console cycle buffer size (KB) — Run consoles drop their oldest lines beyond this size. Applies to runs started afterwards. |

### Editor | General | Editor Tabs

Page id `editor.tabs`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.tabs.limit` | int (0–100) | `10` | user | Closing Policy | Tab limit — Maximum number of tabs per editor group; 0 keeps every tab open. Pinned tabs are never closed by the limit. |
| `editor.tabs.closeUnmodifiedFirst` | bool | `true` | user | Closing Policy | When the tab limit is reached, close non-modified tabs first |
| `editor.tabs.activateOnClose` | enum: `left`, `right`, `recent` | `recent` | user | Closing Policy | When the current tab is closed, activate |
| `editor.tabs.showPinnedIcon` | bool | `true` | user | Appearance | Show pin icon on pinned tabs |

### Editor | General | Breadcrumbs

Page id `editor.breadcrumbs`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.breadcrumbs.show` | bool | `true` | user |  | Show breadcrumbs — The classes, functions, rules and tags around the caret. Click one to go there; the arrow next to it lists its siblings. |
| `editor.breadcrumbs.placement` | enum: `top`, `bottom` | `bottom` | user |  | Placement |
| `editor.breadcrumbs.disabledLanguages` | list | `[]` | user | Languages | Show breadcrumbs for — one checkbox per language; holds the file type ids switched off. |

### Editor | General | Auto Import

Page id `editor.general.auto-import`. See [Optimize Imports and Auto Import](../user/imports.md).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `php.autoImport.addOnTheFly` | bool | `false` | user | PHP | Add unambiguous imports on the fly — Import a class as soon as you stop typing its name when exactly one class has that name. |
| `php.autoImport.optimizeOnTheFly` | bool | `false` | user | PHP | Optimize imports on the fly — Remove unused 'use' statements while you edit, but not while you work on the imports or type a name one of them provides. |
| `php.autoImport.showPopup` | bool | `true` | user | PHP | Show import popup — Show a hint with the class to import on unresolved class names; press Alt+Enter to import it. |
| `php.autoImport.namespaceScope` | bool | `true` | user | PHP | Enable auto-import in namespace scope — Completion adds a 'use' statement in files with a namespace; otherwise it inserts the fully qualified name. |
| `php.autoImport.fileScope` | bool | `false` | user | PHP | Enable auto-import in file scope — Completion adds a 'use' statement in files without a namespace. |
| `php.autoImport.fqnInPhpDoc` | bool | `false` | user | PHP | Prefer FQN in PHPDoc — Class names completed or imported in PHPDoc comments are written fully qualified. |
| `php.autoImport.globalNamespace` | bool | `true` | user | PHP | Import classes from the global namespace — Add 'use Exception;' for PHP's own classes in namespaced files instead of writing '\Exception'. |
| `javascript.autoImport.addOnTheFly` | bool | `false` | user | JavaScript and TypeScript | Add unambiguous imports on the fly — Import names the TypeScript server can resolve to exactly one module after you stop typing. |
| `javascript.autoImport.optimizeOnTheFly` | bool | `false` | user | JavaScript and TypeScript | Optimize imports on the fly — Remove unused imports while you edit, but not on the lines you are editing. |
| `javascript.autoImport.showPopup` | bool | `true` | user | JavaScript and TypeScript | Show import popup — Show a hint with the module to import from on unresolved names; press Alt+Enter to import. |
| `javascript.autoImport.pathStyle` | enum: `shortest`, `project-relative`, `relative`, `non-relative` | `shortest` | user | JavaScript and TypeScript | Use paths relative to — How completion and Auto Import write the module path of a new import. |
| `javascript.autoImport.typeImports` | bool | `false` | user | JavaScript and TypeScript | Use type imports in TypeScript — Import types with 'import type' when nothing else is imported from the module. |

### Editor | General | Postfix Completion

Page id `editor.postfix`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.postfix.enabled` | bool | `true` | user |  | Enable postfix completion — Type a template key after an expression, e.g. $user.if or items.forof, to wrap it. |
| `editor.postfix.showInCompletion` | bool | `true` | user |  | Show postfix templates in the completion autopopup |
| `editor.postfix.expandKey` | enum: `tab`, `space`, `enter` | `tab` | user |  | Expand templates with |
| `editor.postfix.disabled` | list | `[]` | user |  | Templates — Clear a template to stop offering it. Holds the ids of the switched-off templates (`php.echo`, `js.log`, `lua.print`). |

### Editor | Color Scheme

Page id `editor.color-scheme`. User schemes are edited through the Color Scheme page, not as
text.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.colorScheme.active` | string | `""` | user |  | Scheme — Color scheme of the editor: Ember, Anvil or the name of a user scheme. Empty follows the theme (Ember for dark themes, Anvil for light ones). |
| `editor.colorScheme.schemes` | objects (`name`) | `[]` | user |  | User schemes — Schemes made with Duplicate or Import: name, parent (Ember or Anvil) and the changed attributes, one per line: the attribute key, then `fg=#rrggbb`, `bg=#rrggbb`, `bold`, `italic`, `effect=<type>` and `effectColor=#rrggbb`. |

### Editor | Font

Page id `editor.font`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.font.family` | enum: `JetBrains Mono`, `Fira Code`, `Cascadia Code`, `Source Code Pro`, `Menlo`, `Consolas`, `monospace` | `JetBrains Mono` | user |  | Font |
| `editor.font.size` | number (6–72) | `13.5` | user |  | Size |
| `editor.font.lineHeight` | number (1.0–3.0) | `1.6` | user |  | Line height |
| `editor.font.ligatures` | bool | `true` | user |  | Enable ligatures — Render character sequences like -> and !== as a single glyph if the font supports it. |

### Editor | Code Style

Page id `editor.code-style`; the language pages are `editor.code-style.<language>`
(`editor.code-style.php`, …). The scheme options are not declared in a manifest, see the
table of keys Forge writes itself above.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.codeStyle.useEditorConfig` | bool | `true` | user, project |  | Enable EditorConfig support — Indents, line separators, charset and the On Save options of .editorconfig files override these settings. |
| `editor.codeStyle.detectIndents` | bool | `true` | user, project |  | Detect and use existing file indents for editing — Files indented differently from the code style keep their indent; the status bar marks it with *. |

### Editor | File and Code Templates

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `fileTemplates.custom` | objects | `[]` | user | — | File and code templates — Templates added in Settings \| Editor \| File and Code Templates and the changed text of built-in ones (`kind` `file`, `include` or `code`, `name`, `extension`, `text`). Built-in templates without an entry use their default text. One entry per `kind` + `name` pair. Edited on the page, see [File and code templates](../user/file-templates.md). |

### Editor | File Types

Page id `editor.file-types`. The options are edited on the page itself, see
[File types](../user/file-types.md#changing-file-types).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `fileTypes.associations` | objects | `[]` | user | — | File name patterns — Patterns added to a file type (`fileType`, `pattern`) or removed from it (`action` `remove`). A pattern added to one type is taken away from the others. One entry per `fileType` + `pattern` pair. |
| `fileTypes.custom` | objects | `[]` | user | — | Custom file types — `id`, `name`, `description`, `lineComment`, `lineCommentAtStart`, `blockCommentStart`, `blockCommentEnd`, `hexPrefix`, `numberPostfixes`, `keywords1`–`keywords4` (one keyword per line), `ignoreCase`, `pairedBraces`, `pairedBrackets`, `pairedParens`, `stringEscapes` (`"true"` / `"false"`). One entry per `name`. |
| `fileTypes.ignoredFiles` | string | `*.hprof;*.pyc;*.pyo;*.rbc;*.yarb;*~;.DS_Store;.git;.hg;.svn;CVS;__pycache__;_svn;vssver.scc;vssver2.scc` | user | — | Ignored Files and Folders — Names hidden from the Project view, separated by `;` (`*` and `?` wildcards). |

### Editor | Inlay Hints

Page id `editor.inlay-hints`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.inlayHints.parameterNames` | bool | `true` | user | Parameters | Parameter names — Show parameter names at call sites for literal and null arguments. |
| `editor.inlayHints.types` | bool | `false` | user | Types | Inferred variable types — The type of variables and properties without a declared type. |
| `editor.inlayHints.returnTypes` | bool | `true` | user | Types | Return types of closures and functions — The inferred return type after the parameter list when none is declared. |
| `editor.inlayHints.implementations` | bool | `true` | user | Code Vision | Implementations and inheritors — Show "2 implementations", "3 inheritors" and "1 override" above PHP classes and methods (Code Vision). Click to go there. |
| `editor.inlayHints.languages.php` | bool | `true` | user | Languages | PHP |
| `editor.inlayHints.languages.javascript` | bool | `true` | user | Languages | JavaScript and TypeScript — Also Vue, Svelte and Astro files. |
| `editor.inlayHints.languages.lua` | bool | `true` | user | Languages | Lua |
| `editor.inlayHints.languages.other` | bool | `true` | user | Languages | Other languages |

### Editor | Live Templates

Page id `editor.liveTemplates`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.liveTemplates.expandKey` | enum: `tab`, `space`, `enter` | `tab` | user |  | By default expand with — Key that expands a live template abbreviation. |
| `editor.liveTemplates.custom` | objects | `[]` | user |  | User and changed templates — New templates and changes to built-in ones: group, abbreviation, description, text, contexts (comma-separated), variables (JSON), builtin (id of the replaced built-in template) and removed. |
| `editor.liveTemplates.disabled` | list | `[]` | user |  | Disabled templates — Ids (`group/abbreviation`) of templates that do not expand. |

### Editor | Emmet

Page ids `editor.emmet`, `editor.emmet.html`, `editor.emmet.css`, `editor.emmet.jsx`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.emmet.expandKey` | enum: `tab`, `enter`, `space` | `tab` | user |  | Expand abbreviation with — Live templates and postfix templates expand first when they match. |
| `editor.emmet.html.enabled` | bool | `true` | user |  | Enable Emmet (HTML) — In HTML, Blade, Twig and other template files, Vue templates and the HTML of PHP files. |
| `editor.emmet.html.preview` | bool | `false` | user |  | Enable abbreviation preview — Show the expanded markup in a popup while typing an abbreviation. |
| `editor.emmet.html.hrefAutoDetect` | bool | `true` | user |  | Enable automatic URL recognition while wrapping text with `<a>` tag |
| `editor.emmet.css.enabled` | bool | `true` | user |  | Enable Emmet (CSS) — In CSS, SCSS, Less, Sass and Stylus, and in `<style>` blocks. |
| `editor.emmet.jsx.enabled` | bool | `true` | user |  | Enable Emmet in JSX |
| `editor.emmet.jsx.classAttribute` | enum: `className`, `class` | `className` | user |  | Class attribute — Attribute written for .class abbreviations in JSX and TSX files. |

The color picker keeps `editor.colorPicker.recentColors` (list, user scope, not shown on a
page): the recently used colors, newest first.

### Editor | Reader Mode

Page id `editor.readerMode`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.readerMode.enabled` | bool | `true` | user |  | Use Reader mode — Open files you only read in Reader mode. Toggle it with the Reader Mode button at the top right of the editor. |
| `editor.readerMode.applyTo` | enum: `libraries`, `readOnly`, `both` | `both` | user |  | Use for |
| `editor.readerMode.renderDocs` | bool | `true` | user | Show | Rendered documentation comments |
| `editor.readerMode.ligatures` | bool | `true` | user | Show | Ligatures |
| `editor.readerMode.increaseLineSpacing` | bool | `true` | user | Show | Increased line spacing |

### Editor | Language Injections

Page id `editor.injections`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.injections.disabled` | list | `[]` | user |  | Disabled injection rules — Ids of the built-in injection rules that are turned off, e.g. `php-html` or `js-sql-keywords`. `// language=` comments always inject. |
| `editor.injections.uninjected` | list | `[]` | user, project |  | Un-injected places — Strings turned off with Un-inject Language (`path#key`); a string is injected again when its text changes. |

### Editor | TODO

Page id `editor.todo`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `todo.patterns` | list | `["\\btodo\\b.*", "\\bfixme\\b.*"]` | user | Patterns | Patterns — Regular expressions matched in comments, one per entry. Add a tab and `case` after a pattern to make it case-sensitive. |

### Editor | Image

Page id `editor.image`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `images.showChessboard` | bool | `true` | user | Image Viewer | Show chessboard — Transparent areas of images are drawn over a chessboard pattern. |
| `images.showGrid` | bool | `false` | user | Image Viewer | Show grid — Pixel grid lines when the zoom is 300% or more. |
| `images.svgDefaultLayout` | enum: `editor`, `split`, `preview` | `split` | user | SVG | SVG files open with |

### Editor | CSV/TSV

Page id `editor.csv`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `csv.defaultView` | enum: `text`, `table` | `text` | user |  | Open CSV/TSV files as |
| `csv.firstRowIsHeader` | bool | `true` | user |  | First row is header — Default of the table view's header toggle. |

### Editor | Vim

Page id `editor.vim` (plugin `forge.vim`, see [Vim emulation](../user/vim-emulation.md)).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `vim.enabled` | bool | `false` | user | Emulation | Enable Vim emulation — Same as Tools \| Vim (Ctrl+Alt+V). |
| `vim.rcFile` | path | `""` | user | Emulation | Configuration file — Leave empty to read ~/.ideavimrc (or ~/_ideavimrc, ~/.config/ideavim/ideavimrc). Supports set, map/noremap, let mapleader, sethandler and :action mappings. |
| `vim.shortcutHandlers` | objects | `[]` | user | Shortcut Conflicts | Shortcuts — Who handles a shortcut in the editor while Vim emulation is on: Vim or the IDE. sethandler lines in ~/.ideavimrc take precedence. Entries `{ shortcut = "Ctrl+R", normal = "vim" \| "ide", insert = …, visual = … }` for the rows changed from the defaults. |

### Editor | Natural Languages | Spelling

Page id `editor.natural-languages.spelling` (plugin `forge.natural-languages`).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `spelling.enabled` | bool | `true` | user | Inspection | Check spelling (Typo inspection) |
| `spelling.severity` | enum: `typo`, `grammar`, `weak`, `warning`, `error` | `typo` | user | Inspection | Severity |
| `spelling.checkComments` | bool | `true` | user | Inspection | Process comments |
| `spelling.checkStrings` | bool | `true` | user | Inspection | Process literals — String literals. |
| `spelling.checkIdentifiers` | bool | `true` | user | Inspection | Process code — Names of declared variables, functions, classes and properties, split at camelCase humps and underscores. |
| `spelling.checkText` | bool | `true` | user | Inspection | Process Markdown and plain text files |
| `spelling.checkCommitMessages` | bool | `true` | user | Inspection | Process commit messages |
| `spelling.checkFileNames` | bool | `false` | user | Inspection | Process file names — Reports typos in the name of the open file in the Problems tool window. |
| `spelling.projectWords` | list | `[]` | user, project | Accepted Words | Project-level dictionary — Accepted words of this project, stored in the project settings. "Save to project-level dictionary" (Alt+Enter on a typo) adds words here. |
| `spelling.applicationWords` | list | `[]` | user | Accepted Words | Application-level dictionary — Accepted words of all projects, stored in the user settings. |
| `spelling.customDictionaries` | list | `[]` | user | Dictionaries | Custom dictionaries — Hunspell dictionaries (.dic; the .aff file with the same name next to it is read too) or plain word lists with one word per line. Absolute paths, or paths relative to the project root. |

### Editor | Natural Languages | Grammar and Style

Page id `editor.natural-languages.grammar` (plugin `forge.natural-languages`).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `grammar.enabled` | bool | `true` | user | Inspection | Check grammar and style (Grammar inspection) — Comments, Markdown and plain text files and commit messages. |
| `grammar.severity` | enum: `grammar`, `typo`, `weak`, `warning`, `error` | `grammar` | user | Inspection | Severity |
| `grammar.rules.repeatedWord` | bool | `true` | user | Rules | Repeated words — "the the" |
| `grammar.rules.articles` | bool | `true` | user | Rules | Use of 'a' and 'an' |
| `grammar.rules.sentenceStart` | bool | `true` | user | Rules | Sentences start with an uppercase letter — Doc comments only. |
| `grammar.rules.doubleSpace` | bool | `true` | user | Rules | Repeated whitespace |

### Version Control | Git

Page id `vcs.git`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `vcs.git.path` | path | `git` | user |  | Path to Git executable — Leave as `git` to use the one on PATH. Remote projects always use the git of their host. |
| `vcs.git.updateMethod` | enum: `merge`, `rebase`, `branchDefault` | `merge` | user |  | Update method — How Update Project (Ctrl+T) brings in remote changes. Branch default follows the branch's git configuration (pull.rebase). |
| `vcs.git.autoFetch` | bool | `true` | user |  | Fetch remotes automatically — In the background, for repositories whose branch has an upstream. Failures are ignored. |
| `vcs.git.autoFetchInterval` | int (1–240) | `10` | user |  | Fetch every (minutes) |
| `vcs.git.commit.subjectMaxLength` | int (20–200) | `72` | user | Commit Message Inspections | Limit subject line length |
| `vcs.git.commit.blankLineAfterSubject` | bool | `true` | user | Commit Message Inspections | Require blank line between subject and body |
| `vcs.git.commit.bodyRightMargin` | int (40–200) | `72` | user | Commit Message Inspections | Right margin for body (columns) |
| `vcs.git.commit.warnOnCrlf` | bool | `true` | user | Commit | Warn if CRLF line separators are about to be committed |
| `vcs.git.commit.signOff` | bool | `false` | user | Commit | Sign-off commits |

### Version Control | Local History

Page id `vcs.local-history`. See [Local History](../user/local-history.md).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `localHistory.keepDays` | int (1–365) | `5` | user |  | Keep local history for (working days) — Only days with changes count. Older revisions and labels are removed when a project opens and every few hours. |
| `localHistory.maxStorageMb` | int (16–10240) | `512` | user |  | Maximum history size per project (MB) — When the stored contents grow larger, the oldest revisions are removed first. |
| `localHistory.maxFileSizeKb` | int (16–102400) | `1024` | user |  | Do not record files larger than (KB) — Larger files and binary files are left out of Local History. |
| `localHistory.excludedFolders` | list | `.git`, `.hg`, `.svn`, `node_modules`, `vendor`, `.idea` | user |  | Excluded folders — Files in folders with these names (or project-relative paths like `storage/logs`) are never recorded. |

### Version Control | Code Hosts

Page id `vcs.hosts`; the GitLab, Forgejo / Gitea and GitHub account pages are below it.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `vcs.hosts.statusBarPipeline` | bool | `true` | user | General | Show the CI status of the current branch in the status bar |
| `vcs.hosts.refreshSeconds` | int (0–3600) | `120` | user | General | Refresh merge requests and pipelines every (seconds) — 0 turns automatic refresh off. |

### Languages & Frameworks | PHP

Page id `lang.php`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `php.languageLevel` | enum: `7.4`, `8.0`, `8.1`, `8.2`, `8.3`, `8.4`, `interpreter` | `8.3` | user, project | Development Environment | PHP language level — Inspections and completion follow the syntax of this version (`interpreter`: the CLI interpreter's version). |
| `php.interpreter` | string | `php` | user, project | Development Environment | CLI Interpreter — The interpreter that runs scripts and tests: the name of an interpreter of the CLI Interpreters dialog, or the path of a local php executable. |
| `php.interpreters` | objects (unique `name`) | `[]` | user | Development Environment | CLI Interpreters — PHP interpreters available in every project: local, Docker, Docker Compose, SSH and WSL (edited in the CLI Interpreters dialog). |
| `php.projectInterpreters` | objects (unique `name`) | `[]` | user, project | Development Environment | CLI Interpreters of this project — Interpreters marked "Visible only for this project". |
| `php.includePaths` | list | `[]` | user, project | Include Path | Include path — Extra directories indexed as libraries (e.g. global PEAR or framework stubs). |
| `php.sourceRoots` | objects (unique `path`) | `[]` | user, project | Directories | Source roots — Folders marked as Sources Root or Test Sources Root with their package prefix (Settings \| Directories, Project view: Mark Directory as). They change the PSR-4 roots composer.json declares (kind `none` unmarks one); New PHP Class, Move Class and the namespace inspection use them. |
| `projectStructure.folders` | objects (unique `path`) | `[]` | user, project | Directories | Excluded and resource folders — Folders marked as Excluded, Resource Root or Test Resources Root (Settings \| Directories, Project view: Mark Directory as), stored in the project. Kind `included` cancels the default exclusion of a folder like `node_modules`. Excluded folders are left out of the indexes, Find in Files, Search Everywhere, inspections and local history. |
| `projectStructure.excludePatterns` | string | `""` | user, project | Directories | Exclude files — File and folder name patterns excluded from the project, separated by `;` (`*.min.js;tmp`). |
| `php.inspections.enabled` | bool | `true` | user | Inspections | Inspections from PHPStorm metadata and attributes — Deprecated symbols, expected values, #[Immutable] writes, unused #[Pure] results, unreachable code after exit points and the PSR-0/PSR-4 namespace check. |
| `php.stubs.path` | path | `""` | user | PHP Runtime | Default stubs path — A phpstorm-stubs checkout used for PHP's own classes, functions and constants (Go to Declaration on \Throwable, array_map). Empty: Forge downloads JetBrains' phpstorm-stubs into its cache folder on first use, or uses Intelephense's stubs. |
| `php.lsp.server` | enum: `auto`, `intelephense`, `phpactor`, `none` | `auto` | user | Language Server | Language server — Started automatically when a PHP project opens. |
| `php.lsp.maxFileSize` | int (100–100000) | `5000` | user | Language Server | Max indexed file size (KB) |
| `php.intelephense.autoInstall` | enum: `ask`, `always`, `never` | `ask` | user | Language Server | When Intelephense is not installed — Forge downloads Intelephense from the npm registry into its own directory (and Node.js when none is installed). Intelephense is not bundled with Forge. |
| `php.intelephense.licenceAccepted` | bool | `false` | user | Language Server | I accept the Intelephense licence — Asked once before Forge downloads Intelephense for you. Its licence (LICENSE.txt in the npm package, intelephense.com) applies to your copy. |

### Languages & Frameworks | PHP | Composer

Page id `lang.php.composer`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `php.composer.json` | path | `composer.json` | user, project |  | Path to composer.json — Relative to the project root. |
| `php.composer.executable` | path | `composer` | user, project | Execution | Path to composer — A composer executable, or a composer.phar (run with the PHP interpreter of Settings \| PHP). Empty: `composer` from PATH. |
| `php.composer.syncWithComposerJson` | bool | `true` | user, project |  | Synchronize IDE settings with composer.json — The PHP language level follows `require.php` of composer.json (the status bar shows it); the language level setting applies when there is no constraint. |
| `php.composer.addPackagesAsLibraries` | bool | `true` | user, project |  | Add packages as libraries — Classes in vendor/ are resolved for navigation, completion and Intelephense. Off: vendor/ is left out. |
| `php.composer.checkUpdates` | bool | `false` | user |  | Check for package updates on project open — Runs `composer outdated --direct` on the project's machine and reports updates in a balloon. |
| `php.composer.packagist` | bool | `true` | user |  | Use Packagist for completion, versions and outdated packages — Package names, versions and descriptions come from packagist.org (cached in Forge's cache directory). Off: only installed packages are used and Forge never contacts Packagist. |

### Languages & Frameworks | PHP | Quality Tools

Page id `lang.php.quality-tools`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `php.tools.phpCsFixer.path` | path | `""` | user, project | PHP CS Fixer | PHP CS Fixer path — Empty: vendor/bin/php-cs-fixer of the project. A relative path starts at the project root. The project's .php-cs-fixer.dist.php is used. |
| `php.tools.pint.path` | path | `""` | user, project | Laravel Pint | Laravel Pint path — Empty: vendor/bin/pint of the project. A relative path starts at the project root. The project's pint.json is used. |

### Languages & Frameworks | PHP | Test Frameworks

Page id `lang.php.test-frameworks`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `php.testFrameworks` | objects | `[]` | user, project |  | Test frameworks — PHPUnit, Pest, Codeception, Behat and PHPSpec per CLI interpreter. Entries: `framework` (`phpunit`, `pest`, `codeception`, `behat`, `phpspec`), `interpreter` (empty: Local), `mode` (`composer`: `path` is `vendor/autoload.php`; `path`: the executable or phar), `path`, `configFile`, `bootstrapFile`. Frameworks the project uses are detected. |
| `run.autoTest.delay` | int (0–600) | `3` | user | Auto-Test | Auto-test delay (seconds) — With Toggle Auto-Test on, a test run reruns this long after the last file was saved. |

### Languages & Frameworks | PHP | Debug

Page id `lang.php.debug`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `php.debug.port` | int (1–65535) | `9003` | user | Xdebug | Debug port — Forge listens here for Xdebug (xdebug.client_port, 9003 by default). |
| `php.debug.ignoreExternalConnections` | bool | `false` | user | Xdebug | Ignore external connections through unregistered server configurations — Connections that match no server and no project file are detached silently. |
| `php.debug.breakAtFirstLine` | bool | `false` | user | Settings | Break at first line in PHP scripts |
| `php.debug.ignoreUnmappedFiles` | bool | `true` | user | Settings | Step out of files that are not mapped into the project — Stepping never stops in code without a local file (e.g. PHAR internals or eval'd code). |
| `php.debug.showInlineValues` | bool | `true` | user | Settings | Show variable values in editor while debugging |
| `php.debug.maxChildren` | int (1–10000) | `100` | user | Evaluation | Max. children — Array elements and object properties loaded per page. |
| `php.debug.maxDepth` | int (1–16) | `1` | user | Evaluation | Max. depth — Nesting levels fetched at once; deeper levels load when expanded. |
| `php.debug.maxData` | int (128–10485760) | `65536` | user | Evaluation | Max. string length — Bytes of string data fetched per value. |
| `php.debug.validation.documentRoot` | string | `"public"` | user, project | Web Server Debug Validation | Path to create validation script — Project folder served by the web server (its document root), e.g. public. Run \| Web Server Debug Validation places a temporary script there and deletes it after the check. |
| `php.debug.validation.url` | string | `"http://localhost:8080"` | user, project | Web Server Debug Validation | URL to validation script — The URL of that folder, e.g. http://localhost:8080. |

### Languages & Frameworks | PHP | Laravel

Page id `lang.php.laravel`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `laravel.routes.source` | enum: `auto`, `artisan`, `static` | `auto` | user, project | Routes | Read routes from — php artisan route:list also sees routes registered by packages, but needs a bootable application. |
| `laravel.completion.enabled` | bool | `true` | user | Code Completion | Complete routes, views, config, translations and model attributes |
| `laravel.completion.columns` | bool | `true` | user | Code Completion | Complete database columns in query builder calls — where(), orderBy(), pluck() and similar, with columns read from the migrations. |
| `laravel.gutter.routes` | bool | `true` | user | Gutter Icons | Show route icons for controller actions |
| `laravel.gutter.views` | bool | `true` | user | Gutter Icons | Show view icons for view() calls and Blade includes |
| `laravel.blade.customDirectives` | list | `[]` | user, project | Blade | Additional Blade directives — Directive names without @, e.g. datetime. They are highlighted and completed like built-in ones. |

### Languages & Frameworks | PHP | Symfony

Page id `lang.php.symfony`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `symfony.environment` | enum: `dev`, `test`, `prod` | `dev` | user, project | Project | Environment — Kernel environment used for the compiled container and console commands. |
| `symfony.appDirectory` | path | `src` | user, project | Project | App directory — Scanned for controllers, console commands and Twig extensions. |
| `symfony.templatesDirectory` | path | `templates` | user, project | Project | Templates directory |
| `symfony.consolePath` | path | `bin/console` | user, project | Project | Console — Path to the console script, relative to the project root. |
| `symfony.containerXml` | path | `""` | user, project | Container | Container XML — Compiled container dump; leave empty to use var/cache/<environment>/*Container.xml. |
| `symfony.gutterIcons` | bool | `true` | user | Editor | Show line markers for routes, templates and commands |
| `symfony.twigCompletion` | bool | `true` | user | Editor | Complete Twig tags, filters and functions |

### Languages & Frameworks | PHP | WordPress

Page id `lang.php.wordpress`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `wordpress.hookCompletion` | bool | `true` | user | Hooks | Complete hook names — Inside add_action, add_filter, do_action, apply_filters and their variants: the project's hooks first, then frequently used WordPress core hooks. |
| `wordpress.gutterIcons` | bool | `true` | user | Hooks | Show gutter icons for hooks — A registration links to where its hook fires, an invocation to the registered callbacks. |
| `wordpress.yodaConditions` | bool | `true` | user | Code Style | Suggest Yoda conditions — A weak warning on comparisons like `$status === 'publish'` with the quick-fix 'Flip to Yoda condition' (WordPress Coding Standards). |
| `wordpress.offerStubs` | bool | `true` | user, project | Stubs | Offer to install the WordPress stubs — When php-stubs/wordpress-stubs is not in vendor/, a notification offers `composer require --dev php-stubs/wordpress-stubs`. Forge never ships the stubs. |

### Languages & Frameworks | PHP | Drupal, Joomla and Shopware

Page id `lang.php.cms`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `cms.drupal.hookCompletion` | bool | `true` | user | Drupal | Complete Drupal hook implementations — After `function` in .module, .install, .theme, .profile and .inc files: `<module>_<hook>` with the hook's parameters. |
| `cms.drupal.gutterIcons` | bool | `true` | user | Drupal | Show gutter icons for hook implementations — Clicking one opens the hook's documentation: its `hook_…` function in the project's *.api.php files, else api.drupal.org. |

### Languages & Frameworks | PHP | Doctrine

Page id `lang.php.doctrine`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `doctrine.sourceDirectories` | list | `["src"]` | user, project | Mapping | Entity source directories — Scanned for #[ORM\Entity] classes and repositories. |
| `doctrine.mappingDirectories` | list | `["config/doctrine", "src/Resources/config/doctrine"]` | user, project | Mapping | Mapping file directories — Directories with *.orm.xml and *.orm.yml mapping files. |
| `doctrine.annotations` | bool | `true` | user, project | Mapping | Read legacy @ORM annotations — Docblock annotations are used when a class has no mapping attributes. |
| `doctrine.dqlInjection` | bool | `true` | user | DQL | Highlight DQL in createQuery() and QueryBuilder strings |
| `doctrine.dqlCompletion` | bool | `true` | user | DQL | Complete entities, aliases and fields in DQL |
| `doctrine.dqlInspections` | bool | `true` | user | DQL | Report unknown entities and fields |

### Languages & Frameworks | Template Data Languages

Page id `lang.templates`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `templates.dataLanguages` | list | `[]` | user, project | Data Languages | Template data languages — Language of the content between template tags, one entry per pattern: `legacy/latex/*.tpl=latex`, `*.mail.twig=text`. Languages: html, xml, latex, css, scss, less, javascript, typescript, json, sql, yaml, ini, shell, text. Compound names like `invoice.tex.twig` are detected automatically. Applies to files opened afterwards. |
| `templates.delimiters` | objects | Smarty `[[ ]]`, Twig and Jinja2 `\VAR{ }` / `\BLOCK{ }` / `\#{ }` in LaTeX | user, project | Delimiters | Template delimiters — Delimiters of a template language in a data language, e.g. Smarty in LaTeX, where `{` is part of the content. Templates without an entry use the language's default delimiters. Applies to open editors. One entry per `template` + `language` pair. |

### Languages & Frameworks | Language Servers

Page id `lang.servers`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `langservers.autoStart` | bool | `true` | user | General | Start language servers for languages found in the project — Servers start lazily when the first matching file is opened. |
| `langservers.scanLimit` | int (100–1000000) | `20000` | user | General | Files scanned to detect languages — Ignored files (.gitignore), node_modules and vendor are skipped. |
| `langservers.hints.enabled` | bool | `true` | user | General | Suggest installing missing language servers — Shows a banner in files that use a feature only a language server provides, such as a JSON $schema, when that server is not installed. |
| `langservers.hints.dismissed` | list | `[]` | user | General | Servers not suggested again — Server ids whose banner was turned off with "Don't show again". Remove an entry to get the suggestion back. |
| `langservers.updates.check` | bool | `true` | user | Downloads | Check for updates of servers installed by Forge once a day — Offers an update when the npm registry has a newer version of a server in Forge's server directory. |
| `langservers.download.npmRegistry` | string | `https://registry.npmjs.org` | user | Downloads | npm registry — Where Forge downloads npm language servers from. Every tarball is checked against the registry's sha512 integrity hash. |
| `langservers.download.nodeMirror` | string | `https://nodejs.org/dist` | user | Downloads | Node.js download mirror — Used only when no Node.js 18 or newer is installed. Downloads are checked against the release's SHASUMS256.txt. |

### Languages & Frameworks | Java

Page id `lang.java` (plugin `forge.java`). See [JavaForge](../user/java.md#eclipse-jdt-language-server).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `java.jdtls.autoInstall` | enum: `ask`, `always`, `never` | `ask` | user | Eclipse JDT Language Server | Install Eclipse JDT LS — When a Java project opens without the language server: ask, install it from download.eclipse.org, or never. |
| `java.jdtls.path` | path | `""` | user | Eclipse JDT Language Server | JDT LS installation — A folder with an unpacked JDT LS (plugins/, config_*/). Empty: the one Forge installed. |
| `java.jdtls.jdk` | path | `""` | user | Eclipse JDT Language Server | JDK for the language server — JDT LS needs Java 21 or newer (Java 25 for releases since 2026). Empty: the project JDK when it is new enough, else the newest detected JDK that is. |
| `java.jdtls.vmOptions` | string | `-Xmx1G` | user | Eclipse JDT Language Server | VM options |
| `java.jdtls.channel` | enum: `milestones`, `snapshots` | `milestones` | user | Eclipse JDT Language Server | Release channel |
| `java.jdtls.downloadSite` | string | `https://download.eclipse.org/jdtls` | user | Eclipse JDT Language Server | Download site |
| `java.debug.hotCodeReplace` | enum: `auto`, `manual`, `never` | `auto` | user | Debugger | Hot code replace — Reload changed classes into a running debug session: automatically after a build, on request (Apply HotSwap), or never. |
| `java.debug.repository` | string | `https://repo1.maven.org/maven2` | user | Debugger | java-debug repository — Maven repository Forge downloads Microsoft's java-debug plugin from (checksum checked). |

### Languages & Frameworks | Kotlin

Page id `lang.kotlin` (plugin `forge.kotlin`). See [KotlinForge](../user/kotlin.md#language-server).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `kotlin.lsp.server` | enum: `kotlin-lsp`, `kotlin-language-server` | `kotlin-lsp` | user | Language Server | Language server |
| `kotlin.lsp.autoInstall` | enum: `ask`, `always`, `never` | `ask` | user | Language Server | Install kotlin-lsp — When a Kotlin project opens without the language server: ask, download it from download.jetbrains.com, or never. |
| `kotlin.lsp.path` | path | `""` | user | Language Server | Server path — An unpacked kotlin-lsp folder (or its bin/intellij-server), or the kotlin-language-server launcher. |
| `kotlin.lsp.jdk` | path | `""` | user | Language Server | JDK for the build import — JAVA_HOME of the server. Empty: the project JDK. |
| `kotlin.lsp.jvmOptions` | string | `""` | user | Language Server | JVM options |
| `kotlin.run.runner` | enum: `auto`, `gradle`, `maven`, `kotlinc` | `auto` | user, project | Run | Run Kotlin applications with |
| `kotlin.compiler.path` | path | `""` | user | Run | Kotlin compiler — The kotlinc of a Kotlin compiler. Empty: the PATH. |
| `kotlin.debug.debugger` | enum: `kotlin-lsp`, `java-debug` | `kotlin-lsp` | user | Debugger | Debugger |
| `kotlin.ktlint.enabled` | bool | `true` | user | ktlint | Check files with ktlint |
| `kotlin.ktlint.path` | path | `""` | user | ktlint | ktlint — Executable or ktlint-cli-*-all.jar. Empty: the PATH, else Forge's download. |
| `kotlin.formatter` | enum: `lsp`, `ktlint` | `lsp` | user | ktlint | Reformat Code with |
| `kotlin.ktlint.formatOnSave` | bool | `false` | user, project | ktlint | Run ktlint format on save |
| `kotlin.detekt.enabled` | bool | `true` | user | detekt | Check the project with detekt |
| `kotlin.detekt.path` | path | `""` | user | detekt | detekt — Executable or detekt-cli-*-all.jar. Empty: the PATH, else Forge's download. |
| `kotlin.detekt.config` | path | `""` | user, project | detekt | Configuration file — Empty: config/detekt/detekt.yml when it exists. |
| `kotlin.download.repository` | string | `https://repo1.maven.org/maven2` | user | Downloads | Maven repository for ktlint and detekt |

### Languages & Frameworks | Groovy

Page id `lang.groovy` (plugin `forge.groovy`). See [Groovy and Scala](../user/groovy-scala.md#running-scripts).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `groovy.sdk.path` | path | `""` | user | Groovy SDK | Groovy SDK — A Groovy installation, its bin folder or the groovy launcher. Empty: GROOVY_HOME, SDKMAN!'s current Groovy, else the PATH. |
| `groovy.run.runner` | enum: `auto`, `groovy`, `gradle` | `auto` | user, project | Run | Run Groovy scripts with |

### Languages & Frameworks | Scala

Page id `lang.scala` (plugin `forge.scala`). See [Groovy and Scala](../user/groovy-scala.md#metals).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `scala.metals.path` | path | `""` | user | Language Server (Metals) | Metals — The metals launcher. Empty: the one Forge installed, else the PATH. |
| `scala.metals.autoInstall` | enum: `ask`, `never` | `ask` | user | Language Server (Metals) | Install Metals — offer to install the pinned version with Coursier, or never ask. |
| `scala.coursier.path` | path | `""` | user | Language Server (Metals) | Coursier (cs) — Empty: cs or coursier on the PATH. |
| `scala.sbt.path` | path | `""` | user | sbt | sbt launcher — Empty: SDKMAN!'s current sbt, else the PATH. |
| `scala.sbt.vmOptions` | string | `""` | user, project | sbt | VM parameters — JVM options of sbt (passed as -J options). |
| `scala.sbt.importOnOpen` | bool | `false` | user, project | sbt | Reload the sbt project when it opens (trusted projects only). |

### Languages & Frameworks | Android SDK

Page id `lang.android` (plugin `forge.android`). See [Android](../user/android.md#the-android-sdk).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `android.sdk.path` | path | `""` | user | Android SDK | Android SDK location — Empty: ANDROID_HOME, ANDROID_SDK_ROOT, sdk.dir of local.properties, else the default folder. Forge does not download the SDK. |
| `android.logcat.bufferSize` | int | `20000` | user | Logcat | Logcat cycle buffer size (entries) |
| `android.logcat.defaultQuery` | string | `package:mine` | user | Logcat | Default filter |
| `android.logcat.history` | int | `500` | user | Logcat | Recent entries on connect (`adb logcat -T`) |

### Languages & Frameworks | Lua

Page id `lang.lua`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `lua.interpreter` | path | `lua` | user, project | Interpreter | Lua interpreter — Runs Lua Script configurations that do not set their own: lua, luajit, love or a full path. |
| `lua.requirePaths` | list | `[]` | user, project | Modules | Module search paths — package.path templates relative to the project, such as scripts/?.lua. Used to resolve require("a.b"); ?.lua, ?/init.lua, lua/?.lua, src/?.lua and lib/?.lua are always tried. |
| `langservers.servers.lua-language-server.path` | path | `""` | user | Lua Language Server | lua-language-server executable — Leave empty to find it on PATH, in Mason or in Forge's server directory. |
| `langservers.lua.environment` | enum: `auto`, `plain`, `love`, `neovim` | `auto` | user | Lua Language Server | Lua environment — Adds the APIs and globals of LÖVE (love.*) or Neovim (vim.*) to lua-language-server. Detected from conf.lua, main.lua or init.lua with a lua/ folder. |
| `langservers.lua.version` | enum: `Lua 5.1`, `Lua 5.2`, `Lua 5.3`, `Lua 5.4`, `LuaJIT` | `Lua 5.4` | user | Lua Language Server | Lua version — Used with Plain Lua; LÖVE and Neovim run LuaJIT. |
| `langservers.lua.libraries` | list | `[]` | user, project | Lua Language Server | Library paths — Extra folders with Lua definitions or third-party code (workspace.library), e.g. ~/.luarocks/share/lua/5.4. |
| `lua.stylua.enabled` | bool | `true` | user | StyLua | Reformat Code with StyLua — Ctrl+Alt+L formats Lua files with StyLua when it is installed; stylua.toml in the project is honoured. |
| `lua.stylua.path` | path | `stylua` | user | StyLua | StyLua executable |
| `lua.busted.path` | path | `busted` | user | Testing | busted executable — Runs busted test configurations that do not set their own executable. |

### Languages & Frameworks | Python

Page id `lang.python`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `python.formatter` | enum: `ruff`, `black`, `lsp`, `none` | `ruff` | user, project | Formatter | Reformat Code with — Ctrl+Alt+L and "Reformat code" on save use this formatter for Python files; pyproject.toml / ruff.toml settings are honoured. |
| `python.ruff.path` | path | `""` | user | Formatter | ruff executable — Leave empty to use the interpreter's environment, Forge's download or PATH. |
| `python.black.path` | path | `""` | user | Formatter | Black executable — Leave empty to use the interpreter's environment or PATH. |
| `python.lsp.useEnvironmentTools` | bool | `true` | user | Language Servers | Use language servers from the interpreter's environment — basedpyright, ruff and ty installed in the selected environment (or the project's .venv) start before Forge's own downloads and PATH. |
| `python.packages.pypi` | bool | `true` | user | Packages | Look up packages on PyPI — Latest versions, search and version completion come from PyPI (cached for 12 hours in Forge's cache folder). |
| `python.packages.index` | string | `https://pypi.org` | user | Packages | Package index — PyPI or a mirror with the same JSON API. |
| `python.inspections.requirements` | bool | `true` | user | Inspections | Report unsatisfied requirements — Dependencies in requirements files and pyproject.toml that are not installed in the interpreter, with "Install requirement". |
| `python.inspections.imports` | bool | `true` | user | Inspections | Report imports of packages missing from the requirements — With the quick fix "Add requirement" to requirements.txt or pyproject.toml. |
| `python.inspections.ignoredPackages` | list | `[]` | user, project | Inspections | Ignored packages — Packages the requirements inspections do not report. |

### Languages & Frameworks | Python | Django

Page id `lang.django`. See [Django, Flask, FastAPI and Celery](../user/python-web-frameworks.md).

| Key | Type | Default | Scope | Section | Description |
| --- | --- | --- | --- | --- | --- |
| `django.settingsModule` | string | `""` | user, project | Project | Settings module — DJANGO_SETTINGS_MODULE, for example mysite.settings. Empty: read from manage.py. |
| `django.manageScript` | path | `""` | user, project | Project | Manage script — The project's manage.py, relative to the project. Empty: the manage.py found in the project. |
| `django.manageCommandsFromProject` | bool | `true` | user | manage.py | Read manage.py commands from the project — Run manage.py help --commands (trusted projects only) so the project's and third-party commands are completed. |
| `django.testRunner` | enum: `django`, `pytest` | `django` | user, project | Tests | Test runner of new Django tests configurations. |

### Languages & Frameworks | Python | Flask, FastAPI and Celery

Page ids `lang.flask`, `lang.fastapi` and `lang.celery`.

| Key | Type | Default | Scope | Section | Description |
| --- | --- | --- | --- | --- | --- |
| `flask.app` | string | `""` | user, project |  | Application (FLASK_APP) — For example app or blog:create_app(). Empty: the module that creates Flask(...). |
| `fastapi.app` | string | `""` | user, project |  | Application — module:attribute that uvicorn serves. Empty: the module that creates FastAPI(...). |
| `celery.app` | string | `""` | user, project |  | Application (-A) — The Celery application. Empty: the module that creates Celery(...). |

### Languages & Frameworks | Jupyter

Page id `lang.jupyter`. See [Jupyter notebooks](../user/jupyter.md). The server token is stored in the OS keychain, not in settings.

| Key | Type | Default | Scope | Section | Description |
| --- | --- | --- | --- | --- | --- |
| `jupyter.server.url` | string | `""` | user, project | Kernels | Jupyter server URL — Run kernels on this server instead of starting ipykernel in the project's interpreter, for example http://localhost:8888 (a pasted URL's ?token= is moved to the keychain). Needed for Docker, SSH and WSL interpreters and remote projects. |
| `jupyter.kernel.installWithoutAsking` | bool | `false` | user | Kernels | Install ipykernel without asking — Install ipykernel into the interpreter with its package manager when a notebook starts a kernel and it is missing. |
| `jupyter.kernel.shutdownOnClose` | bool | `true` | user | Kernels | Shut down the kernel when the notebook is closed |
| `jupyter.outputs.limitHeight` | bool | `true` | user | Outputs | Scroll long outputs — Outputs taller than 400 pixels get their own scroll bar. |
| `jupyter.dataframe.pageSize` | int | `50` | user | Outputs | DataFrame rows per page |
| `jupyter.diff.view` | enum: `cells`, `json` | `cells` | user | Diff | Show notebook diffs — How the diff viewer shows .ipynb files first. |

### Languages & Frameworks | Go

Page id `lang.go` (`go.goroot` is set by the GOROOT list of the page).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `go.goroot` | path | `""` | user, project | GOROOT | GOROOT — The Go SDK of the project. Empty: the newest SDK Forge finds (PATH, /usr/local/go, ~/sdk, Homebrew, Program Files, Forge's downloads). |
| `go.sdk.downloadSource` | string | `https://go.dev/dl` | user | SDK Downloads | SDK download source — Where Download Go SDK gets the release list (?mode=json, with SHA-256 checksums) and the archives. |
| `go.gopath` | path | `""` | user, project | GOPATH | Project GOPATH — GOPATH for builds, runs and tests of this project. Empty: Go's default (GOPATH or ~/go). |
| `go.goflags` | string | `""` | user, project | Build Tags and Environment | GOFLAGS — Flags every go command of the project gets, e.g. -mod=mod or -buildvcs=false. |
| `go.buildTags` | list | `[]` | user, project | Build Tags and Environment | Build tags — Tags for builds, runs, tests and gopls (-tags), e.g. integration. |
| `go.environment` | list | `[]` | user, project | Build Tags and Environment | Environment — Variables for go commands of the project, one KEY=VALUE per entry (GOPRIVATE, CGO_ENABLED, GOEXPERIMENT). |
| `go.formatter` | enum: `gofmt`, `goimports`, `lsp`, `none` | `gofmt` | user, project | Code Style | Reformat Code with — Ctrl+Alt+L and "Reformat code" on save use this tool for Go files. |
| `go.goimports.path` | path | `""` | user | Code Style | goimports executable — Empty: Forge's tools folder, GOPATH/bin or PATH. |
| `go.vet.onSave` | bool | `true` | user, project | External Annotators | Run go vet on save — Problems of the saved file's package appear in the editor and the Problems tool window. go vet builds the package, so it runs only in trusted projects. |
| `go.golangciLint.onSave` | bool | `false` | user, project | External Annotators | Run golangci-lint on save — Runs the project's .golangci.yml configuration for the saved file's package (trusted projects only). |
| `go.golangciLint.path` | path | `""` | user | External Annotators | golangci-lint executable — golangci-lint is not installed by Forge (GPL-3.0). Empty: GOPATH/bin or PATH. |
| `go.delve.path` | path | `""` | user | Debugger | Delve (dlv) executable — Empty: Forge's tools folder, GOPATH/bin or PATH. Forge can install Delve with your Go toolchain. |
| `go.test.fuzztime` | string | `10s` | user, project | Tests | Default fuzz time — How long fuzzing runs when a Go Test configuration does not set it (-fuzztime). |
| `go.modules.proxyLookups` | bool | `true` | user | Go Modules | Look up module versions on the module proxy — Version completion in go.mod and Add Dependency ask GOPROXY (proxy.golang.org by default), cached for 12 hours in Forge's cache folder. |
### Languages & Frameworks | Rust

Page id `lang.rust` (`rust.toolchain` is set by the Toolchain list of the page).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `rust.toolchain` | string | `""` | user, project | Toolchain | Toolchain — The toolchain of the project's cargo, rustc, rustfmt and Clippy commands (cargo +<toolchain>). Empty: rustup's choice, which follows rust-toolchain.toml. |
| `rust.environment` | list | `[]` | user, project | Cargo | Environment — Variables for cargo commands of the project, one KEY=VALUE per entry (RUSTFLAGS, CARGO_TARGET_DIR, RUST_LOG). |
| `rust.cargo.features` | list | `[]` | user, project | Cargo | Features — Cargo features every build, run, test and rust-analyzer use; the Cargo tool window's checkboxes edit them. |
| `rust.cargo.allFeatures` | bool | `false` | user, project | Cargo | Use all features |
| `rust.cargo.noDefaultFeatures` | bool | `false` | user, project | Cargo | Do not use the default features |
| `rust.test.runner` | enum: `cargo`, `nextest` | `cargo` | user, project | Tests | Test runner — cargo-nextest runs tests in parallel processes; doctests and benchmarks always run with cargo test. |
| `rust.test.format` | enum: `auto`, `json`, `text` | `auto` | user, project | Tests | Test results — libtest's JSON output needs a nightly toolchain (-Z unstable-options); on stable Forge reads the plain output. |
| `rust.formatter` | enum: `rustfmt`, `lsp`, `none` | `rustfmt` | user, project | Rustfmt | Reformat Code with — Ctrl+Alt+L and "Reformat code" on save use this tool for Rust files; rustfmt reads the project's rustfmt.toml. |
| `rust.rustfmt.extraArgs` | string | `""` | user, project | Rustfmt | Additional rustfmt arguments — For example --config imports_granularity=Crate. |
| `rust.clippy.onSave` | bool | `false` | user, project | External Linter | Run Clippy on save — Clippy's problems of the saved file's package appear in the editor and the Problems tool window, with its suggestions as quick fixes (Alt+Enter). Clippy builds the code, so it runs only in trusted projects. |
| `rust.clippy.extraArgs` | string | `""` | user, project | External Linter | Additional Clippy arguments — Lint options after --, for example -W clippy::pedantic. |
| `rust.analyzer.checkOnSave` | bool | `true` | user, project | rust-analyzer | Check on save — rust-analyzer runs cargo check (or Clippy) when a file is saved. |
| `rust.analyzer.checkCommand` | enum: `clippy`, `check` | `clippy` | user, project | rust-analyzer | Check command |
| `rust.analyzer.target` | string | `""` | user, project | rust-analyzer | Target triple — Analyze for another target, e.g. wasm32-unknown-unknown. Empty: the host. |
| `rust.analyzer.procMacros` | bool | `true` | user, project | rust-analyzer | Expand procedural macros — Runs the project's proc macros (trusted projects only; Safe Mode does not start rust-analyzer). |
| `rust.analyzer.buildScripts` | bool | `true` | user, project | rust-analyzer | Run build scripts — Runs build.rs for generated code and cfgs (trusted projects only). |
| `rust.analyzer.path` | path | `""` | user | rust-analyzer | rust-analyzer executable — Empty: the toolchain's rust-analyzer component, Forge's download or the PATH. |
| `rust.debugger` | enum: `auto`, `lldb`, `gdb` | `auto` | user | Debugger | Debugger — LLDB needs lldb-dap (Settings | Build | Debugger); GDB needs GDB 14 or newer. |
| `rust.debugger.prettyPrinters` | bool | `true` | user | Debugger | Show Rust types with rustc's pretty printers — Vec, String, Option, HashMap and friends as values (the scripts of rust-lldb and rust-gdb from the toolchain's sysroot). |
### Tools | Python Integrated Tools

Page id `tools.python-integrated-tools`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `python.testing.runner` | enum: `auto`, `pytest`, `unittest` | `auto` | user, project | Testing | Default test runner — Runs tests from the gutter and Run Context Configuration. Autodetect: pytest when the project uses it (configuration or dependency), unittest for files with only test cases. |
| `python.pytest.reporter` | enum: `junitxml`, `reportLog` | `junitxml` | user, project | Testing | pytest results — JUnit XML is built into pytest; a report log needs the pytest-reportlog plugin. |
| `python.tox.path` | path | `""` | user | tox | tox executable — Leave empty to run tox with the project's interpreter (python -m tox). |
| `python.profiler.snapshots` | int | `20` | user | Profiler | Profiler snapshots to keep — cProfile snapshots (.pstats) of a project kept in Forge's cache folder. |

### Build, Execution, Deployment | Debugger | Python Debugger

Page id `build.python-debugger`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `python.debugger.justMyCode` | bool | `true` | user | debugpy | Step only through the project's code (justMyCode) — Library and standard library frames are skipped while stepping and break only on exceptions raised in the project. |
| `python.debugger.subProcess` | bool | `true` | user | debugpy | Attach to subprocesses automatically — Child Python processes (multiprocessing, subprocess) are debugged too. |
| `python.debugger.installWithoutAsking` | bool | `false` | user | debugpy | Install debugpy without asking — When the interpreter has no debugpy, Debug installs it (pip install debugpy) instead of asking first. |

### Build, Execution, Deployment | Python Console

Page id `build.python-console`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `python.console.ipython` | bool | `true` | user | Python Console | Use IPython if available — The Python Console runs IPython when the interpreter has it (magics, Out[] history). |
| `python.console.interpreterOptions` | string | `""` | user, project | Python Console | Interpreter options — Passed to the interpreter that runs the console, for example -X dev. |

### Project | Python Interpreter

Page id `project.python-interpreter` (edited on the page, not as rows).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `python.interpreter` | string | `""` | user, project |  | Python Interpreter — The project's interpreter: the path of a local python executable (a detected environment) or the name of a configured interpreter. |
| `python.interpreters` | objects | `[]` | user |  | Python interpreters — Interpreters available in every project: local, Docker, Docker Compose, SSH and WSL. |
| `python.projectInterpreters` | objects | `[]` | user, project |  | Python interpreters of this project |

### Languages & Frameworks | Schemas and DTDs | JSON Schema Mappings

Page id `lang.json-schema` (plugin `forge.json-schema`). See [JSON Schema](../user/json-schema.md).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `jsonSchema.mappings` | objects | `[]` | user, project |  | JSON Schema mappings — `name`, `schema` (a project path, an absolute path or a URL; empty: no schema), `version` (`4`, `6`, `7`, `2019-09`, `2020-12` or empty for automatic) and `files` (one file, directory ending in `/` or pattern per line). |

### Languages & Frameworks | Schemas and DTDs | Remote JSON Schemas

Page id `lang.json-schema.remote` (plugin `forge.json-schema`).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `jsonSchema.remote.enabled` | bool | `false` | user |  | Allow downloading JSON schemas from remote sources — Schemas named by a URL ($schema, mappings, $ref) are downloaded into Forge's cache folder and refreshed daily. Built-in schemas are bundled and need no download. |
| `jsonSchema.remote.schemaStore` | bool | `true` | user |  | Use schemastore.org JSON Schema catalog — Find schemas for other well-known files (renovate.json, .babelrc, Helm charts, ...) in the SchemaStore catalog. |

### Languages & Frameworks | Markdown

Page id `lang.markdown`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `markdown.defaultLayout` | enum: `editor`, `split`, `preview` | `split` | user | Preview | Default layout — How Markdown files open. The buttons in the top-right corner of the editor switch it per file. |
| `markdown.syncScroll` | bool | `true` | user | Preview | Synchronize scrolling between the editor and the preview |
| `markdown.frontMatter` | enum: `table`, `hidden` | `table` | user | Preview | Front matter — The YAML (---) or TOML (+++) block at the start of a file. |
| `markdown.mermaid` | bool | `true` | user | Markdown Extensions | Mermaid — Render ```mermaid code blocks as diagrams. |

### Languages & Frameworks | Node.js

Page id `lang.node`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `web.packageManager` | enum: `auto`, `npm`, `pnpm`, `yarn`, `bun` | `auto` | user, project | Package Manager | Package manager — Auto uses the packageManager field of package.json, else the lock file (pnpm-lock.yaml, yarn.lock, bun.lock, package-lock.json). |
| `web.npm.gutterIcons` | bool | `true` | user | npm Scripts | Show run icons next to scripts in package.json |
| `web.npm.debugArguments` | string | `--inspect` | user | npm Scripts | Node options when debugging scripts — Set as NODE_OPTIONS for Debug; attach Chrome DevTools or another inspector client to the port. |

### Languages & Frameworks | TypeScript

Page id `lang.typescript`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `web.navigation.components` | bool | `true` | user | Frameworks | Navigate from JSX and template tags to components, props and events — Ctrl+B and Ctrl+Click on <UserCard>, title=… and @select resolve through imports before the language server is asked. |
| `angular.gutterIcons` | bool | `true` | user | Angular | Show line markers between components, templates and styles |

### Languages & Frameworks | ESLint

Page id `lang.eslint`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `web.eslint.path` | path | `""` | user, project |  | ESLint package — Empty: node_modules/.bin/eslint of the project. A relative path starts at the project root. The project's ESLint configuration is used. |

### Languages & Frameworks | Prettier

Page id `lang.prettier`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `web.prettier.path` | path | `""` | user, project |  | Prettier package — Empty: node_modules/.bin/prettier of the project. A relative path starts at the project root. The project's Prettier configuration is used. |

### Tools | Actions on Save

Page id `tools.actions-on-save`. See [Actions on Save](../user/actions-on-save.md).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `actionsOnSave.reformat.enabled` | bool | `false` | user, project |  | Reformat code — Reformat Code (Ctrl+Alt+L) when the file is saved. |
| `actionsOnSave.reformat.scope` | enum: `whole`, `changed` | `whole` | user, project |  | Reformat — "Changed lines" reformats only the lines changed since the last commit; files that are not committed yet are reformatted completely. |
| `actionsOnSave.reformat.fileTypes` | list | `[]` | user, project |  | Reformat file types — File type ids (php, javascript, ...) reformatted on save; empty means all file types. |
| `actionsOnSave.optimizeImports` | bool | `false` | user, project |  | Optimize imports — Remove unused imports and sort the others (PHP). |
| `actionsOnSave.rearrange` | bool | `false` | user, project |  | Rearrange code — Order class members: constants, properties, constructor, methods (PHP). |
| `actionsOnSave.codeCleanup` | bool | `false` | user, project |  | Run code cleanup — Apply the safe quick fixes: unused imports, unnecessary semicolons, a redundant closing tag (PHP). |
| `actionsOnSave.eslint` | bool | `false` | user, project |  | Run eslint --fix — Fix ESLint problems in saved JavaScript, TypeScript and Vue files. (HTML, CSS & JavaScript plugin) |
| `actionsOnSave.prettier.enabled` | bool | `false` | user, project |  | Run Prettier — Run `prettier --write` on saved files that match the patterns below. (HTML, CSS & JavaScript plugin) |
| `actionsOnSave.prettier.files` | string | `*.js; *.ts; *.jsx; *.tsx; *.cjs; *.cts; *.mjs; *.mts; *.vue; *.astro` | user, project |  | Run for files — Patterns separated by `;`; patterns with a `/` match the path, the others the file name. (HTML, CSS & JavaScript plugin) |
| `actionsOnSave.phpCsFixer` | bool | `false` | user, project |  | Run PHP CS Fixer — Run `php-cs-fixer fix` on saved PHP files. (PHP plugin) |
| `actionsOnSave.pint` | bool | `false` | user, project |  | Run Laravel Pint — Run `pint` on saved PHP files. (PHP plugin) |
| `actionsOnSave.runOnAutosave` | bool | `false` | user, project |  | Also run on autosave — Run Reformat code, Optimize imports, Rearrange code and Run code cleanup when files are saved automatically too. External tools only run on explicit save (Ctrl+S). |

### Tools | Terminal

Page id `tools.terminal`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `terminal.shellPath` | path | `""` | user | Application Settings | Shell path — Leave empty to use the system default shell. |
| `terminal.tabName` | string | `Local` | user | Application Settings | Default tab name |
| `terminal.audibleBell` | bool | `false` | user | Application Settings | Audible bell — Play a short tone when a program rings the terminal bell. |
| `terminal.copyOnSelection` | bool | `false` | user | Application Settings | Copy to clipboard on selection |
| `terminal.shellIntegration` | bool | `true` | user | Application Settings | Shell integration — bash and zsh report their working directory (shown in the tab tooltip) and command boundaries (green and red marks in the scrollbar for succeeded and failed commands). Applies to new sessions. |
| `terminal.showApplicationTitle` | bool | `true` | user | Application Settings | Show application title — Name terminal tabs after the title the running program sets, e.g. Claude Code's current task. |
| `terminal.showProgressInStatusBar` | bool | `true` | user | Application Settings | Show progress in the status bar — Programs such as Claude Code, winget and systemd report progress (OSC 9;4). It is always shown on the terminal tab. |
| `terminal.optionAsMeta` | bool | `true` | user | Application Settings | Use Option as Meta key — macOS only. |
| `terminal.cursorShape` | enum: `block`, `underline`, `vertical` | `block` | user | Application Settings | Cursor shape |
| `terminal.cursorBlink` | bool | `true` | user | Application Settings | Blinking cursor |
| `terminal.startDirectory` | path | `""` | user, project | Project Settings | Start directory — Leave empty to start in the project root. |
| `terminal.environment` | list | `[]` | user, project | Project Settings | Environment variables — One NAME=value pair per entry. |
| `terminal.font.family` | string | `""` | user | Font | Font — Leave empty to use the editor font. Missing symbols fall back to installed Nerd Fonts and emoji fonts. |
| `terminal.font.size` | number (6–72) | `13` | user | Font | Size |
| `terminal.font.lineHeight` | number (1.0–3.0) | `1.2` | user | Font | Line height |
| `terminal.renderer` | enum: `auto`, `webgl`, `dom` | `auto` | user | Rendering | Renderer — WebGL draws on the GPU and is the fastest. Automatic falls back to DOM when WebGL is unavailable. |
| `terminal.scrollback` | int (0–1000000) | `10000` | user | Rendering | Scrollback buffer — Number of lines kept above the visible area. |

### Tools | Command Line Tool Support

Page id `tools.command-line-tools`. See [Command Line Tool Support](../user/command-line-tools.md).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `commandLineTools.tools` | objects | `[]` | user, project | — | Command line tools — Tools that Tools \| Run Command… (Ctrl+Shift+X) runs. Entries: `kind` (`composer`, `artisan`, `symfony`, `wp-cli`, `drush`, `symfony-based`, `php-script`), `name`, `alias` (unique), `path`, `interpreter` (empty: the project default), `showConsoleIn` (`toolWindow` or `popup`), `enabled` (`false` turns it off). |
| `commandLineTools.autoDetect` | bool | `true` | user, project | — | Add the tools the project uses automatically — Composer, Laravel Artisan, Symfony Console, WP-CLI and Drush, unless a tool of that kind or alias is configured. |

### Tools | Database

Page id `tools.database`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `database.pageSize` | int (10–100000) | `500` | user | Data Editor and Viewer | Result set page size — Rows fetched per page in consoles and table editors. |
| `database.showNullAs` | string | `<null>` | user | Data Editor and Viewer | Show NULL as |
| `database.suggestFromEnv` | bool | `true` | user | General | Suggest data sources from .env files — Offer to add the database configured in Laravel's DB_* or Symfony's DATABASE_URL. |

### Tools | AI Agents

Page id `tools.ai-agents`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `agents.claude-code.path` | path | `""` | user | Claude Code | Claude Code executable — Leave empty to find claude on PATH. |
| `agents.claude-code.model` | enum: `default`, `sonnet`, `opus`, `haiku` | `default` | user | Claude Code | Default model |
| `agents.claude-code.permissionMode` | enum: `default`, `acceptEdits`, `plan`, `bypassPermissions` | `default` | user | Claude Code | Default permission mode — How much the agent may do before asking. Can be changed per chat. |
| `agents.gemini-cli.path` | path | `""` | user | Gemini CLI | Gemini CLI executable — Leave empty to find gemini on PATH. |
| `agents.gemini-cli.model` | enum: `default`, `gemini-2.5-pro`, `gemini-2.5-flash`, `gemini-2.5-flash-lite` | `default` | user | Gemini CLI | Default model |
| `agents.gemini-cli.permissionMode` | enum: `default`, `acceptEdits`, `bypassPermissions` | `default` | user | Gemini CLI | Default approval mode — Gemini CLI cannot ask in the IDE, so the mode decides which tools it may use. Can be changed per chat. |

### Tools | Browser Companion

Page id `tools.browser-companion`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `browserCompanion.enabled` | bool | `true` | user | Built-in Server | Accept open requests from paired browsers — Runs a small server on 127.0.0.1 that only paired browser extensions can use. |
| `browserCompanion.port` | int (1024–65535) | `63342` | user | Built-in Server | Port — If the port is taken (for example by PhpStorm), the next 9 ports are tried. The extension finds Forge in this range. |
| `browserCompanion.focusWindow` | bool | `true` | user | Built-in Server | Bring Forge to the front when a file is opened |

### Tools | External Tools

Page id `tools.external-tools`. See [External Tools](../user/run-debug.md#external-tools).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `tools.externalTools` | objects | `[]` | user |  | External Tools — Programs run from Tools \| External Tools, the editor and Project view context menus, the keymap and Before launch tasks: name, group, description, program, arguments and working directory with macros such as $FilePath$, plus the options and output filters ($FILE_PATH$:$LINE$, one per line). |

### Tools | HTTP Client

Page id `tools.http-client`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `httpClient.environment` | string | `""` | user, project |  | Environment — Environment of http-client.env.json (and http-client.private.env.json) that requests of .http files run with. Empty: dev, else the first one; $none: no environment. Chosen in the toolbar of .http editors. |

### Build, Execution, Deployment | JDKs

Page id `build.jdks` (plugin `forge.java`, a custom page: the project JDK, language level and JDK list are edited there). See [JavaForge](../user/java.md#jdks).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `java.jdks` | list | `[]` | user |  | Added JDKs — JDK homes added by hand (Add JDK from Disk…); detected and downloaded JDKs are listed without this. |
| `java.projectJdk` | string | `""` | user, project |  | Project JDK — JDK home (or name, like temurin-21) the project builds and runs with. Empty: the newest detected JDK. |
| `java.languageLevel` | enum: ``, `8`, `11`, `17`, `21`, `25` | `""` | user, project |  | Language level — Java release of the sources. Empty: from the build (toolchain, maven.compiler.release) or the project JDK. |
| `java.download.adoptiumApi` | string | `https://api.adoptium.net` | user | Downloads | Adoptium API — Where Download JDK… looks up Eclipse Temurin builds (and their SHA-256 checksums). |

### Build, Execution, Deployment | Build Tools

Page id `build.tools` (plugin `forge.java`). See [JavaForge](../user/java.md#editing-build-scripts).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `java.mavenCentral.enabled` | bool | `true` | user | Maven Central | Search Maven Central — Complete dependency coordinates and versions in build.gradle(.kts) and pom.xml and report newer versions (cached in the user cache dir). |
| `java.mavenCentral.searchUrl` | string | `""` | user | Maven Central | Search API — Empty: https://search.maven.org. |
| `java.mavenCentral.repositoryUrl` | string | `""` | user | Maven Central | Repository — Where maven-metadata.xml (the versions) comes from. Empty: https://repo1.maven.org/maven2. |

### Build, Execution, Deployment | Build Tools | Gradle

Page id `build.tools.gradle` (plugin `forge.java`). See [JavaForge](../user/java.md#gradle-settings-and-the-wrapper).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `java.build.preferredTool` | enum: `gradle`, `maven` | `gradle` | user, project |  | Build tool of projects with both — Which build Forge uses when a folder has a pom.xml and a Gradle build. |
| `java.gradle.distribution` | enum: `wrapper`, `specified`, `local` | `wrapper` | user, project | Gradle | Use Gradle from — The wrapper (gradle/wrapper/gradle-wrapper.properties), a Gradle home you choose, or gradle on the PATH. |
| `java.gradle.home` | path | `""` | user | Gradle | Gradle home — Used with "Specified location": the Gradle installation folder (or its bin/gradle). |
| `java.gradle.jvm` | path | `""` | user, project | Gradle | Gradle JVM — JDK home (or name, like temurin-21) Gradle syncs, builds and runs with. Empty: the project JDK. |
| `java.gradle.offline` | bool | `false` | user, project | Gradle | Offline work — Gradle resolves dependencies from its caches only (--offline). |
| `java.gradle.buildAndRunUsing` | enum: `gradle`, `idea` | `gradle` | user, project | Build and run | Build and run using — Build Project and Application configurations run through Gradle (forgeRun) or Eclipse JDT LS compiles and Forge runs java itself. |
| `java.gradle.runTestsUsing` | enum: `gradle`, `idea` | `gradle` | user, project | Build and run | Run tests using — JUnit tests run through Gradle's test task or the JUnit Platform Console Launcher on JDT LS's class path (TestNG always runs through Gradle). |
| `java.junit.consoleLauncher` | path | `""` | user | Build and run | JUnit Platform Console Launcher — junit-platform-console-standalone.jar for "Run tests using: IntelliJ IDEA". Empty: the newest in ~/.m2 or Gradle's cache. |
| `java.gradle.servicesUrl` | string | `""` | user |  | Gradle services (hidden) — Where Update Wrapper looks up Gradle releases and their SHA-256 sums. Empty: https://services.gradle.org. |

### Build, Execution, Deployment | Build Tools | Maven

Page id `build.tools.maven` (plugin `forge.java`).

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `java.maven.useWrapper` | bool | `true` | user, project |  | Use the Maven wrapper (mvnw) — Run Maven through the project's mvnw when it has one. |
| `java.maven.executable` | path | `""` | user |  | Maven home path — The mvn executable used without a wrapper. Empty: mvn on the PATH. |
| `java.maven.offline` | bool | `false` | user, project |  | Work offline |
| `java.maven.skipTests` | bool | `false` | user, project |  | Skip tests — Maven goals run with -DskipTests (the Maven tool window's Skip Tests toggle; test runs ignore it). |
| `java.maven.profiles` | list | `[]` | user, project |  | Profiles — Profiles the Maven tool window selected: id to activate, !id to deactivate. |
| `java.maven.downloadSources` | bool | `true` | user |  | Download sources of dependencies |

### Build, Execution, Deployment | Debugger

Page id `build.debugger`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `debugger.lldb.path` | path | `""` | user | Debug Adapters | LLDB DAP executable — Path of lldb-dap (lldb-vscode in older LLVM releases). Empty: lldb-dap on the PATH. |
| `debugger.gdb.path` | path | `""` | user | Debug Adapters | GDB executable — GDB 14 or newer (its built-in DAP server, gdb -i dap). Empty: gdb on the PATH. |
| `debugger.dap.requestTimeout` | int (1000–120000) | `10000` | user | Debug Adapter Protocol | Debug adapter request timeout (ms) — How long Forge waits for an answer of a debug adapter before it reports an error. |
| `debugger.dap.trace` | bool | `false` | user | Debug Adapter Protocol | Log Debug Adapter Protocol messages — Write every message to and from debug adapters to the Forge log (trace level). |
| `debugger.showInlineValues` | bool | `true` | user | Data Views | Show variable values in editor while debugging — For debug adapter sessions; PHP uses the setting under Languages & Frameworks \| PHP \| Debug. |
| `debugger.memory.bytesPerRow` | int (4–64) | `16` | user | Data Views | Memory view: bytes per row |

### Build, Execution, Deployment | Deployment

Page id `build.deployment`. No settings keys: the servers are project data in
`.forge/deployment.toml` (FTP passwords in the OS keychain), see [Deployment](../user/deployment.md).

### Build, Execution, Deployment | Deployment | Options

Page id `build.deployment.options`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `deployment.excludeNames` | string | `.svn;.cvs;.idea;.DS_Store;.git;.hg;.forge` | user, project |  | Exclude items by name — Names separated by semicolons; * and ? are wildcards. These files and folders are never uploaded, downloaded or compared. |
| `deployment.overwriteUpToDate` | bool | `false` | user, project | Transfer | Overwrite up-to-date files — Off: files whose deployed copy has the same size and is not older are skipped. |
| `deployment.preserveTimestamps` | bool | `true` | user, project | Transfer | Preserve files timestamps — The copy gets the modification time of the source (FTP servers need MFMT). |
| `deployment.deleteTargetItems` | bool | `false` | user, project | Transfer | Delete target items when source ones do not exist — When a folder is uploaded or downloaded, files that exist only on the target are deleted. |
| `deployment.createEmptyDirs` | bool | `false` | user, project | Transfer | Create empty directories |
| `deployment.warnOverNewer` | enum: `timestampAndSize`, `content`, `no` | `timestampAndSize` | user, project | Transfer | Warn when uploading over newer file |
| `deployment.compareBy` | enum: `content`, `size`, `timestamp` | `content` | user, project | Transfer | Sync with Deployed compares by |
| `deployment.autoUpload` | enum: `always`, `explicit`, `never` | `never` | user, project | Automatic upload | Upload changed files automatically to the default server — Off in Safe Mode: an untrusted project never uploads on its own. |

### Build, Execution, Deployment | Docker

Page id `build.docker`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `docker.connection` | enum: `auto`, `unix`, `pipe`, `tcp`, `ssh`, `context` | `auto` | user | Connection | Connect to Docker daemon with |
| `docker.socketPath` | path | `""` | user | Connection | Socket path — Unix socket (e.g. /var/run/docker.sock, ~/.docker/run/docker.sock) or named pipe (\\.\pipe\docker_engine). Empty: the first existing default. |
| `docker.tcp.url` | string | `tcp://localhost:2375` | user | Connection | Engine API URL — For TCP connections, e.g. tcp://192.168.1.20:2376. |
| `docker.tcp.tls` | bool | `false` | user | Connection | Use TLS (verify the daemon certificate) |
| `docker.tcp.certPath` | path | `""` | user | Connection | Certificates folder — Folder with ca.pem, cert.pem and key.pem (like DOCKER_CERT_PATH). |
| `docker.ssh.url` | string | `""` | user | Connection | SSH host — For SSH connections: ssh://user@host[:port] or a Host alias of ~/.ssh/config. Keys come from the ssh-agent and ~/.ssh; the host key must be in known_hosts. |
| `docker.ssh.socketPath` | string | `""` | user | Connection | Docker socket on the host — Empty: /var/run/docker.sock (rootless: /run/user/<uid>/docker.sock). |
| `docker.context` | string | `""` | user | Connection | Docker context — For Docker context connections: a context of `docker context ls` (~/.docker/contexts). |
| `docker.cliPath` | path | `docker` | user | Tools | Docker executable — Used for docker compose, image builds and exec terminals. |
| `docker.logs.tail` | int (10–100000) | `500` | user | Tools | Show the last log lines — Lines loaded when a Log tab opens; new lines follow. |

### Build, Execution, Deployment | Docker | Docker Registry

Page id `build.docker.registries`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `docker.registries` | list | `[]` | user |  | Registry accounts — One entry per registry: server, a tab, the user name. Passwords are kept in the OS keychain. |

### Build, Execution, Deployment | Trusted Locations

Page id `build.trusted-locations`. No settings keys: trusted projects and locations are kept in
`<config dir>/forge/trusted-projects.toml`, outside the settings files, so a project's own
settings cannot trust it (see [Project trust and Safe Mode](../user/project-trust.md)).

### Build, Execution, Deployment | Kubernetes

Page id `build.kubernetes`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `kubernetes.kubeconfig` | path | `""` | user | Configuration | Kubeconfig file — Empty: $KUBECONFIG or ~/.kube/config. |
| `kubernetes.defaultContext` | string | `""` | user | Configuration | Default context — Empty: the kubeconfig's current-context. |
| `kubernetes.defaultNamespace` | string | `""` | user | Configuration | Default namespace — Empty: the context's namespace, else default. |
| `kubernetes.kubectlPath` | path | `kubectl` | user | Tools | kubectl executable — Used on this machine for exec terminals in pods and for applying manifests. |
| `kubernetes.remoteProjectHost` | bool | `true` | user | Remote projects | In remote projects, use Kubernetes of the project host — Contexts come from the host's kubeconfig ($KUBECONFIG or ~/.kube/config) and kubectl runs on the host: logs, exec terminals, port forwards (tunnelled back through the project's SSH connection) and applying manifests. Off: this machine's kubeconfig. |
| `kubernetes.hostKubectlPaths` | list | `[]` | user | Remote projects | kubectl from the project host — One entry per host: user@host, a tab, the kubectl path on that host. Hosts without an entry use kubectl from their PATH. Edited in the "kubectl from the Project Host" table. |
