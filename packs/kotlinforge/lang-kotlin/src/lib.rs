//! KotlinForge's Kotlin language support, like IntelliJ IDEA's. Pure Rust (no file system or
//! processes), so the desktop backend and the browser mode (WebAssembly) share it; code
//! intelligence itself comes from JetBrains' kotlin-lsp (or fwcd's kotlin-language-server).
//!
//! - [`source`] scans a Kotlin file: package, imports, `@file:JvmName`, classes and objects
//!   with binary names, functions with backtick names, `init` blocks and supertype lambdas.
//! - [`discovery`] finds `main` functions (top-level → `FileNameKt`, `@JvmStatic` in objects
//!   and companions) and JUnit 5 / 4 / TestNG tests; [`kotest`] reads Kotest specs
//!   (FunSpec, StringSpec, DescribeSpec, BehaviorSpec).
//! - [`tests`] plugs both into the shared test runner (`forge-testing`) through Gradle or
//!   Maven with JUnit XML reports.
//! - [`launch`] builds Kotlin/JVM runs (Gradle init script, Maven `exec:java`, `kotlinc`)
//!   and Kotlin script runs; [`lsp`] the language server's launch.
//! - [`lint`] reads ktlint and detekt reports; [`coroutines`] the coroutine dumps of the
//!   debugger's Coroutines view.

pub mod coroutines;
pub mod discovery;
pub mod kotest;
pub mod launch;
pub mod lint;
pub mod lsp;
pub mod source;
pub mod tests;

pub use discovery::{discover_tests, main_functions, MainFunction};
pub use kotest::{discover_specs, KotestSpec};
pub use tests::KotlinTests;
