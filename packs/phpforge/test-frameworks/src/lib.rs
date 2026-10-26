//! PHP test frameworks like PHPStorm's: PHPUnit, Pest, Codeception, Behat and PHPSpec.
//!
//! - [`frameworks`]: Settings | PHP | Test Frameworks (`php.testFrameworks`): one
//!   configuration per framework and interpreter (Composer autoloader or a path to the
//!   executable, default configuration and bootstrap files), detected from composer.json,
//!   `vendor/bin` and the frameworks' configuration files.
//! - [`command`]: the command lines of the Pest, Codeception, Behat and PHPSpec run
//!   configurations (after the PHP interpreter) and how each reports its results (TeamCity
//!   messages or JUnit XML).
//! - [`discover`]: the tests of a file for the gutter ▶ icons and Run Context Configuration,
//!   each with its temporary run configuration.
//! - [`gherkin`]: `.feature` files (features, scenarios, outlines, examples, steps, tags).
//! - [`steps`]: Behat step definitions (`@Given /regex/`, turnip `:param` patterns,
//!   `#[Given('…')]` attributes) and which of them a step uses.
//! - [`rerun`]: the configuration that reruns only the failed tests of a run.
//! - [`provider`]: the frameworks as one provider of the shared test runner
//!   (`forge_testing::TestFrameworks`), plus Run with Coverage (`--coverage-clover`).
//!
//! Pure (no I/O), so the desktop backend runs it next to the project and browser mode runs
//! it as WebAssembly.

pub mod command;
pub mod discover;
pub mod frameworks;
pub mod gherkin;
pub mod provider;
pub mod rerun;
pub mod steps;

pub use command::{build, TestCommand};
pub use discover::{discover, TestTarget};
pub use frameworks::{detect, effective, Framework, FrameworkConfig, SETTINGS_KEY};
pub use provider::PhpTestFrameworks;
pub use rerun::{rerun_failed, FailedTest};
