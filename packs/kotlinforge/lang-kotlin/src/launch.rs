//! Command lines of Kotlin programs.
//!
//! - Kotlin/JVM applications (JavaForge's "Application" configuration with a Kotlin main
//!   class, `AppKt`) need the Kotlin standard library on the class path, so they run
//!   through the build: Gradle with a small init script that adds a `forgeRunKotlin`
//!   (`JavaExec` over the module's runtime class path, any main class), Maven with
//!   `exec:java`, or without a build through `kotlinc` (compile with the runtime, then
//!   `java -cp`).
//! - Kotlin scripts (`*.kts`, `*.main.kts`) run with `kotlin` or `kotlinc -script`; Gradle
//!   build scripts are not scripts to run.

use forge_runtools::args::split_args;
use serde::{Deserialize, Serialize};

/// The task the init script registers in every JVM project.
pub const GRADLE_RUN_TASK: &str = "forgeRunKotlin";
/// Environment variables the init script reads (one argument per line).
pub const RUN_ARGS_ENV: &str = "FORGE_RUN_ARGS";
pub const RUN_JVM_ARGS_ENV: &str = "FORGE_RUN_JVM_ARGS";

/// The Gradle init script (Groovy, Gradle 6.4+), written to Forge's cache directory.
pub const GRADLE_INIT_SCRIPT: &str = r#"// Written by Forge (KotlinForge): runs any main class of a JVM project.
allprojects { p ->
    p.plugins.withId('java') {
        if (p.tasks.findByName('forgeRunKotlin') == null) {
            p.tasks.register('forgeRunKotlin', JavaExec) {
                group = 'application'
                description = 'Runs a main class of this project (Forge).'
                classpath = p.sourceSets.main.runtimeClasspath
                mainClass.set(p.providers.gradleProperty('forge.mainClass').orElse(''))
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

/// How a Kotlin application runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Runner {
    /// Gradle when the project has a Gradle build, else Maven, else `kotlinc`.
    #[default]
    Auto,
    Gradle,
    Maven,
    Kotlinc,
}

impl Runner {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "gradle" => Runner::Gradle,
            "maven" => Runner::Maven,
            "kotlinc" => Runner::Kotlinc,
            _ => Runner::Auto,
        }
    }

    /// The runner for a project's build tool (`gradle`, `maven`, none).
    pub fn resolve(self, build_tool: Option<&str>) -> Runner {
        match (self, build_tool) {
            (Runner::Auto, Some("gradle")) => Runner::Gradle,
            (Runner::Auto, Some("maven")) => Runner::Maven,
            (Runner::Auto, _) => Runner::Kotlinc,
            (r, _) => r,
        }
    }
}

/// A command line with the environment it needs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KotlinCommand {
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

/// A Kotlin application resolved for one run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KotlinApp {
    pub runner: Option<Runner>,
    pub main_class: String,
    /// Build tool program (`/p/gradlew`, `mvn`).
    pub build_program: String,
    /// Gradle project path of the module (`:app`, empty for the root project).
    pub gradle_path: String,
    /// Maven module folder (`app`).
    pub module: String,
    /// The init script file (Gradle).
    pub init_script: String,
    /// `kotlinc` and `java` (no build).
    pub kotlinc: String,
    pub java: String,
    /// Kotlin sources to compile with `kotlinc` (files or folders).
    pub sources: Vec<String>,
    /// The jar `kotlinc` writes.
    pub output_jar: String,
    pub vm_options: String,
    pub arguments: String,
    pub working_dir: String,
    pub windows: bool,
    pub offline: bool,
    /// Debug: a JDWP agent that listens on this port and waits for the debugger.
    pub debug_port: Option<u16>,
}

/// POSIX shell quoting.
fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,@+%".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// `cmd.exe` quoting.
fn cmd_quote(s: &str) -> String {
    if !s.is_empty() && !s.contains([' ', '"', '&', '|', '<', '>', '^', '%']) {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
}

fn vm_args(app: &KotlinApp) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(port) = app.debug_port {
        out.push(javaforge_lang_java::launch::jdwp_listen(port, true));
    }
    out.extend(split_args(&app.vm_options));
    out
}

