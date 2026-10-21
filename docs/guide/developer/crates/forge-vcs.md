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

## Modules

| Module | Summary |
| --- | --- |
| `blame.rs` | Parser for `git blame --porcelain`. |
| `branch.rs` | Branches and tags from `git for-each-ref`, plus recently checked out branches from the reflog. |
| `cmd.rs` | Running the `git` executable. No shell is involved: arguments are passed as-is, so paths with spaces or quotes need no escaping on any platform. |
| `diff.rs` | Parser for unified diffs as printed by `git diff`. |
| `log.rs` | Commit history: parser for a NUL/RS separated `git log` format, ref decorations and per-commit changed files. |
| `status.rs` | Parser for `git status --porcelain=v2 -z --branch`. |

## Main public items

```rust
pub use blame::{parse_blame, Blame, BlameCommit};
pub use branch::{parse_recent_checkouts, parse_refs, BranchInfo, Branches};
pub use diff::{parse_diff, DiffLine, FileDiff, Hunk, LineKind};
pub use log::{…};
pub use status::{parse_status, FileState, RepoStatus, StatusEntry};
pub use cmd::{GitExec, GitOutput, GitRequest, LocalGit};
pub struct RepoInfo { … }
pub struct RemoteResult { … }
pub struct Repo { … }
```

## Used by

- IPC: `git_info`, `git_status`, `git_show`, `git_diff`, `git_diff_head`, `git_blame`, `git_log`, `git_commit_details`, `git_branches`, `git_checkout`, `git_create_branch`, `git_delete_branch`, `git_merge`, `git_rebase`, `git_stage`, `git_unstage`, `git_rollback`, `git_commit`, `git_push`, `git_pull`, `git_fetch` (`app/src-tauri/src/commands/git.rs`)
- UI: `app/ui/src/features/git`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md)
- External: `serde`, `tracing`

## Tests

7 source files, about 1737 lines; 10 unit tests and 11 integration tests in `tests/` (`repo.rs`).

```sh
cargo test -p forge-vcs
```
