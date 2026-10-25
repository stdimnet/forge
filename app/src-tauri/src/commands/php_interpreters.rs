//! PHP CLI interpreters (Settings | Languages & Frameworks | PHP, "CLI Interpreters"
//! dialog): detection, Reload (version, php.ini, Xdebug, extensions), phpinfo, the
//! automatic path mappings, and wrapping run configurations' command lines so they run in
//! Docker, Docker Compose, over SSH or in WSL. The logic lives in `phpforge-interpreters`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::output::OutputPathMapping;
use forge_process::CommandLine;
use phpforge_interpreters::detect::{self, DetectedInterpreter, SearchEnv};
use phpforge_interpreters::mapping::{self, Mapper};
use phpforge_interpreters::model::{GLOBAL_KEY, PROJECT_KEY, SELECTED_KEY};
use phpforge_interpreters::probe::{self, InterpreterInfo};
use phpforge_interpreters::{
    wrap, InterpreterKind, InterpreterSet, Invocation, PathMapping, PhpInterpreter, Program,
    SshTarget, WrapContext,
};
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// The configured interpreters of both settings layers.
pub(crate) fn interpreter_set(state: &AppState) -> InterpreterSet {
    let settings = state.read_settings();
    InterpreterSet::from_settings(settings.get(GLOBAL_KEY), settings.get(PROJECT_KEY))
}

/// The project's interpreter (`php.interpreter`), or `name` when it is set (a run
/// configuration's own interpreter).
pub(crate) fn resolve(state: &AppState, name: &str) -> PhpInterpreter {
    let selected = match name.trim() {
        "" => state
            .read_settings()
            .get_as::<String>(SELECTED_KEY)
            .unwrap_or_default(),
        n => n.to_string(),
    };
    interpreter_set(state).resolve(&selected)
}

/// The `php` executable for tools that only run locally (framework helpers, Composer's
/// phar): the project interpreter's path when it is a local one, else `php`.
pub(crate) fn local_php(state: &AppState) -> String {
    let interp = resolve(state, "");
    if interp.kind == InterpreterKind::Local {
        interp.php().to_string()
    } else {
        "php".into()
    }
}

/// An interpreter with everything its command lines need.
#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    pub interp: PhpInterpreter,
    pub ctx: WrapContext,
}

/// The compose files of an interpreter (the project's default compose file when none is
/// set), as (project-relative or absolute, absolute) paths.
fn compose_files(interp: &PhpInterpreter, backend: &dyn Backend, root: &str) -> Vec<String> {
    if !interp.compose_files.is_empty() {
        return interp.compose_files.clone();
    }
    [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yml",
        "docker-compose.yaml",
    ]
    .into_iter()
    .find(|f| matches!(backend.read_optional(f), Ok(Some(_))))
    .map(|f| vec![mapping::join(root, f)])
    .unwrap_or_default()
}

/// The text of a compose file: through the backend when it is in the project.
fn read_compose(backend: &dyn Backend, root: &str, abs: &str) -> Option<String> {
    let rel = abs
        .strip_prefix(root.trim_end_matches('/'))
        .map(|r| r.trim_start_matches('/'));
    match rel {
        Some(rel) => backend.read_optional(rel).ok().flatten(),
        None if backend.info().remote.is_none() => std::fs::read_to_string(abs).ok(),
        None => None,
    }
}

/// The merged compose files: `(directory of the first file, the service's volumes)`.
fn compose_service(
    interp: &PhpInterpreter,
    backend: &dyn Backend,
    root: &str,
) -> Option<(String, Vec<String>, Vec<String>)> {
    use forge_containers::compose;
    let files: Vec<String> = compose_files(interp, backend, root)
        .iter()
        .map(|f| mapping::join(root, f))
        .collect();
    let env = backend
        .read_optional(".env")
        .ok()
        .flatten()
        .map(|t| compose::parse_env_file(&t))
        .unwrap_or_default();
    let mut merged: Option<compose::ComposeFile> = None;
    for f in &files {
        let Some(text) = read_compose(backend, root, f) else {
            continue;
        };
        let parsed = compose::parse(f, &text, &env);
        merged = Some(match merged {
            Some(base) => compose::merge(base, parsed),
            None => parsed,
        });
    }
    let merged = merged?;
    let dir = Path::new(files.first()?)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| root.to_string());
    let services = merged.services.iter().map(|s| s.name.clone()).collect();
    let volumes = merged
        .service(&interp.service)
        .map(|s| s.volumes.clone())
        .unwrap_or_default();
    Some((dir, volumes, services))
}

