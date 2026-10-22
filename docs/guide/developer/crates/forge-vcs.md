---
title: forge-vcs
description: "Forge IDE version control: drives the git CLI and parses its output."
order: 250
tags: [crate, rust]
---

# forge-vcs

`crates/forge-vcs` — Forge IDE version control: drives the git CLI and parses its output.

## Purpose

Forge version control. Drives the `git` command line (no libgit2, no shell) and parses
its machine-readable output into serializable types for the UI.

Paths in and out are relative to the *workspace* root, which may be a subdirectory of the
repository; `Repo` converts them to and from repository paths.

The parsers and the three-way merge (`merge`) are pure and also build for WebAssembly
(`default-features = false`); running git needs the default `git` feature.

## Modules

| Module | Summary |
| --- | --- |
| `blame.rs` | Parser for `git blame --porcelain`. |
| `branch.rs` | Branches and tags from `git for-each-ref`, plus recently checked out branches from the reflog. |
| `cmd.rs` | Running the `git` executable. No shell is involved: arguments are passed as-is, so paths with spaces or quotes need no escaping on any platform. |
| `diff.rs` | Parser for unified diffs as printed by `git diff`. |
| `log.rs` | Commit history: parser for a NUL/RS separated `git log` format, ref decorations and per-commit changed files. |
| `merge.rs` | Three-way merge for the merge window: diff3 chunking of yours/base/theirs (`merge_three_way` → `MergeModel` with `left`/`right`/`both`/`conflict` chunks), word-level "Resolve Simple Conflicts" (`resolve_simple`), `auto_merge`, line separator helpers. Pure. |
| `repo.rs` | `Repo`: every git operation of the IDE, through a `GitExec` (local or remote host). |
| `repo/conflicts.rs` | Merge conflicts: unmerged files and their sides (`git ls-files -u`), the operation (merge, rebase, cherry-pick, revert, unstash) and pane titles, stage versions `:1:`/`:2:`/`:3:` (`merge_file`), Accept Yours/Theirs (`accept_conflict_side`). Yours is git's `theirs` in a rebase. |
| `status.rs` | Parser for `git status --porcelain=v2 -z --branch`. |

## Main public items

```rust
pub use blame::{parse_blame, Blame, BlameCommit};
pub use branch::{parse_recent_checkouts, parse_refs, BranchInfo, Branches};
pub use diff::{parse_diff, DiffLine, FileDiff, Hunk, LineKind};
pub use log::{…};
pub use status::{parse_status, FileState, RepoStatus, StatusEntry};
pub use cmd::{GitExec, GitOutput, GitRequest, LocalGit};
pub mod merge; // merge_three_way, resolve_simple, auto_merge, MergeModel, MergeChunk
pub use repo::{ConflictSet, ConflictFile, ConflictSide, MergeFile, MergeOperation, SideChange, …};
pub struct RepoInfo { … }
pub struct RemoteResult { … }
pub struct Repo { … }
```

## Used by

- IPC: `git_info`, `git_status`, `git_show`, `git_diff`, `git_diff_head`, `git_blame`, `git_log`, `git_commit_details`, `git_branches`, `git_checkout`, `git_create_branch`, `git_delete_branch`, `git_merge`, `git_rebase`, `git_stage`, `git_unstage`, `git_rollback`, `git_commit`, `git_push`, `git_pull`, `git_fetch`, `git_conflicts`, `git_merge_file`, `git_accept_conflict_side`, `git_resolve_merge` (`app/src-tauri/src/commands/git.rs`)
- UI: `app/ui/src/features/git`
- Browser mode: `merge` runs as WebAssembly (`vcs_merge` in `forge-wasm`) for the git mock; the rest uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md)
- External: `serde`, `similar`, `tracing`

## Tests

10 source files; unit tests (including the three-way merge with fixture texts) and integration tests against temporary repositories in `tests/` (`repo.rs`, `conflicts.rs`: merges, rebases and unstashes that stop with conflicts).

```sh
cargo test -p forge-vcs
```
