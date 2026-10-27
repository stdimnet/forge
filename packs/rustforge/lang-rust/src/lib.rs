//! RustForge's Rust and Cargo support, in the style of RustRover. Pure Rust (no I/O), shared
//! by the desktop backend and the WebAssembly build of browser mode.
//!
//! - [`metadata`]: Cargo workspaces from `cargo metadata` (members, targets, features,
//!   dependency trees), module paths of files.
//! - [`toolchain`]: `rustup toolchain list`, `rustup show active-toolchain`,
//!   `rust-toolchain(.toml)`, `rustc -vV`, installed components.
//! - [`scan`]: `fn main`, tests, benchmarks, test modules and doctests of a file.
//! - [`context`]: the project's Cargo context and the location rules of test results.
//! - [`command`]: the "Cargo" run configuration, Cargo tool window actions, `fn main` run
//!   targets.
//! - [`provider`]: the Cargo Test provider (libtest JSON or plain output, nextest,
//!   cargo-llvm-cov, doctests, Rerun Failed Tests) and test binaries for the debugger.
//! - [`messages`]: cargo's JSON messages (executables, rustc / Clippy problems with fixes).
//! - [`analyzer`]: rust-analyzer's configuration.
//! - [`format`]: rustfmt arguments and config files.
//! - [`debug`]: rustc's LLDB / GDB pretty printers for the built-in debug adapters.

pub mod analyzer;
pub mod command;
pub mod context;
pub mod debug;
pub mod format;
pub mod messages;
pub mod metadata;
pub mod provider;
pub mod scan;
pub mod toolchain;

pub use analyzer::AnalyzerSettings;
pub use command::{
    action_command, command_line, run_targets, CargoCommand, RunTarget, Selection, CARGO,
};
pub use context::{CargoContext, CONTEXT_KEY};
pub use debug::PrettyPrinters;
pub use messages::{
    artifacts, build_failure, pick_executable, problems, Artifact, RustFix, RustProblem,
};
pub use metadata::{
    parse_metadata, CargoPackage, CargoTarget, CargoWorkspace, DependencyNode, TargetKind,
};
pub use provider::{debug_test, discover_file, CargoTestProvider, DebugTest, CARGO_TEST};
pub use scan::{scan, ItemKind, RustItem};
pub use toolchain::{ActiveReason, ActiveToolchain, RustcVersion, Toolchain, ToolchainFile};

/// Whether a project file belongs to Rust (sources, manifests, toolchain files).
pub fn is_rust_path(path: &str) -> bool {
    let name = path.replace('\\', "/");
    let name = name.rsplit('/').next().unwrap_or("");
    name.ends_with(".rs")
        || matches!(
            name,
            "Cargo.toml"
                | "Cargo.lock"
                | "rust-toolchain"
                | "rust-toolchain.toml"
                | "rustfmt.toml"
                | ".rustfmt.toml"
        )
}

#[cfg(test)]
mod tests {
    #[test]
    fn rust_paths() {
        assert!(super::is_rust_path("src/main.rs") && super::is_rust_path("crates/x/Cargo.toml"));
        assert!(!super::is_rust_path("Cargo.toml.orig"));
    }
}
