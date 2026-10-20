---
title: Plugins reference
description: Every built-in plugin with its id, pack, dependencies, the project types that activate it and what it contributes.
order: 80
tags: [plugins, packs, reference]
---

# Plugins reference

Generated from the manifests in `app/src-tauri/plugins/*.toml`. A plugin is active when it is
enabled (Settings | Plugins) and, if it lists project types, when the open project uses one of
them (a language, framework, pack or tool id from
[project detection](../getting-started/project-detection.md)). Disabling a plugin also disables
the plugins that depend on it.

Pack ids: `core`, `phpforge`, `webforge`, `luaforge`, and `scriptforge` / `configforge` for the
script and config language plugins.

| Plugin | Id | Pack | Depends on | Active for (project types) | Contributes |
| --- | --- | --- | --- | --- | --- |
| Browser Companion | `forge.browser-companion` | core |  | always | 1 commands; 3 settings |
| GitLab, Forgejo and GitHub | `forge.code-hosts` | core | `forge.git` | always | tool windows: Pull Requests; 2 commands; 2 settings |
| Forge Platform (required) | `forge.core` | core |  | always | tool windows: Project; 2 commands; 47 settings; 3 keybindings |
| Database Tools and SQL | `forge.database` | core |  | always | tool windows: Database; 2 commands; 3 settings |
| Git | `forge.git` | core |  | always | tool windows: Commit, Git; 2 commands; 8 settings; 1 keybindings |
| Bash | `forge.bash` | scriptforge |  | always | file types: Shell Script |
| Python | `forge.python` | scriptforge |  | always | file types: Python |
| YAML | `forge.yaml` | configforge |  | always | file types: YAML |
| JSON | `forge.json` | configforge |  | always | file types: JSON |
| .env | `forge.dotenv` | configforge |  | always | file types: Dotenv |
| Ignore Files | `forge.ignore` | configforge |  | always | file types: Ignore File |
| Docker | `forge.docker` | configforge |  | always | file types: Dockerfile, Docker Compose |
| Postfix | `forge.postfix` | configforge |  | always | file types: Postfix main.cf, Postfix master.cf |
| E-Mail | `forge.eml` | configforge |  | always | file types: E-Mail |
| INI | `forge.ini` | configforge |  | always | file types: INI |
| Config Files | `forge.conf` | configforge |  | always | file types: Config |
| Language Servers | `forge.langservers` | core | `forge.core` | always | 1 commands; 6 settings |
| Lua | `forge.lua` | luaforge |  | `luaforge` | 1 commands; 6 settings; file types: Lua; 1 LSP servers; run types: Lua Script |
| AI Assistant | `forge.ai` | core |  | always | tool windows: AI Chat; 3 AI providers; 1 chat agents; 2 code generators |
| Claude Code | `forge.claude-code` | core | `forge.ai` | always | 3 settings; 1 AI providers; 1 chat agents |
| Gemini CLI | `forge.gemini-cli` | core | `forge.ai` | always | 3 settings; 1 AI providers; 1 chat agents |
| PHP | `forge.php` | phpforge |  | `php` | 1 commands; 16 settings; 1 LSP servers; run types: PHP Script, PHP Built-in Web Server; 4 code generators |
| Composer | `forge.composer` | phpforge | `forge.php` | `composer` | 2 commands; 5 settings; run types: Composer Script |
| PHPUnit | `forge.phpunit` | phpforge | `forge.php` | `phpunit`, `pest` | run types: PHPUnit; 1 code generators |
| Xdebug | `forge.xdebug` | phpforge | `forge.php` | `php` | tool windows: Debug; 1 commands; 8 settings; run types: PHP Remote Debug |
| Laravel | `forge.laravel` | phpforge | `forge.php`, `forge.composer` | `laravel` | tool windows: Routes; 1 commands; 6 settings; run types: Artisan; 1 code generators; 1 keybindings |
| Symfony | `forge.symfony` | phpforge | `forge.php`, `forge.composer` | `symfony` | tool windows: Symfony; 2 commands; 7 settings |
| Symfony Services | `forge.symfony-services` | phpforge | `forge.yaml` | `symfony` | file types: Symfony Services |
| Doctrine | `forge.doctrine` | phpforge | `forge.php` | `doctrine` | tool windows: Doctrine; 1 commands; 6 settings |
| Run/Debug | `forge.run` | core |  | always | tool windows: Run; 3 commands; 2 settings; run types: Shell Script |
| Docker and Kubernetes | `forge.services` | core |  | always | tool windows: Services; 2 commands; 12 settings |
| Terminal | `forge.terminal` | core |  | always | tool windows: Terminal; 1 commands; 7 settings; 1 keybindings |
| HTML, CSS & JavaScript | `forge.web` | webforge |  | `webforge` | tool windows: npm; 2 commands; 4 settings; 3 LSP servers; run types: npm |
| React | `forge.react` | webforge | `forge.web` | `react`, `next` |  |
| Vue.js | `forge.vue` | webforge | `forge.web` | `vue`, `nuxt` | 1 LSP servers |
| Angular | `forge.angular` | webforge | `forge.web` | `angular` | 1 settings; 1 LSP servers |


> **Note:** some contributions are declared ahead of their implementation: the "PHP Built-in
> Web Server" and "PHP Remote Debug" run configuration types, the "Google Gemini" AI provider
> kind, the "Assistant" chat agent and the AI code generators ("Tests with AI", "Documentation
> with AI"), the PHPUnit "Test Method" and Laravel "Eloquent Model" generators, and the commands
> `composer.install`, `composer.update` and `php.runFile` have no implementation on this
> version.
