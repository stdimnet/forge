//! Forge's catalog of language servers for many languages, and everything around it:
//!
//! - [`catalog`]: the data-driven catalog (`catalog.toml`) of languages and servers with
//!   commands, initialization options, settings, install hints and licences,
//! - [`detect`]: finding installed servers (PATH, npm/nvm/volta, pip/pipx, cargo, go,
//!   dotnet, gem, Mason, Windows `.cmd` shims),
//! - [`scan`]: which languages a project uses,
//! - [`config`]: building a [`forge_lsp::LspServerConfig`] for a project,
//! - [`plan`]: which servers start, and the rows of the settings page,
//! - [`install`]: running an install command with streamed output,
//! - [`managed`]: installs of npm servers without prerequisites (Node.js and packages
//!   downloaded and verified into Forge's data directory) and their update check,
//! - [`licence`]: the Intelephense licence key in the OS keychain.
//!
//! PHP servers (the built-in php-lsp, Intelephense, phpactor) stay in the PHP pack; the app
//! lists them next to these rows.

pub mod catalog;
pub mod config;
pub mod detect;
pub mod install;
pub mod licence;
pub mod managed;
pub mod plan;
pub mod prefs;
pub mod scan;

pub use catalog::{Catalog, InstallOption, Language, ServerSpec};
pub use config::{build_config, merge_json, ConfigContext};
pub use detect::{DetectEnv, Detector};
pub use plan::{plan, rows, Decision, LiveState, RowStatus, ServerRow, Skip};
pub use prefs::{server_key, Prefs, ServerPrefs};
pub use scan::{scan_project, ProjectScan};
