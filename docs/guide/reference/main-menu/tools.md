---
title: "Tools menu"
description: "Every entry of the Tools menu with its shortcut, availability and what it does."
order: 80
tags: [main menu, reference, tools]
---

# Tools menu

Terminal and the tools of plugins (Docker, Laravel, Symfony). Open it with ☰ (`Alt+\`) or from the menu bar, see [Main menu](../../user/workbench.md#main-menu).

![Tools menu](../../../screenshots/docs-menu-tools.png)

**Available**: *yes* — implemented; *not yet* — shown disabled until Forge implements it; *dynamic* — filled at runtime. Shortcuts are the Windows/Linux and macOS defaults.

| Entry | Windows / Linux | macOS | Available | What it does |
| --- | --- | --- | --- | --- |
| Run Anything… | — | — | not yet | Not available yet; the entry is disabled. |
| Save as Live Template… | — | — | yes | Opens Settings \| Editor \| Live Templates with a new template made from the selection. [More](../../user/live-templates.md#save-as-live-template) ([screenshot](../../../screenshots/save-as-live-template.png)) |
| Save File as Template… | — | — | not yet | Not available yet; the entry is disabled. |
| Open in Terminal | `Ctrl+Shift+T` | `Cmd+T` | yes | Opens a new terminal tab (Tools | Open in Terminal). [More](../../user/terminal.md#tabs) ([screenshot](../../../screenshots/docs-terminal.png)) |
| IDE Scripting Console | — | — | not yet | Not available yet; the entry is disabled. |
| Render All Doc Comments | — | — | yes | Shows every documentation comment of the file as formatted text, or the text again. [More](../../user/code-structure.md#rendered-documentation-comments) ([screenshot](../../../screenshots/rendered-doc-comments.png)) |
| **Docker** | | | | submenu |
| Docker › Pull Image… | — | — | yes | Pulls a Docker image. [More](../../user/services.md#pulling-images-and-registries) |
| Docker › Refresh Services | — | — | yes | Refreshes the Services tree. [More](../../user/services.md) |
| **Laravel** | | | | submenu |
| Laravel › Run Artisan Command… | `Ctrl+Shift+X` | `Cmd+Shift+X` | yes | Runs an artisan command of the Laravel project. [More](../../user/laravel.md#run-artisan-command) ([screenshot](../../../screenshots/docs-artisan.png)) |
| Laravel › Show Routes | — | — | yes | Shows the Laravel Routes tool window. [More](../../user/laravel.md#routes-tool-window) |

## Entries of plugins

Tools collects the actions of active plugins; the screenshot shows a Laravel project.

| Submenu | Entries | Appears in |
| --- | --- | --- |
| Docker | Pull Image…, Refresh Services | always (plugin `forge.services`) |
| Laravel | Run Artisan Command… (`Ctrl+Shift+X`), Show Routes | Laravel projects (`forge.laravel`) |
| Symfony | Run Console Command…, Show Routes, Refresh Symfony Index | Symfony projects (`forge.symfony`) |
