//! Running Groovy code, like IntelliJ IDEA's Groovy Script configuration: a script (or a
//! class with a `main` method) runs with `groovy` from a Groovy SDK, or through the
//! project's Gradle build, where an init script adds a `forgeRunGroovy` task that runs
//! `groovy.ui.GroovyMain` on the main source set's class path (the project's own Groovy
//! version and dependencies, no SDK needed).

use std::path::{Path, PathBuf};

use forge_runtools::args::split_args;
use serde::{Deserialize, Serialize};

use crate::source::parse;

/// The Gradle task the init script registers.
pub const GRADLE_RUN_TASK: &str = "forgeRunGroovy";
/// Environment variables the init script reads (one value per line).
pub const RUN_ARGS_ENV: &str = "FORGE_RUN_ARGS";
pub const RUN_JVM_ARGS_ENV: &str = "FORGE_RUN_JVM_ARGS";
pub const RUN_DIR_ENV: &str = "FORGE_RUN_DIR";

/// The Gradle init script of [`Runner::Gradle`].
pub const GRADLE_INIT_SCRIPT: &str = r#"// Written by Forge (JavaForge Groovy): runs a Groovy script with the project's class path.
allprojects { p ->
    p.plugins.withId('groovy') {
        if (p.tasks.findByName('forgeRunGroovy') == null) {
            p.tasks.register('forgeRunGroovy', JavaExec) {
                group = 'application'
                description = 'Runs a Groovy script with the main source set (Forge).'
                classpath = p.sourceSets.main.runtimeClasspath
                mainClass.set('groovy.ui.GroovyMain')
                def lines = { String name -> (System.getenv(name) ?: '').split('\n').findAll { !it.isEmpty() } }
                args(lines('FORGE_RUN_ARGS'))
                jvmArgs(lines('FORGE_RUN_JVM_ARGS'))
                standardInput = System.in
                if (System.getenv('FORGE_RUN_DIR')) {
                    workingDir = file(System.getenv('FORGE_RUN_DIR'))
                }
            }
        }
    }
}
"#;

/// What can run in a Groovy file: the file itself as a script, or the `main` methods of
/// its classes (both run with `groovy <file>`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Runnable {
    /// Top-level statements: a script (gutter on the first line).
    pub script: bool,
    pub mains: Vec<MainMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainMethod {
    /// Binary class name.
    pub class: String,
    /// Simple class name.
    pub name: String,
    /// 0-based line of `main`.
    pub line: u32,
}

/// Scripts and `main` methods of a Groovy file (Spock specs and tests are not runnable).
pub fn runnable(text: &str) -> Runnable {
    let file = parse(text);
    let mains = file
        .types
        .iter()
        .filter(|t| t.kind == "class")
        .flat_map(|t| {
            t.methods
                .iter()
                .filter(|m| m.name == "main" && m.is_static && !m.quoted)
                .map(|m| MainMethod {
                    class: t.class.clone(),
                    name: t.name.clone(),
                    line: m.line,
                })
        })
        .collect();
    Runnable {
        script: file.script,
        mains,
    }
}

/// How a Groovy script runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Runner {
    /// Gradle when the project's Gradle build applies the `groovy` plugin, else `groovy`.
    #[default]
    Auto,
    /// `groovy` of a Groovy SDK.
    Groovy,
    /// The project's Gradle build (`forgeRunGroovy`).
    Gradle,
}

impl Runner {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "groovy" => Runner::Groovy,
            "gradle" => Runner::Gradle,
            _ => Runner::Auto,
        }
    }

    /// The runner for a project whose Gradle build does (`gradle_groovy`) or does not apply
    /// the Groovy plugin.
    pub fn resolve(self, gradle_groovy: bool) -> Self {
        match self {
            Runner::Auto if gradle_groovy => Runner::Gradle,
            Runner::Auto => Runner::Groovy,
            r => r,
        }
    }
}

/// A Groovy script run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScriptRun {
    /// Absolute path of the script.
    pub script: String,
    pub arguments: String,
    /// JVM options (`JAVA_OPTS` for `groovy`, `jvmArgs` through Gradle).
    pub vm_options: String,
    pub working_dir: String,
    /// `groovy` (or `groovy.bat`) for [`Runner::Groovy`].
    pub groovy: String,
    /// Gradle (wrapper or executable) for [`Runner::Gradle`].
    pub build_program: String,
    pub init_script: String,
    /// The Gradle project path of the module (`:tools`), empty for the root.
    pub gradle_path: String,
    pub offline: bool,
}

