//! JavaForge's Groovy basics, like IntelliJ IDEA's Groovy plugin. Pure Rust (no file system
//! or processes), so the desktop backend and the browser mode (WebAssembly) share it.
//!
//! - [`source`] scans a Groovy file: package, classes with their supertypes, methods (also
//!   Spock's string-named features and their block labels), `main` methods and whether
//!   the file is a script.
//! - [`spock`] reads Spock specifications from it; [`tests`] plugs them into the shared
//!   test runner (`forge-testing`) through Gradle or Maven with JUnit XML reports.
//! - [`launch`] builds Groovy script runs (`groovy` from the Groovy SDK, or the project's
//!   Gradle build with an init script) and finds the Groovy SDK (GROOVY_HOME, SDKMAN!).

pub mod launch;
pub mod source;
pub mod spock;
pub mod tests;

pub use launch::{runnable, Runnable};
pub use spock::{specs, Spec};
pub use tests::SpockTests;
