# Roadmap

## Milestone 1 — Foundation (done)

- [x] Workspace, Tauri shell, PHPStorm-style workbench (toolbar, stripes, tool windows, tabs, status bar)
- [x] Plugin API + registry, layered settings with searchable Settings dialog; every setting in the dialog is applied
- [x] Editor + LSP client: hover docs, completion, diagnostics, go to definition
- [x] Language servers for ~30 languages: catalog, detection, install from Settings, several servers per file, project-wide symbol search
- [x] PHPForge language pack: Intelephense (bring your own license, one-click install) / phpactor, PHPDoc & attribute hover, Generate menu (Alt+Insert: getters/setters/constructor)
- [x] Git: Commit tool window, diff, inline blame, branches, AI commit messages, Project view Git submenu (Add to .gitignore, tags, stash, remotes)
- [x] AI providers: Anthropic, OpenRouter, Ollama, llama.cpp, LM Studio, any OpenAI-compatible API
- [x] AI chat: Claude Code rendered as chat (no terminal), tool calls & diffs as cards; Gemini CLI as a second agent; chat history per project
- [x] PHPStorm keymap 1:1 (engine in Rust), Find Action, Search Everywhere, main menu
- [x] Project type detection (packs activate per project) and New Project wizard with PHP, Web and Lua generators
- [x] Terminal (PTY, tabs, WebGL, OSC progress/notifications, shell integration), Run/Debug configurations, Run tool window with PHPUnit test tree
- [x] Laravel plugin: routes (route files or `artisan route:list`), Routes tool window, Blade highlighting and completion, `route()`/`view()`/`config()`/`env()`/`__()`/column/model property completion and navigation, gutter icons, Run Artisan Command… (Ctrl+Shift+X)
- [x] Symfony plugin: services/parameters (YAML + compiled container), routes, Twig completion & navigation, console commands, line markers
- [x] Doctrine plugin: entity mapping (attributes, annotations, XML, YAML), DQL highlighting, completion and inspections in `createQuery()` and QueryBuilder strings
- [x] Code hosts: GitLab, Forgejo/Gitea and GitHub pull/merge requests with inline review threads, issues, pipelines
- [x] WebForge pack: npm tool window and package.json run icons (npm, pnpm, yarn, bun), React/Vue/Angular component navigation, Angular template and style line markers, Node.js and TypeScript settings with the web language servers
- [x] LuaForge: Lua index in Rust (functions, tables, modules, `require` resolution, EmmyLua/LuaLS annotations), Structure, Go to Class/Symbol, Go to Declaration, annotation hover, lua-language-server settings, "Lua Script" run configurations with gutter run icons (lua, luajit, LÖVE), StyLua as Reformat Code
- [x] Remote development: SSH backend with a remote agent, indexing, language servers and terminals on the host; Laravel/Symfony/Doctrine analysis in the agent, `.forge/settings.toml` on the host, data sources through SSH tunnels, Xdebug through a remote port forward, Claude Code / Gemini CLI on the host, `ProxyJump` / `ProxyCommand`
- [x] Database tools: data sources (MySQL/MariaDB, PostgreSQL, SQLite), schema tree, SQL console, data grid
- [x] Xdebug debugger (DBGp): breakpoints, frames, variables, watches, stepping, path mappings
- [x] Services: Docker, Compose (incl. `!reset` / `!override`) and Kubernetes (pods, logs, exec, port forwarding)
- [x] Scratches & consoles, HTTP client (`.http` files, environments, response handlers)
- [x] Code navigation: Go to Declaration (incl. library stubs), Show/Find Usages, Find tool window, Recent Locations, bookmarks, TODO, notifications
- [x] Refactoring: Rename (Shift+F6, in place and dialog), Show Context Actions (Alt+Enter) with quick fixes and intentions
- [x] Search: trigram index for Find in Files, Go to File and Search Everywhere; Replace in Files in the Find window
- [x] Split editors, pinned tabs, editor tab settings; file encodings and line separators; restored tool window layout
- [x] Splash screen, update check and download (signed `latest.json` from GitHub releases), release workflow, Pride Versioning

