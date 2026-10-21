---
title: Git workflow
description: Branch names, commit style, no trailers, manual-only CI, worktrees and keeping the documentation in step with changes.
order: 150
tags: [git, workflow, ci, commits]
---

# Git workflow

- Repository: <https://github.com/Reeperk/Forge>, default branch `main`.
- **Branch names** describe the work: `feat/…`, `fix/…`, `docs/…`, `chore/…`
  (`feat/laravel-plugin`, `fix/tree-double-click`, `docs/user-and-developer-guide`).
- **Small commits** with imperative English subjects: "Add routes tool window",
  "Fix double toggle of the Terminal".
- **No co-author or other trailers** in commit messages, and no mention of AI tools.
- **Commit early and often** during long tasks so work is not lost.
- Do not push or open pull requests unless you were asked to.

## CI

`.github/workflows/ci.yml` runs **only on manual dispatch** (`workflow_dispatch`): UI
typecheck/test/build on Ubuntu, and fmt, clippy and tests on Ubuntu, Windows and macOS. Never
trigger workflows yourself, and every branch must keep the `workflow_dispatch`-only trigger.
Run the [quality gates](quality-gates.md) locally instead. When the owner dispatches it, the UI
and Linux jobs run on the project's self-hosted runner (`.github/scripts/linux-deps.sh` checks
its system packages), Windows and macOS on GitHub's runners.

`.github/workflows/release.yml` runs **only when a GitHub release is published**. It builds the
Linux, Windows and macOS bundles (with the remote agents), signs the updater artifacts and uploads
them plus `latest.json` to that release. Only the project owner publishes releases; never add
push, pull request or schedule triggers to any workflow. See [Release builds](release-builds.md).

## Worktrees

Several git worktrees can build in parallel; give each its own `CARGO_TARGET_DIR` (a shared target
dir can return stale builds of workspace crates) and consider `CARGO_PROFILE_DEV_DEBUG=0`.

## Pull requests

- Run the quality gates.
- Attach screenshots for UI changes.
- Update the docs: the matching page in `docs/guide/` for every user-visible change (see
  [`docs/guide/README.md`](../README.md)), `docs/ARCHITECTURE.md` for new IPC contracts, the
  README and `docs/ROADMAP.md` for new plugins.
