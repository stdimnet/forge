---
title: Developer guide
description: Contributor documentation - architecture, repository layout, development setup, extension points, plugins and packs, quality gates, releases and one page per crate.
order: 0
tags: [developer, contributing]
---

# Developer guide

Forge is a Rust + Tauri 2 desktop application with a SolidJS + TypeScript + CodeMirror 6 UI.
These pages explain how the code is organized and how to extend it. The binding rules for
humans and coding agents are in [`AGENTS.md`](../../../AGENTS.md); step-by-step recipes are the
skills in [`.claude/skills/`](../../../.claude/skills/); the detailed IPC contracts are in
[`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md).

## Start here

| Page | Covers |
| --- | --- |
| [Architecture overview](architecture.md) | the big picture: shell, crates, packs, UI, IPC, browser mode |
| [Repository layout](repo-layout.md) | where everything lives |
| [Development setup](dev-setup.md) | toolchains, `pnpm dev`, `pnpm tauri dev`, demo projects |
| [Rust computes, TypeScript renders](rust-computes.md) | the main design rule and how to apply it |
| [The WebAssembly bridge](wasm-bridge.md) | running pure crates in browser mode |
| [Unity editor bridge protocol](unity-bridge-protocol.md) | the localhost protocol between Forge and its Unity package |

## Extending Forge

| Page | Covers |
| --- | --- |
| [Contribution points](contribution-points.md) | every `register*` API of the UI with examples |
| [IPC commands and browser mocks](ipc-and-mocks.md) | Rust command, typed client, mock |
| [Writing a plugin or pack](plugins-and-packs.md) | manifest, crate, UI feature, activation — a walkthrough |
| [Plugin SDK](plugins/index.md) | third-party plugins: sandbox, manifest, `forge` API, permissions, marketplace index |
| [Adding a language](adding-a-language.md) | file types, syntax modes, template data languages, language packs |
| [Settings](settings.md) | declaring, reading and showing settings |
| [Localization](localization.md) | translatable UI text: catalogs, `t()`, key naming, German, `pnpm i18n:check` |
| [Writing accessible UI](accessibility.md) | ARIA roles and names, trees, dialogs and focus, keyboard operation, focus rings, theme contrast, announcements, axe audit and keyboard-only flow |
| [Keymap actions](keymap-actions.md) | shortcuts through the keymap engine |

## Quality and delivery

| Page | Covers |
| --- | --- |
| [Screenshots and scenarios](screenshots.md) | Playwright scenarios, checking UI changes |
| [Quality gates](quality-gates.md) | the checks before every commit |
| [Release builds](release-builds.md) | AppImage, deb/rpm, installers, remote agents |
| [Search benchmark](search-benchmark.md) | measure the search index and compare a change against the baseline |
| [Versioning](versioning.md) | Pride Versioning, `pnpm version:bump`, releasing a version |
| [Git workflow](git-workflow.md) | branches, commits, CI, documentation updates |

## Crates

[One page per crate](crates/index.md) in `crates/` and `packs/`: purpose, main types, the IPC
commands it backs, and its tests.
