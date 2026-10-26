//! Command lines of Maven and Gradle: goals and tasks, Build Project and test runs.

use serde::{Deserialize, Serialize};

use crate::detect::{BuildProject, BuildTool, Module};

/// How to run the build tool.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExecOptions {
    /// Use the wrapper (`gradlew`, `mvnw`) when the build has one (IntelliJ's default).
    pub use_wrapper: bool,
    /// The user's Maven home / Gradle executable (`mvn`, `/opt/gradle/bin/gradle`).
    pub executable: Option<String>,
    /// Absolute folder of the build (where the wrapper lives); runs start there.
    pub build_dir: String,
    pub windows: bool,
    /// Work offline (`--offline`, `-o`).
    pub offline: bool,
}

/// A command line of the build tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Working directory (the build's folder).
    pub cwd: String,
}

/// The program that runs the build: the wrapper, the configured executable, or `mvn` /
/// `gradle` from `PATH`.
pub fn executable(tool: BuildTool, project_wrapper: bool, opts: &ExecOptions) -> String {
    if project_wrapper && opts.use_wrapper {
        let script = match (tool, opts.windows) {
            (BuildTool::Gradle, false) => "gradlew",
            (BuildTool::Gradle, true) => "gradlew.bat",
            (BuildTool::Maven, false) => "mvnw",
            (BuildTool::Maven, true) => "mvnw.cmd",
        };
        let dir = opts.build_dir.trim_end_matches(['/', '\\']);
        if dir.is_empty() {
            return script.to_string();
        }
        let sep = if opts.windows { '\\' } else { '/' };
        return format!("{dir}{sep}{script}");
    }
    match opts
        .executable
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        Some(e) => e.to_string(),
        None => tool.id().replace("maven", "mvn"),
    }
}

fn base(project: &BuildProject, opts: &ExecOptions) -> BuildCommand {
    let mut args = Vec::new();
    if opts.offline {
        args.push(
            match project.tool {
                BuildTool::Gradle => "--offline",
                BuildTool::Maven => "-o",
            }
            .to_string(),
        );
    }
    BuildCommand {
        program: executable(project.tool, project.wrapper, opts),
        args,
        cwd: opts.build_dir.clone(),
    }
}

/// A Gradle task in a module (`:app:test`); tasks that already name a project stay as
/// they are.
fn gradle_task(module: Option<&Module>, task: &str) -> String {
    match module {
        Some(m) if !task.starts_with(':') && !task.starts_with('-') && !m.path.is_empty() => {
            format!("{}:{task}", m.path)
        }
        _ => task.to_string(),
    }
}

/// Maven's arguments that limit a run to a module (and what it depends on).
fn maven_module(module: Option<&Module>) -> Vec<String> {
    match module {
        Some(m) => vec!["-pl".into(), m.dir.clone(), "-am".into()],
        None => Vec::new(),
    }
}

/// Run goals (Maven) or tasks (Gradle), optionally in one module, with extra arguments
/// (`-DskipTests`, `--info`).
pub fn goals_command(
    project: &BuildProject,
    opts: &ExecOptions,
    module: Option<&Module>,
    goals: &[String],
    extra: &[String],
) -> BuildCommand {
    let mut cmd = base(project, opts);
    match project.tool {
        BuildTool::Gradle => cmd
            .args
            .extend(goals.iter().map(|g| gradle_task(module, g))),
        BuildTool::Maven => {
            cmd.args.extend(maven_module(module));
            cmd.args.extend(goals.iter().cloned());
        }
    }
    cmd.args.extend(extra.iter().cloned());
    cmd
}

/// Build Project (Ctrl+F9): compile main and test sources.
pub fn build_project_command(project: &BuildProject, opts: &ExecOptions) -> BuildCommand {
    let goals: Vec<String> = match project.tool {
        BuildTool::Gradle => vec!["classes".into(), "testClasses".into()],
        BuildTool::Maven => vec!["test-compile".into()],
    };
    goals_command(project, opts, None, &goals, &[])
}

/// Which tests a run selects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TestFilter {
    All,
    /// Every test in a package (and its subpackages).
    Package {
        name: String,
    },
    /// One class (binary name: `com.acme.CartTest$Totals`).
    Class {
        name: String,
    },
    Method {
        class: String,
        method: String,
    },
    /// Several classes or methods (Rerun Failed Tests).
    Tests {
        tests: Vec<(String, Option<String>)>,
    },
}

