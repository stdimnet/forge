# Roadmap

## Milestone 1 — Foundation (in progress)

- [x] Workspace, Tauri shell, PHPStorm-style workbench (toolbar, stripes, tool windows, tabs, status bar)
- [ ] Plugin API + registry, layered settings with searchable Settings dialog
- [ ] Editor + LSP client: hover docs, completion, diagnostics, go to definition
- [x] Language servers for ~30 languages: catalog, detection, install from Settings, several servers per file, project-wide symbol search
- [ ] PHPForge language pack: Intelephense (bring your own license) / phpactor, PHPDoc & attribute hover, Generate menu (Alt+Insert: getters/setters/constructor)
- [ ] Git: Commit tool window, diff, inline blame, branches, AI commit messages
- [ ] AI providers: Anthropic, OpenRouter, Ollama, llama.cpp, LM Studio, any OpenAI-compatible API
- [x] AI chat: Claude Code rendered as chat (no terminal), tool calls & diffs as cards
- [ ] PHPStorm keymap 1:1, Find Action, Search Everywhere
- [x] Project type detection (packs activate per project) and New Project wizard with PHP, Web and Lua generators
- [x] Terminal (PTY, tabs), Run/Debug configurations, Run tool window with PHPUnit test tree
- [x] Laravel plugin: routes (route files or `artisan route:list`), Routes tool window, Blade highlighting and completion, `route()`/`view()`/`config()`/`env()`/`__()`/column/model property completion and navigation, gutter icons, Run Artisan Command… (Ctrl+Shift+X)
- [x] Symfony plugin: services/parameters (YAML + compiled container), routes, Twig completion & navigation, console commands, line markers
- [x] Doctrine plugin: entity mapping (attributes, annotations, XML, YAML), DQL highlighting, completion and inspections in `createQuery()` and QueryBuilder strings
- [x] Code hosts: GitLab, Forgejo/Gitea and GitHub pull/merge requests with inline review threads, issues, pipelines
- [x] WebForge pack: npm tool window and package.json run icons (npm, pnpm, yarn, bun), React/Vue/Angular component navigation, Angular template and style line markers, Node.js and TypeScript settings with the web language servers
- [x] LuaForge: Lua index in Rust (functions, tables, modules, `require` resolution, EmmyLua/LuaLS annotations), Structure, Go to Class/Symbol, Go to Declaration, annotation hover, lua-language-server settings, "Lua Script" run configurations with gutter run icons (lua, luajit, LÖVE), StyLua as Reformat Code

## Next branches

| Branch | Scope |
| --- | --- |
| `feat/laravel-plugin` | Routes, Blade, Eloquent, artisan runner |
| `feat/database-tools` | SQL editor, schema browser, data grid (MySQL, PostgreSQL, SQLite) |
| `feat/xdebug-run-configs` | Xdebug via DAP, custom run/debug configuration types |
| `feat/docker-kubernetes` | Services tool window: containers, compose, pods, logs |
| `feat/remote-sessions` | SSH remote backend with remote indexing & LSP |
| `feat/gemini-cli-plugin` | Gemini CLI as a chat agent |
| `feat/scratch-files` | Scratches & consoles (Ctrl+Alt+Shift+Insert) |
