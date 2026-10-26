//! The tests of a file as the gutter ▶ icons and Run Context Configuration see them, and
//! the failed tests Rerun Failed Tests receives. Shared by every test framework provider
//! (PHP frameworks, manifest-declared frameworks).

use forge_runtools::Configuration;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetKind {
    Class,
    Method,
    File,
    Describe,
    Test,
    Dataset,
    Feature,
    Scenario,
    Example,
    /// A package or module of tests (Go packages, Rust modules).
    Suite,
}

impl TargetKind {
    /// Whether the target groups other tests (its gutter icon says "Run Tests").
    pub fn is_group(self) -> bool {
        matches!(
            self,
            TargetKind::Class
                | TargetKind::File
                | TargetKind::Describe
                | TargetKind::Feature
                | TargetKind::Suite
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestTarget {
    /// 0-based line of the declaration.
    pub line: u32,
    pub kind: TargetKind,
    /// Shown as `Run '<label>'`.
    pub label: String,
    /// Result key (empty: no result is shown for it): `<path>` for files and classes,
    /// `<path>::<test>` for tests, like the result keys of a finished test tree.
    pub key: String,
    pub configuration: Configuration,
}

/// The tests of a file and the configuration that runs the whole file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestFile {
    /// Id of the framework (`phpunit`, `busted`, …) that found the tests.
    pub framework: Option<String>,
    pub targets: Vec<TestTarget>,
    /// Runs every test of the file (Run Context Configuration outside a test).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<Configuration>,
}

impl TestFile {
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty() && self.file.is_none()
    }
}

/// A failed test of the tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FailedTest {
    pub name: String,
    /// Method or member of its location.
    pub member: Option<String>,
    /// Workspace-relative file.
    pub path: Option<String>,
}
