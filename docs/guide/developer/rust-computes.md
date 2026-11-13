---
title: Rust computes, TypeScript renders
description: Forge's central design rule - logic, models and parsers in Rust crates with tests, the UI renders and forwards events - and how to apply it.
order: 40
tags: [architecture, rust, typescript, design]
---

# Rust computes, TypeScript renders

Logic, data models and parsers belong in Rust crates with unit tests. The SolidJS UI renders what
Rust computed and forwards user events. `app/src-tauri` stays thin: it wires crates to IPC.

## Why

- **One implementation**: the desktop app (IPC) and browser mode (WebAssembly) run the same code,
  so mocks never drift from the real behaviour.
- **Testable without a UI**: parsers, rankers and state machines are plain Rust functions with
  `cargo test`.
- **Memory safety and speed** for the heavy parts: indexing, fuzzy ranking, output parsing.

## What lives where

| In Rust | In TypeScript |
| --- | --- |
| parsing (PHP, Lua, DQL, `.http`, compose files, kubeconfig, git output, TeamCity messages, ANSI) | rendering components |
| ranking (Search Everywhere, Go to File, popups' speed search) | highlighting the positions Rust returned |
| models of complex views (Services tree and details, run console lines and test trees, chat conversation) | applying model updates to signals |
| keymap resolution, settings search and validation, file type detection | capturing DOM events |
| completion, navigation and gutter data of frameworks | CodeMirror extensions that show them |

**Exception:** per-keystroke editor work — CodeMirror tokenizers and decorations — stays in
TypeScript because an IPC round trip per token would be too slow. Even there, data that is not
needed per keystroke comes from Rust (DQL injection highlighting is computed by the backend
shortly after each edit; old spans are mapped through changes meanwhile).

## Examples in the codebase

| Feature | Rust | UI |
| --- | --- | --- |
| Search Everywhere | `forge_fuzzy::everywhere` ranks candidates of all providers (`search_rank`) | providers collect candidates; the popup renders groups |
| Settings dialog | `forge_plugin_api::settings_model` builds the page tree, filters it and splits texts into highlighted segments (`settings_search`) | renders the tree and controls |
| Run tool window | `forge-output` turns process output into styled console lines and test tree updates | `Console.tsx` and `TestTree.tsx` apply deltas |
| AI Chat | `forge_agent_cli::conversation` folds agent events into rendered blocks (Markdown AST, diffs, permission summaries) | `features/ai-chat/model.ts` applies updates |
| Markdown | `forge-markdown` produces a safe AST (no raw HTML) | Solid components render the AST |
| Services | `forge_containers::tree` builds the tree, details and toolbar state | `ServicesView.tsx` renders them |

## Applying the rule

1. Put the logic in a crate (`crates/forge-<area>` or a pack) and test it there.
2. Keep the pure part free of file system, processes, network and tokio, so it can be built to
   WebAssembly. Split runtime parts into a separate crate (`forge-output` for `forge-process`,
   `forge-sql` for `forge-db`, `phpforge-dql` for `phpforge-orm-doctrine`) or behind a cargo
   feature (`runtime` in `forge-agent-cli` and `forge-containers`, `client` in `forge-remote`
   and `forge-scratches`, `fs` in `webforge-lang-web`).
3. Expose it through an IPC command ([IPC commands and mocks](ipc-and-mocks.md)) and, for browser
   mode, through `forge-wasm` ([The WebAssembly bridge](wasm-bridge.md)).
4. In the UI, call the typed client and render the answer. Avoid re-implementing decisions in
   TypeScript.
