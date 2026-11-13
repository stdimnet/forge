//! JavaForge's Groovy IPC: the Groovy SDK, runnable scripts and Groovy Script runs. The
//! logic lives in `packs/javaforge/lang-groovy`; Spock runs through `SpockTests` and
//! JavaForge's build preparation (`commands/java.rs`, `run.rs`).
//!
//! - `groovy_status`: the Groovy SDK (setting, GROOVY_HOME, SDKMAN!, PATH) and its version,
//!   and whether the Gradle build applies the `groovy` plugin.
//! - `groovy_runnable`: scripts and `main` methods of a file for the Run gutter.
//! - [`command_line`] builds `groovy-script` configurations: `groovy <script>` or the
//!   `forgeRunGroovy` task of an init script through Gradle. Both run project code, so they
//!   start in `run::start_built` (refused in Safe Mode).

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::{CommandLine, RunConfiguration};
use javaforge_build::BuildTool;
use javaforge_lang_groovy::launch::{self, Runner, ScriptRun};
use javaforge_lang_groovy::Runnable;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::java;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// The Groovy Script configuration type.
pub const SCRIPT: &str = "groovy-script";

fn windows() -> bool {
    cfg!(windows)
}

/// The Groovy launcher and where it came from.
fn find_groovy(state: &AppState, backend: &dyn Backend) -> Option<(PathBuf, &'static str)> {
    let own = java::setting(state, "groovy.sdk.path");
    if !own.is_empty() {
        let p = Path::new(&own);
        let launcher = launch::launcher_in(p, p.is_file(), windows());
        return launcher.is_file().then_some((launcher, "setting"));
    }
    if backend.info().remote.is_none() {
        let env = |k: &str| std::env::var(k).ok();
        let home = dirs::home_dir();
        for c in launch::groovy_candidates(&env, home.as_deref(), windows()) {
            if c.is_file() {
                let source = if c.to_string_lossy().contains(".sdkman") {
                    "SDKMAN!"
                } else {
                    "GROOVY_HOME"
                };
                return Some((c, source));
            }
        }
    }
    backend
        .find_executable(&["groovy".to_string()], None)
        .ok()
        .flatten()
        .map(|p| (p, "PATH"))
}

