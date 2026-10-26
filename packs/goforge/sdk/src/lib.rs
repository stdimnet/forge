//! GoForge on the user's machine: Go SDKs, the Go tools Forge uses and module proxy
//! lookups. Everything that is pure Go knowledge lives in `goforge-lang-go`.
//!
//! - [`detect`]: SDKs found without running anything (PATH, GOROOT, install folders,
//!   Homebrew, Program Files, `~/sdk`, Forge's downloads), version from `VERSION`.
//! - [`download`]: SDK downloads from `go.dev/dl` (`?mode=json` lists SHA-256 checksums),
//!   only after the user agreed, into `<data dir>/forge/go-sdks`.
//! - [`tools`]: gopls, Delve, goimports and golangci-lint: where they are and the pinned
//!   `go install` of the first three (into `<data dir>/forge/go-tools/bin`).
//! - [`proxy`]: module versions from the module proxy, cached in the user cache dir, and the
//!   module paths of the module cache.

pub mod detect;
pub mod download;
pub mod proxy;
pub mod tools;

#[cfg(test)]
pub(crate) mod testserver;

pub use detect::{detect, sdk_from_folder, DetectEnv, GoSdk};
pub use download::{downloadable, Downloadable, GoFile, GoRelease};
pub use proxy::{cached_modules, ModuleProxy};
pub use tools::{install_command, GoTool, InstallCommand, ToolDirs, ToolStatus};
