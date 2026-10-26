---
title: User guide
description: One page per Forge feature, written for people who use Forge as their IDE.
order: 0
tags: [user guide]
---

# User guide

Each page describes one feature the way PHPStorm users know it: tool windows, actions,
shortcuts (Windows/Linux, with macOS where it differs) and Settings paths. Where a feature is
not finished, the page says so.

## Working in the IDE

| Page | Covers |
| --- | --- |
| [Workbench and tool windows](workbench.md) | main toolbar, tool windows, Project view, tabs, Recent Files, Switcher, status bar |
| [Windows, modes and layouts](windows-and-layouts.md) | several project windows, Presentation / Distraction-Free / Zen / Full Screen, Zoom IDE, tool window view modes (Float, Undock, Window), editor tabs in their own window, Window \| Layouts |
| [Editor](editor.md) | completion, hover, Parameter Info, Go to Declaration, problems, formatting, editing actions, folding, find, gutter, context menu |
| [Sticky lines, breadcrumbs and Reader mode](code-structure.md) | Sticky Lines, structure breadcrumbs, custom folding regions, rendered doc comments (`Ctrl+Alt+Q`), Reader mode |
| [Markdown, images and CSV](markdown-images-csv.md) | Markdown preview (Mermaid, tables, links, images, front matter), image editor and SVG preview, Edit as Table for CSV/TSV |
| [Optimize Imports and Auto Import](imports.md) | Optimize Imports (`Ctrl+Alt+O`) for files, folders and the project, Auto Import on the fly and on completion, the import popup, Import class, the PHP code style's Imports tab |
| [Actions on Save](actions-on-save.md) | Reformat code, Optimize imports, Rearrange code, code cleanup, ESLint, Prettier, PHP CS Fixer and Laravel Pint on save |
| [Live templates](live-templates.md) | abbreviations + `Tab`, Insert Live Template, Surround With, Unwrap/Remove, Save as Live Template, template sets and settings |
| [Emmet and color previews](emmet-and-colors.md) | Emmet abbreviations in HTML, Blade, Twig, Vue, PHP, JSX and CSS, abbreviation preview, Surround with Emmet, Update/Remove Tag, edit points, color swatches and the color picker |
| [JSON Schema](json-schema.md) | validation, completion, hover and Go to Declaration from JSON schemas in JSON and YAML files: composer.json, package.json, tsconfig, ESLint, Prettier, Compose, GitHub Actions, GitLab CI, `$schema`, JSON Schema Mappings, the status bar widget, remote schemas |
| [Spelling and grammar](spelling-and-grammar.md) | Typo and Grammar inspections in comments, strings, names, Markdown and commit messages, Alt+Enter suggestions, Rename to, project-level and application-level dictionaries, custom Hunspell dictionaries |
| [Language injections](language-injections.md) | SQL, HTML, CSS, JSON and RegExp in PHP and JavaScript strings, `// language=` comments, Inject language or reference, Edit Fragment, Run SQL |
| [File and code templates](file-templates.md) | New ▸ PHP Class and other templates, the New PHP Class dialog with PSR-4 namespaces, `Alt+Insert` New popup, Settings \| Editor \| File and Code Templates |
| [Vim emulation](vim-emulation.md) | Tools \| Vim, Normal/Insert/Visual/Replace modes, motions, operators, text objects, registers, macros, ex commands, `~/.ideavimrc`, shortcut conflicts |
| [Search Everywhere and Find Action](search-everywhere.md) | Double Shift, Go to Class/File/Symbol, Find Action, fuzzy matching |
| [Scopes and file colors](scopes-and-file-colors.md) | named scopes in PHPStorm's scope language (`file:app//*.php`, `src:`, `test:`, `lib:`, `&&`, `\|\|`, `!`), predefined scopes, scopes in Find in Files, Replace in Files and Find Usages, file colors in editor tabs and the Project view |
| [Structural Search and Replace](structural-search.md) | templates with `$variables$` for PHP, JavaScript and TypeScript, Count / Text / Type filters, the template library, Replace with preview, Replace All and Undo |
| [Type, Method and Call Hierarchy](hierarchies.md) | the Hierarchy tool window: Type Hierarchy (`Ctrl+H`: class, supertypes, subtypes), Method Hierarchy (`Ctrl+Shift+H`) with PHPStorm's marks, Call Hierarchy (`Ctrl+Alt+H`: callers and callees with call sites), scopes and sorting |
| [Go to Test and Create Test](go-to-test.md) | Navigate \| Test (`Ctrl+Shift+T`) between classes, components and their tests, Choose Test popup, Create New PHP Test / Create New Test dialog (PHPUnit, Pest, Vitest, Jest, PSR-4 `autoload-dev`, Laravel Unit/Feature and Symfony layouts, test method stubs) |
| [File Structure, Navigation Bar and Related Symbol](navigation-bar-and-file-structure.md) | File Structure popup (`Ctrl+F12`, Show Inherited, speed search), Navigation Bar (`Alt+Home`, Top / In Status Bar / Don't Show, members), Related Symbol (`Ctrl+Alt+Home`: tests, views, templates, routes, migrations) |
| [File types and syntax highlighting](file-types.md) | file type detection, highlighted languages, config formats, template data languages, Settings \| Editor \| File Types (patterns, custom file types, ignored files) |
| [Language servers](language-servers.md) | detection, installation, per-server settings, Intelephense licence |
| [Keymap](keymap.md) | PHPStorm, VS Code, Eclipse, Sublime Text and Emacs keymaps, your own keymaps, changing keyboard and mouse shortcuts, contexts |
| [Shortcuts by topic](shortcuts/index.md) | every implemented shortcut explained by topic, plus a printable [reference card](shortcuts/reference-card.md) |
| [Color schemes](color-schemes.md) | Settings \| Editor \| Color Scheme: Ember, Anvil and your own schemes, attributes with foreground, background, bold, italic and effects, live preview, IntelliJ `.icls` import and export |
| [Settings and themes](settings-and-themes.md) | the Settings dialog, settings files, Settings for New Projects, Restore Defaults, Export / Import Settings, Restore Default Settings, themes, window controls |
| [Invalidate Caches and Repair IDE](repair-ide.md) | Reload All from Disk, the step-by-step Repair IDE, Invalidate Caches and its options |
| [Plugins and packs](plugins-and-packs.md) | packs, activation, Settings \| Plugins |

## Languages and frameworks

| Page | Covers |
| --- | --- |
| [PHP](php.md) | PHP index, Structure, hover, Generate (`Alt+Insert`), PHP settings |
| [Composer](composer.md) | composer.json toolbar (Install, Update, Validate, Show Outdated, Audit), outdated packages with Update package, package and version completion from Packagist, Manage Composer Dependencies, `composer audit` (Vulnerable Dependencies), composer.phar download |
| [Laravel](laravel.md) | Blade, routes, views, config, Eloquent, Routes tool window, artisan commands |
| [Symfony](symfony.md) | services, routes, Twig, console, Symfony tool window |
| [WordPress](wordpress.md) | hook completion, navigation, gutter icons and Find Usages, the WordPress tool window, WP-CLI, stubs, the WordPress code style with Yoda hints, plugin and theme templates |
| [Drupal, Joomla and Shopware](cms.md) | CMS detection, Drupal file types and hook completion with Go to the hook docs, Shopware Twig tags, Drush and Shopware console |
| [Doctrine and DQL](doctrine.md) | entity mappings, Doctrine tool window, DQL in PHP strings |
| [WebForge](webforge.md) | npm scripts and package managers, React, Vue, Angular navigation |
| [LuaForge](luaforge.md) | Lua index, LÖVE, Neovim, lua-language-server, StyLua, Lua runs |
| [PyForge (Python)](python.md) | Python interpreters (venv, uv, Poetry, conda, Pipenv, Hatch, Docker, SSH, WSL), Python Packages, requirements inspections, basedpyright / ty / ruff, Black, Python runs |
| [JavaForge](java.md) | Eclipse JDT LS (installed on request), JDKs (detection, Temurin downloads, project JDK, language level), Maven and Gradle (wrappers, Build Project, Gradle / Maven tool windows), Application runs with a Run gutter on `main`, JUnit 5/4 and TestNG with JaCoCo coverage, the Java debugger (java-debug) and Remote JVM Debug |
| [KotlinForge](kotlin.md) | kotlin-lsp (downloaded on request), Kotlin runs through Gradle, Maven or kotlinc, Kotlin scripts, JUnit 5 and Kotest with a Run gutter, the Kotlin debugger with a Coroutines tab, ktlint and detekt in the Problems view |

## Version control

| Page | Covers |
| --- | --- |
| [Git](git.md) | Commit tool window, AI commit messages, diff, blame, branches, log |
| [Local History](local-history.md) | revisions recorded on save, refactorings, VCS operations and external changes; Show History, Show History for Selection, Revert, Create Patch, labels, Recent Changes (`Alt+Shift+C`) |
| [Code hosts](code-hosts.md) | GitHub, GitLab, Forgejo/Gitea pull/merge requests, issues, pipelines |

## Running, debugging and tools

| Page | Covers |
| --- | --- |
| [Project trust and Safe Mode](project-trust.md) | "Trust and Open Project?", Safe Mode for untrusted projects (no scripts, runs, tests, debugger, build tools or project-local language servers), the Safe Mode banner, Settings \| Build, Execution, Deployment \| Trusted Locations, remote projects |
| [Run/Debug configurations](run-debug.md) | configuration types, run widget, Run tool window, Analyze Stack Trace, test results |
| [Testing](testing.md) | the shared test runner of every pack: gutter run icons, the test tree, Click to see difference, Rerun Failed Tests, Test History, auto-test, busted, and the result formats (TeamCity, JUnit, TAP, Go, libtest, pytest, Jest / Vitest) |
| [Code coverage](coverage.md) | Run with Coverage, the Coverage tool window (line and branch coverage per directory and file), coverage marks with hits in the editor gutter, the Project view, Show Coverage Data (`Ctrl+Alt+F6`), importing LCOV, Cobertura, Clover, JaCoCo and Go reports |
| [Run targets (Run on)](run-targets.md) | running, testing and debugging a configuration in a Docker image, a Docker Compose service, over SSH, in WSL or a Dev Container, with path mappings and debugger ports |
| [PHP test frameworks](test-frameworks.md) | Settings \| PHP \| Test Frameworks, PHPUnit, Pest, Codeception, Behat and PHPSpec configurations, gutter icons, Rerun Failed Tests, Test History, auto-test, Gherkin and Behat step navigation |
| [Command Line Tool Support](command-line-tools.md) | Tools \| Run Command… (`Ctrl+Shift+X`): Composer, Artisan, Symfony Console, WP-CLI, Drush and custom tools with completion, the Command Line Tools Console, Settings \| Tools \| Command Line Tool Support |
| [JavaScript debugging and testing](javascript-debugging.md) | js-debug for Node.js, npm scripts, tests and Chrome/Edge (TypeScript through source maps, attach), Jest, Vitest, Mocha, Playwright, Cypress and `node --test` |
| [Debugging](debugging.md) | Xdebug and debug adapters (LLDB, GDB): Attach to Process, breakpoints, logpoints, watchpoints, stepping, evaluate, memory, disassembly, registers, servers and path mappings |
| [Terminal](terminal.md) | tabs, shells, links |
| [Database tools and SQL console](database.md) | data sources, consoles, data grid, export |
| [Scratches, consoles and HTTP client](scratches-and-http.md) | scratch files, runs, `.http` requests, environments |
| [Services](services.md) | Docker, Docker Compose, Kubernetes |
| [Remote development](remote-development.md) | SSH, the Forge agent, what runs remotely |
| [Dev Containers and WSL](dev-containers-and-wsl.md) | devcontainer.json, building with the Docker CLI and the build log, Start / Stop / Rebuild / Delete, WSL distributions, `\\wsl$` paths, Terminal › WSL |
| [Deployment](deployment.md) | SFTP, FTP/FTPS and local servers, mappings, upload on save, Sync with Deployed |
| [Remote Host and SSH sessions](remote-host-and-ssh.md) | Remote Host tool window, Edit Remote File, SSH configurations, SSH terminal sessions |
| [AI](ai.md) | providers, AI commit messages, Claude Code and Gemini CLI chat |
| [Browser Companion](browser-companion.md) | browser extension, `forge://` links, error page integrations |
