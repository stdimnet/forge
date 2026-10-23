//! Command Line Tool Support (Settings | Tools | Command Line Tool Support, Tools | Run
//! Command… / Ctrl+Shift+X): the configured and detected tools, their commands (from
//! `list --format=json` / `wp cli cmd-dump`, cached per tool in the user cache dir until
//! composer.lock changes), completion in the console and running commands. Tools run
//! through the project backend and the PHP interpreter (local, Docker, Compose, SSH, WSL);
//! output streams on `run://<runId>` like a Run tool window process. The logic lives in
//! `phpforge-cli-tools`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::CommandLine;
use phpforge_cli_tools::definitions::{
    parse_symfony_list, parse_wp_cli_dump, ArgumentDef, CommandDef,
};
use phpforge_cli_tools::invoke::{list_invocation, parse_input, run_invocation, ToolInvocation};
use phpforge_cli_tools::model::{
    detect, effective_tools, tools_from_setting, ListFormat, ShowConsoleIn, Tool, ToolKind,
    DETECTION_FILES,
};
use phpforge_cli_tools::{cache, Completion, AUTO_DETECT_KEY, TOOLS_KEY};
use phpforge_interpreters::Program;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::php_interpreters as interpreters;
use crate::commands::run::{start_built, Built, RunState};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// How long `list --format=json` may take (Composer with many plugins is slow).
const LIST_TIMEOUT: Duration = Duration::from_secs(60);

/// Detected tools and loaded commands, per project root.
#[derive(Default)]
pub struct CliToolsState {
    detected: Mutex<HashMap<String, Vec<Tool>>>,
    /// By `<root>\0<alias>`.
    commands: Mutex<HashMap<String, Loaded>>,
}

#[derive(Clone)]
struct Loaded {
    fingerprint: String,
    commands: ToolCommands,
}

/// The commands of a tool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCommands {
    pub alias: String,
    pub commands: Vec<CommandDef>,
    /// `cache`, `tool` (just listed), `builtin` (the tool could not run; a standard list)
    /// or `none`.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliRunStarted {
    pub run_id: String,
    pub command_line: String,
    pub alias: String,
    pub tool: String,
    pub show_console_in: ShowConsoleIn,
}

fn root_key(backend: &dyn Backend) -> String {
    backend.info().root.to_string_lossy().into_owned()
}

fn detect_tools(state: &AppState, backend: &dyn Backend) -> Vec<Tool> {
    let mut present: Vec<String> = Vec::new();
    for dir in ["", "bin", "vendor/bin"] {
        if let Ok(entries) = backend.list_dir(dir) {
            present.extend(entries.into_iter().map(|e| e.path));
        }
    }
    let composer = state
        .read_settings()
        .get_as::<String>(phpforge_composer::EXECUTABLE_KEY)
        .unwrap_or_default();
    detect(
        &|p| DETECTION_FILES.contains(&p) && present.iter().any(|x| x == p),
        &composer,
    )
}

/// The tools of the open project: configured ones plus detected ones. `refresh` detects
/// again; otherwise the last detection of this project is reused.
fn tools(state: &AppState, cli: &CliToolsState, refresh: bool) -> Result<Vec<Tool>> {
    let backend = state.backend()?;
    let key = root_key(backend.as_ref());
    let detected = {
        let cached = cli
            .detected
            .lock()
            .expect("cli tools poisoned")
            .get(&key)
            .cloned();
        match cached {
            Some(d) if !refresh => d,
            _ => {
                let d = detect_tools(state, backend.as_ref());
                cli.detected
                    .lock()
                    .expect("cli tools poisoned")
                    .insert(key, d.clone());
                d
            }
        }
    };
    let settings = state.read_settings();
    let configured = tools_from_setting(settings.get(TOOLS_KEY));
    let auto = settings.get_as::<bool>(AUTO_DETECT_KEY).unwrap_or(true);
    Ok(effective_tools(configured, detected, auto))
}

/// The command line of `inv`, through the tool's PHP interpreter when it runs PHP (or
/// names an interpreter).
fn command_line(
    state: &AppState,
    backend: &dyn Backend,
    tool: &Tool,
    inv: ToolInvocation,
) -> Result<Built> {
    let (program, through) = match &inv.program {
        Program::Php => (
            interpreters::local_php(state),
            Some(interpreters::resolve(state, &tool.interpreter)),
        ),
        Program::Other(p) => (
            p.clone(),
            interpreters::interpreter_set(state)
                .find(&tool.interpreter)
                .is_some()
                .then(|| interpreters::resolve(state, &tool.interpreter)),
        ),
    };
    let cmd = CommandLine {
        program,
        args: inv.args,
        cwd: backend.info().root.clone(),
        env: inv.env,
    };
    let Some(interp) = through else {
        return Ok(Built {
            cmd,
            path_mappings: Vec::new(),
            debug_server: None,
            output_filters: Vec::new(),
            tests: Default::default(),
        });
    };
    let prepared = interpreters::prepare(state, backend, interp);
    let program = match inv.program {
        Program::Php => Program::Php,
        Program::Other(_) => Program::Other(cmd.program.clone()),
    };
    Ok(Built {
        cmd: prepared.wrap(program, &cmd)?,
        path_mappings: prepared.output_mappings(),
        debug_server: None,
        output_filters: Vec::new(),
        tests: Default::default(),
    })
}

