---
title: Project type detection
description: How Forge recognizes languages, frameworks and tools in a project and which plugins it activates for them.
order: 40
tags: [projects, detection, plugins, packs]
---

# Project type detection

When a project opens, Forge reads marker files in the project root **and one folder level
below it** (so a monorepo with `backend/` and `frontend/` is understood) and decides which
languages, frameworks and tools the project uses. Plugins that belong to a framework only
activate when that framework is found, so a Laravel project does not show Symfony tool windows
and a plain Lua project does not start PHP language servers.

## What is detected

### Languages

| Language | Detected from |
| --- | --- |
| PHP | `composer.json`, or any `.php` file |
| TypeScript | `tsconfig.json`, `.ts` or `.tsx` files |
| JavaScript | `.js`, `.mjs` or `.jsx` files |
| HTML | `.html` or `.htm` files |
| CSS | `.css` or `.scss` files |
| Lua | a `.lua` file in the root, two or more `.lua` files in a folder, `.luarc.json`, `.luarc.jsonc`, `.luacheckrc`, `stylua.toml`, `.stylua.toml`, `selene.toml` or a `*.rockspec` |
| Rust | `Cargo.toml` |
| Go | `go.mod` |
| Python | `pyproject.toml`, `requirements.txt` or `setup.py` |

### Frameworks

| Framework | Detected from |
| --- | --- |
| Laravel | `laravel/framework` in `composer.json`, or an `artisan` file |
| Symfony | `symfony/framework-bundle`, or `bin/console` together with `config/bundles.php` |
| Doctrine | `doctrine/orm` |
| Drupal | `drupal/core` or `drupal/core-recommended` |
| Magento | `magento/framework`, `magento/product-community-edition`, or `app/etc/env.php` with `bin/magento` |
| WordPress | `johnpbloch/wordpress`, `roots/wordpress`, `wp-config.php`, `wp-config-sample.php` or `wp-includes/` |
| Livewire | `livewire/livewire` |
| PHPUnit, Pest | `phpunit/phpunit`, `pestphp/pest` |
| Next.js, Nuxt, React, Vue.js, Svelte, Solid, Vite | `next`, `nuxt`, `react`, `vue`, `svelte`, `solid-js`, `vite` in `package.json`; Vite also from a `vite.config.*` file |
| Angular | `@angular/core` in `package.json`, or `angular.json` |
| LÖVE | `conf.lua`, or a `main.lua` that uses `love.` |
| Neovim | `init.lua` next to a `lua/` folder (or one that uses `vim.`), or `lua/` together with `plugin/` |

Versions come from `composer.lock` / the npm lock files when present, otherwise from the
version constraints in the manifests. The PHP version comes from `require.php`.

### Tools

`composer`, `npm`, `pnpm`, `yarn`, `bun` (from lock files and `package.json`), `luarocks`
(`*.rockspec`), `docker` (`Dockerfile`, `compose.yaml`/`compose.yml`,
`docker-compose.yml`/`.yaml`) and `git` (a `.git` folder).

Folders that are never scanned as sub-projects: `vendor`, `node_modules`, `target`, `dist`,
`build`, `storage`, `var`, `public` and hidden folders. At most 64 sub-folders are checked.

## Primary type

The most specific framework becomes the project's **primary** type, in this order: Laravel,
Symfony, Drupal, WordPress, Magento, Next.js, Nuxt, Angular, Svelte, Vue.js, React, Solid, LÖVE,
Neovim, Vite. Without a framework the primary language is used (PHP, TypeScript, JavaScript,
Lua, Rust, Go, Python, HTML, CSS).

The primary type appears

- as the badge next to the project on the Welcome screen,
- in the status bar (for example "Laravel 12"); click it to open Settings | Plugins. Laravel
  and Symfony replace this widget with their own (see [Laravel](../user/laravel.md) and
  [Symfony](../user/symfony.md)),
- in a one-time balloon "Laravel project detected — Laravel support enabled" with a
  **Configure** link to the framework's settings page.

![Detected project balloon](../../screenshots/project-detected-balloon.png)

## Packs and plugins that activate

| Pack | Activates for |
| --- | --- |
| PHPForge | PHP projects; Laravel, Symfony, Doctrine, Composer, PHPUnit/Pest and Xdebug plugins for their framework or tool |
| WebForge | JavaScript, TypeScript, HTML or CSS; React for `react`/`next`, Vue.js for `vue`/`nuxt`, Angular for `angular` |
| LuaForge | Lua projects |

Core plugins (Git, Terminal, Run/Debug, Database, Services, AI, language servers, …) are
always active. Each plugin can still be switched off in Settings | Plugins, see
[Plugins and packs](../user/plugins-and-packs.md). The exact rules are the `projectTypes`
of each plugin, listed in [Reference: plugins](../reference/plugins.md).

## Re-detect

Detection runs when the project opens. If you add a framework later (for example
`composer require symfony/framework-bundle`), run **Detect Project Type** with Find Action
(`Ctrl+Shift+A`), or reopen the project, so the new plugins activate.
