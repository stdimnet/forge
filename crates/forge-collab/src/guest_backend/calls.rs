//! What a guest's project backend may ask the host: [`HostCall`].
//!
//! A guest is untrusted, so this enum is deliberately not the Forge agent's `Call`: it has no
//! way to start a process, run raw git arguments, start a language server or name an absolute
//! path. Every variant carries only what [`Policy::check_call`](crate::permissions::Policy)
//! can verify, and the host builds every command line itself.
//!
//! Calls travel as an opaque MessagePack payload inside `GuestMessage::Call`, replies inside
//! `HostMessage::CallResult` (the MessagePack encoding of the call's reply type, noted on each
//! variant). Terminals and runs (S5) are requests here too; the keys typed into a terminal
//! and console input travel as `GuestMessage::TerminalInput` / `RunInput`, in order.

use forge_backend::protocol::PhpCall;
use forge_backend::FrameworkCall;
use forge_core::search::FindRequest;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::share::{RunId, TerminalId};
use crate::{Error, Result};

/// Largest call payload the host decodes (the frame limit already bounds it).
pub const MAX_CALL: usize = crate::protocol::MAX_GUEST_FRAME;

/// A request from a guest's project backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HostCall {
    /// Measures the round trip -> `()`
    Ping,
    /// The project's name -> [`OpenedProject`]
    Open,
    /// One folder level; `""` is the project root -> `Vec<FileEntry>`
    ListDir { path: String },
    /// -> `String`
    ReadFile { path: String },
    /// -> `Option<String>`; excluded files read as missing
    ReadOptional { path: String },
    /// Files Edit -> `()`
    WriteFile { path: String, contents: String },
    /// Files Edit -> `()`
    CreateFile { path: String },
    /// Files Edit -> `()`
    CreateDir { path: String },
    /// Files Edit -> `()`
    Rename { from: String, to: String },
    /// Files Edit -> `()`
    Delete { path: String },
    /// The file's raw bytes (other encodings than UTF-8) -> `ByteBuf`
    ReadBytes { path: String },
    /// Files Edit -> `()`
    WriteBytes { path: String, bytes: ByteBuf },
    /// -> `Vec<FileMatch>`; `exact` matches the query as a substring instead of fuzzy
    FindFiles {
        query: String,
        limit: usize,
        #[serde(default)]
        exact: bool,
    },
    /// -> `FindResult`
    FindInFiles(FindRequest),
    /// -> `ProjectInfo`
    Detect,
    /// Start sending `HostMessage::FsChanged` for the project -> `()`
    Watch,
    /// A read-only git query -> [`forge_vcs::GitOutput`]
    Git(GitQuery),
    /// A git command that changes the repository, the index or the working tree; VCS Full
    /// access -> [`forge_vcs::GitOutput`]
    GitAction(GitAction),
    /// PHP index queries (language server settings are refused) -> the call's reply type
    Php(PhpCall),
    /// Laravel, Symfony and Doctrine analysis (nothing that runs project code) -> the call's
    /// reply type
    Framework(FrameworkCall),
    /// Language server features, run by the host's servers -> JSON of the call's reply type
    Lsp(LspQuery),
    /// Open a terminal on the host in the project folder, with the host's shell (titled
    /// "<guest>: Local"); Terminal Full access -> [`SharedTerminal`](crate::share::SharedTerminal)
    TerminalOpen { cols: u16, rows: u16 },
    /// Close a terminal this guest opened; Terminal Full access -> `()`
    TerminalClose { terminal: TerminalId },
    /// Start the host's run configuration `configuration` (by name; a running one is rerun),
    /// as if the host clicked Run; Run Full access -> `()`
    RunStart { configuration: String },
    /// Stop a run of the host; Run Full access -> `()`
    RunStop { run: RunId },
}

/// Reply to [`HostCall::Open`]. The host's absolute project path is not sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenedProject {
    pub name: String,
}

