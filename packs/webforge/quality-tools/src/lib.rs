//! WebForge code quality tools (Settings | Languages & Frameworks | Code Quality Tools, like
//! PHPStorm's JavaScript | Code Quality Tools and Prettier): ESLint, Prettier, Stylelint and
//! Biome.
//!
//! - [`model`]: the tools and one configuration per tool (`web.<id>.*`: Disabled /
//!   Automatic / Manual, package, configuration file, options, working directories, "Run
//!   for files", on save, on Reformat Code), Biome's priority and the Node.js interpreter.
//! - [`detect`]: which tools apply to a file: the nearest `node_modules/<package>` and the
//!   nearest configuration file (automatic) or the configured ones (manual); Biome takes
//!   over ESLint and Prettier for its files when it applies.
//! - [`command`]: the command lines (`node <package bin> …`) to check, fix and format the
//!   editor text through stdin, and to fix the saved file on save.
//! - [`parse`]: their JSON reports as [`Problem`]s with project paths.
//!
//! The crate is pure: the app runs the commands through the project backend, so remote
//! projects run the tools on their host.

pub mod command;
pub mod detect;
pub mod model;
pub mod parse;
pub mod problem;

pub use command::Invocation;
pub use detect::{applicable, formatter, locate, locate_all, Launcher, Located, ProjectFs, Skip};
pub use model::{Mode, ToolConfig, WebTool, WebToolsSettings};
pub use parse::{parse_report, FileOutput, Report};
pub use problem::{PathContext, Problem, Severity};

/// Exit codes after which a tool's output is its result (not a crash).
pub fn exit_ok(tool: WebTool, code: i32) -> bool {
    match tool {
        // 1: problems found (ESLint, Biome); Stylelint uses 2 for problems.
        WebTool::Eslint | WebTool::Biome => code == 0 || code == 1,
        WebTool::Stylelint => code == 0 || code == 2,
        WebTool::Prettier => code == 0,
    }
}
