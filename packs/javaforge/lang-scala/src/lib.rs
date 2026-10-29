//! JavaForge's Scala basics, like IntelliJ IDEA's Scala plugin. Pure Rust (no file system or
//! processes), so the desktop backend and the browser mode (WebAssembly) share it; code
//! intelligence comes from Metals.
//!
//! - [`source`] scans a Scala file: packages, objects, classes and traits, `main` methods,
//!   `App` objects and Scala 3 `@main` methods.
//! - [`suites`] finds ScalaTest suites (FunSuite, FlatSpec, WordSpec, FunSpec, FreeSpec) and
//!   MUnit suites with their tests; [`tests`] plugs them into the shared test runner
//!   (`forge-testing`) through sbt with JUnit XML reports.
//! - [`sbt`] reads sbt builds (`build.sbt`, `project/build.properties`, sbt's own output)
//!   and builds sbt command lines.
//! - [`metals`] has the Metals version Forge installs with Coursier and its options.

pub mod metals;
pub mod sbt;
pub mod source;
pub mod suites;
pub mod tests;

pub use sbt::{SbtBuild, SbtProject};
pub use source::{main_objects, MainObject};
pub use tests::ScalaTests;
