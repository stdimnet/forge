//! Forge code hosts: GitLab (REST v4), Forgejo/Gitea (`/api/v1`), GitHub (REST, plus
//! GraphQL for review threads) and Azure DevOps (REST 7.1, work items as issues, builds as
//! pipelines) behind one [`CodeHost`] trait.
//!
//! Repositories are detected from `git remote -v` ([`remote`]); accounts and their tokens
//! (OS keychain, service `forge-forges`) are managed by [`ForgeService`].

pub mod accounts;
pub mod azure;
pub mod error;
pub mod forgejo;
pub mod github;
pub mod gitlab;
pub mod host;
pub mod http;
mod json;
pub mod model;
pub mod remote;
pub mod service;

pub use accounts::{Account, KeyringStore, MemoryStore, SecretStore, KEYCHAIN_SERVICE};
pub use azure::AzureDevOps;
pub use error::{HostError, HostResult};
pub use forgejo::Forgejo;
pub use github::GitHub;
pub use gitlab::GitLab;
pub use host::CodeHost;
pub use model::*;
pub use service::{open_host, review_ref, AccountInfo, ForgeService, TokenSource};
