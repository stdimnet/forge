//! JavaForge's Maven and Gradle support, like IntelliJ IDEA's build tool integration.
//! Pure Rust: the caller hands in file names and texts, so the desktop backend, the remote
//! agent and the browser mode (WebAssembly) share it.
//!
//! - [`detect`] reads a build ([`BuildProject`]): the tool, its wrapper (`mvnw`,
//!   `gradlew`), the Kotlin or Groovy DSL, modules (`settings.gradle` `include`, `pom.xml`
//!   `<modules>`), JaCoCo, the test frameworks the build depends on, the Java release and
//!   Gradle's `application` main class.
//! - [`goals`] lists Maven lifecycle phases and Gradle tasks for the build tool window and
//!   Run Anything.
//! - [`command`] builds command lines: goals and tasks, Build Project (Ctrl+F9) and test
//!   runs (Gradle `test --tests`, Maven Surefire `-Dtest=`) with their report folders and
//!   JaCoCo reports.

pub mod command;
pub mod detect;
pub mod goals;

pub use command::{
    build_project_command, executable, goals_command, test_command, BuildCommand, ExecOptions,
    TestFilter, TestRun,
};
pub use detect::{detect, BuildProject, BuildTool, Module, TestFrameworkUse};
pub use goals::{goals, Goal};
