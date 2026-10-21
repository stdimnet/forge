---
title: FAQ
description: Frequently asked questions about Forge - licensing, PHPStorm compatibility, languages, AI, privacy, platforms and the project's status.
order: 900
tags: [faq]
---

# FAQ

## General

**What is Forge?**
A PHPStorm-style IDE for Windows, Linux and macOS, written in Rust (Tauri 2) with a SolidJS UI.
The layout, wording and keymap follow PHPStorm; the colors are Forge's own.

**Is it free?**
Forge is MIT-licensed. Third-party tools you connect keep their own terms: Intelephense premium
needs your own licence, AI providers need your own API key or subscription.

**Which version is this guide for?**
Forge 0.1.0 as on the branch that contains this guide. Recent additions and what is not available
yet are listed on the [start page](index.md#recently-added).

**Which platforms are supported?**
Windows 10/11, Linux (x86_64; WebKitGTK 4.1) and macOS. Remote development needs a Linux host
(x86_64 or aarch64).

## Compared with PHPStorm

**Can I use my PHPStorm shortcuts?**
Yes: the default keymaps are PHPStorm's (Windows/Linux and macOS). Custom PHPStorm keymaps cannot
be imported yet; re-create changes in Settings | Keymap.

**Does Forge read my `.idea` folder or `.run` configurations?**
No. Forge keeps its own data in `.forge/` and ignores `.idea/`.

**Is PHP support as deep as PHPStorm's?**
Not yet. Completion and diagnostics come from Intelephense (or Phpactor); Forge adds its own
index with navigation (Go to Declaration including PHP's own stubs, Find/Show Usages,
implementations, gutter icons) that also works without a server, Generate actions, Laravel,
Symfony and Doctrine support. Refactorings, inspections with quick-fixes and Replace in Files
are not implemented yet. See
[Coming from PHPStorm](getting-started/from-phpstorm.md).

**Why does a shortcut say "not available yet"?**
The action exists in PHPStorm's keymap but not in Forge yet. Shortcuts stay reserved so they do
not do something unexpected; the [keymap reference](reference/keymap.md) shows the status of every
action.

## Languages

**Which languages are supported?**
About 70 languages are highlighted. PHP, Laravel, Symfony, Doctrine, JavaScript/TypeScript, React,
Vue, Angular and Lua have dedicated packs. About 30 more languages (Python, Go, Rust, C/C++, Java,
Kotlin, C#, Ruby, Zig, YAML, TOML, XML, Markdown, Docker, Bash, SQL, LaTeX, Twig, …) get
completion and diagnostics through [language servers](user/language-servers.md).

**Does Forge include language servers?**
No. They are separate programs you install, from the Language Servers settings page with one
click or with your package manager. Forge never bundles paid tools or keys.

**Do I need Node.js?**
Only for npm scripts and web project generators. npm-based language servers (Intelephense, TypeScript, Vue, …) do not need it: Forge
downloads an official Node.js build for them when none (18 or newer) is installed.

## AI

**Does Forge send my code anywhere?**
Only to the services you configure: the AI provider for commit messages (the diff of the files you
selected) and the agent CLI you chat with. See [AI: Privacy](user/ai.md#privacy).

**Can I use a local model?**
Yes: Ollama, LM Studio, llama.cpp or any OpenAI-compatible server, for AI commit messages. The
AI Chat runs Claude Code or Gemini CLI.

**Do I need a Claude subscription for the AI Chat?**
You need a working `claude` (Claude Code) or `gemini` (Gemini CLI) installation with your own
login. Forge only runs the CLI.

## Projects

**Where are my settings stored?**
User settings in `<config dir>/forge/settings.toml`, project settings in `.forge/settings.toml`;
secrets in the OS keychain. See [File locations](reference/file-locations.md).

**Should I commit `.forge/`?**
Commit `settings.toml`, `runConfigurations.toml`, `php-servers.json` and `dataSources.toml` if your
team should share them; ignore `runConfigurations.local.toml` and `cache/`.

**Can I open several projects at once?**
One project per window. Opening another project replaces the current one; multiple windows are not
available yet.

**Can I work on a server over SSH?**
Yes, see [Remote development](user/remote-development.md). Frameworks, language servers,
databases (through SSH tunnels), Xdebug and the AI agents work there; a few features are still local-only.

## Contributing

**How can I help?**
Read the [developer guide](developer/index.md), [`CONTRIBUTING.md`](../../CONTRIBUTING.md) and
[`AGENTS.md`](../../AGENTS.md). Every user-visible change should update this guide.

**Can I write my own plugin?**
Plugins are built into Forge for now (there is no Marketplace or external plugin loading). Add one
to the repository: [Writing a plugin or pack](developer/plugins-and-packs.md).
