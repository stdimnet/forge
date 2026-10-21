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

## Milestone 2 — next

| Area | Scope |
| --- | --- |
| Refactorings | Extract Method/Variable/Constant, Move, Change Signature, Safe Delete, Inline |
| Local History | Changes per file and project, revert, labels |
| VCS | Get from VCS (clone), Commit Message History (Ctrl+M), Replace in Files popup (Ctrl+Shift+R) |
| Windows | Several project windows, Distraction Free / Presentation / Zen modes |
| PHP | Composer Install/Update commands, Type/Call/Method Hierarchy, Live Templates, Surround With, Optimize Imports |
| Services | Kubernetes contexts from the project's host |
| Plugins | Marketplace, settings export/import |
