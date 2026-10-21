---
title: Forge documentation
description: The complete guide to Forge, the PHPStorm-style IDE built with Rust and Tauri - getting started, every feature, reference tables and contributor docs.
order: 0
tags: [start, overview]
---

# Forge documentation

Forge is a fast, memory-safe IDE in the spirit of PHPStorm for Windows, Linux and macOS, built
with Rust + Tauri 2 and a SolidJS UI. Layout, wording and the default keymap follow PHPStorm 1:1;
the look is Forge's own ("Ember" dark, "Anvil" light).

![Forge workbench](../screenshots/workbench-dark.png)

Languages and large feature sets ship as **packs**:

| Pack | Focus |
| --- | --- |
| **PHPForge** | PHP, Composer, PHPUnit, Xdebug, Laravel, Symfony, Doctrine (DQL) |
| **WebForge** | HTML, CSS, JavaScript/TypeScript, npm/pnpm/yarn/bun, React, Vue, Angular |
| **LuaForge** | Lua 5.1–5.4, LuaJIT, LÖVE, Neovim |

The core adds Git and code review (GitHub, GitLab, Forgejo/Gitea), Run/Debug configurations, a
terminal, database tools, Scratches with an HTTP client, Docker/Compose/Kubernetes, remote
development over SSH, language servers for about 30 more languages, AI (your own providers,
Claude Code and Gemini CLI as a native chat) and a browser companion.

## Where to start

| Section | For |
| --- | --- |
| [Getting started](getting-started/index.md) | installing Forge, the first start, opening and creating projects, switching from PHPStorm |
| [User guide](user/index.md) | one page per feature |
| [Reference](reference/index.md) | keymap, settings, file types, language servers, CLI, file locations, environment variables, plugins |
| [Developer guide](developer/index.md) | architecture, extending Forge, quality gates, releases, one page per crate |
| [FAQ](faq.md) | short answers |
| [Troubleshooting](troubleshooting.md) | AppImage/FUSE, WebKitGTK, language servers, remote agent, keychain, Wayland |

## Popular pages

- [Keymap reference](reference/keymap.md) — every shortcut on Windows/Linux and macOS
- [Language servers](user/language-servers.md) — install Intelephense and friends, licence keys
- [Laravel](user/laravel.md) and [Symfony](user/symfony.md)
- [Xdebug debugging](user/debugging.md)
- [Remote development](user/remote-development.md)
- [AI](user/ai.md)

## Recently added

- **Split editors, pinned tabs and the tab context menu**, a tab limit, **Recent Locations**
  (`Ctrl+Shift+E`), **Bookmarks** (`F11`, `Ctrl+F11`, `Alt+2`), the **TODO** and
  **Notifications** tool windows and Find in Files results in the **Find** tool window
  ([Workbench](user/workbench.md#editor-tabs)).
- **PHP navigation like PHPStorm**: `Ctrl+Click` on every PHP symbol (also `\Throwable` and
  `array_map`, opened read-only in the PHP stubs), Show Usages on declarations, Find Usages
  (`Alt+F7`) in the Find tool window, Implementation / Super Method / Type Declaration and the
  implemented / overridden gutter icons ([PHP](user/php.md#navigation)).
- **Splash screen and updates**: the wordmark fills with molten metal while Forge loads, the
  last project reopens, and signed updates are offered on startup or with Help | Check for
  Updates… ([First start](getting-started/first-start.md#the-splash-screen)).
- **Main menu**: PHPStorm's menus behind ☰, merged into the toolbar or as a menu bar
  ([Workbench](user/workbench.md#main-menu)).
- **Find in Files** (`Ctrl+Shift+F`) and the **Choose Declaration** popup
  ([Search Everywhere](user/search-everywhere.md#find-in-files), [Editor](user/editor.md#go-to-declaration)).
- **One-click language server installs**, including Intelephense with its own Node.js
  ([Language servers](user/language-servers.md#installing-a-server)).
- **HTTP client**: environment picker, response handlers, body files, dynamic variables
  ([Scratches and HTTP client](user/scratches-and-http.md#http-client-http-files)).
- **Services**: Kubernetes port forwarding, container picker, deployment logs, Docker over SSH
  and Docker contexts ([Services](user/services.md)).
- **Remote parity**: frameworks, language servers, databases, Xdebug and AI agents in remote
  projects, `ProxyJump`/`ProxyCommand` ([Remote development](user/remote-development.md)).
- **Dialog windows and OS-styled window controls**
  ([Settings and themes](user/settings-and-themes.md#window-controls)).

## Not available yet

Refactorings other than Rename (Extract, Move, ...), Local History, Get from VCS, multiple
project windows and a plugin Marketplace. The pages mark smaller gaps with notes; the
[roadmap](../ROADMAP.md) lists what comes next.

## About this guide

The guide is plain Markdown with a small frontmatter contract so any documentation generator can
render it; see [About this guide](README.md). Every user-visible change to Forge updates the
matching page.
