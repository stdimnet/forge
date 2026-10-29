//! The contribution point of the shared test runner: a test framework provider owns run
//! configuration types and answers what the Run tool window, the gutter and Rerun Failed
//! Tests need. Manifest-declared frameworks ([`crate::spec::SpecProvider`]) and code-backed
//! ones (the PHP frameworks of `phpforge-test-frameworks`) register in one
//! [`TestFrameworks`] registry.

use std::collections::BTreeMap;

use forge_output::{JUnitFlavor, LocationRule, TestReport};
use forge_runtools::Configuration;
use serde::{Deserialize, Serialize};

use crate::coverage::CoverageFormat;
use crate::spec::{SpecProvider, TestFrameworkSpec};
use crate::targets::{FailedTest, TestFile};

/// What building a test command may use besides the configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandContext {
    /// Folder for report files (`${reportDir}`).
    pub report_dir: String,
    /// Run with Coverage: where the coverage report goes (`${coverageFile}`).
    pub coverage_file: Option<String>,
    /// Setting values the provider asked for (`programSetting`), as strings.
    pub settings: BTreeMap<String, String>,
}

impl CommandContext {
    pub fn setting(&self, key: Option<&str>) -> Option<String> {
        self.settings.get(key?).cloned()
    }
}

/// Where Run with Coverage finds the report and how to read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageRequest {
    /// Path of the report: absolute or relative to the working directory.
    pub file: String,
    pub format: CoverageFormat,
    /// Rewrites of the report's paths when they differ from the test locations' (JaCoCo
    /// reports name main sources, test locations test sources); empty: the command's
    /// `location_rules`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub location_rules: Vec<LocationRule>,
    /// Raw coverage data that a pack's collector turns into `file` after the run (CForge:
    /// gcov's `.gcda` files or LLVM's `.profraw` profiles become LCOV); `None`: the test
    /// tool writes the report itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collect: Option<CoverageCollect>,
}

/// What a coverage collector needs after the run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageCollect {
    /// The collector (`cpp`).
    pub kind: String,
    /// Folders with raw data (the build folder with `.gcda` files), absolute.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dirs: Vec<String>,
    /// The programs that ran (llvm-cov reads their coverage mapping), absolute.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binaries: Vec<String>,
    /// Tools by name (`gcov`, `llvm-profdata`, `llvm-cov`) → executable.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, String>,
}

/// A test run's command line and how its results are read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCommand {
    pub program: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    pub report: TestReport,
    pub flavor: JUnitFlavor,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub location_rules: Vec<LocationRule>,
    /// The file a single-file run tests: tests reported without a location belong to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageRequest>,
    /// Folders (relative to the project) where the build tool writes its JUnit XML reports
    /// itself (Gradle `build/test-results/test`, Maven `target/surefire-reports`), with
    /// [`TestReport::JUnitFile`]: they are emptied before the run and read (recursively)
    /// after it, instead of a report folder Forge creates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub report_dirs: Vec<String>,
}

/// A test framework (or a family of them) as the shared test runner sees it.
pub trait TestFrameworkProvider: Send + Sync {
    /// Framework id (`busted`, `php`).
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// The run configuration types the provider builds, discovers and reruns.
    fn types(&self) -> Vec<String>;
    /// Whether a project file may hold tests of the provider (cheap, by its path).
    fn accepts(&self, path: &str) -> bool;
    /// The tests of a file for the gutter ▶ icons; `None` when it has none.
    fn discover(&self, path: &str, text: &str) -> Option<TestFile>;
    /// The configuration that runs only the failed tests of a run.
    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration>;
    /// The command of a configuration. `None`: the provider's pack builds its commands
    /// itself (the PHP frameworks run through the PHP CLI interpreter).
    fn command(
        &self,
        _cfg: &Configuration,
        _ctx: &CommandContext,
    ) -> Option<Result<TestCommand, String>> {
        None
    }
    /// Whether configurations of `type_id` can run with coverage.
    fn supports_coverage(&self, _type_id: &str) -> bool {
        false
    }
}