/// Read-only git queries. Each is exactly one command line that Forge's version control code
/// issues; [`GitQuery::to_args`] builds it on the host and [`parse_git`](super::git) turns the
/// guest's command into the query. Paths are repository-relative pathspecs, as git gets them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitQuery {
    /// `rev-parse --show-toplevel --show-prefix` in the project root
    Discover,
    /// `rev-parse --verify -q HEAD`
    HasHead,
    /// `rev-parse HEAD`
    Head,
    /// `status --porcelain=v2 -z --branch --untracked-files=all`
    Status,
    /// `show <rev>:<path>`; an empty `rev` is the index
    Show { rev: String, path: String },
    /// `show -s --format=<meta> <rev>`
    CommitMeta { rev: String },
    /// `diff-tree -r -z --name-status -M <parent> <sha>`, or `--root <sha>` without a parent
    CommitFiles { parent: Option<String>, sha: String },
    /// `diff --no-ext-diff --no-color -M [--cached] [HEAD] -- <paths>`
    Diff {
        staged: bool,
        against_head: bool,
        paths: Vec<String>,
    },
    /// `blame --porcelain [--contents -] -- <path>`; `contents` is the unsaved buffer
    Blame {
        path: String,
        contents: Option<String>,
    },
    /// `log <format> --decorate=full --date-order ...`
    Log {
        limit: usize,
        skip: usize,
        text: Option<String>,
        author: Option<String>,
        rev: Option<String>,
        all: bool,
        path: Option<String>,
    },
    /// `for-each-ref <format> --sort=-committerdate refs/heads refs/remotes refs/tags`
    Refs,
    /// `reflog --format=%gs -n 300`
    Reflog,
    /// `--literal-pathspecs ls-files -z --cached -- <paths>`
    TrackedAmong { paths: Vec<String> },
    /// `ls-files -z --cached -- <paths>`
    LsFiles { paths: Vec<String> },
    /// `ls-tree -r -z --name-only HEAD -- <paths>`
    LsTreeHead { paths: Vec<String> },
    /// `remote`
    Remotes,
    /// `remote -v`
    RemotesVerbose,
    /// `show-ref --verify --quiet refs/heads/<name>`
    HasBranch { name: String },
}

/// Git commands that change something. They run the host's hooks and credentials, so they
/// need VCS Full access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitAction {
    /// `add -A -- <paths>`
    Stage { paths: Vec<String> },
    /// `add -- <paths>` (new files before a commit of exactly these files)
    AddNew { paths: Vec<String> },
    /// `restore --staged -- <paths>`
    Unstage { paths: Vec<String> },
    /// `rm --cached -r -q -- <paths>` (before the first commit)
    UnstageInitial { paths: Vec<String> },
    /// `restore --source=HEAD --staged --worktree -- <paths>`
    RestoreFromHead { paths: Vec<String> },
    /// `rm --cached -f -q -- <paths>`
    ForgetAdded { paths: Vec<String> },
    /// `commit -q --cleanup=strip [--amend] (--no-edit | -F -) [--only -- <paths>]`
    Commit {
        message: String,
        amend: bool,
        paths: Vec<String>,
    },
    /// `checkout <rev> --`
    Checkout { rev: String },
    /// `checkout -b <name> --track <remote_branch>`
    CheckoutTracking { name: String, remote_branch: String },
    /// `checkout -b <name> [<start>]`
    CheckoutNew { name: String, start: Option<String> },
    /// `branch <name> [<start>]`
    BranchNew { name: String, start: Option<String> },
    /// `branch -d|-D <name>`
    BranchDelete { name: String, force: bool },
    /// `branch -m <old> <new>`
    BranchRename { old: String, new: String },
    /// `merge --no-edit <rev>`
    Merge { rev: String },
    /// `rebase <onto>`
    Rebase { onto: String },
    /// `push [--force-with-lease] [-u <remote> HEAD]`
    Push {
        force: bool,
        set_upstream: Option<String>,
    },
    /// `pull --no-edit --rebase|--no-rebase`
    Pull { rebase: bool },
    /// `fetch --all --prune`
    Fetch,
    /// `fetch <remote> <refspec>`
    FetchRefspec { remote: String, refspec: String },
}