/// A command line: program, arguments and extra environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroovyCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The command line of a script run.
pub fn script_command(run: &ScriptRun, runner: Runner) -> Result<GroovyCommand, String> {
    let script = run.script.trim();
    if script.is_empty() {
        return Err("Script path is not specified".into());
    }
    if script.ends_with(".gradle") {
        return Err("Gradle build scripts run through Gradle tasks, not as Groovy scripts".into());
    }
    let args = split_args(&run.arguments);
    let vm = split_args(&run.vm_options);
    match runner {
        Runner::Groovy | Runner::Auto => {
            let program = match run.groovy.trim() {
                "" => "groovy".to_string(),
                g => g.to_string(),
            };
            let mut a = vec![script.to_string()];
            a.extend(args);
            let env = if vm.is_empty() {
                Vec::new()
            } else {
                vec![("JAVA_OPTS".to_string(), vm.join(" "))]
            };
            Ok(GroovyCommand {
                program,
                args: a,
                env,
            })
        }
        Runner::Gradle => {
            if run.build_program.trim().is_empty() {
                return Err("Groovy scripts run through Gradle: no Gradle build was found".into());
            }
            let task = match run.gradle_path.trim().trim_end_matches(':') {
                "" => GRADLE_RUN_TASK.to_string(),
                p => format!("{p}:{GRADLE_RUN_TASK}"),
            };
            let mut a = vec![
                "--console=plain".to_string(),
                "-q".into(),
                "--init-script".into(),
                run.init_script.clone(),
                task,
            ];
            if run.offline {
                a.push("--offline".into());
            }
            let mut script_args = vec![script.to_string()];
            script_args.extend(args);
            let mut env = vec![
                (RUN_ARGS_ENV.to_string(), script_args.join("\n")),
                (RUN_JVM_ARGS_ENV.to_string(), vm.join("\n")),
            ];
            if !run.working_dir.trim().is_empty() {
                env.push((RUN_DIR_ENV.to_string(), run.working_dir.clone()));
            }
            Ok(GroovyCommand {
                program: run.build_program.clone(),
                args: a,
                env,
            })
        }
    }
}

/// Whether a Gradle build file applies the Groovy plugin.
pub fn gradle_applies_groovy(text: &str) -> bool {
    text.contains("id 'groovy'")
        || text.contains("id \"groovy\"")
        || text.contains("id(\"groovy\")")
        || text.contains("apply plugin: 'groovy'")
        || text.contains("apply plugin: \"groovy\"")
        || text.lines().any(|l| {
            let l = l.trim();
            l == "groovy" || l == "`groovy`" || l.contains("{ groovy }")
        })
}

/// Where a Groovy SDK's launcher may be, best first: `GROOVY_HOME`, then SDKMAN!'s current
/// Groovy (`SDKMAN_DIR`, else `~/.sdkman`). The PATH is searched by the caller.
pub fn groovy_candidates(
    env: &dyn Fn(&str) -> Option<String>,
    home: Option<&Path>,
    windows: bool,
) -> Vec<PathBuf> {
    let exe = if windows { "groovy.bat" } else { "groovy" };
    let mut out = Vec::new();
    if let Some(g) = env("GROOVY_HOME").filter(|g| !g.trim().is_empty()) {
        out.push(Path::new(g.trim()).join("bin").join(exe));
    }
    let sdkman = env("SDKMAN_DIR")
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".sdkman")));
    if let Some(d) = sdkman {
        out.push(d.join("candidates/groovy/current/bin").join(exe));
    }
    out
}

/// The launcher of a configured path: the launcher itself (`is_file`), a Groovy home or its
/// `bin` folder.
pub fn launcher_in(path: &Path, is_file: bool, windows: bool) -> PathBuf {
    let exe = if windows { "groovy.bat" } else { "groovy" };
    if is_file {
        return path.to_path_buf();
    }
    if path.file_name().is_some_and(|n| n == "bin") {
        return path.join(exe);
    }
    path.join("bin").join(exe)
}

