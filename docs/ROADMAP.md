# Roadmap

## Milestone 1 — Foundation (in progress)

- [x] Workspace, Tauri shell, PHPStorm-style workbench (toolbar, stripes, tool windows, tabs, status bar)
- [ ] Plugin API + registry, layered settings with searchable Settings dialog
- [ ] Editor + LSP client: hover docs, completion, diagnostics, go to definition
- [ ] PHPForge language pack: Intelephense (bring your own license) / phpactor, PHPDoc & attribute hover, Generate menu (Alt+Insert: getters/setters/constructor)
- [ ] Git: Commit tool window, diff, inline blame, branches, AI commit messages
- [ ] AI providers: Anthropic, OpenRouter, Ollama, llama.cpp, LM Studio, any OpenAI-compatible API
- [x] AI chat: Claude Code rendered as chat (no terminal), tool calls & diffs as cards
- [ ] PHPStorm keymap 1:1, Find Action, Search Everywhere
- [x] Project type detection (packs activate per project) and New Project wizard with PHP, Web and Lua generators

## Next branches

| Branch | Scope |
| --- | --- |
| `feat/laravel-plugin` | Routes, Blade, Eloquent, artisan runner |
| `feat/symfony-plugin` | Container/services, Twig, routes, console |
| `feat/doctrine-dql` | DQL highlighting & completion, entity mapping |
| `feat/database-tools` | SQL editor, schema browser, data grid (MySQL, PostgreSQL, SQLite) |
| `feat/xdebug-run-configs` | Xdebug via DAP, custom run/debug configuration types |
| `feat/docker-kubernetes` | Services tool window: containers, compose, pods, logs |
| `feat/gitlab-forgejo` | GitLab, Forgejo/Gitea merge requests & issues |
| `feat/remote-sessions` | SSH remote backend with remote indexing & LSP |
| `feat/gemini-cli-plugin` | Gemini CLI as a chat agent |
| `feat/webforge` | Web pack |
| `feat/luaforge` | Lua pack |
| `feat/scratch-files` | Scratches & consoles (Ctrl+Alt+Shift+Insert) |
