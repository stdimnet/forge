---
title: "Tools menu"
description: "Every entry of the Tools menu with its shortcut, availability and what it does."
order: 80
tags: [main menu, reference, tools]
---

# Tools menu

Terminal, Vim emulation, Run Command… and the tools of plugins (Composer, Docker, Laravel, Symfony). Open it with ☰ (`Alt+\`) or from the menu bar, see [Main menu](../../user/workbench.md#main-menu).

![Tools menu](../../../screenshots/docs-menu-tools.png)

**Available**: *yes* — implemented; *not yet* — shown disabled until Forge implements it; *dynamic* — filled at runtime. Shortcuts are the Windows/Linux and macOS defaults.

| Entry | Windows / Linux | macOS | Available | What it does |
| --- | --- | --- | --- | --- |
| Run Anything… | `Double Ctrl` | `Double Ctrl` | yes | Runs or debugs configurations, scripts, PHP files and commands from one popup. [More](../../user/run-debug.md#run-anything) ([screenshot](../../../screenshots/run-anything.png)) |
| Run Command… | `Ctrl+Shift+X` | `Cmd+Shift+X` | yes | Opens the Command Line Tools Console popup: run Composer, Artisan, Symfony Console, WP-CLI, Drush and custom tools with completion (plugin `forge.command-line-tools`, PHP projects). [More](../../user/command-line-tools.md#run-command) ([screenshot](../../../screenshots/docs-run-command.png)) |
| WP-CLI… | — | — | yes | Opens the Command Line Tools Console with `wp ` typed in (plugin `forge.wordpress`, WordPress projects). [More](../../user/wordpress.md#wp-cli) |
| Drush…, Shopware Console… | — | — | yes | Open the Command Line Tools Console with `drush ` or the Shopware `bin/console` alias typed in (plugin `forge.cms`). [More](../../user/cms.md#drush-and-the-shopware-console) |
| WordPress › Run WP-CLI Command…, Show Hooks, Install WordPress Stubs…, Refresh Hook Index | — | — | yes | The WordPress submenu (plugin `forge.wordpress`). [More](../../user/wordpress.md) |
| Save as Live Template… | — | — | yes | Opens Settings \| Editor \| Live Templates with a new template made from the selection. [More](../../user/live-templates.md#save-as-live-template) ([screenshot](../../../screenshots/save-as-live-template.png)) |
| Save File as Template… | — | — | not yet | Not available yet; the entry is disabled. |
| Open in Terminal | `Ctrl+Shift+T` | `Cmd+T` | yes | Opens a new terminal tab (Tools | Open in Terminal). [More](../../user/terminal.md#tabs) ([screenshot](../../../screenshots/docs-terminal.png)) |
| IDE Scripting Console | — | — | not yet | Not available yet; the entry is disabled. |
| **External Tools** | | | dynamic | submenu: the enabled [External Tools](../../user/run-debug.md#external-tools) (Settings \| Tools \| External Tools) ([screenshot](../../../screenshots/external-tools-menu.png)) |
| Vim | `Ctrl+Alt+V` | `Cmd+Alt+V` | yes | Turns Vim emulation on or off (check mark; plugin `forge.vim`). [More](../../user/vim-emulation.md) ([screenshot](../../../screenshots/vim-emulation.png)) |
| Render All Doc Comments | — | — | yes | Shows every documentation comment of the file as formatted text, or the text again. [More](../../user/code-structure.md#rendered-documentation-comments) ([screenshot](../../../screenshots/rendered-doc-comments.png)) |
| **Composer** | | | | submenu |
| Composer › Add Dependency… | — | — | yes | Opens Manage Composer Dependencies: search Packagist, choose a version, install, update or remove packages. [More](../../user/composer.md#manage-composer-dependencies) ([screenshot](../../../screenshots/composer-dependencies.png)) |
| Composer › Install | — | — | yes | Runs `composer install` in the Run tool window. [More](../../user/composer.md#the-composerjson-toolbar) |
| Composer › Update | — | — | yes | Runs `composer update`. [More](../../user/composer.md#the-composerjson-toolbar) |
| Composer › Validate | — | — | yes | Runs `composer validate`. [More](../../user/composer.md#the-composerjson-toolbar) |
| Composer › Show Outdated | — | — | yes | Runs `composer outdated --direct`. [More](../../user/composer.md#outdated-packages) |
| Composer › Audit | — | — | yes | Runs `composer audit` and shows the advisories in the Problems tool window. [More](../../user/composer.md#composer-audit) ([screenshot](../../../screenshots/composer-audit.png)) |
| **Docker** | | | | submenu |
| Docker › Pull Image… | — | — | yes | Pulls a Docker image. [More](../../user/services.md#pulling-images-and-registries) |
| Docker › Refresh Services | — | — | yes | Refreshes the Services tree. [More](../../user/services.md) |
| **Laravel** | | | | submenu |
| Laravel › Run Artisan Command… | — | — | yes | Opens Run Command with the Artisan alias typed in. [More](../../user/laravel.md#run-artisan-command) |
| Laravel › Show Routes | — | — | yes | Shows the Laravel Routes tool window. [More](../../user/laravel.md#routes-tool-window) |

## Entries of plugins

Tools collects the actions of active plugins; the screenshot shows a Laravel project.

| Submenu | Entries | Appears in |
| --- | --- | --- |
| — | Vim (check mark) | always (plugin `forge.vim`) |
| Composer | Add Dependency…, Install, Update, Validate, Show Outdated, Audit | Composer projects (`forge.composer`) |
| Docker | Pull Image…, Refresh Services | always (plugin `forge.services`) |
| Laravel | Run Artisan Command…, Show Routes | Laravel projects (`forge.laravel`) |
| Symfony | Run Console Command…, Show Routes, Refresh Symfony Index | Symfony projects (`forge.symfony`) |
