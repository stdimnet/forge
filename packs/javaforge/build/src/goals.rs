//! Maven lifecycle phases and Gradle tasks, grouped like IntelliJ's Maven and Gradle tool
//! windows (Lifecycle; build, verification, application, documentation, help, other).

use serde::{Deserialize, Serialize};

use crate::detect::{BuildProject, BuildTool};

/// A goal or task the build tool window lists and Run Anything offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    /// `test`, `clean`, `bootRun`.
    pub name: String,
    /// `Lifecycle`, `Plugins`, `build`, `verification`, `application`, …
    pub group: String,
    pub description: String,
}

fn goal(name: &str, group: &str, description: &str) -> Goal {
    Goal {
        name: name.into(),
        group: group.into(),
        description: description.into(),
    }
}

const MAVEN_LIFECYCLE: [(&str, &str); 9] = [
    ("clean", "Removes the files of the previous build"),
    ("validate", "Validates that the project is correct"),
    ("compile", "Compiles the sources"),
    ("test", "Runs the unit tests"),
    ("package", "Packages the compiled code (JAR, WAR)"),
    ("verify", "Runs integration tests and checks"),
    ("install", "Installs the package into the local repository"),
    ("site", "Generates the project's site"),
    ("deploy", "Copies the package to the remote repository"),
];

const GRADLE_TASKS: [(&str, &str, &str); 12] = [
    ("assemble", "build", "Assembles the outputs of this project"),
    ("build", "build", "Assembles and tests this project"),
    ("classes", "build", "Assembles main classes"),
    ("clean", "build", "Deletes the build directory"),
    (
        "jar",
        "build",
        "Assembles a jar archive containing the classes of the 'main' feature",
    ),
    ("testClasses", "build", "Assembles test classes"),
    ("check", "verification", "Runs all checks"),
    ("test", "verification", "Runs the test suite"),
    (
        "javadoc",
        "documentation",
        "Generates Javadoc API documentation for the 'main' feature",
    ),
    (
        "dependencies",
        "help",
        "Displays all dependencies declared in the project",
    ),
    (
        "properties",
        "help",
        "Displays the properties of the project",
    ),
    (
        "tasks",
        "help",
        "Displays the tasks runnable from the project",
    ),
];

/// The goals (Maven) or tasks (Gradle) of a build.
pub fn goals(project: &BuildProject) -> Vec<Goal> {
    let mut out = Vec::new();
    match project.tool {
        BuildTool::Maven => {
            for (name, d) in MAVEN_LIFECYCLE {
                out.push(goal(name, "Lifecycle", d));
            }
            if project.jacoco {
                out.push(goal(
                    "jacoco:report",
                    "Plugins",
                    "Generates the JaCoCo coverage report",
                ));
            }
            if project.spring_boot {
                out.push(goal(
                    "spring-boot:run",
                    "Plugins",
                    "Runs the Spring Boot application",
                ));
            }
        }
        BuildTool::Gradle => {
            for (name, group, d) in GRADLE_TASKS {
                out.push(goal(name, group, d));
            }
            if project.application {
                out.push(goal(
                    "run",
                    "application",
                    "Runs this project as a JVM application",
                ));
            }
            if project.spring_boot {
                out.push(goal(
                    "bootRun",
                    "application",
                    "Runs this project as a Spring Boot application",
                ));
                out.push(goal(
                    "bootJar",
                    "build",
                    "Assembles an executable jar archive",
                ));
            }
            if project.jacoco {
                out.push(goal(
                    "jacocoTestReport",
                    "verification",
                    "Generates code coverage report for the test task",
                ));
            }
            for t in &project.custom_tasks {
                if !out.iter().any(|g| &g.name == t) {
                    out.push(goal(t, "other", ""));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradle_lists_plugin_and_custom_tasks() {
        let p = BuildProject {
            tool: BuildTool::Gradle,
            name: "x".into(),
            wrapper: true,
            build_file: "build.gradle".into(),
            settings_file: None,
            kotlin_dsl: false,
            modules: vec![],
            jacoco: true,
            application: true,
            main_class: None,
            spring_boot: false,
            custom_tasks: vec!["bundle".into(), "test".into()],
            java_version: None,
            test_frameworks: vec![],
        };
        let names: Vec<String> = goals(&p).into_iter().map(|g| g.name).collect();
        assert!(names.contains(&"run".to_string()));
        assert!(names.contains(&"jacocoTestReport".to_string()));
        assert_eq!(names.iter().filter(|n| *n == "test").count(), 1);
        assert_eq!(names.last().map(String::as_str), Some("bundle"));
        let maven = BuildProject {
            tool: BuildTool::Maven,
            ..p
        };
        let g = goals(&maven);
        assert_eq!(g[0].name, "clean");
        assert_eq!(g[0].group, "Lifecycle");
        assert!(g.iter().any(|g| g.name == "jacoco:report"));
    }
}
