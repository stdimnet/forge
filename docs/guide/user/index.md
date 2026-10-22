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
| [Editor](editor.md) | completion, hover, Parameter Info, Go to Declaration, problems, formatting, editing actions, folding, find, gutter, context menu |
| [Markdown, images and CSV](markdown-images-csv.md) | Markdown preview (Mermaid, tables, links, images, front matter), image editor and SVG preview, Edit as Table for CSV/TSV |
| [Actions on Save](actions-on-save.md) | Reformat code, Optimize imports, Rearrange code, code cleanup, ESLint, Prettier, PHP CS Fixer and Laravel Pint on save |
| [Live templates](live-templates.md) | abbreviations + `Tab`, Insert Live Template, Surround With, Unwrap/Remove, Save as Live Template, template sets and settings |
| [Emmet and color previews](emmet-and-colors.md) | Emmet abbreviations in HTML, Blade, Twig, Vue, PHP, JSX and CSS, abbreviation preview, Surround with Emmet, Update/Remove Tag, edit points, color swatches and the color picker |
| [Spelling and grammar](spelling-and-grammar.md) | Typo and Grammar inspections in comments, strings, names, Markdown and commit messages, Alt+Enter suggestions, Rename to, project-level and application-level dictionaries, custom Hunspell dictionaries |
| [Language injections](language-injections.md) | SQL, HTML, CSS, JSON and RegExp in PHP and JavaScript strings, `// language=` comments, Inject language or reference, Edit Fragment, Run SQL |
| [File and code templates](file-templates.md) | New ▸ PHP Class and other templates, the New PHP Class dialog with PSR-4 namespaces, `Alt+Insert` New popup, Settings \| Editor \| File and Code Templates |
| [Search Everywhere and Find Action](search-everywhere.md) | Double Shift, Go to Class/File/Symbol, Find Action, fuzzy matching |
| [File types and syntax highlighting](file-types.md) | file type detection, highlighted languages, config formats, template data languages |
| [Language servers](language-servers.md) | detection, installation, per-server settings, Intelephense licence |
| [Keymap](keymap.md) | PHPStorm keymaps, changing shortcuts, contexts |
| [Shortcuts by topic](shortcuts/index.md) | every implemented shortcut explained by topic, plus a printable [reference card](shortcuts/reference-card.md) |
| [Settings and themes](settings-and-themes.md) | the Settings dialog, settings files, themes, window controls |
| [Plugins and packs](plugins-and-packs.md) | packs, activation, Settings \| Plugins |

## Languages and frameworks

| Page | Covers |
| --- | --- |
| [PHP](php.md) | PHP index, Structure, hover, Generate (`Alt+Insert`), PHP settings |
| [Laravel](laravel.md) | Blade, routes, views, config, Eloquent, Routes tool window, artisan |
| [Symfony](symfony.md) | services, routes, Twig, console, Symfony tool window |
| [Doctrine and DQL](doctrine.md) | entity mappings, Doctrine tool window, DQL in PHP strings |
| [WebForge](webforge.md) | npm scripts and package managers, React, Vue, Angular navigation |
| [LuaForge](luaforge.md) | Lua index, LÖVE, Neovim, lua-language-server, StyLua, Lua runs |

## Version control

| Page | Covers |
| --- | --- |
| [Git](git.md) | Commit tool window, AI commit messages, diff, blame, branches, log |
| [Code hosts](code-hosts.md) | GitHub, GitLab, Forgejo/Gitea pull/merge requests, issues, pipelines |

## Running, debugging and tools

| Page | Covers |
| --- | --- |
| [Run/Debug configurations](run-debug.md) | configuration types, run widget, Run tool window, test results |
| [Xdebug debugging](debugging.md) | listening, breakpoints, stepping, evaluate, servers and path mappings |
| [Terminal](terminal.md) | tabs, shells, links |
| [Database tools and SQL console](database.md) | data sources, consoles, data grid, export |
| [Scratches, consoles and HTTP client](scratches-and-http.md) | scratch files, runs, `.http` requests, environments |
| [Services](services.md) | Docker, Docker Compose, Kubernetes |
| [Remote development](remote-development.md) | SSH, the Forge agent, what runs remotely |
| [AI](ai.md) | providers, AI commit messages, Claude Code and Gemini CLI chat |
| [Browser Companion](browser-companion.md) | browser extension, `forge://` links, error page integrations |
