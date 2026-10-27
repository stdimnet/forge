//! The shared test runner and coverage model of every pack.
//!
//! - [`targets`]: the tests of a file (gutter ▶ icons, Run Context Configuration) and the
//!   failed tests of a run.
//! - [`provider`]: the contribution point. A [`TestFrameworkProvider`] owns run
//!   configuration types and answers discovery, command lines and Rerun Failed Tests;
//!   [`TestFrameworks`] is the registry the backend asks.
//! - [`spec`]: frameworks declared as manifest data (`[[contributions.testFrameworks]]`):
//!   detection, test file globs, line-pattern discovery, per-scope arguments, the result
//!   format, location rules, rerun and coverage.
//! - [`coverage`]: coverage reports (LCOV, Cobertura, Clover, JaCoCo, Go coverprofile), the
//!   Coverage tool window's summary and the editor's line marks.
//!
//! Test results themselves (the event protocol, the output parsers and the test tree) live
//! in `forge-output` ([`forge_output::testevents`], [`forge_output::formats`]).
//!
//! Pure (no I/O): the desktop backend and the WebAssembly build share it.

pub mod coverage;
pub mod provider;
pub mod spec;
pub mod targets;

pub use coverage::{CoverageFormat, CoverageNode, CoverageReport, LineMark, LineStatus};
pub use provider::{
    CommandContext, CoverageCollect, CoverageRequest, FrameworkInfo, TestCommand, TestFrameworkProvider,
    TestFrameworks,
};
pub use spec::{FieldOption, SpecField, SpecProvider, TestFrameworkSpec};
pub use targets::{FailedTest, TargetKind, TestFile, TestTarget};