/// The version of `groovy --version`: `Groovy Version: 4.0.24 JVM: 21.0.5 …` → `4.0.24`.
pub fn parse_version(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("Groovy Version:")?;
        rest.split_whitespace().next().map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_mains_are_runnable() {
        let script = include_str!("../tests/fixtures/release.groovy");
        let r = runnable(script);
        assert!(r.script);
        assert!(r.mains.is_empty());
        let app = "package com.acme\nclass Tool {\n  static void main(String[] args) { println 'x' }\n}\n";
        let r = runnable(app);
        assert!(!r.script);
        assert_eq!(r.mains[0].class, "com.acme.Tool");
        assert_eq!(r.mains[0].line, 2);
        assert_eq!(
            runnable(include_str!("../tests/fixtures/CartSpec.groovy")),
            Runnable::default()
        );
    }

    #[test]
    fn groovy_and_gradle_command_lines() {
        let run = ScriptRun {
            script: "/p/scripts/release.groovy".into(),
            arguments: "2.1.0 \"Spring release\"".into(),
            vm_options: "-Xmx512m".into(),
            groovy: "/sdk/groovy/bin/groovy".into(),
            ..Default::default()
        };
        let c = script_command(&run, Runner::Groovy).unwrap();
        assert_eq!(c.program, "/sdk/groovy/bin/groovy");
        assert_eq!(
            c.args,
            ["/p/scripts/release.groovy", "2.1.0", "Spring release"]
        );
        assert_eq!(c.env, [("JAVA_OPTS".to_string(), "-Xmx512m".to_string())]);

        let gradle = ScriptRun {
            build_program: "/p/gradlew".into(),
            init_script: "/cache/run-groovy.gradle".into(),
            gradle_path: ":tools".into(),
            working_dir: "/p".into(),
            offline: true,
            ..run.clone()
        };
        let c = script_command(&gradle, Runner::Gradle).unwrap();
        assert_eq!(c.program, "/p/gradlew");
        assert_eq!(
            c.args,
            [
                "--console=plain",
                "-q",
                "--init-script",
                "/cache/run-groovy.gradle",
                ":tools:forgeRunGroovy",
                "--offline"
            ]
        );
        assert_eq!(
            c.env[0].1,
            "/p/scripts/release.groovy\n2.1.0\nSpring release"
        );
        assert_eq!(c.env[2], (RUN_DIR_ENV.to_string(), "/p".to_string()));
        assert!(script_command(&ScriptRun::default(), Runner::Groovy).is_err());
        assert!(script_command(
            &ScriptRun {
                script: "build.gradle".into(),
                ..Default::default()
            },
            Runner::Groovy
        )
        .is_err());
        assert!(script_command(
            &ScriptRun {
                script: "a.groovy".into(),
                ..Default::default()
            },
            Runner::Gradle
        )
        .is_err());
        assert_eq!(Runner::Auto.resolve(true), Runner::Gradle);
        assert_eq!(Runner::parse("groovy").resolve(true), Runner::Groovy);
    }

    #[test]
    fn sdk_lookup_and_versions() {
        let env = |k: &str| match k {
            "GROOVY_HOME" => Some("/opt/groovy-4.0.24".to_string()),
            _ => None,
        };
        let c = groovy_candidates(&env, Some(Path::new("/home/me")), false);
        assert_eq!(
            c,
            [
                PathBuf::from("/opt/groovy-4.0.24/bin/groovy"),
                PathBuf::from("/home/me/.sdkman/candidates/groovy/current/bin/groovy")
            ]
        );
        let c = groovy_candidates(&|_| None, None, true);
        assert!(c.is_empty());
        assert_eq!(
            parse_version(
                "Groovy Version: 4.0.24 JVM: 21.0.5 Vendor: Eclipse Adoptium OS: Linux\n"
            )
            .as_deref(),
            Some("4.0.24")
        );
        assert_eq!(parse_version("command not found"), None);
        assert!(gradle_applies_groovy("plugins {\n  id 'groovy'\n}"));
        assert!(gradle_applies_groovy("plugins { groovy }"));
        assert!(!gradle_applies_groovy("plugins { java }"));
        assert_eq!(
            launcher_in(Path::new("/opt/groovy"), false, false),
            PathBuf::from("/opt/groovy/bin/groovy")
        );
        assert_eq!(
            launcher_in(Path::new("/opt/groovy/bin"), false, false),
            PathBuf::from("/opt/groovy/bin/groovy")
        );
    }
}
