---
title: Crates
description: One page per Rust crate in crates/ and packs/ - purpose, modules, public items, IPC, dependencies and tests.
order: 200
tags: [crates, rust]
---

# Crates

Every Rust crate of the workspace. The pages are generated from each crate's `lib.rs` documentation, module headers and `Cargo.toml`, plus the IPC commands in `app/src-tauri/src/commands/` that use it. The desktop shell itself is the crate `forge-app` in `app/src-tauri` (see [Architecture overview](../architecture.md)).

## Core crates (`crates/`)

| Crate | Description |
| --- | --- |
| [forge-agent-cli](forge-agent-cli.md) | Runs AI coding agent CLIs (Claude Code, Gemini CLI) headless and normalizes their output |
| [forge-ai](forge-ai.md) | Forge IDE AI provider layer: Anthropic and OpenAI-compatible chat models |
| [forge-backend](forge-backend.md) | Forge IDE project backends: the Backend trait, the local backend and the remote agent protocol |
| [forge-bookmarks](forge-bookmarks.md) | Bookmarks: lists, mnemonics and line tracking through edits (pure, WebAssembly-friendly) |
| [forge-codeintel](forge-codeintel.md) | Code intelligence models: workspace edits with LSP positions and Find Usages grouping (pure, WebAssembly-friendly) |
| [forge-codestyle](forge-codestyle.md) | Code style schemes (Settings \| Editor \| Code Style): options per language, presets, scheme storage and resolution, import/export and the formatting passes and previews (pure, WebAssembly-friendly) |
| [forge-colors](forge-colors.md) | Color values in code: finding CSS colors (hex, rgb, hsl, hwb, named, Tailwind) in CSS, markup and script files, parsing and formatting them in their notation (pure, WebAssembly-friendly) |
| [forge-containers](forge-containers.md) | Forge IDE containers: Docker Engine API, Docker Compose and Kubernetes for the Services tool window |
| [forge-core](forge-core.md) | Forge IDE core: workspace, file system, settings and event bus |
| [forge-diff](forge-diff.md) | Forge IDE comparisons: folder diff (Compare Directories), pure and WebAssembly-friendly |
| [forge-db](forge-db.md) | Forge IDE database tools: data sources, introspection, query execution and data editing |
| [forge-debug](forge-debug.md) | Forge IDE debugger: DBGp (Xdebug 3) server, path mappings and breakpoints |
| [forge-editing](forge-editing.md) | Editor actions computed in Rust: postfix completion templates, Complete Current Statement and Cyclic Expand Word (pure, WebAssembly-friendly) |
| [forge-emmet](forge-emmet.md) | Emmet: abbreviation parser and expander for HTML, JSX and CSS, Surround with Emmet, edit points, Update/Remove Tag (pure, WebAssembly-friendly) |
| [forge-file-templates](forge-file-templates.md) | File and code templates (Settings \| Editor \| File and Code Templates): a Velocity subset renderer, the built-in templates, PSR-4 namespace mapping and New-file rendering (pure, WebAssembly-friendly) |
| [forge-filetypes](forge-filetypes.md) | File type recognition for Forge: names, extensions, shebangs and template data languages (pure, WebAssembly-friendly) |
| [forge-find](forge-find.md) | Find in Files and TODO scanning: query compilation, file masks, line matching and comment patterns (pure, WebAssembly-friendly) |
| [forge-forges](forge-forges.md) | Forge IDE code hosts: GitLab, Forgejo/Gitea and GitHub merge requests, issues and pipelines |
| [forge-fuzzy](forge-fuzzy.md) | PHPStorm-style fuzzy matching and Search Everywhere ranking (pure, WebAssembly-friendly) |
| [forge-injections](forge-injections.md) | Language injections: SQL, HTML, CSS, JSON and RegExp inside PHP and JavaScript/TypeScript strings, `// language=` comments, fragment escaping and the injection intentions (pure, WebAssembly-friendly) |
| [forge-jsonschema](forge-jsonschema.md) | JSON Schema for JSON, JSON5/JSONC and YAML files: position-keeping parsers, a draft 4–2020-12 validator with PHPStorm's messages, completion, hover, Go to Declaration, schema mappings, bundled schemas and remote schema downloads |
| [forge-keymap](forge-keymap.md) | PHPStorm-compatible keymap: shortcut model, default keymaps, overrides, conflicts and the key resolver |
| [forge-langservers](forge-langservers.md) | Forge IDE: catalog, detection, configuration and installation of language servers |
| [forge-livetemplates](forge-livetemplates.md) | Live templates: template sets, contexts, variables and expansion, Surround With and Unwrap/Remove (pure, WebAssembly-friendly) |
| [forge-lsp](forge-lsp.md) | Forge IDE: generic Language Server Protocol client and server manager |
| [forge-markdown](forge-markdown.md) | Forge Markdown: a safe AST (no raw HTML) for chat messages and documentation popups, built with pulldown-cmark (pure, also built to WebAssembly) |
| [forge-viewers](forge-viewers.md) | Forge file viewers: image metadata for the image editor and CSV/TSV tables for Edit as Table (pure, also built to WebAssembly) |
| [forge-open](forge-open.md) | Forge IDE open requests: forge:// links, the CLI launcher and the Browser Companion local server |
| [forge-output](forge-output.md) | Forge process output: ANSI styling, console lines, file links and TeamCity test trees (pure, also built to WebAssembly) |
| [forge-plugin-api](forge-plugin-api.md) | Stable plugin API for Forge language packs and feature plugins |
| [forge-process](forge-process.md) | Forge IDE processes: PTY terminal sessions, run configurations and the process runner |
| [forge-project](forge-project.md) | Forge IDE projects: project type detection and New Project generators |
| [forge-remote](forge-remote.md) | Forge IDE remote development: SSH connections, host keys, ~/.ssh/config and the Forge agent deployment |
| [forge-remote-agent](forge-remote-agent.md) | Headless Forge agent: serves a project's files, git, processes, language servers and PHP index over stdio for remote development |
| [forge-runtools](forge-runtools.md) | Run Anything, compound and before-launch plans, External Tools and their macros (pure, also built to WebAssembly) |
| [forge-save-actions](forge-save-actions.md) | Actions on Save: which actions run for a saved file, reformatting only changed lines and running ESLint, Prettier, PHP CS Fixer and Laravel Pint through the project's backend |
| [forge-scratches](forge-scratches.md) | Forge scratch files (PHPStorm's Scratches and Consoles): storage, naming, languages, run commands and the HTTP request client |
| [forge-scopes](forge-scopes.md) | Scopes (PHPStorm's scope language, predefined and named scopes) and file colors (pure, WebAssembly-friendly) |
| [forge-spell](forge-spell.md) | Spelling and grammar inspections for comments, strings, identifiers and text: Hunspell dictionaries, word extraction, suggestions and a small rule-based English grammar checker (pure, WebAssembly-friendly) |
| [forge-ssr](forge-ssr.md) | Structural Search and Replace: templates with `$variables$` matched on tree-sitter syntax trees (PHP, JavaScript, TypeScript) |
| [forge-sql](forge-sql.md) | Forge SQL text: dialects, statement splitting, literals, row changes and export (pure, also built to WebAssembly) |
| [forge-update](forge-update.md) | Forge update checks: when to check, which release is an update, the update feed, simulated updates and download speed |
| [forge-vcs](forge-vcs.md) | Forge IDE version control: drives the git CLI and parses its output |
| [forge-vim](forge-vim.md) | Vim emulation (IdeaVim-style): modes, motions, operators, text objects, registers, ex commands and ~/.ideavimrc parsing (pure, built to WebAssembly) |
| [forge-wasm](forge-wasm.md) | WebAssembly build of Forge's pure Rust logic for the browser-mode UI |
| [forge-window](forge-window.md) | Forge window chrome: desktop detection, title bar button layouts and dialog window geometry (pure, also built to WebAssembly) |

## Pack crates (`packs/`)

| Crate | Description |
| --- | --- |
| [luaforge-lang-lua](luaforge-lang-lua.md) | LuaForge: Lua language support (lexer, structural parser, EmmyLua/LuaLS annotations, project index, require resolution, hover, navigation; pure, also built to WebAssembly) |
| [phpforge-cli-tools](phpforge-cli-tools.md) | PHPForge: Command Line Tool Support (tool model and detection, `list --format=json` and `wp cli cmd-dump` parsing, console input parsing and completion; pure, also built to WebAssembly) |
| [phpforge-test-frameworks](phpforge-test-frameworks.md) | PHPForge: PHP test frameworks (PHPUnit, Pest, Codeception, Behat, PHPSpec): settings and detection, run configuration command lines, test discovery for gutter icons, Gherkin and Behat step definitions, Rerun Failed Tests (pure, also built to WebAssembly) |
| [phpforge-composer](phpforge-composer.md) | PHPForge: Composer support (version constraints, composer.json/lock analysis, Packagist metadata, outdated packages, audit advisories, completion) |
| [phpforge-dql](phpforge-dql.md) | PHPForge: Doctrine DQL tokenizer, parser, highlighting, completion and diagnostics, and DQL sites in PHP source (pure, also built to WebAssembly) |
| [phpforge-fw-laravel](phpforge-fw-laravel.md) | PHPForge: Laravel support (routes, views, Blade components, config/env, Eloquent, translations, artisan) |
| [phpforge-fw-symfony](phpforge-fw-symfony.md) | PHPForge: Symfony support (service container, routes, Twig templates, console commands) |
| [phpforge-lang-php](phpforge-lang-php.md) | PHPForge: PHP language support (tree-sitter index, docblocks, hover, code generation, language server config) |
| [phpforge-orm-doctrine](phpforge-orm-doctrine.md) | PHPForge: Doctrine ORM support (entity mapping, repositories, DQL parsing and completion) |
| [webforge-fw-angular](webforge-fw-angular.md) | WebForge: Angular support (components, directives, pipes, templates and styles) |
| [webforge-fw-react](webforge-fw-react.md) | WebForge: React support (JSX components, props and usages) |
| [webforge-fw-vue](webforge-fw-vue.md) | WebForge: Vue.js support (single-file components, props, emits and usages) |
| [webforge-lang-web](webforge-lang-web.md) | WebForge: package.json, npm scripts, package managers and the shared JavaScript/TypeScript/HTML scanning used by the framework plugins |
