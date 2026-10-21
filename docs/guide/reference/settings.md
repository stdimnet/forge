---
title: Settings reference
description: Every setting key Forge's plugins declare, with type, default, scope and where it appears in the Settings dialog.
order: 20
tags: [settings, reference, configuration]
---

# Settings reference

All settings are declared by plugin manifests (`[[contributions.settings]]` in
`app/src-tauri/plugins/*.toml`) and shown on the Settings page whose id is their `group`. Values
are layered: default < user (`<config dir>/forge/settings.toml`) < project
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

### Appearance & Behavior | Appearance

Page id `appearance`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `appearance.theme` | enum: `dark`, `light`, `solarized-dark`, `solarized-light` | `dark` | user |  | Theme |
| `appearance.uiFont.family` | enum: `Inter`, `system-ui`, `Segoe UI`, `Roboto` | `Inter` | user | Font | Font — Font used for menus, tool windows and dialogs. |
| `appearance.uiFont.size` | int (10–20) | `13` | user | Font | Size |
| `appearance.mainMenu` | enum: `hamburger`, `merged`, `toolbar` | `hamburger` | user | UI Options | Main menu — Hamburger menu: behind the button at the left of the main toolbar. Merged: the menu names in the main toolbar. Separate toolbar: a classic menu bar above the main toolbar. |
| `appearance.compactMode` | bool | `false` | user | UI Options | Compact mode — Reduces paddings and the height of toolbars and tool window headers. |
| `appearance.smoothScrolling` | bool | `true` | user | UI Options | Smooth scrolling — Animate scrolling in lists and trees. |
| `appearance.showSplash` | bool | `true` | user | UI Options | Show splash screen on startup — The Forge wordmark fills with molten metal while the IDE loads, and offers updates. |
| `appearance.windowControls` | enum: `auto`, `windows`, `macos`, `gnome`, `kde`, `linux`, `native` | `auto` | user | Window | Window controls — Look of the title bar buttons of the main window and dialogs. "Native" lets the operating system draw the title bar. |
| `appearance.showToolWindowBars` | bool | `true` | user | Tool Windows | Show tool window bars — The stripes with tool window buttons along the window edges. |
| `appearance.showToolWindowNumbers` | bool | `true` | user | Tool Windows | Show tool window numbers |
| `projectView.openFilesWithSingleClick` | bool | `true` | user | Project View | Open files with single click — In the Project tool window a single click opens files and expands folders. Turn it off to select with a single click and open with a double click, like PHPStorm. |

### Appearance & Behavior | System Settings

Page id `system`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `system.reopenLastProject` | bool | `true` | user | Project | Reopen projects on startup — the last local project, when Forge starts without a project argument |
| `system.confirmExit` | bool | `true` | user | Project | Confirm before exiting the IDE |
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

Page id `keymap`. The override lists are edited through the Keymap page, not as text.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `keymap.active` | enum: `auto`, `$default`, `macOS` | `auto` | user |  | Keymap |
| `keymap.overrides.default` | list | `[]` | user |  | Changed shortcuts (Windows / Linux) — One entry per action: the action id, then its shortcuts, separated by tabs. |
| `keymap.overrides.macOS` | list | `[]` | user |  | Changed shortcuts (macOS) — One entry per action: the action id, then its shortcuts, separated by tabs. |

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
| `editor.stripTrailingSpaces` | enum: `none`, `modified`, `all` | `modified` | user | On Save | Remove trailing spaces on |
| `editor.ensureNewlineAtEof` | bool | `true` | user | On Save | Ensure every saved file ends with a line break |
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

### Editor | General | Postfix Completion

Page id `editor.postfix`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.postfix.enabled` | bool | `true` | user |  | Enable postfix completion — Type a template key after an expression, e.g. $user.if or items.forof, to wrap it. |
| `editor.postfix.showInCompletion` | bool | `true` | user |  | Show postfix templates in the completion autopopup |
| `editor.postfix.expandKey` | enum: `tab`, `space`, `enter` | `tab` | user |  | Expand templates with |
| `editor.postfix.disabled` | list | `[]` | user |  | Templates — Clear a template to stop offering it. Holds the ids of the switched-off templates (`php.echo`, `js.log`, `lua.print`). |

### Editor | Font

Page id `editor.font`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.font.family` | enum: `JetBrains Mono`, `Fira Code`, `Cascadia Code`, `Source Code Pro`, `Menlo`, `Consolas`, `monospace` | `JetBrains Mono` | user |  | Font |
| `editor.font.size` | number (6–72) | `13.5` | user |  | Size |
| `editor.font.lineHeight` | number (1.0–3.0) | `1.6` | user |  | Line height |
| `editor.font.ligatures` | bool | `true` | user |  | Enable ligatures — Render character sequences like -> and !== as a single glyph if the font supports it. |

### Editor | Code Style

Page id `editor.code-style`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `editor.codeStyle.lineSeparator` | enum: `lf`, `crlf`, `system` | `lf` | user, project |  | Line separator — For new files. Existing files keep theirs; change it in the status bar. |
| `editor.codeStyle.hardWrapAt` | int (40–400) | `120` | user, project |  | Hard wrap at (columns) — The right margin, drawn as a line in the editor. EditorConfig's max_line_length overrides it. |
| `editor.codeStyle.visualGuides` | string | `120` | user, project |  | Visual guides — Comma-separated column numbers, e.g. 80, 120. |
| `editor.codeStyle.useEditorConfig` | bool | `true` | user, project |  | Enable EditorConfig support — Indents, line separators, charset and the On Save options of .editorconfig files override these settings. |
| `editor.codeStyle.detectIndents` | bool | `true` | user, project |  | Detect and use existing file indents for editing — Files indented differently from the code style keep their indent; the status bar marks it with *. |

