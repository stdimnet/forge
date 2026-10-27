//! Vulnerable dependency check, like IntelliJ's Package Checker, with each ecosystem's own
//! audit tool doing the lookup.
//!
//! - [`model`]: the shared model (package, installed version, advisory id, severity, title,
//!   affected range, fixed version, URL) that every report becomes.
//! - [`parse`]: the JSON of `composer audit`, `npm audit`, `pnpm audit`, `yarn audit` /
//!   `yarn npm audit`, `pip-audit`, `cargo audit`, `govulncheck` and OWASP
//!   dependency-check reports; [`cvss`] rates advisories that only carry a vector.
//! - [`tool`]: the tools, their command lines and which of them a project needs
//!   ([`tool::detect`]), plus the cache key of a target (manifest and lock file contents).
//! - [`declare`]: where composer.json / composer.lock, package.json, requirements files,
//!   pyproject.toml, Cargo.toml, go.mod, pom.xml and Gradle scripts declare a package.
//! - [`problems`]: the "Vulnerable Dependencies" problems of a manifest and the "Upgrade
//!   to <fixed version>" edit; [`report`]: per-target results and cache entries.
//!
//! The crate is pure: the desktop app runs the tools through the project's backend and
//! browser mode feeds recorded reports through WebAssembly.

pub mod cvss;
pub mod declare;
pub mod model;
pub mod parse;
pub mod problems;
pub mod report;
pub mod text;
pub mod tool;

pub use model::{Ecosystem, Findings, Notice, NoticeKind, Severity, Vulnerability};
pub use problems::{FileProblem, Upgrade};
pub use report::{CacheEntry, Status, TargetResult};
pub use tool::{DetectOptions, Target, Tool};

/// Setting keys (see `app/src-tauri/plugins/vulnerabilities.toml`).
pub const ENABLED_KEY: &str = "vulnerabilities.enabled";
pub const ON_OPEN_KEY: &str = "vulnerabilities.checkOnOpen";
pub const CACHE_HOURS_KEY: &str = "vulnerabilities.cacheHours";
pub const DEPENDENCY_CHECK_KEY: &str = "vulnerabilities.dependencyCheckReport";
