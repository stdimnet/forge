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

## Milestone 2 — PHPStorm parity (next)

| Area | Scope |
| --- | --- |
| Refactorings | Extract Method/Variable/Constant, Move, Change Signature, Safe Delete, Inline |
| Local History | Changes per file and project, revert, labels |
| VCS | Get from VCS (clone), Commit Message History (Ctrl+M), interactive rebase, cherry-pick, shelf |
| Windows | Several project windows, Distraction Free / Presentation / Zen modes |
| PHP | Composer Install/Update commands, Type/Call/Method Hierarchy, Live Templates, Surround With, Optimize Imports, Go to Test |
| Services | Kubernetes contexts from the project's host |

## Milestone 3 — JavaForge and more languages

| Area | Scope |
| --- | --- |
| JavaForge pack | Java with Eclipse JDT LS (managed install, JDK detection and download), Maven and Gradle (tool windows, run configurations, dependency sync), JUnit 5 / TestNG test runner, Java debugger (JDWP through DAP) |
| JVM frameworks | Spring Boot (beans, endpoints, `application.yml` completion), Kotlin (kotlin-lsp), Jakarta EE basics |
| PyForge pack | Python with basedpyright / ruff, virtualenv / uv / Poetry interpreters, pytest runner, debugpy, Django and FastAPI |
| GoForge / RustForge packs | gopls and rust-analyzer with run configurations, test runners and debugging (Delve, CodeLLDB) |
| Shared | Language-neutral test runner and debugger (DAP) used by every pack |

## Milestone 4 — Code quality and tooling

| Area | Scope |
| --- | --- |
| PHP quality tools | PHPStan, Psalm, PHP CS Fixer, PHP_CodeSniffer, Rector as inspections and quick fixes |
| Web quality tools | ESLint, Prettier, Stylelint, Biome |
| Testing | Code coverage in the gutter and the Project view (PHPUnit, Pest, Jest, Vitest, JUnit) |
| Profiling | Xdebug profiler and SPX snapshots, call tree and hot spots |
| Database | ER diagrams, schema compare and migrations, query plans |
| Inspections | Forge's own inspections with severities, profiles and "Inspect Code" for the whole project |

## Milestone 5 — AI and collaboration

| Area | Scope |
| --- | --- |
| AI in the editor | Inline completion (local or hosted models), "Explain / Fix with AI" in Alt+Enter, AI-generated tests and docs |
| Agents | Agent tasks that run, test and fix in a sandbox, with diffs to review |
| Code review | Review pull/merge requests inside Forge with AI summaries |
| Collaboration | Shared sessions (Code With Me style): follow, co-edit, shared terminal |

## Milestone 6 — Platform and release

| Area | Scope |
| --- | --- |
| Plugins | Marketplace, public plugin SDK with docs and templates, settings export/import and sync |
| Releases | Signed Windows (Authenticode) and macOS (notarised) builds, Flatpak and AUR packages, stable and beta channels |
| Localization | Translatable UI, German as the first translation |
| Accessibility | Screen reader support, high-contrast themes, keyboard-only navigation everywhere |