Each `- [ ]` item below is one pull request of the [auto-runner](../.claude/skills/auto-runner/SKILL.md)
(large items are split). The detailed feature lists with links to the JetBrains documentation
are in [research/jetbrains-gap-analysis.md](research/jetbrains-gap-analysis.md).

## Milestone 2 — PHPStorm parity (next)

### Editor
- [x] Quick Documentation (`Ctrl+Q`), Documentation tool window, External Documentation (`Shift+F1`), Quick Definition (`Ctrl+Shift+I`), Type Info, Context Info, Error Description
- [x] Postfix completion and Complete Current Statement (`Ctrl+Shift+Enter`), Cyclic Expand Word
- [x] Live Templates with Surround With, Unwrap/Remove, Save as Live Template and Laravel/Symfony template sets
- [x] File and code templates (New ▸ PHP Class/Interface/Enum/Trait with PSR-4 namespace, Vue/React components)
- [x] Create symbols from usage (Alt+Enter: create method, field, class, constant)
- [x] Language injections (SQL/HTML/JSON/RegExp in PHP and JS strings, `// language=`, Edit Fragment, Run injected SQL)
- [x] Multiple carets from the keyboard (Clone Caret Above/Below, Column Selection Mode) and Paste from History
- [x] Actions on Save (reformat, optimize imports, rearrange, code cleanup, run tools)
- [x] Emmet, color previews and picker
- [x] Markdown preview (Mermaid, links), image/SVG viewer, CSV/TSV table editor
- [x] Sticky lines, structure breadcrumbs, Reader mode / rendered doc comments, custom folding regions
- [x] Spellchecking and grammar for comments, strings and identifiers
- [x] Code style schemes with presets (PSR-12, PER-CS, Symfony, Laravel, WordPress) and per-language pages
- [x] JSON Schema validation and completion (composer.json, package.json, compose, CI files)
- [x] Vim emulation

### Navigation and search
- [x] File Structure popup (`Ctrl+F12`), Navigation Bar (`Alt+Home`), Related Symbol (`Ctrl+Alt+Home`)
- [x] Last/Next Edit Location, Next/Previous Method, Next/Previous Change, Select In… and autoscroll from source
- [x] Highlight Usages in File, Find Usages in File, Find Usages settings (scope, options)
- [x] Type/Call/Method Hierarchy
- [x] Scopes and file colors; Structural Search and Replace; Analyze External Stack Trace
- [x] Go to Test and Create Test (PHPUnit/Pest, JS test ↔ subject)

### Refactoring
- [x] Extract Method/Variable/Constant, Introduce Field/Parameter, Inline
- [x] Move, Copy, Safe Delete, Change Signature, Make Static
- [x] Extract Interface/Superclass, Pull Members Up/Push Members Down, Refactor This popup
- [x] JS/TS refactorings (extract component, arrow/async conversions, destructuring)
- [x] Optimize Imports and auto-import settings (on the fly, on save, grouping)

### Version control
- [x] Merge conflict resolver (three-way merge, Resolve Simple Conflicts)
- [x] File and selection history, Compare with Revision/Branch, Compare Files/Folders/Clipboard
- [x] Stash/Unstash, Pull/Merge/Rebase/Reset/Tag/Remotes dialogs, shelf
- [x] Log actions (revert, undo, reset, edit message, squash/fixup), interactive rebase, cherry-pick
- [x] Changelists and partial commits, commit checks, patches
- [x] Get from VCS (clone), Commit Message History (`Ctrl+M`), worktrees, GPG/SSH signing
- [x] Local History with labels, selection history and Recent Changes