/// What the UI lists about a registered framework.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkInfo {
    pub id: String,
    pub name: String,
    pub types: Vec<String>,
    pub coverage: bool,
}

/// The registered test framework providers, in registration order.
#[derive(Default)]
pub struct TestFrameworks {
    providers: Vec<Box<dyn TestFrameworkProvider>>,
}

impl std::fmt::Debug for TestFrameworks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.providers.iter().map(|p| p.id()))
            .finish()
    }
}

impl TestFrameworks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a provider; a later one with the same id replaces the earlier.
    pub fn register(&mut self, provider: Box<dyn TestFrameworkProvider>) {
        self.providers.retain(|p| p.id() != provider.id());
        self.providers.push(provider);
    }

    /// Add the frameworks declared by manifests.
    pub fn register_specs<'a>(&mut self, specs: impl IntoIterator<Item = &'a TestFrameworkSpec>) {
        for s in specs {
            self.register(Box::new(SpecProvider(s.clone())));
        }
    }

    pub fn providers(&self) -> impl Iterator<Item = &dyn TestFrameworkProvider> {
        self.providers.iter().map(|p| p.as_ref())
    }

    pub fn info(&self) -> Vec<FrameworkInfo> {
        self.providers
            .iter()
            .map(|p| {
                let types = p.types();
                FrameworkInfo {
                    id: p.id().to_string(),
                    name: p.name().to_string(),
                    coverage: types.iter().any(|t| p.supports_coverage(t)),
                    types,
                }
            })
            .collect()
    }

    /// The provider of a run configuration type.
    pub fn for_type(&self, type_id: &str) -> Option<&dyn TestFrameworkProvider> {
        self.providers()
            .find(|p| p.types().iter().any(|t| t == type_id))
    }

    pub fn is_test_type(&self, type_id: &str) -> bool {
        self.for_type(type_id).is_some()
    }

    /// The tests of a file: the first provider that finds some. `active` limits the
    /// providers (the frameworks the project uses); `None` asks every provider.
    pub fn discover(&self, path: &str, text: &str, active: Option<&[String]>) -> TestFile {
        self.providers()
            .filter(|p| active.is_none_or(|a| a.iter().any(|id| id == p.id())))
            .filter(|p| p.accepts(path))
            .find_map(|p| p.discover(path, text))
            .unwrap_or_default()
    }

    /// The tests of a file in a project that uses the frameworks `detected` (ids). When a
    /// detected framework claims the file, frameworks with detection rules (`detectable`)
    /// that the project does not use are skipped, so a `*.test.ts` file belongs to Vitest
    /// in a Vitest project and to Jest in a Jest project; otherwise, or when those find no
    /// tests, every framework is asked.
    pub fn discover_in_project(
        &self,
        path: &str,
        text: &str,
        detected: &[String],
        detectable: &[String],
    ) -> TestFile {
        let claimed = self
            .providers()
            .any(|p| detected.iter().any(|d| d == p.id()) && p.accepts(path));
        if !claimed {
            return self.discover(path, text, None);
        }
        let active: Vec<String> = self
            .providers()
            .map(|p| p.id().to_string())
            .filter(|id| detected.contains(id) || !detectable.contains(id))
            .collect();
        let found = self.discover(path, text, Some(&active));
        if found.is_empty() {
            // The file names another framework (imports it): ask every framework.
            return self.discover(path, text, None);
        }
        found
    }

    pub fn rerun_failed(
        &self,
        cfg: &Configuration,
        failed: &[FailedTest],
    ) -> Option<Configuration> {
        self.for_type(&cfg.type_id)?.rerun_failed(cfg, failed)
    }

    /// The command of a test configuration whose provider builds it.
    pub fn command(
        &self,
        cfg: &Configuration,
        ctx: &CommandContext,
    ) -> Option<Result<TestCommand, String>> {
        self.for_type(&cfg.type_id)?.command(cfg, ctx)
    }

    pub fn supports_coverage(&self, type_id: &str) -> bool {
        self.for_type(type_id)
            .is_some_and(|p| p.supports_coverage(type_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::targets::{TargetKind, TestTarget};

    struct Fake;

    impl TestFrameworkProvider for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn name(&self) -> &str {
            "Fake"
        }
        fn types(&self) -> Vec<String> {
            vec!["fake-a".into(), "fake-b".into()]
        }
        fn accepts(&self, path: &str) -> bool {
            path.ends_with(".fake")
        }
        fn discover(&self, path: &str, _text: &str) -> Option<TestFile> {
            Some(TestFile {
                framework: Some("fake".into()),
                targets: vec![TestTarget {
                    line: 0,
                    kind: TargetKind::Test,
                    label: "t".into(),
                    key: format!("{path}::t"),
                    configuration: Configuration::new("t", "fake-a"),
                }],
                file: None,
            })
        }
        fn rerun_failed(
            &self,
            cfg: &Configuration,
            failed: &[FailedTest],
        ) -> Option<Configuration> {
            (!failed.is_empty()).then(|| cfg.clone().with_option("only", &failed[0].name))
        }
        fn supports_coverage(&self, type_id: &str) -> bool {
            type_id == "fake-a"
        }
    }

    #[test]
    fn routes_types_files_and_reruns_to_their_provider() {
        let mut r = TestFrameworks::new();
        r.register(Box::new(Fake));
        r.register_specs([&crate::spec::tests::busted()]);
        assert_eq!(r.for_type("fake-b").map(|p| p.id()), Some("fake"));
        assert_eq!(r.for_type("busted").map(|p| p.id()), Some("busted"));
        assert!(r.for_type("phpunit").is_none());
        assert_eq!(
            r.discover("x.fake", "", None).framework.as_deref(),
            Some("fake")
        );
        assert!(r
            .discover("x.fake", "", Some(&["busted".into()]))
            .is_empty());
        assert!(r.discover("x.txt", "", None).is_empty());
        let cfg = Configuration::new("c", "fake-b");
        let failed = [FailedTest {
            name: "boom".into(),
            ..FailedTest::default()
        }];
        assert_eq!(
            r.rerun_failed(&cfg, &failed).unwrap().option("only"),
            "boom"
        );
        assert!(r.command(&cfg, &CommandContext::default()).is_none());
        assert!(r.supports_coverage("fake-a") && !r.supports_coverage("fake-b"));
        assert!(r.supports_coverage("busted"));
        let info = r.info();
        assert_eq!(info[1].types, ["busted"]);
    }

    #[test]
    fn project_detection_picks_between_frameworks_of_the_same_files() {
        let a = crate::spec::tests::busted();
        let mut b = a.clone();
        b.id = "other".into();
        let mut r = TestFrameworks::new();
        r.register(Box::new(Fake));
        r.register_specs([&a, &b]);
        let text = "describe(\"Cart\", function()\n  it(\"adds\", function() end)\nend)\n";
        let ids = ["busted".to_string(), "other".to_string()];
        let path = "spec/cart_spec.lua";
        let found =
            |detected: &[String]| r.discover_in_project(path, text, detected, &ids).framework;
        assert_eq!(found(&["other".into()]).as_deref(), Some("other"));
        assert_eq!(found(&["busted".into()]).as_deref(), Some("busted"));
        assert_eq!(
            found(&[]).as_deref(),
            Some("busted"),
            "nothing detected: the first"
        );
        assert_eq!(
            r.discover_in_project("x.fake", "", &["other".into()], &ids)
                .framework
                .as_deref(),
            Some("fake"),
            "providers without detection stay"
        );
    }
}