/// Whether the project's Gradle build (root or a module) applies the Groovy plugin, and
/// the Gradle path of the module that does.
fn gradle_groovy_module(state: &AppState, backend: &dyn Backend) -> Option<String> {
    let build = java::read_build(state, backend)?;
    if build.tool != BuildTool::Gradle {
        return None;
    }
    let root = backend
        .read_optional(&build.build_file)
        .ok()
        .flatten()
        .unwrap_or_default();
    if launch::gradle_applies_groovy(&root) {
        return Some(String::new());
    }
    build.modules.iter().find_map(|m| {
        ["build.gradle", "build.gradle.kts"].iter().find_map(|f| {
            let t = backend
                .read_optional(&format!("{}/{f}", m.dir))
                .ok()
                .flatten()?;
            launch::gradle_applies_groovy(&t).then(|| m.path.clone())
        })
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroovyStatus {
    pub groovy: Option<String>,
    /// `setting`, `GROOVY_HOME`, `SDKMAN!` or `PATH`.
    pub source: Option<String>,
    pub version: Option<String>,
    /// The Gradle build applies the `groovy` plugin (scripts can run through Gradle).
    pub gradle_groovy: bool,
    /// How scripts run with the current settings: `groovy` or `gradle`.
    pub runner: String,
}

#[tauri::command]
pub async fn groovy_status(app: AppHandle) -> Result<GroovyStatus> {
    blocking(move || status_of(&app.state::<AppState>())).await
}

fn status_of(state: &AppState) -> Result<GroovyStatus> {
    let backend = state.backend()?;
    let found = find_groovy(state, backend.as_ref());
    // In Safe Mode a launcher inside the project (`node_modules/.bin/groovy`) does not
    // run, not even for its version.
    let version = found
        .as_ref()
        .filter(|(p, _)| {
            state
                .refuse_project_program(p, "Running the project's Groovy")
                .is_ok()
        })
        .and_then(|(p, _)| {
            let mut cmd = CommandLine::new(p.display().to_string()).args(["--version"]);
            cmd.cwd = backend.info().root.clone();
            capture(backend.as_ref(), &cmd, "", Duration::from_secs(30))
                .ok()
                .and_then(|o| launch::parse_version(&format!("{}\n{}", o.stdout, o.stderr)))
        });
    let gradle_groovy = gradle_groovy_module(state, backend.as_ref()).is_some();
    let runner = Runner::parse(&java::setting(state, "groovy.run.runner")).resolve(gradle_groovy);
    Ok(GroovyStatus {
        groovy: found.as_ref().map(|(p, _)| p.display().to_string()),
        source: found.map(|(_, s)| s.to_string()),
        version,
        gradle_groovy,
        runner: match runner {
            Runner::Gradle => "gradle".into(),
            _ => "groovy".into(),
        },
    })
}

#[tauri::command]
pub fn groovy_runnable(text: String) -> Runnable {
    javaforge_lang_groovy::runnable(&text)
}

/// The command line of a Groovy Script configuration (`None` for other types).
pub(crate) fn command_line(
    state: &AppState,
    cfg: &RunConfiguration,
) -> Result<Option<CommandLine>> {
    if cfg.type_id != SCRIPT {
        return Ok(None);
    }
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let cwd = match cfg.working_directory.trim() {
        "" => root.clone(),
        d if Path::new(d).is_absolute() => PathBuf::from(d),
        d => root.join(d),
    };
    let script = cfg.option("script").trim();
    let script_abs = if Path::new(script).is_absolute() || script.is_empty() {
        script.to_string()
    } else {
        root.join(script).display().to_string()
    };
    let module = gradle_groovy_module(state, backend.as_ref());
    let runner = Runner::parse(
        match cfg.option("runner").trim() {
            "" => java::setting(state, "groovy.run.runner"),
            r => r.to_string(),
        }
        .as_str(),
    )
    .resolve(module.is_some());
    let mut run = ScriptRun {
        script: script_abs,
        arguments: cfg.arguments.clone(),
        vm_options: cfg.interpreter_options.clone(),
        working_dir: cwd.display().to_string(),
        ..Default::default()
    };
    match runner {
        Runner::Gradle => {
            if backend.info().remote.is_some() {
                return Err(ForgeError::Other(
                    "Groovy scripts of remote projects run with groovy: choose it in the configuration".into(),
                ));
            }
            let build = java::read_build(state, backend.as_ref())
                .filter(|b| b.tool == BuildTool::Gradle)
                .ok_or_else(|| ForgeError::Other("Groovy scripts run through Gradle: the project has no Gradle build (choose groovy in the configuration)".into()))?;
            let opts = java::exec_options(state, BuildTool::Gradle, &root);
            run.build_program =
                javaforge_build::executable(BuildTool::Gradle, build.wrapper, &opts);
            run.offline = opts.offline;
            run.gradle_path = module.unwrap_or_default();
            run.init_script = crate::commands::java_build::init_script(
                "forge-run-groovy.gradle",
                launch::GRADLE_INIT_SCRIPT,
            )?
            .display()
            .to_string();
        }
        _ => {
            run.groovy = match cfg.option("groovy").trim() {
                "" => find_groovy(state, backend.as_ref())
                    .map(|(p, _)| p.display().to_string())
                    .ok_or_else(|| ForgeError::Other("groovy was not found: install a Groovy SDK (SDKMAN!: sdk install groovy) or set it in Settings | Languages & Frameworks | Groovy".into()))?,
                own => own.to_string(),
            };
        }
    }
    let built = launch::script_command(&run, runner).map_err(ForgeError::Other)?;
    let mut cmd = CommandLine::new(built.program).args(built.args);
    cmd.cwd = cwd;
    for (k, v) in built.env {
        cmd.env.insert(k, v);
    }
    let home = if runner == Runner::Gradle {
        java::gradle_java_home(state)
    } else {
        java::jdk_home(state, cfg)
    };
    if let Some(home) = home {
        cmd.env.entry("JAVA_HOME".into()).or_insert(home);
    }
    cmd.env.extend(cfg.env.clone());
    Ok(Some(cmd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn safe_mode_never_runs_a_project_groovy() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = forge_core::paths::canonicalize(dir.path())
            .unwrap()
            .join("app");
        let bin = root.join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        let marker = dir.path().join("ran");
        let groovy = bin.join("groovy");
        std::fs::write(
            &groovy,
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&groovy, std::fs::Permissions::from_mode(0o755)).unwrap();
        let state = AppState::for_tests(dir.path().join("trusted-projects.toml"));
        state.set_workspace(forge_core::Workspace::open(&root).unwrap());
        assert!(state.safe_mode.is_on());
        let status = status_of(&state).unwrap();
        assert!(!marker.exists(), "the project's groovy ran in Safe Mode");
        if status.groovy.as_deref() == Some(&*groovy.to_string_lossy()) {
            assert_eq!(status.version, None);
        }
    }
}