### Run, debug, PHP
- [x] PHP CLI interpreters: several local ones, Docker, Docker Compose, SSH, WSL, with path mappings
- [x] PHP Built-in Web Server, PHP Web Page and HTTP request debugging; Xdebug validation; logpoints, temporary breakpoints, Set Value, debugger console
- [x] Composer UI (add dependency, version completion, outdated markers, Install/Update, `composer audit`)
- [x] Command Line Tool Support (Composer/Artisan/Symfony/WP-CLI console with completion)
- [x] `.phpstorm.meta.php` advanced metadata and JetBrains attributes; include paths and PSR-4 roots
- [x] Test frameworks: Pest configuration, Codeception, Behat, PHPSpec; test history and auto-test
- [x] Run Anything (double `Ctrl`), compound run configurations, External Tools

### Workbench and project
- [x] Project Structure (Mark Directory As sources/tests/excluded), file nesting, Show Members
- [x] Several project windows, Distraction Free/Presentation/Zen/Full Screen, floating tool windows, editor tabs in their own window, layouts, IDE zoom
- [x] Invalidate Caches/Repair IDE, Settings for New Projects, Restore Defaults, custom file types
- [x] Other keymaps (VS Code, Eclipse, Sublime, Emacs), mouse shortcuts, color scheme editor and import
- [x] Project trust (safe mode) before running a project's scripts

### Deployment and remote
- [x] Deployment (SFTP/FTP/local servers, mappings, upload on save, compare with server)
- [x] Remote Host tool window, SSH configurations and SSH terminal sessions in local projects
- [x] Dev Containers and WSL projects

### Frameworks
- [x] WordPress (hooks, WP-CLI, code style); Drupal, Joomla, Shopware basics
- [x] Symfony translations, forms and events; Laravel dashboard (logs, `ide.json`)
- [x] Kubernetes contexts from the project's host

## Milestone 3 — Language packs I: Java, Python, Go, Rust, C/C++

- [ ] Shared debugging layer: DAP client in Rust used by every pack (attach to process, attach to DAP server, logpoints), native views (memory, disassembly, registers, core dumps, watchpoints)
- [ ] Shared test runner and coverage for every pack; run targets (Docker, WSL, SSH) for run, test and debug
- [ ] JavaScript/TypeScript debugger (Chrome, Node.js, attach) and JS test runners (Jest, Vitest, Mocha, Playwright, Cypress)
- [ ] JavaForge: Eclipse JDT LS (managed), JDK detection and download, Maven and Gradle, JUnit 5/TestNG, Java debugger
- [ ] JavaForge: Gradle and Maven tool windows (tasks, dependencies, sync, offline mode), Gradle wrapper, build and run through Gradle/Maven, Gradle Kotlin and Groovy DSL editing with completion
- [ ] KotlinForge: Kotlin (kotlin-lsp, managed), Kotlin/JVM run configurations, Kotlin scripts (`.kts`), JUnit 5/Kotest runners, Kotlin debugger (coroutines view), ktlint and detekt
- [ ] JavaForge: Spring Boot (run dashboard, beans, endpoints, `application.yml` completion), Jakarta EE basics
- [ ] JVM extras: Groovy and Scala basics (highlighting, run, Gradle/sbt import), Android basics (Gradle Android projects, adb devices, Logcat)
- [ ] PyForge: basedpyright/ruff/ty/Black, interpreters (venv, uv, Poetry, conda, pipenv, Hatch; Docker/SSH/WSL), Python Packages tool window, requirements inspections
- [ ] PyForge: pytest/unittest/tox runners, debugpy with debug console, Python console, profiler
- [ ] PyForge: Django (manage.py, template navigation, server/tests configurations), Flask, FastAPI, Celery
- [ ] PyForge: Jupyter notebooks (editor, kernels, outputs, DataFrame views, notebook diffs)
- [ ] GoForge: SDK/GOPATH, `go.mod`/`go.work`, gofmt/goimports/vet/generate, golangci-lint, Go Build/Test (bench, fuzz), Delve attach/remote, pprof
- [ ] RustForge: rustup toolchains, Cargo tool window and configurations, Clippy, rustfmt, doctest/nextest, LLDB/GDB renderers
- [ ] CForge: clangd (managed), CMake profiles/presets and tool window, compilation database, toolchains (GCC/Clang/MSVC/MinGW), build actions, CTest/GoogleTest/Catch2, debugger
- [ ] CForge v2: sanitizers, Valgrind, perf, vcpkg/Conan, embedded (OpenOCD, SVD, PlatformIO, ESP-IDF, Zephyr)