/// `(program, args, env)` of a Kotlin application run with `runner` (already resolved).
pub fn application_command(app: &KotlinApp, runner: Runner) -> Result<KotlinCommand, String> {
    let main = app.main_class.trim();
    if main.is_empty() {
        return Err("Main class is not specified".into());
    }
    let args = split_args(&app.arguments);
    match runner {
        Runner::Gradle | Runner::Auto => {
            if app.build_program.trim().is_empty() {
                return Err("Kotlin runs through Gradle: no Gradle build was found".into());
            }
            let task = match app.gradle_path.trim().trim_end_matches(':') {
                "" => GRADLE_RUN_TASK.to_string(),
                p => format!("{p}:{GRADLE_RUN_TASK}"),
            };
            let mut a = vec![
                "--console=plain".to_string(),
                "-q".into(),
                "--init-script".into(),
                app.init_script.clone(),
                task,
                format!("-Pforge.mainClass={main}"),
            ];
            if app.offline {
                a.push("--offline".into());
            }
            let mut env = vec![
                (RUN_ARGS_ENV.to_string(), args.join("\n")),
                (RUN_JVM_ARGS_ENV.to_string(), vm_args(app).join("\n")),
            ];
            if !app.working_dir.trim().is_empty() {
                env.push(("FORGE_RUN_DIR".into(), app.working_dir.clone()));
            }
            Ok(KotlinCommand {
                program: app.build_program.clone(),
                args: a,
                env,
            })
        }
        Runner::Maven => {
            if app.build_program.trim().is_empty() {
                return Err("Kotlin runs through Maven: no pom.xml was found".into());
            }
            let mut a = vec!["-q".to_string()];
            if !app.module.trim().is_empty() {
                a.push("-pl".into());
                a.push(app.module.trim().into());
            }
            if app.offline {
                a.push("-o".into());
            }
            a.push("compile".into());
            a.push("exec:java".into());
            a.push(format!("-Dexec.mainClass={main}"));
            if !args.is_empty() {
                a.push(format!("-Dexec.args={}", app.arguments.trim()));
            }
            // exec:java runs inside Maven's JVM: VM options and the debugger go there.
            let opts = vm_args(app);
            let env = if opts.is_empty() {
                Vec::new()
            } else {
                vec![("MAVEN_OPTS".to_string(), opts.join(" "))]
            };
            Ok(KotlinCommand {
                program: app.build_program.clone(),
                args: a,
                env,
            })
        }
        Runner::Kotlinc => {
            if app.sources.is_empty() {
                return Err("No Kotlin sources to compile".into());
            }
            let kotlinc = if app.kotlinc.trim().is_empty() {
                "kotlinc"
            } else {
                app.kotlinc.trim()
            };
            let java = if app.java.trim().is_empty() {
                "java"
            } else {
                app.java.trim()
            };
            let mut compile: Vec<String> = vec![kotlinc.to_string()];
            compile.extend(app.sources.iter().cloned());
            compile.extend([
                "-include-runtime".into(),
                "-d".into(),
                app.output_jar.clone(),
            ]);
            let mut run: Vec<String> = vec![java.to_string()];
            run.extend(vm_args(app));
            run.extend(["-cp".into(), app.output_jar.clone(), main.to_string()]);
            run.extend(args);
            if app.windows {
                let line = format!(
                    "{} && {}",
                    compile
                        .iter()
                        .map(|a| cmd_quote(a))
                        .collect::<Vec<_>>()
                        .join(" "),
                    run.iter()
                        .map(|a| cmd_quote(a))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                Ok(KotlinCommand {
                    program: "cmd".into(),
                    args: vec!["/C".into(), line],
                    env: Vec::new(),
                })
            } else {
                let line = format!(
                    "{} && exec {}",
                    compile
                        .iter()
                        .map(|a| sh_quote(a))
                        .collect::<Vec<_>>()
                        .join(" "),
                    run.iter()
                        .map(|a| sh_quote(a))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                Ok(KotlinCommand {
                    program: "sh".into(),
                    args: vec!["-c".into(), line],
                    env: Vec::new(),
                })
            }
        }
    }
}

/// Whether a file is a Kotlin script to run (not a Gradle build or settings script).
pub fn is_runnable_script(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.ends_with(".kts") && !name.ends_with(".gradle.kts")
}

/// A Kotlin script run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScriptRun {
    pub script: String,
    /// `kotlin` (Kotlin 1.9+ runs scripts) or `kotlinc`.
    pub program: String,
    pub arguments: String,
    pub vm_options: String,
}

/// `kotlin script.main.kts args`, or `kotlinc -script script.kts args`.
pub fn script_command(r: &ScriptRun) -> Result<KotlinCommand, String> {
    let script = r.script.trim();
    if script.is_empty() {
        return Err("Script file is not specified".into());
    }
    if !is_runnable_script(script) {
        return Err(format!(
            "{script} is not a Kotlin script: Gradle build scripts run through Gradle"
        ));
    }
    let program = match r.program.trim() {
        "" => "kotlin".to_string(),
        p => p.to_string(),
    };
    let file = program.rsplit(['/', '\\']).next().unwrap_or(&program);
    let kotlinc = file.starts_with("kotlinc");
    let mut args = Vec::new();
    // JVM options pass through `-J`.
    args.extend(
        split_args(&r.vm_options)
            .into_iter()
            .map(|o| format!("-J{o}")),
    );
    if kotlinc {
        args.push("-script".into());
    }
    args.push(script.to_string());
    args.extend(split_args(&r.arguments));
    Ok(KotlinCommand {
        program,
        args,
        env: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> KotlinApp {
        KotlinApp {
            main_class: "com.acme.AppKt".into(),
            build_program: "/p/gradlew".into(),
            gradle_path: ":app".into(),
            init_script: "/cache/forge/kotlin/run.init.gradle".into(),
            vm_options: "-Xmx256m".into(),
            arguments: "--port 8080".into(),
            ..Default::default()
        }
    }

    #[test]
    fn gradle_runs_any_main_class_through_the_init_script() {
        let mut a = app();
        a.debug_port = Some(5006);
        let c = application_command(&a, Runner::Gradle).unwrap();
        assert_eq!(c.program, "/p/gradlew");
        assert_eq!(
            c.args,
            [
                "--console=plain",
                "-q",
                "--init-script",
                "/cache/forge/kotlin/run.init.gradle",
                ":app:forgeRunKotlin",
                "-Pforge.mainClass=com.acme.AppKt"
            ]
        );
        assert_eq!(c.env[0], (RUN_ARGS_ENV.into(), "--port\n8080".into()));
        assert!(c.env[1].1.starts_with("-agentlib:jdwp="));
        assert!(c.env[1].1.ends_with("\n-Xmx256m"));
        a.gradle_path = String::new();
        assert_eq!(
            application_command(&a, Runner::Gradle).unwrap().args[4],
            "forgeRunKotlin"
        );
        assert!(
            GRADLE_INIT_SCRIPT.contains(GRADLE_RUN_TASK)
                && GRADLE_INIT_SCRIPT.contains(RUN_ARGS_ENV)
        );
    }

    #[test]
    fn maven_and_kotlinc_runs() {
        let mut a = app();
        a.build_program = "mvn".into();
        a.module = "app".into();
        let c = application_command(&a, Runner::Maven).unwrap();
        assert_eq!(
            c.args,
            [
                "-q",
                "-pl",
                "app",
                "compile",
                "exec:java",
                "-Dexec.mainClass=com.acme.AppKt",
                "-Dexec.args=--port 8080"
            ]
        );
        assert_eq!(c.env, [("MAVEN_OPTS".to_string(), "-Xmx256m".to_string())]);

        let mut a = app();
        a.sources = vec!["/p/src/App.kt".into()];
        a.output_jar = "/tmp/forge kotlin/app.jar".into();
        a.java = "/jdk/bin/java".into();
        let c = application_command(&a, Runner::Kotlinc).unwrap();
        assert_eq!(c.program, "sh");
        assert_eq!(
            c.args[1],
            "kotlinc /p/src/App.kt -include-runtime -d '/tmp/forge kotlin/app.jar' && exec /jdk/bin/java -Xmx256m -cp '/tmp/forge kotlin/app.jar' com.acme.AppKt --port 8080"
        );
        a.windows = true;
        let c = application_command(&a, Runner::Kotlinc).unwrap();
        assert_eq!(c.program, "cmd");
        assert!(c.args[1].contains("&& /jdk/bin/java"));
        a.main_class = " ".into();
        assert!(application_command(&a, Runner::Kotlinc).is_err());
        assert_eq!(Runner::Auto.resolve(Some("maven")), Runner::Maven);
        assert_eq!(Runner::Auto.resolve(None), Runner::Kotlinc);
        assert_eq!(Runner::parse("gradle").resolve(None), Runner::Gradle);
    }

    #[test]
    fn scripts() {
        assert!(is_runnable_script("tools/release.main.kts"));
        assert!(!is_runnable_script("app/build.gradle.kts"));
        assert!(!is_runnable_script("settings.gradle.kts"));
        assert!(!is_runnable_script("App.kt"));
        let c = script_command(&ScriptRun {
            script: "tools/release.main.kts".into(),
            arguments: "1.2.0 --dry-run".into(),
            vm_options: "-Xmx1g".into(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(c.program, "kotlin");
        assert_eq!(
            c.args,
            ["-J-Xmx1g", "tools/release.main.kts", "1.2.0", "--dry-run"]
        );
        let c = script_command(&ScriptRun {
            script: "a.kts".into(),
            program: "/opt/kotlinc/bin/kotlinc".into(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(c.args, ["-script", "a.kts"]);
        assert!(script_command(&ScriptRun {
            script: "build.gradle.kts".into(),
            ..Default::default()
        })
        .unwrap_err()
        .contains("Gradle"));
    }
}
