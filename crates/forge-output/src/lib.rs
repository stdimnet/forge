//! Process output for the Run tool window and the terminal, computed in Rust so the UI only
//! renders it.
//!
//! - [`ansi`]: ANSI escape sequences (SGR colors and styles) into styled segments.
//! - [`console`]: segments into console lines with `\r` progress lines, as deltas the UI
//!   applies to its line list.
//! - [`filters`]: External Tools output filters (`$FILE_PATH$:$LINE$`) as file links.
//! - [`links`]: clickable file references (`app/User.php:42`, stack traces) and URLs.
//! - [`teamcity`]: TeamCity service messages (`phpunit --teamcity`, Pest, Jest) into a test
//!   tree that is sent to the UI as node updates.
//! - [`junit`]: JUnit XML reports (Codeception, Behat, PHPSpec, pytest) as service messages
//!   for the same test tree.
//! - [`testevents`]: the test event protocol every test runner format is turned into.
//! - [`formats`]: parsers for TAP, Go `test2json`, libtest JSON, Jest / Vitest JSON and
//!   pytest report logs, and report sniffing.
//! - [`run`]: everything above combined for one run.
//! - [`stacktrace`]: Analyze External Stack Trace (frames linked to project files).
//! - [`osc`]: terminal OSC sequences (progress, notifications, working directory) and OSC 8
//!   link targets.
//!
//! The crate is pure (no I/O) so `forge-wasm` can run it in browser mode.

pub mod ansi;
pub mod console;
pub mod filters;
pub mod formats;
pub mod junit;
pub mod links;
pub mod osc;
pub mod run;
pub mod stacktrace;
pub mod teamcity;
pub mod testevents;
mod utf16;

pub use ansi::{AnsiParser, Segment, Style};
pub use console::{ConsoleDelta, ConsoleLine, ConsoleModel, Span};
pub use filters::{compile_filters, FilterMatch, OutputFilter, FILTER_MACROS};
pub use junit::{junit_messages, JUnitFlavor};
pub use links::{
    file_links_utf16, find_file_links, find_links, to_workspace_path, FileLink, LinkContext,
    OutputPathMapping,
};
pub use osc::{link_target, parse_osc, LinkTarget, OscAction, Progress, ProgressState};
pub use run::{OutputEvent, RunInput, RunOutput, RunOutputOptions, TestReport};
pub use teamcity::{
    format_duration, parse_location_hint, parse_service_message, LocationRule, ServiceMessage,
    StoredNode, TestCounts, TestLocation, TestNode, TestNodeKind, TestStatus, TestSummary,
    TestTree,
};
pub use testevents::{LineParser, ParsedLine, TestEvent};
pub use utf16::{byte_to_utf16, utf16_to_byte};
