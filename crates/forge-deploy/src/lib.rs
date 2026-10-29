//! Deployment, like PHPStorm's Tools | Deployment: servers (SFTP, FTP/FTPS, a local or
//! mounted folder) with a root path, a web server URL, path mappings and excluded paths;
//! upload, download, sync and compare with the deployed files; automatic upload of saved
//! files to the project's default server.
//!
//! Pure modules (also built for WebAssembly without the `client` feature):
//! - [`config`]: servers and the default server of a project (`.forge/deployment.toml`).
//! - [`mapping`]: local path ↔ deployment path ↔ web path, excluded paths and names.
//! - [`options`]: Deployment | Options (automatic upload, overwrite rules, timestamps).
//! - [`compare`]: comparing the project with the deployed files (timestamps, size,
//!   content) through `forge_diff`'s folder diff.
//! - [`browse`]: the nodes of the Remote Host tool window and the names it creates.
//!
//! With `client` (the default):
//! - [`fs`]: the file operations every server type offers, rooted at its root path.
//! - [`local`], [`ftp`], [`sftp`]: the server types (SFTP runs on forge-remote's SSH
//!   sessions and saved SSH configurations).
//! - [`transfer`]: upload, download, sync, delete and reading deployed files.
//! - [`connect`]: opening a server, with passwords from the OS keychain ([`secrets`]).
//! - [`remote_host`]: Remote Host connections, its context menu operations and Edit
//!   Remote File (a temporary copy uploaded back on save, with a conflict check).

pub mod browse;
pub mod compare;
pub mod config;
pub mod error;
pub mod mapping;
pub mod options;

#[cfg(feature = "client")]
pub mod connect;
#[cfg(feature = "client")]
pub mod fs;
#[cfg(feature = "client")]
pub mod ftp;
#[cfg(feature = "client")]
pub mod local;
#[cfg(feature = "client")]
pub mod remote_host;
#[cfg(feature = "client")]
pub mod secrets;
#[cfg(feature = "client")]
pub mod sftp;
#[cfg(feature = "client")]
pub mod transfer;

pub use config::{DeployServer, DeploymentConfig, ExcludedPath, ExcludedSide, Mapping, ServerKind};
pub use error::{DeployError, Result};
pub use options::{AutoUpload, CompareBy, DeployOptions, WarnNewer};
