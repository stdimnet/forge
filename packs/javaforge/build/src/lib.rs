//! JavaForge's Maven and Gradle support, like IntelliJ IDEA's build tool integration.
//! Pure Rust: the caller hands in file names and texts, so the desktop backend, the remote
//! agent and the browser mode (WebAssembly) share it.
//!
//! - [`detect`] reads a build ([`BuildProject`]): the tool, its wrapper (`mvnw`,
//!   `gradlew`), the Kotlin or Groovy DSL, modules (`settings.gradle` `include`, `pom.xml`
//!   `<modules>`), JaCoCo, the test frameworks the build depends on, the Java release,
//!   Gradle's `application` main class and the wrapper's Gradle version.
//! - [`goals`] lists Maven lifecycle phases and Gradle tasks for the build tool window and
//!   Run Anything.
//! - [`command`] builds command lines: goals and tasks (offline, Maven profiles and Skip
//!   Tests), Build Project (Ctrl+F9), test runs (Gradle `test --tests`, Maven Surefire
//!   `-Dtest=`) with their report folders and JaCoCo reports, Application runs through
//!   Gradle (`forgeRun`) and the syncs.
//! - [`model`] is the Gradle / Maven tool window's model: projects, tasks, Maven plugins and
//!   profiles, dependency trees from the Gradle sync's init script and `mvn
//!   dependency:tree`.
//! - [`wrapper`] reads and updates `gradle-wrapper.properties` (`distributionSha256Sum`).
//! - [`deps`] finds declared dependencies and the "newer version available" inspection,
//!   [`version`] orders Maven versions.
//! - [`dsl`] completes Gradle Kotlin / Groovy DSL scripts and `pom.xml` coordinates;
//!   [`central`] has Maven Central's URLs and parsers, `net` (feature `net`) downloads and
//!   caches them and asks services.gradle.org about releases.

pub mod central;
pub mod command;
pub mod deps;
pub mod detect;
pub mod dsl;
pub mod goals;
pub mod model;
#[cfg(feature = "net")]
pub mod net;
pub mod version;
pub mod wrapper;

#[cfg(all(test, feature = "net"))]
pub(crate) mod testserver;

pub use command::{
    build_project_command, executable, goals_command, test_command, BuildCommand, ExecOptions,
    TestFilter, TestRun,
};
pub use detect::{detect, BuildProject, BuildTool, Module, TestFrameworkUse};
pub use goals::{goals, Goal};