## Milestone 4 — Code quality, tooling and databases

- [ ] PHP quality tools: PHPStan, Psalm, PHP CS Fixer, PHP_CodeSniffer, PHP Mess Detector, Laravel Pint, Rector (on the fly and on save, also in Docker/remote interpreters)
- [ ] Web quality tools: ESLint, Prettier, Stylelint, Biome with fix on save
- [ ] Inspections: Forge's own inspections with severities and profiles, Inspect Code, Run Inspection by Name, Code Cleanup, suppressions, duplicates, data flow, command-line inspector
- [ ] Code coverage for PHP (Xdebug/PCOV), JS (c8/Istanbul) and the language packs
- [ ] Profiling: Xdebug and SPX snapshots, V8 CPU/heap
- [ ] Vulnerable dependency check (`composer audit`, npm audit)
- [ ] Database connectivity: SSH/SSL per data source, SQL Server, more engines (ClickHouse, Redis, MongoDB, Oracle, …), sessions, read-only/production colors
- [ ] Database explorer: full object tree, schema filter, virtual foreign keys, full-text search, DDL data sources
- [ ] SQL editor: resolve and inspections, dialect per file, formatter, user parameters, query history, Run SQL file, live templates
- [ ] Data editor: header sort/filter, value editor, foreign-key navigation, transpose, aggregates, compare data, charts
- [ ] Database objects: Create/Modify Table dialogs, Generate DDL, import (CSV/Excel/JSON), dump/restore with native tools
- [ ] ER diagrams, schema compare and migrations, query plans
- [ ] Web tooling: File Watchers, Live Edit, Vite/Webpack aliases, Svelte/Astro/Nuxt/Nx/Prisma/Deno/Bun, OpenAPI, GraphQL, HTTP client extras (OAuth 2.0, cURL, WebSocket/gRPC)

## Milestone 5 — Language packs II: .NET and Ruby

- [ ] .NET pack: Roslyn LSP (managed), .NET SDK, solutions/projects, build window, run configurations and launch profiles, netcoredbg debugger, xUnit/NUnit/MSTest, NuGet window, `dotnet new` templates, EditorConfig cleanup
- [ ] .NET pack: ASP.NET Core (Razor/Blazor, endpoints), EF Core commands, Aspire, Docker, Hot Reload, user secrets, Azure DevOps code host
- [ ] .NET pack: Unity and Godot integration (editor bridge, tests, debugging)
- [ ] Ruby pack: ruby-lsp, version managers, Bundler, Rake, RSpec/Minitest, rdbg debugger, RuboCop, ERB, IRB console
- [ ] Ruby pack: Rails (navigation, routes, generators, console, ActiveRecord data sources, i18n, Hotwire)

## Milestone 6 — AI and collaboration

- [ ] AI in the editor: Inline completion (local or hosted models), "Explain / Fix with AI" in Alt+Enter, AI-generated tests and docs
- [ ] Agents: Agent tasks that run, test and fix in a sandbox, with diffs to review
- [ ] Code review: Review pull/merge requests inside Forge with AI summaries
- [ ] Collaboration: Shared sessions (Code With Me style): follow, co-edit, shared terminal — in progress on `feat/code-with-me` (not the auto-runner's; leave it to its owner)
- [ ] MCP: Forge as an MCP server for external agents

## Milestone 7 — Platform and release

- [ ] Plugins: Marketplace, public plugin SDK with docs and templates, settings export/import and sync
- [ ] Releases: Signed Windows (Authenticode) and macOS (notarised) builds, Flatpak and AUR packages, stable and beta channels
- [ ] Localization: Translatable UI, German as the first translation
- [ ] Accessibility: Screen reader support, high-contrast themes, keyboard-only navigation everywhere, Presentation Assistant
