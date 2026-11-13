---
title: forge-forges
description: "Forge IDE code hosts: GitLab, Forgejo/Gitea, GitHub and Azure DevOps merge requests, issues and pipelines."
order: 100
tags: [crate, rust]
---

# forge-forges

`crates/forge-forges` — Forge IDE code hosts: GitLab, Forgejo/Gitea, GitHub and Azure DevOps merge requests, issues and pipelines.

## Purpose

Forge code hosts: GitLab (REST v4), Forgejo/Gitea (`/api/v1`), GitHub (REST, plus
GraphQL for review threads) and Azure DevOps (REST 7.1 with a personal access token: pull
requests, threads, votes and completion; work items found with WIQL as issues; builds as
pipelines; changed files diffed locally with `similar`, since the API has no patches)
behind one `CodeHost` trait.

Repositories are detected from `git remote -v` (`remote`); accounts and their tokens
(OS keychain, service `forge-forges`) are managed by `ForgeService`.

## Modules

| Module | Summary |
| --- | --- |
| `accounts.rs` | Code host accounts. The list lives in `<config dir>/forge/forges.toml`; tokens are kept in the OS keychain (service `forge-forges`, account = account id), never in files. |
| `azure.rs` | Azure DevOps Services and Server (REST `api-version=7.1`, Basic authentication with a PAT). The account's server is the organization or collection; a repository is `project/repository`. |
| `error.rs` |  |
| `forgejo.rs` | Forgejo and Gitea (`/api/v1`, `Authorization: token ...`). Both share the API; Codeberg runs Forgejo. Pull request review comments are grouped into threads by file and line, since the API has no thread objects. |
| `github.rs` | GitHub REST API (github.com and GitHub Enterprise Server, `Authorization: Bearer`). Review threads are rebuilt from `in_reply_to_id`; their resolved state and resolving use the GraphQL API, which REST does not cover. |
| `gitlab.rs` | GitLab REST API v4 (gitlab.com and self-managed). Authentication with a personal access token (`PRIVATE-TOKEN`). Projects are addressed by their URL-encoded full path, so nested groups work without looking up numeric ids. |
| `host.rs` | The `CodeHost` trait implemented by GitLab, Forgejo/Gitea, GitHub and Azure DevOps. `submit_review` sends a `NewReview` (verdict, body, inline comments with optional start lines) as one review on GitHub and Forgejo/Gitea, and as threads with line ranges plus a vote (10 approved, -5 waiting for author) on Azure DevOps; its default posts the comments one by one and approves (GitLab). `review_events` lists the verdicts a host accepts. |
| `http.rs` | A small JSON REST client shared by the host implementations: authentication headers, `Link`-header pagination and translating HTTP failures into friendly `HostError`s. |
| `json.rs` | Lenient accessors for API JSON. Hosts differ between versions (fields appear, vanish or become null), so missing values fall back to defaults instead of failing a whole list. |
| `model.rs` | Host-neutral types for merge requests, issues and pipelines. GitHub and Forgejo call merge requests "pull requests"; the UI picks the wording from `HostKind`. |
| `remote.rs` | Detect the code host and project of a repository from its git remotes (`https://`, `ssh://`, scp-like `git@host:path`, nested GitLab groups, self-hosted servers known through accounts, Azure DevOps `dev.azure.com`, `ssh.dev.azure.com:v3`, `*.visualstudio.com` and `/_git/` remotes). |
| `service.rs` | Accounts plus host construction: the entry point the IDE uses. |

## Main public items

```rust
pub use azure::AzureDevOps;
pub use accounts::{Account, KeyringStore, MemoryStore, SecretStore, KEYCHAIN_SERVICE};
pub use error::{HostError, HostResult};
pub use forgejo::Forgejo;
pub use github::GitHub;
pub use gitlab::GitLab;
pub use host::CodeHost;
pub use model::*;
pub use service::{open_host, review_ref, AccountInfo, ForgeService, TokenSource};
```

## Used by

- IPC: `forges_accounts`, `forges_login`, `forges_logout`, `forges_verify`, `forges_repo`, `forges_merge_requests`, `forges_merge_request`, `forges_mr_files`, `forges_discussions`, `forges_create_mr`, `forges_comment`, `forges_reply`, `forges_resolve`, `forges_approve`, `forges_merge`, `forges_close`, `forges_issues`, `forges_issue`, `forges_issue_comment`, `forges_pipelines`, `forges_ci_status`, `forges_members`, `forges_file`, `forges_checkout` (`app/src-tauri/src/commands/forges.rs`); review mode through [`forge-code-review`](forge-code-review.md)
- UI: `app/ui/src/features/forges`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-vcs`](forge-vcs.md)
- External: `async-trait`, `dirs`, `futures-util`, `keyring`, `reqwest`, `serde`, `serde_json`, `similar`, `thiserror`, `tokio`, `toml`, `tracing`

## Tests

13 source files; unit tests and integration tests in `tests/` (`azure.rs`, `forgejo.rs`, `github.rs`, `gitlab.rs`, `service.rs`) against a local mock HTTP server, without network access.

```sh
cargo test -p forge-forges
```
