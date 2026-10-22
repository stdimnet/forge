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
| `history.rs` | File, folder and selection history: `FileRevision`, `BlockRevision`, `HistoryQuery` and the parsers of `git log -z --name-status` and `git log -L` output. Pure. |
| `log.rs` | Commit history: parser for a NUL/RS separated `git log` format, ref decorations and per-commit changed files. |
| `ops.rs` | Options of the Pull, Merge, Rebase, Reset HEAD and Tag dialogs (`PullOptions`, `MergeOptions`, `RebaseOptions`, `ResetMode`, `TagOptions`, `OperationAction`) and the git command lines they stand for; forbidden combinations are rejected. Pure. |
| `patch.rs` | `git diff --binary` patches: split per file (`split_patch`, `join_patch`) and apply a file's hunks to its base text (`apply_file_patch`). Pure. |
| `merge.rs` | Three-way merge for the merge window: diff3 chunking of yours/base/theirs (`merge_three_way` → `MergeModel` with `left`/`right`/`both`/`conflict` chunks), word-level "Resolve Simple Conflicts" (`resolve_simple`), `auto_merge`, line separator helpers. Pure. |
| `rebase.rs` | Git Log actions that rewrite history and the interactive rebase: which actions a selection allows with PHPStorm's reasons (`log_action_states` from `SelectionFacts`, protected branch patterns), the initial plan with autosquash (`autosquash`) and the `git-rebase-todo` (`plan_todo`, `render_todo`: messages are applied with `exec` amend steps from blobs, the todo is copied in by `sequence.editor`). Pure. |
| `repo.rs` | `Repo`: every git operation of the IDE, through a `GitExec` (local or remote host). |
| `repo/rewrite.rs` | The Git Log context menu: `log_action_states` (pushed / protected detection via `for-each-ref --contains`), `cherry_pick` (skips empty picks, `-x` for protected commits), `revert_commits`, `undo_commit`, `reword`, `squash_commits`, `drop_commits`, `rebase_plan`, `interactive_rebase` (`git rebase -i --autostash` without an editor), `undo_rewrite`, `commits_patch`, `tree_files`. |
| `repo/conflicts.rs` | Merge conflicts: unmerged files and their sides (`git ls-files -u`), the operation (merge, rebase, cherry-pick, revert, unstash; a rebase only when `git status` confirms it, since `rebase --skip` leaves a stale `REBASE_HEAD`) and pane titles, stage versions `:1:`/`:2:`/`:3:` (`merge_file`), Accept Yours/Theirs (`accept_conflict_side`). Yours is git's `theirs` in a rebase. |
| `repo/ops.rs` | Pull/Merge/Rebase with options (`pull_with`, `merge_with`, `rebase_with`), the operation in progress (`operation_state`, via `REBASE_HEAD`/`MERGE_HEAD`/… and the long status, which also drops a stale `REBASE_HEAD`), Continue/Abort/Skip (`operation_action`), `reset_head`, `tag`, `resolve_commit`, `edit_remote`, stash details (`stash_files`, `stash_file`, `stash_clear`, `stash_branch`). |
| `repo/shelve.rs` | Shelve (`changes_patch` incl. untracked files, then roll back), Unshelve (`git apply`, falling back to `--3way` with conflicts for the resolver; partial by file) and `shelf_file` (before/after for Show Diff). |
| `repo/history.rs` | Show History (`file_history`, `--follow` for files), Show History for Selection (`block_history`, `git log -L`), Annotate Revision (`blame_at`) and Compare with Branch on a folder (`changes_since`). |
| `shelf.rs` | `Shelf`: shelved changelists stored in Forge's per-project data dir (`<id>/changes.json` + `shelved.patch`): list, create, rename, delete, remove files. |
| `status.rs` | Parser for `git status --porcelain=v2 -z --branch`. |

## Main public items

```rust
pub use blame::{parse_blame, Blame, BlameCommit};
pub use branch::{parse_recent_checkouts, parse_refs, BranchInfo, Branches};
pub use diff::{parse_diff, DiffLine, FileDiff, Hunk, LineKind};
pub use history::{parse_block_history, parse_file_history, BlockRevision, FileRevision, HistoryQuery};
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

- IPC: `git_info`, `git_status`, `git_show`, `git_diff`, `git_diff_head`, `git_blame`, `git_log`, `git_commit_details`, `git_branches`, `git_checkout`, `git_create_branch`, `git_delete_branch`, `git_merge`, `git_rebase`, `git_stage`, `git_unstage`, `git_rollback`, `git_commit`, `git_push`, `git_pull`, `git_fetch`, `git_conflicts`, `git_merge_file`, `git_accept_conflict_side`, `git_resolve_merge` (`app/src-tauri/src/commands/git.rs`); `git_pull_with`, `git_merge_with`, `git_rebase_with`, `git_operation_state`, `git_operation_action`, `git_reset_head`, `git_tag_with`, `git_resolve_commit`, `git_edit_remote`, `git_stash_files`, `git_stash_file`, `git_stash_clear`, `git_stash_branch`, `git_shelf_list`, `git_shelve`, `git_unshelve`, `git_shelf_delete`, `git_shelf_rename`, `git_shelf_file` (`commands/git_ops.rs`)
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
