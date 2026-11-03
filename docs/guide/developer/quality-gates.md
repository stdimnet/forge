---
title: Quality gates
description: The checks to run before every commit - rustfmt, clippy, cargo tests, TypeScript typecheck, vitest, production build, browser extension checks and test rules.
order: 130
tags: [quality, testing, ci, clippy]
---

# Quality gates

Run all of these and fix every failure before committing. CI runs only on manual dispatch, so
local checks are the gate. Recipe:
[`quality-gates`](../../../.claude/skills/quality-gates/SKILL.md).

## Rust (repository root)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- With several worktrees, give each its own `CARGO_TARGET_DIR`.
- After changing plugin manifests or project generators:
  `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`.

## UI (`app/ui`)

```sh
pnpm typecheck
pnpm test        # vitest
pnpm build
pnpm version:check   # Cargo.toml, tauri.conf.json and package.json agree
```

## Browser extension (`browser-extension`, only when it changed)

```sh
pnpm typecheck
pnpm test
pnpm build       # dist/chrome and dist/firefox
```

## Test rules

- Tests must not need network access, installed language servers or real code-host accounts.
  Use fakes and local mock servers: a fake language server (`crates/forge-lsp/tests/fake_server.rs`),
  recorded agent CLI streams and fake CLI scripts (`crates/forge-agent-cli/tests/`), in-memory
  pipes for the remote protocol, local HTTP mock servers for code hosts and AI providers,
  SQLite for database tests.
- Parsers and models are tested in Rust; UI logic with vitest (pure modules, e.g. tree
  navigation, settings application, keymap dispatch).

## Before you commit

- UI changes have screenshots that you looked at ([Screenshots](screenshots.md)).
- No stray files: mocks never write into real projects; caches go to the user cache dir.
- User-visible changes update the matching page in `docs/guide/`.
- Small commits with imperative English subjects, no trailers ([Git workflow](git-workflow.md)).