/// The SSH configuration an interpreter names.
fn ssh_target(interp: &PhpInterpreter) -> Option<SshTarget> {
    let all = forge_remote::store::load();
    let c = all.get(&interp.ssh_config)?;
    Some(SshTarget {
        host: c.host.clone(),
        port: c.port,
        user: c.user.clone(),
        key_file: match c.auth {
            forge_remote::config::AuthMethod::KeyPair => c.key_file.clone(),
            _ => None,
        },
    })
}

/// The mappings that follow from the interpreter (Docker's mount, compose volumes, WSL).
fn automatic_mappings(
    interp: &PhpInterpreter,
    backend: &dyn Backend,
    root: &str,
) -> Vec<PathMapping> {
    let compose = if interp.kind == InterpreterKind::Compose {
        compose_service(interp, backend, root)
    } else {
        None
    };
    mapping::automatic(
        interp,
        root,
        compose.as_ref().map(|(d, v, _)| (d.as_str(), v.as_slice())),
    )
}

pub(crate) fn prepare(state: &AppState, backend: &dyn Backend, interp: PhpInterpreter) -> Prepared {
    let root = backend.info().root.to_string_lossy().replace('\\', "/");
    let automatic = if interp.kind.is_remote() {
        automatic_mappings(&interp, backend, &root)
    } else {
        Vec::new()
    };
    let ctx = WrapContext {
        mappings: mapping::effective(&interp, automatic),
        docker: state
            .read_settings()
            .get_as::<String>("docker.cliPath")
            .unwrap_or_default(),
        ssh: (interp.kind == InterpreterKind::Ssh)
            .then(|| ssh_target(&interp))
            .flatten(),
        root,
    };
    Prepared { interp, ctx }
}

impl Prepared {
    /// `cmd` (as if run locally) through the interpreter; `program` says whether its
    /// program is PHP (replaced by the interpreter's executable) or another tool.
    pub fn wrap(&self, program: Program, cmd: &CommandLine) -> Result<CommandLine> {
        self.wrap_serving(program, cmd, Vec::new())
    }

    /// `wrap` for a program that serves `publish` ports (the built-in web server).
    pub fn wrap_serving(
        &self,
        program: Program,
        cmd: &CommandLine,
        publish: Vec<u16>,
    ) -> Result<CommandLine> {
        let inv = Invocation {
            program,
            args: cmd.args.clone(),
            cwd: cmd.cwd.to_string_lossy().into_owned(),
            env: cmd.env.clone(),
            publish,
        };
        let w = wrap(&self.interp, &inv, &self.ctx).map_err(ForgeError::Other)?;
        Ok(CommandLine {
            program: w.program,
            args: w.args,
            cwd: PathBuf::from(w.cwd),
            env: w.env,
        })
    }

    /// Remote → local pairs for links in the Run console.
    pub fn output_mappings(&self) -> Vec<OutputPathMapping> {
        if !self.interp.kind.is_remote() {
            return Vec::new();
        }
        Mapper::new(&self.ctx.root, &self.ctx.mappings)
            .pairs()
            .iter()
            .map(|(local, remote)| OutputPathMapping {
                remote: remote.clone(),
                local: local.clone(),
            })
            .collect()
    }

    /// The debugger's server for runs through this interpreter (matched by
    /// `PHP_IDE_CONFIG=serverName=…`, which the wrapped command line sets).
    pub fn debug_server(&self) -> Option<forge_debug::ServerConfig> {
        if !self.interp.kind.is_remote() {
            return None;
        }
        let mappings = Mapper::new(&self.ctx.root, &self.ctx.mappings)
            .pairs()
            .iter()
            .map(|(local, remote)| forge_debug::PathMapping {
                local: local.clone(),
                remote: remote.clone(),
            })
            .collect();
        Some(forge_debug::ServerConfig {
            id: format!("interpreter:{}", self.interp.id),
            name: self.interp.debug_server_name(),
            host: String::new(),
            port: 80,
            debugger: "Xdebug".into(),
            use_path_mappings: true,
            mappings,
        })
    }
}

/// Run PHP with `args` through the interpreter and collect its output.
fn run_php(
    prepared: &Prepared,
    backend: &dyn Backend,
    args: &[&str],
    timeout: Duration,
    safe_mode: bool,
) -> Result<(Option<i32>, String, String)> {
    let mut cmd = CommandLine::new("php").args(args.iter().copied());
    cmd.cwd = PathBuf::from(&prepared.ctx.root);
    let cmd = prepared.wrap(Program::Php, &cmd)?;
    // A PHP inside the project, or a Docker Compose service built from its files.
    if safe_mode
        && (cmd.args.iter().any(|a| a == "compose")
            || forge_trust::runs_project_program(&backend.info().root, &cmd.program, &cmd.args))
    {
        return Err(ForgeError::Other(forge_trust::refusal(
            "Running this interpreter",
        )));
    }
    let out = capture(backend, &cmd, "", timeout)
        .map_err(|e| ForgeError::Other(format!("Cannot run {}: {e}", cmd.program)))?;
    Ok((out.code, out.stdout, out.stderr))
}

