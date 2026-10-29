//! WebForge's Node.js test runners on the shared test runner (`forge-testing`): Jest,
//! Vitest, Mocha, Playwright Test, Cypress and `node --test`.
//!
//! - [`discovery`]: the `describe` / `it` / `test` calls of a file for the gutter ▶ icons,
//!   nested by the call they are written in.
//! - [`runner`]: [`JsTestRunner`] wraps a runner's manifest spec
//!   (`[[contributions.testFrameworks]]` in WebForge's manifest) with the Node.js specifics:
//!   the CLI script run with Node.js, Playwright's JSON report file, Mocha coverage with
//!   c8, Rerun Failed Tests limited to the failed files, and the program Debug starts with
//!   js-debug.
//!
//! Pure (no I/O): the desktop backend and the WebAssembly build share it.

pub mod discovery;
pub mod runner;

pub use runner::{coverage_wrap, register, CoverageTool, JsTestRunner, NodeProgram, Runner};

#[cfg(test)]
mod tests;