fn cache_file(root: &str, tool: &Tool) -> Option<PathBuf> {
    Some(
        dirs::cache_dir()?
            .join("forge")
            .join("command-line-tools")
            .join(cache::file_name(root, tool)),
    )
}

/// Commands every Laravel / Symfony application has, when the tool cannot run.
fn builtin(kind: ToolKind) -> Vec<CommandDef> {
    match kind {
        ToolKind::Artisan => phpforge_fw_laravel::artisan::builtin_commands()
            .into_iter()
            .map(|c| CommandDef {
                arguments: c
                    .arguments
                    .iter()
                    .map(|a| ArgumentDef {
                        name: a.clone(),
                        required: true,
                        ..Default::default()
                    })
                    .collect(),
                usage: c.usage.into_iter().collect(),
                name: c.name,
                description: c.description,
                ..Default::default()
            })
            .collect(),
        ToolKind::Symfony => phpforge_fw_symfony::console::builtin_commands()
            .into_iter()
            .map(|c| CommandDef {
                name: c.name,
                description: c.description,
                aliases: c.aliases,
                ..Default::default()
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The last lines of a failed list run, for the console's notice.
fn failure(code: Option<i32>, stderr: &str, stdout: &str) -> String {
    let text = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let tail: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(3)
        .collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    match (code, tail.is_empty()) {
        (Some(c), true) => format!("exited with code {c}"),
        (None, true) => "did not finish in time".into(),
        (_, false) => tail.join("\n"),
    }
}

/// Run a tool's list command and parse what it prints.
fn list_commands(
    backend: &dyn Backend,
    cmd: &CommandLine,
    format: ListFormat,
) -> std::result::Result<Vec<CommandDef>, String> {
    let out = capture(backend, cmd, "", LIST_TIMEOUT).map_err(|e| e.to_string())?;
    match format {
        ListFormat::WpCli => parse_wp_cli_dump(&out.stdout),
        _ => parse_symfony_list(&out.stdout),
    }
    .filter(|c| !c.is_empty())
    .ok_or_else(|| failure(out.code, &out.stderr, &out.stdout))
}

fn load_commands(
    state: &AppState,
    cli: &CliToolsState,
    backend: &dyn Backend,
    tool: &Tool,
    reload: bool,
) -> Result<ToolCommands> {
    let root = root_key(backend);
    let lock = backend.read_optional("composer.lock").ok().flatten();
    let fingerprint = cache::fingerprint(tool, lock.as_deref());
    let key = format!("{root}\0{}", tool.alias);
    if !reload {
        let mem = cli
            .commands
            .lock()
            .expect("cli tools poisoned")
            .get(&key)
            .cloned();
        if let Some(l) = mem.filter(|l| l.fingerprint == fingerprint) {
            return Ok(l.commands);
        }
    }
    let file = cache_file(&root, tool);
    let cached = (!reload)
        .then(|| file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()))
        .flatten()
        .and_then(|text| cache::read(&text, &fingerprint));
    let result = if let Some(commands) = cached {
        ToolCommands {
            alias: tool.alias.clone(),
            commands,
            source: "cache".into(),
            error: None,
        }
    } else if let Some(inv) = list_invocation(tool) {
        let built = command_line(state, backend, tool, inv)?;
        let (parsed, error) = match list_commands(backend, &built.cmd, tool.kind.format()) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e)),
        };
        match parsed {
            Some(commands) => {
                if let Some(f) = &file {
                    if let Some(dir) = f.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    if let Err(e) = std::fs::write(f, cache::write(&fingerprint, &commands)) {
                        tracing::warn!("cannot write {}: {e}", f.display());
                    }
                }
                ToolCommands {
                    alias: tool.alias.clone(),
                    commands,
                    source: "tool".into(),
                    error: None,
                }
            }
            None => {
                let commands = builtin(tool.kind);
                ToolCommands {
                    alias: tool.alias.clone(),
                    source: if commands.is_empty() {
                        "none"
                    } else {
                        "builtin"
                    }
                    .into(),
                    commands,
                    error: error.map(|e| format!("{} {}", built.cmd.display(), e)),
                }
            }
        }
    } else {
        ToolCommands {
            alias: tool.alias.clone(),
            commands: Vec::new(),
            source: "none".into(),
            error: None,
        }
    };
    cli.commands.lock().expect("cli tools poisoned").insert(
        key,
        Loaded {
            fingerprint,
            commands: result.clone(),
        },
    );
    Ok(result)
}

/// The project's command line tools: configured ones, then detected ones.
#[tauri::command]
pub async fn cli_tools_list(app: AppHandle) -> Result<Vec<Tool>> {
    blocking(move || {
        tools(
            &app.state::<AppState>(),
            &app.state::<CliToolsState>(),
            true,
        )
    })
    .await
}