### Editor | Code Style | PHP

Page id `editor.code-style.php`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `php.codeStyle.useTabs` | bool | `false` | user, project | Tabs and Indents | Use tab character |
| `php.codeStyle.tabSize` | int (1–16) | `4` | user, project | Tabs and Indents | Tab size |
| `php.codeStyle.indent` | int (0–16) | `4` | user, project | Tabs and Indents | Indent |
| `php.codeStyle.continuationIndent` | int (0–16) | `4` | user, project | Tabs and Indents | Continuation indent |
| `php.codeStyle.braces` | enum: `psr12`, `allman`, `kr` | `psr12` | user, project | Braces Placement | Braces placement — Used by Reformat Code (Intelephense's formatter) and the Generate actions (Alt+Insert). |
| `php.codeStyle.blankLines.keepMax` | int (0–10) | `2` | user, project | Blank Lines | Keep maximum blank lines in code — Blank lines are applied by Reformat Code (Ctrl+Alt+L); strings, heredocs and comments are left as they are. |
| `php.codeStyle.blankLines.afterNamespace` | int (0–10) | `1` | user, project | Blank Lines | After namespace |
| `php.codeStyle.blankLines.afterImports` | int (0–10) | `1` | user, project | Blank Lines | After imports |
| `php.codeStyle.blankLines.aroundMethods` | int (0–10) | `1` | user, project | Blank Lines | Around method |

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

### Editor | TODO

Page id `editor.todo`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `todo.patterns` | list | `["\\btodo\\b.*", "\\bfixme\\b.*"]` | user | Patterns | Patterns — Regular expressions matched in comments, one per entry. Add a tab and `case` after a pattern to make it case-sensitive. |

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
| `php.languageLevel` | enum: `7.4`, `8.0`, `8.1`, `8.2`, `8.3`, `8.4` | `8.3` | user, project | Development Environment | PHP language level — Inspections and completion follow the syntax of this version. |
| `php.interpreter` | path | `php` | user, project | Development Environment | CLI Interpreter — Path to the php executable used to run scripts and tests. |
| `php.includePaths` | list | `[]` | user, project | Include Path | Include path — Extra directories indexed as libraries (e.g. global PEAR or framework stubs). |
| `php.stubs.path` | path | `""` | user | PHP Runtime | Default stubs path — A phpstorm-stubs checkout used for PHP's own classes, functions and constants (Go to Declaration on \Throwable, array_map). Empty: Forge downloads JetBrains' phpstorm-stubs into its cache folder on first use, or uses Intelephense's stubs. |
| `php.lsp.server` | enum: `auto`, `intelephense`, `phpactor`, `none` | `auto` | user | Language Server | Language server — Started automatically when a PHP project opens. |
| `php.lsp.maxFileSize` | int (100–100000) | `5000` | user | Language Server | Max indexed file size (KB) |
| `php.intelephense.autoInstall` | enum: `ask`, `always`, `never` | `ask` | user | Language Server | When Intelephense is not installed — Forge downloads Intelephense from the npm registry into its own directory (and Node.js when none is installed). Intelephense is not bundled with Forge. |
| `php.intelephense.licenceAccepted` | bool | `false` | user | Language Server | I accept the Intelephense licence — Asked once before Forge downloads Intelephense for you. Its licence (LICENSE.txt in the npm package, intelephense.com) applies to your copy. |

### Languages & Frameworks | PHP | Composer

Page id `lang.php.composer`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `php.composer.executable` | path | `composer` | user, project |  | Path to composer — A composer executable or composer.phar. |
| `php.composer.json` | path | `composer.json` | user, project |  | Path to composer.json — Relative to the project root. |
| `php.composer.syncWithComposerJson` | bool | `true` | user, project |  | Synchronize IDE settings with composer.json — The PHP language level follows `require.php` of composer.json (the status bar shows it); the language level setting applies when there is no constraint. |
| `php.composer.addPackagesAsLibraries` | bool | `true` | user, project |  | Add packages as libraries — Classes in vendor/ are resolved for navigation, completion and Intelephense. Off: vendor/ is left out. |
| `php.composer.checkUpdates` | bool | `false` | user |  | Check for package updates on project open — Runs `composer outdated --direct` on the project's machine and reports updates in a balloon. |

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

### Tools | HTTP Client

Page id `tools.http-client`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `httpClient.environment` | string | `""` | user, project |  | Environment — Environment of http-client.env.json (and http-client.private.env.json) that requests of .http files run with. Empty: dev, else the first one; $none: no environment. Chosen in the toolbar of .http editors. |

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

### Build, Execution, Deployment | Kubernetes

Page id `build.kubernetes`.

| Key | Type | Default | Scope | Section | Title and description |
| --- | --- | --- | --- | --- | --- |
| `kubernetes.kubeconfig` | path | `""` | user | Configuration | Kubeconfig file — Empty: $KUBECONFIG or ~/.kube/config. |
| `kubernetes.defaultContext` | string | `""` | user | Configuration | Default context — Empty: the kubeconfig's current-context. |
| `kubernetes.defaultNamespace` | string | `""` | user | Configuration | Default namespace — Empty: the context's namespace, else default. |
| `kubernetes.kubectlPath` | path | `kubectl` | user | Tools | kubectl executable — Used for exec terminals in pods. |