impl TestFilter {
    /// The selected tests as `(class, method)` pairs; the package as a `pkg.*` class.
    fn entries(&self) -> Vec<(String, Option<String>)> {
        match self {
            TestFilter::All => Vec::new(),
            TestFilter::Package { name } => vec![(format!("{name}.*"), None)],
            TestFilter::Class { name } => vec![(name.clone(), None)],
            TestFilter::Method { class, method } => vec![(class.clone(), Some(method.clone()))],
            TestFilter::Tests { tests } => tests.clone(),
        }
    }
}

/// A test run: its command line and where the build tool writes the results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRun {
    pub command: BuildCommand,
    /// JUnit XML folders relative to the build's folder.
    pub report_dirs: Vec<String>,
    /// The JaCoCo XML report relative to the build's folder (Run with Coverage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_report: Option<String>,
}

fn in_module(module: Option<&Module>, path: &str) -> String {
    match module {
        Some(m) if !m.dir.is_empty() => format!("{}/{path}", m.dir),
        _ => path.to_string(),
    }
}

/// Run tests through the build tool: Gradle `cleanTest test --tests …` (so up-to-date
/// tests run again, as IntelliJ does), Maven Surefire `test -Dtest=…`. With `coverage`,
/// JaCoCo's XML report is generated as well (`jacocoTestReport`, `jacoco:report`).
pub fn test_command(
    project: &BuildProject,
    opts: &ExecOptions,
    module: Option<&Module>,
    filter: &TestFilter,
    coverage: bool,
    extra: &[String],
) -> TestRun {
    let entries = filter.entries();
    match project.tool {
        BuildTool::Gradle => {
            let mut tasks = vec!["cleanTest".to_string(), "test".to_string()];
            if coverage {
                tasks.push("jacocoTestReport".into());
            }
            let mut cmd = goals_command(project, opts, module, &tasks, &[]);
            for (class, method) in &entries {
                cmd.args.push("--tests".into());
                cmd.args.push(match method {
                    Some(m) => format!("{class}.{m}"),
                    None => class.clone(),
                });
            }
            cmd.args.extend(extra.iter().cloned());
            TestRun {
                command: cmd,
                report_dirs: vec![in_module(module, "build/test-results/test")],
                coverage_report: coverage
                    .then(|| in_module(module, "build/reports/jacoco/test/jacocoTestReport.xml")),
            }
        }
        BuildTool::Maven => {
            let mut goals = vec!["test".to_string()];
            if coverage {
                goals.push("org.jacoco:jacoco-maven-plugin:report".into());
            }
            let mut cmd = goals_command(project, opts, module, &goals, &[]);
            if !entries.is_empty() {
                let list: Vec<String> = entries
                    .iter()
                    .map(|(class, method)| {
                        let class = match class.strip_suffix(".*") {
                            // Surefire: packages as paths with `**`.
                            Some(pkg) => format!("{}/**/*", pkg.replace('.', "/")),
                            None => class.clone(),
                        };
                        match method {
                            Some(m) => format!("{class}#{m}"),
                            None => class,
                        }
                    })
                    .collect();
                cmd.args.push(format!("-Dtest={}", list.join(",")));
                cmd.args
                    .push("-Dsurefire.failIfNoSpecifiedTests=false".into());
            }
            cmd.args.extend(extra.iter().cloned());
            TestRun {
                command: cmd,
                report_dirs: vec![in_module(module, "target/surefire-reports")],
                coverage_report: coverage
                    .then(|| in_module(module, "target/site/jacoco/jacoco.xml")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(tool: BuildTool, wrapper: bool) -> BuildProject {
        BuildProject {
            tool,
            name: "shop".into(),
            wrapper,
            build_file: String::new(),
            settings_file: None,
            kotlin_dsl: false,
            modules: vec![Module {
                name: "app".into(),
                dir: "app".into(),
                path: if tool == BuildTool::Gradle {
                    ":app".into()
                } else {
                    String::new()
                },
            }],
            jacoco: true,
            application: false,
            main_class: None,
            spring_boot: false,
            custom_tasks: vec![],
            java_version: None,
            test_frameworks: vec![],
        }
    }

    fn opts(windows: bool) -> ExecOptions {
        ExecOptions {
            use_wrapper: true,
            executable: None,
            build_dir: if windows { "C:\\p".into() } else { "/p".into() },
            windows,
            offline: false,
        }
    }

    #[test]
    fn wrapper_executable_or_path_tool() {
        let o = opts(false);
        assert_eq!(executable(BuildTool::Gradle, true, &o), "/p/gradlew");
        assert_eq!(
            executable(BuildTool::Maven, true, &opts(true)),
            "C:\\p\\mvnw.cmd"
        );
        assert_eq!(executable(BuildTool::Maven, false, &o), "mvn");
        let no_wrapper = ExecOptions {
            use_wrapper: false,
            executable: Some("/opt/gradle/bin/gradle".into()),
            ..o
        };
        assert_eq!(
            executable(BuildTool::Gradle, true, &no_wrapper),
            "/opt/gradle/bin/gradle"
        );
    }

    #[test]
    fn goals_and_build_project() {
        let g = project(BuildTool::Gradle, true);
        let m = g.modules.first();
        let cmd = goals_command(
            &g,
            &opts(false),
            m,
            &["build".into(), ":other:x".into()],
            &["--info".into()],
        );
        assert_eq!(cmd.args, [":app:build", ":other:x", "--info"]);
        assert_eq!(cmd.cwd, "/p");
        assert_eq!(
            build_project_command(&g, &opts(false)).args,
            ["classes", "testClasses"]
        );
        let mvn = project(BuildTool::Maven, false);
        let offline = ExecOptions {
            offline: true,
            ..opts(false)
        };
        let cmd = goals_command(
            &mvn,
            &offline,
            mvn.modules.first(),
            &["package".into()],
            &[],
        );
        assert_eq!(cmd.program, "mvn");
        assert_eq!(cmd.args, ["-o", "-pl", "app", "-am", "package"]);
        assert_eq!(
            build_project_command(&mvn, &opts(false)).args,
            ["test-compile"]
        );
    }

    #[test]
    fn gradle_tests_with_filters_and_coverage() {
        let g = project(BuildTool::Gradle, true);
        let run = test_command(
            &g,
            &opts(false),
            g.modules.first(),
            &TestFilter::Method {
                class: "com.acme.CartTest".into(),
                method: "adds".into(),
            },
            true,
            &[],
        );
        assert_eq!(
            run.command.args,
            [
                ":app:cleanTest",
                ":app:test",
                ":app:jacocoTestReport",
                "--tests",
                "com.acme.CartTest.adds"
            ]
        );
        assert_eq!(run.report_dirs, ["app/build/test-results/test"]);
        assert_eq!(
            run.coverage_report.as_deref(),
            Some("app/build/reports/jacoco/test/jacocoTestReport.xml")
        );
        let all = test_command(
            &g,
            &opts(false),
            None,
            &TestFilter::Package {
                name: "com.acme".into(),
            },
            false,
            &[],
        );
        assert_eq!(
            all.command.args,
            ["cleanTest", "test", "--tests", "com.acme.*"]
        );
        assert_eq!(all.coverage_report, None);
    }

    #[test]
    fn maven_tests_with_filters_and_coverage() {
        let m = project(BuildTool::Maven, true);
        let run = test_command(
            &m,
            &opts(false),
            None,
            &TestFilter::Tests {
                tests: vec![
                    ("com.acme.CartTest".into(), Some("adds".into())),
                    ("com.acme.Outer$Inner".into(), None),
                ],
            },
            true,
            &["-X".into()],
        );
        assert_eq!(run.command.program, "/p/mvnw");
        assert_eq!(
            run.command.args,
            [
                "test",
                "org.jacoco:jacoco-maven-plugin:report",
                "-Dtest=com.acme.CartTest#adds,com.acme.Outer$Inner",
                "-Dsurefire.failIfNoSpecifiedTests=false",
                "-X"
            ]
        );
        assert_eq!(run.report_dirs, ["target/surefire-reports"]);
        assert_eq!(
            run.coverage_report.as_deref(),
            Some("target/site/jacoco/jacoco.xml")
        );
        let pkg = test_command(
            &m,
            &opts(false),
            m.modules.first(),
            &TestFilter::Package {
                name: "com.acme".into(),
            },
            false,
            &[],
        );
        assert_eq!(pkg.command.args[..4], ["-pl", "app", "-am", "test"]);
        assert_eq!(pkg.command.args[4], "-Dtest=com/acme/**/*");
        assert_eq!(pkg.report_dirs, ["app/target/surefire-reports"]);
    }
}
