//! JavaForge's Java language support, like IntelliJ IDEA's. Pure Rust (no file system or
//! processes), so the desktop backend and the browser mode (WebAssembly) share it; code
//! intelligence itself comes from Eclipse JDT LS.
//!
//! - [`source`] scans a Java file: package, imports, types (nested ones with binary names)
//!   and methods with annotations and modifiers, tolerating half-typed code.
//! - [`discovery`] finds `main` methods (Run gutter icons, also Java 25 instance mains and
//!   compact source files) and tests: JUnit 5 (`@Test`, `@ParameterizedTest`, `@Nested`,
//!   …), JUnit 4 and 3, TestNG (method and class level `@Test`).
//! - [`tests`] plugs JUnit and TestNG into the shared test runner (`forge-testing`): gutter
//!   targets, Rerun Failed Tests and command lines through Gradle or Maven with their JUnit
//!   XML reports and JaCoCo coverage.
//! - [`launch`] builds "Application" command lines and the JDWP arguments of debug runs.
//! - [`jdtls`] describes how JDT LS starts (command line, initialization options with the
//!   java-debug bundle, `java.*` settings).

pub mod discovery;
pub mod jdtls;
pub mod launch;
pub mod source;
pub mod tests;

pub use discovery::{discover_tests, main_methods, Framework, MainMethod, TestDiscovery};
pub use tests::JavaTests;
