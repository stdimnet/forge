//! Forge's inspection framework, as in PHPStorm (Settings | Editor | Inspections, Code |
//! Inspect Code, Run Inspection by Name, Code Cleanup).
//!
//! Every problem Forge shows belongs to an inspection: Forge's own checks (the PHP index
//! inspections, PHP data flow, duplicated code, requirements, Spring properties) as well as
//! the problems of external tools (language servers, Clippy, go vet, golangci-lint, ktlint,
//! detekt, ...), which [`ToolMapping`]s map onto inspection ids.
//!
//! - [`severity`]: Error, Warning, Weak Warning, Server Problem, Typo, Info and custom
//!   severities.
//! - [`spec`]: what a pack contributes (`[[contributions.inspections]]`,
//!   `[[contributions.inspectionMappings]]` in its manifest).
//! - [`registry`]: the inspections of the enabled plugins, the Settings tree and the lookup
//!   of a tool's problem.
//! - [`profile`]: profiles (Default, Project Default, copies), what an inspection does for
//!   a file (enabled, severity, scope-specific severities, options).
//! - [`store`]: where profiles live (user settings, `.forge/inspectionProfiles/*.xml`).
//! - [`io`]: PHPStorm profile XML and Forge JSON.
//! - [`suppress`]: `@noinspection` / `noinspection` comments and the Suppress actions.
//! - [`duplicates`]: the token-based duplicate code detector.
//! - [`engine`]: providers (file, project and external-tool problems) and the batch run
//!   with progress and cancel; [`report`]: the result tree and HTML / XML / JSON export;
//!   [`cleanup`]: applying quick fixes (Code Cleanup).
//!
//! The crate is pure (no file system), so the desktop app, the command-line inspector and
//! the browser-mode WebAssembly build share it.

pub mod cleanup;
pub mod duplicates;
pub mod engine;
pub mod io;
pub mod lang;
pub mod problem;
pub mod profile;
pub mod registry;
pub mod report;
pub mod severity;
pub mod spec;
pub mod stmts;
pub mod store;
pub mod suppress;
pub mod text;

pub use engine::{
    run, CancelFlag, DuplicatesProvider, ExternalProblem, FileInput, InspectionProvider,
    ProblemSource, RunContext, RunOptions, RunProgress,
};
pub use problem::{FileEdit, Fix, Location, Problem};
pub use profile::{Effective, Profile, ProfileLevel, ProfilesState, ResolvedProfile, ToolSettings};
pub use registry::Registry;
pub use report::{Grouping, Report, ReportProblem};
pub use severity::{EditorSeverity, SeverityDef};
pub use spec::{InspectionSpec, OptionKind, OptionSpec, ToolMapping};
