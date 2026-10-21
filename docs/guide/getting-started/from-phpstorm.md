---
title: Coming from PHPStorm
description: A quick switch guide for PHPStorm users - what works the same, what is different and what Forge does not have yet.
order: 50
tags: [phpstorm, migration, keymap]
---

# Coming from PHPStorm

Forge copies PHPStorm's New UI on purpose: the layout, the names of tool windows and actions,
the Settings tree and the default keymap are the same. The colors are Forge's own ("Ember"
dark and "Anvil" light). This page lists what you can rely on, what works differently, and
what is not there yet.

## Same as in PHPStorm

- **Keymap.** Forge ships PHPStorm's default keymaps for Windows/Linux and macOS and uses
  PHPStorm's action ids (`GotoClass`, `ReformatCode`, `CheckinProject`, …). `Double Shift`,
  `Ctrl+Shift+A`, `Ctrl+N`, `Ctrl+Shift+N`, `Ctrl+E`, `Ctrl+B`, `Alt+Insert`, `Ctrl+K`,
  `Shift+F10`, `Alt+1` … `Alt+0` and `Alt+F12` do what you expect. See the
  [keymap reference](../reference/keymap.md).
- **Workbench.** Main menu (☰, merged or as a menu bar), main toolbar with project and branch
  widgets, run widget and search; tool
  window stripes on the left, right and bottom; editor tabs with breadcrumbs; status bar.
- **Tool windows** with the same names and shortcuts: Project (`Alt+1`), Run (`Alt+4`), Debug
  (`Alt+5`), Problems (`Alt+6`), Structure (`Alt+7`), Services (`Alt+8`), Git (`Alt+9`), Commit
  (`Alt+0`), Terminal (`Alt+F12`), Database, Pull Requests, AI Chat.
- **Settings tree.** Appearance & Behavior, Keymap, Editor, Plugins, Version Control,
  Languages & Frameworks, Tools, Build, Execution, Deployment. `Ctrl+Alt+S` opens it.
- **Run/Debug configurations** with the same types for PHP (PHP Script, PHP Built-in Web
  Server, PHPUnit, PHP Remote Debug, Composer, Artisan) and "Store as project file".
- **Xdebug** on port 9003 with breakpoints, stepping (`F7`, `F8`, `Shift+F8`, `F9`),
  Evaluate Expression (`Alt+F8`), servers with path mappings and the "Incoming Connection From
  Xdebug" dialog.
- **Scratches and Consoles** (`Ctrl+Alt+Shift+Insert`), including `.http` request files.
- **Database tools** for MySQL/MariaDB, PostgreSQL and SQLite with query consoles and a data
  grid.
- **Laravel and Symfony support** in the spirit of Laravel Idea and the Symfony plugin: route,
  view, config, service and template completion and navigation, gutter icons, artisan and
  console runners.

## Different from PHPStorm

| Topic | PHPStorm | Forge |
| --- | --- | --- |
| PHP intelligence | built-in PHP engine | an external language server: Intelephense (free, or premium with your own licence key) or Phpactor, plus Forge's own PHP index for structure, search, hover and code generation. See [Language servers](../user/language-servers.md). |
| Other languages | plugins | language servers from a catalog of about 30, installed from Settings, see [Language servers](../user/language-servers.md) |
| Projects | `.idea/` folder | `.forge/` folder with TOML/JSON files; there is no project model to import |
| Project tree | single click selects | single click **opens** files by default; switch off "Open files with single click" (Settings | Appearance & Behavior | Appearance) for PHPStorm's behaviour |
| AI | AI Assistant subscription | bring your own provider (Anthropic, OpenAI-compatible, OpenRouter, Ollama, LM Studio, llama.cpp) or your installed Claude Code / Gemini CLI |
| Plugins | Marketplace | built-in plugins grouped in packs (PHPForge, WebForge, LuaForge); no Marketplace yet |
| Remote development | Gateway | built in: SSH connection, Forge uploads a small agent to the host |
| Theme | Darcula/Light/New UI themes | Forge Ember (dark), Forge Anvil (light), Solarized Dark and Light |

## Not there yet

The following PHPStorm features are not implemented on this version. Their shortcuts stay
bound and show "*Action* is not available yet" so you are not surprised by something else
happening.

- Get from VCS
- Replace in Files (`Ctrl+Shift+R`), Find Usages in File (`Ctrl+F7`), Highlight Usages in File
- Refactorings: Rename (`Shift+F6`), Extract/Introduce, Move, Change Signature, Safe Delete
- Show Context Actions / quick-fixes (`Alt+Enter`), Quick Documentation popup (`Ctrl+Q`; hover
  documentation works with the mouse), File Structure popup (`Ctrl+F12`)
- Local History
- Live templates, Surround With, Optimize Imports, Type/Call/Method Hierarchy
- Go to Test (`Ctrl+Shift+T`)
- Distraction Free / Presentation / Zen modes
- Get from VCS, stash/unstash, Commit and Push
- Marketplace, settings export/import, automatic updates

The [keymap reference](../reference/keymap.md) marks every action as implemented or not.

## Moving a project over

1. Open the project folder in Forge. The `.idea/` folder is ignored; nothing is converted.
2. Install the language servers you need from Settings | Languages & Frameworks |
   Language Servers. For PHP, accept the offer "Intelephense is not installed" (or click **Install
   Intelephense**) and, if you own a licence, enter the key on the same page.
3. Set the PHP interpreter and language level in Settings | Languages & Frameworks | PHP.
4. Recreate run configurations in Run | Edit Configurations (the run widget in the toolbar).
   PHPStorm's `.run/` XML files are not imported.
5. Recreate data sources in the Database tool window. Laravel's `DB_*` and Symfony's
   `DATABASE_URL` in `.env` are offered automatically.
6. If you changed shortcuts in PHPStorm, re-assign them in Settings | Keymap. Keymap files
   cannot be imported yet.
