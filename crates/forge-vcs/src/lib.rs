//! Forge version control. Drives the `git` command line (no libgit2, no shell) and parses
//! its machine-readable output into serializable types for the UI.
//!
//! Paths in and out are relative to the *workspace* root, which may be a subdirectory of the
//! repository; [`Repo`] converts them to and from repository paths.

//!
//! The parsers and the three-way merge ([`merge`]) are pure and also build for WebAssembly
//! (`default-features = false`); running git needs the `git` feature (on by default).

mod blame;
mod branch;
pub mod changelists;
pub mod clone;
#[cfg(feature = "git")]
mod cmd;
mod diff;
mod history;
pub mod hunks;
mod log;
#[cfg(feature = "git")]
mod manage;
pub mod merge;
pub mod message_history;
pub mod ops;
pub mod patch;
pub mod rebase;
#[cfg(feature = "git")]
mod repo;
#[cfg(feature = "git")]
pub mod shelf;
pub mod signing;
mod status;
pub mod worktree;

pub use blame::{parse_blame, Blame, BlameCommit};
pub use branch::{parse_recent_checkouts, parse_refs, BranchInfo, Branches, REF_FORMAT};
pub use diff::{parse_diff, DiffLine, FileDiff, Hunk, LineKind};
pub use history::{
    parse_block_history, parse_file_history, BlockRevision, FileRevision, HistoryQuery,
};
pub use log::{
    is_object_id, parse_decorations, parse_log, parse_name_status, ChangedFile, Commit,
    CommitDetails, LogQuery, RefKind, RefLabel, COMMIT_META_FORMAT, LOG_FORMAT,
};
#[cfg(feature = "git")]
pub use manage::{
    append_ignore_patterns, ignore_pattern, ignore_patterns_for, parse_remotes, parse_stash_list,
    Remote, StashEntry,
};
pub use signing::{
    GitSigningConfig, GpgKey, SignatureStatus, Signing, SigningFormat, SigningKey, SshKey,
};
pub use status::{parse_status, FileState, RepoStatus, StatusEntry};
pub use worktree::{NewWorktree, Worktree};

pub use changelists::{Changelist, ChangelistFile, ChangelistView, Changelists};
#[cfg(feature = "git")]
pub use cmd::{
    git_binary, set_git_program, GitBytesOutput, GitExec, GitOutput, GitRequest, LocalGit,
};
pub use hunks::ChangeHunk;
pub use ops::{MergeOptions, OperationAction, PullOptions, RebaseOptions, ResetMode, TagOptions};
pub use rebase::{
    ActionState, LogActionStates, RebaseAction, RebaseCommit, RebaseEntry, RebasePlan,
};
#[cfg(feature = "git")]
pub use repo::{
    parse_rebase_status, parse_unmerged, ConflictFile, ConflictSet, ConflictSide, MergeFile,
    MergeOperation, RemoteResult, Repo, RepoInfo, SideChange,
};
#[cfg(feature = "git")]
pub use repo::{ApplyPatchResult, CommitFile, CommitOptions};
#[cfg(feature = "git")]
pub use repo::{OngoingOperation, OperationState, ShelfFileContents, StashFileContents};
#[cfg(feature = "git")]
pub use repo::{RewriteResult, UndoneCommit};
#[cfg(feature = "git")]
pub use shelf::{Shelf, ShelvedChangelist, ShelvedFile};