/// Local PHP executables with their versions (none for remote projects: their host is
/// searched by its own interpreters).
#[tauri::command]
pub async fn php_interpreters_detect(
    state: State<'_, AppState>,
) -> Result<Vec<DetectedInterpreter>> {
    if state.backend().is_ok_and(|b| b.info().remote.is_some()) {
        return Ok(Vec::new());
    }
    blocking(|| {
        Ok(detect::detect(&SearchEnv::system(), |p| {
            detect::version_of(p, Duration::from_secs(5))
        }))
    })
    .await
}

/// Reload: version, php.ini, Xdebug and extensions of an interpreter (`php -v`,
/// `php --ini`, `php -m`). Docker interpreters may pull their image first.
#[tauri::command]
pub async fn php_interpreter_info(
    interpreter: PhpInterpreter,
    state: State<'_, AppState>,
) -> Result<InterpreterInfo> {
    let backend = state.backend()?;
    let prepared = prepare(&state, backend.as_ref(), interpreter);
    let safe_mode = state.safe_mode.is_on();
    blocking(move || {
        let timeout = Duration::from_secs(180);
        let mut outputs: Vec<String> = Vec::new();
        for (i, args) in probe::PROBES.iter().enumerate() {
            let (code, stdout, stderr) =
                run_php(&prepared, backend.as_ref(), args, timeout, safe_mode)?;
            if i == 0 && code != Some(0) && probe::parse_version(&stdout).is_none() {
                let message = if stderr.trim().is_empty() {
                    stdout
                } else {
                    stderr
                };
                return Err(ForgeError::Other(match message.trim() {
                    "" => format!("The interpreter exited with code {code:?}"),
                    m => m.to_string(),
                }));
            }
            outputs.push(stdout);
        }
        InterpreterInfo::from_outputs(&outputs[0], &outputs[1], &outputs[2])
            .map_err(ForgeError::Other)
    })
    .await
}

/// "Show phpinfo": the output of `php -i`.
#[tauri::command]
pub async fn php_interpreter_phpinfo(
    interpreter: PhpInterpreter,
    state: State<'_, AppState>,
) -> Result<String> {
    phpinfo(&state, interpreter).await
}

/// `php -i` of an interpreter (stderr when it printed nothing).
pub(crate) async fn phpinfo(state: &AppState, interpreter: PhpInterpreter) -> Result<String> {
    let backend = state.backend()?;
    let prepared = prepare(state, backend.as_ref(), interpreter);
    let safe_mode = state.safe_mode.is_on();
    blocking(move || {
        let (_, stdout, stderr) = run_php(
            &prepared,
            backend.as_ref(),
            probe::PHPINFO,
            Duration::from_secs(180),
            safe_mode,
        )?;
        Ok(if stdout.trim().is_empty() {
            stderr
        } else {
            stdout
        })
    })
    .await
}

/// The mappings Forge derives for an interpreter (shown above the user's own).
#[tauri::command]
pub async fn php_interpreter_mappings(
    interpreter: PhpInterpreter,
    state: State<'_, AppState>,
) -> Result<Vec<PathMapping>> {
    let backend = state.backend()?;
    blocking(move || {
        let root = backend.info().root.to_string_lossy().replace('\\', "/");
        Ok(automatic_mappings(&interpreter, backend.as_ref(), &root))
    })
    .await
}

/// The services of compose files (the project's default compose file when `files` is
/// empty).
#[tauri::command]
pub async fn php_interpreter_compose_services(
    files: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    let backend = state.backend()?;
    blocking(move || {
        let root = backend.info().root.to_string_lossy().replace('\\', "/");
        let interp = PhpInterpreter {
            kind: InterpreterKind::Compose,
            compose_files: files,
            ..Default::default()
        };
        Ok(compose_service(&interp, backend.as_ref(), &root)
            .map(|(_, _, services)| services)
            .unwrap_or_default())
    })
    .await
}

/// Installed WSL distributions (`wsl.exe -l -q`); empty outside Windows.
#[tauri::command]
pub async fn php_interpreter_wsl_distributions() -> Result<Vec<String>> {
    if !cfg!(windows) {
        return Ok(Vec::new());
    }
    blocking(|| {
        let out = std::process::Command::new("wsl.exe")
            .args(["-l", "-q"])
            .output()
            .map_err(|e| ForgeError::Other(format!("Cannot run wsl.exe: {e}")))?;
        Ok(probe::parse_wsl_list(&out.stdout))
    })
    .await
}