/// Language server requests. Paths are workspace-relative, positions are LSP positions
/// (0-based line, UTF-16 character). The host answers from its own servers, with the text of
/// its shared document (or the file on disk) synchronized first; the guest sends no text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LspQuery {
    /// The guest opened or changed the file -> `()`
    Sync { path: String },
    /// The guest closed the file -> `()`
    Close { path: String },
    /// -> `Option<EditorHover>`
    Hover {
        path: String,
        line: u32,
        character: u32,
    },
    /// -> `Option<EditorCompletionList>`
    Completion {
        path: String,
        line: u32,
        character: u32,
    },
    /// `item` is the completion item as JSON text -> `Option<EditorCompletionItem>`
    CompletionResolve { path: String, item: String },
    /// -> `Vec<EditorLocation>`
    Definition {
        path: String,
        line: u32,
        character: u32,
    },
    /// -> `Vec<EditorLocation>`
    References {
        path: String,
        line: u32,
        character: u32,
    },
    /// -> `Vec<EditorSymbol>`
    DocumentSymbols { path: String },
    /// -> `Option<EditorSignatureHelp>`
    SignatureHelp {
        path: String,
        line: u32,
        character: u32,
    },
    /// Files Edit -> `Vec<TextEdit>`
    Format {
        path: String,
        tab_size: u32,
        insert_spaces: bool,
    },
    /// -> `Vec<EditorWorkspaceSymbol>`
    WorkspaceSymbols { query: String },
}

impl HostCall {
    /// A short name for logs and denial messages.
    pub fn name(&self) -> &'static str {
        match self {
            HostCall::Ping => "ping",
            HostCall::Open => "open",
            HostCall::ListDir { .. } => "list_dir",
            HostCall::ReadFile { .. } => "read_file",
            HostCall::ReadBytes { .. } => "read_bytes",
            HostCall::WriteBytes { .. } => "write_bytes",
            HostCall::ReadOptional { .. } => "read_optional",
            HostCall::WriteFile { .. } => "write_file",
            HostCall::CreateFile { .. } => "create_file",
            HostCall::CreateDir { .. } => "create_dir",
            HostCall::Rename { .. } => "rename",
            HostCall::Delete { .. } => "delete",
            HostCall::FindFiles { .. } => "find_files",
            HostCall::FindInFiles(_) => "find_in_files",
            HostCall::Detect => "detect",
            HostCall::Watch => "watch",
            HostCall::Git(_) => "git",
            HostCall::GitAction(_) => "git_action",
            HostCall::Php(_) => "php",
            HostCall::Framework(_) => "framework",
            HostCall::Lsp(_) => "lsp",
            HostCall::TerminalOpen { .. } => "open terminal",
            HostCall::TerminalClose { .. } => "close terminal",
            HostCall::RunStart { .. } => "run",
            HostCall::RunStop { .. } => "stop run",
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        rmp_serde::to_vec_named(self).map_err(|e| Error::Protocol(e.to_string()))
    }

    /// Decode a payload from a guest; anything that is not exactly a call is an error.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_CALL {
            return Err(Error::Protocol("call is too large".into()));
        }
        rmp_serde::from_slice(bytes).map_err(|e| Error::Protocol(e.to_string()))
    }
}

impl GitAction {
    /// What the action does, for the session log (no commit messages or file lists).
    pub fn label(&self) -> String {
        match self {
            Self::Stage { paths } | Self::AddNew { paths } => {
                format!("add ({} paths)", paths.len())
            }
            Self::Unstage { paths } | Self::UnstageInitial { paths } => {
                format!("unstage ({} paths)", paths.len())
            }
            Self::RestoreFromHead { paths } => format!("rollback ({} paths)", paths.len()),
            Self::ForgetAdded { paths } => format!("forget ({} paths)", paths.len()),
            Self::Commit { .. } => "commit".into(),
            Self::Checkout { rev } => format!("checkout {rev}"),
            Self::CheckoutTracking { name, .. } | Self::CheckoutNew { name, .. } => {
                format!("checkout {name}")
            }
            Self::BranchNew { name, .. } => format!("branch {name}"),
            Self::BranchDelete { name, .. } => format!("branch -d {name}"),
            Self::BranchRename { old, new } => format!("branch -m {old} {new}"),
            Self::Merge { rev } => format!("merge {rev}"),
            Self::Rebase { onto } => format!("rebase {onto}"),
            Self::Push { .. } => "push".into(),
            Self::Pull { .. } => "pull".into(),
            Self::Fetch | Self::FetchRefspec { .. } => "fetch".into(),
        }
    }
}