/// The commands of the tool `alias`: from memory or the cache while composer.lock is
/// unchanged, else listed by running the tool. `reload` lists again.
#[tauri::command]
pub async fn cli_tools_commands(
    alias: String,
    reload: bool,
    app: AppHandle,
) -> Result<ToolCommands> {
    blocking(move || {
        let state = app.state::<AppState>();
        let cli = app.state::<CliToolsState>();
        let tools = tools(&state, &cli, false)?;
        let tool = tools
            .iter()
            .find(|t| t.alias == alias)
            .ok_or_else(|| ForgeError::Other(format!("unknown tool alias '{alias}'")))?;
        let backend = state.backend()?;
        load_commands(&state, &cli, backend.as_ref(), tool, reload)
    })
    .await
}

/// Completion of the console input at `offset` (UTF-16). Commands of the tool being typed
/// are loaded first when needed.
#[tauri::command]
pub async fn cli_tools_complete(
    input: String,
    offset: usize,
    app: AppHandle,
) -> Result<Completion> {
    blocking(move || {
        let state = app.state::<AppState>();
        let cli = app.state::<CliToolsState>();
        let tools = tools(&state, &cli, false)?;
        let mut commands = HashMap::new();
        let alias = phpforge_cli_tools::cmdline::split(&input)
            .into_iter()
            .next()
            .unwrap_or_default();
        if let Some(tool) = tools.iter().find(|t| t.enabled && t.alias == alias) {
            let backend = state.backend()?;
            let loaded = load_commands(&state, &cli, backend.as_ref(), tool, false)?;
            commands.insert(alias, loaded.commands);
        }
        Ok(phpforge_cli_tools::complete(
            &input, offset, &tools, &commands,
        ))
    })
    .await
}

/// Run the console input (`alias command args`) as run `runId`; its output streams on
/// `run://<runId>`, and `run_stop` / `run_write` / `run_dispose` apply.
#[tauri::command]
pub async fn cli_tools_run(
    input: String,
    run_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    runs: State<'_, RunState>,
) -> Result<CliRunStarted> {
    let handle = app.clone();
    // Preparing a remote interpreter reads compose files through the backend.
    let (tool, built) = blocking(move || {
        let state = handle.state::<AppState>();
        let tools = tools(&state, &handle.state::<CliToolsState>(), false)?;
        let parsed = parse_input(&input, &tools).map_err(ForgeError::Other)?;
        let tool = parsed.tool.clone();
        let backend = state.backend()?;
        let built = command_line(
            &state,
            backend.as_ref(),
            &tool,
            run_invocation(&tool, &parsed.args),
        )?;
        Ok((tool, built))
    })
    .await?;
    let started = start_built(run_id, built, false, app, &state, &runs).await?;
    Ok(CliRunStarted {
        run_id: started.run_id,
        command_line: started.command_line,
        tool: tool.display_name().to_string(),
        alias: tool.alias,
        show_console_in: tool.show_console_in,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use forge_backend::LocalBackend;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in for PHP: prints the recorded artisan list, or fails like a broken app.
    fn fake_php(dir: &std::path::Path) -> String {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packs/phpforge/cli-tools/tests/fixtures/artisan-list.json"
        );
        let script = dir.join("fake-php");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nif [ \"$2\" = list ]; then echo 'PHP Deprecated: x'; cat '{fixture}'; exit 0; fi\necho 'In Application.php line 12:' >&2\necho '  Class \"Redis\" not found' >&2\nexit 1\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script.to_string_lossy().into_owned()
    }

    #[test]
    fn lists_commands_through_the_backend() {
        let dir = tempfile::tempdir().unwrap();
        let php = fake_php(dir.path());
        let backend = LocalBackend::open(dir.path()).unwrap();
        let mut cmd = CommandLine::new(&php).args(["artisan", "list", "--format=json"]);
        cmd.cwd = dir.path().to_path_buf();
        let cmds = list_commands(&backend, &cmd, ListFormat::Symfony).unwrap();
        assert!(cmds.iter().any(|c| c.name == "make:model"));

        let mut broken = CommandLine::new(&php).args(["artisan", "about"]);
        broken.cwd = dir.path().to_path_buf();
        let err = list_commands(&backend, &broken, ListFormat::Symfony).unwrap_err();
        assert_eq!(
            err,
            "In Application.php line 12:\n  Class \"Redis\" not found"
        );
    }

    #[test]
    fn explains_failures_and_has_builtin_lists() {
        assert_eq!(failure(Some(255), "", ""), "exited with code 255");
        assert_eq!(failure(None, "", ""), "did not finish in time");
        assert_eq!(failure(Some(1), "", "a\nb\nc\nd\n"), "b\nc\nd");
        let artisan = builtin(ToolKind::Artisan);
        let model = artisan.iter().find(|c| c.name == "make:model").unwrap();
        assert_eq!(model.arguments[0].name, "name");
        assert!(builtin(ToolKind::Symfony)
            .iter()
            .any(|c| c.name == "cache:clear"));
        assert!(builtin(ToolKind::WpCli).is_empty());
    }
}
