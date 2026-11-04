---
name: quality-gates
description: Run Forge's quality checks (rustfmt, clippy, cargo tests, TypeScript typecheck, vitest, production build) before committing or merging. Use before every commit, when asked if a change is ready, or when CI-equivalent checks are needed locally.
---

# Quality gates

Run all of these and fix every failure before committing. CI is manual-only for now
(`workflow_dispatch`), so local checks are the gate.

## Rust (repo root)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- Don't set `CARGO_PROFILE_DEV_DEBUG`: the dev profile already keeps debug info small, and a
  second profile duplicates every artifact in `target/`.
- With several git worktrees, give each its own `CARGO_TARGET_DIR`. A target dir shared
  between worktrees can return stale builds of workspace crates.
- If you changed plugin manifests, run `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`.

## UI (`app/ui`)

```sh
pnpm typecheck
pnpm test        # vitest
pnpm build
pnpm i18n:check      # keys used in the UI exist in the English catalog; German coverage
pnpm version:check   # Cargo.toml, tauri.conf.json and package.json agree
pnpm workflows:check # release.yml runs only on `release: published`, ci.yml only manually
```

## Browser extension (`browser-extension`, only when it changed)

```sh
pnpm typecheck
pnpm test        # vitest
pnpm build       # dist/chrome and dist/firefox
```

## Before you commit

- UI changes have screenshots (`verify-ui-with-screenshots` skill).
- No stray files: mocks must never write into real projects, and caches belong in the user
  cache dir, not the project.
- Commit style: small commits with imperative English subjects ("Add routes tool window"),
  no co-author or other trailers. Branch names describe the work (`feat/laravel-plugin`,
  `fix/tree-double-click`, `docs/contributor-skills`).
- All code, comments, docs and UI text are in English.
