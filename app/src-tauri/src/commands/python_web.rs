//! PyForge web frameworks IPC (PyCharm Professional's Django, Flask, FastAPI and Celery
//! support); the analysis lives in `packs/pyforge/frameworks`:
//!
//! - the project model for the Endpoints and Django Structure tool windows (routes,
//!   models, Celery tasks, template folders), read from the project's files in one
//!   round trip (also for remote projects) and cached for a few seconds;
//! - template and URL name navigation, completion, gutter marks and inspections for the
//!   editor, on the editor's text;
//! - the commands of Tools | Run manage.py Task…: `manage.py help --commands` when the
//!   project is trusted (it runs project code), else Django's built-in commands and the
//!   project's `management/commands`;
//! - `run::build` hands the web run configurations to [`to_python`], which turns them into
//!   Python runs (interpreters, run targets, debugpy).

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use forge_backend::Backend;
use forge_core::{CollectRequest, ForgeError, Result};
use forge_process::RunConfiguration;
use pyforge_frameworks::django::{self, ManageCommand};
use pyforge_frameworks::run::{self, PythonRun};
use pyforge_frameworks::templates::{self, Completions, FileMarks};
use pyforge_frameworks::{Location, Options, Project, WebProject};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::python::{self, run_captured, setting_bool, setting_string};
use crate::commands::test_frameworks;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// How long the analysed project is reused before the files are read again.
const TTL: Duration = Duration::from_secs(5);
const MAX_FILES: usize = 20_000;
const MAX_FILE_SIZE: u64 = 1024 * 1024;
const MAX_TOTAL_SIZE: u64 = 48 * 1024 * 1024;
const HELP_TIMEOUT: Duration = Duration::from_secs(30);

struct Cached {
    key: String,
    options: Options,
    at: Instant,
    project: Arc<Project>,
}

static CACHE: LazyLock<Mutex<Option<Cached>>> = LazyLock::new(|| Mutex::new(None));
/// `manage.py help --commands` per project (it boots Django: slow).
static COMMANDS: LazyLock<Mutex<BTreeMap<String, Vec<ManageCommand>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn options(state: &AppState) -> Options {
    Options {
        settings_module: setting_string(state, "django.settingsModule"),
        manage_script: setting_string(state, "django.manageScript"),
        flask_app: setting_string(state, "flask.app"),
        fastapi_app: setting_string(state, "fastapi.app"),
        celery_app: setting_string(state, "celery.app"),
    }
}

fn skipped(path: &str) -> bool {
    [
        ".venv/",
        "venv/",
        "node_modules/",
        ".tox/",
        "site-packages/",
        "__pycache__/",
    ]
    .iter()
    .any(|d| path.starts_with(d) || path.contains(&format!("/{d}")))
}

/// Read the project's Python, template and dependency files.
fn load(backend: &dyn Backend, options: Options) -> Result<Project> {
    let mut files: BTreeMap<String, String> = backend
        .collect_files(&CollectRequest {
            extensions: pyforge_frameworks::EXTENSIONS
                .iter()
                .map(|e| e.to_string())
                .collect(),
            max_files: MAX_FILES,
            max_file_size: MAX_FILE_SIZE,
            max_total: MAX_TOTAL_SIZE,
        })?
        .into_iter()
        .filter(|f| !skipped(&f.path))
        .map(|f| (f.path, f.text))
        .collect();
    for name in pyforge_frameworks::FILE_NAMES {
        if let Ok(Some(text)) = backend.read_optional(name) {
            files.insert(name.to_string(), text);
        }
    }
    Ok(Project::new(files, options))
}

/// The analysed project, read again after [`TTL`] or with `refresh`.
pub(crate) fn project(state: &AppState, refresh: bool) -> Result<Arc<Project>> {
    let backend = state.backend()?;
    let key = backend.info().display_root();
    let options = options(state);
    let mut guard = CACHE
        .lock()
        .map_err(|_| ForgeError::Other("python web cache poisoned".into()))?;
    if let Some(c) = guard.as_ref() {
        if !refresh && c.key == key && c.options == options && c.at.elapsed() < TTL {
            return Ok(c.project.clone());
        }
    }
    let project = Arc::new(load(backend.as_ref(), options.clone())?);
    *guard = Some(Cached {
        key,
        options,
        at: Instant::now(),
        project: project.clone(),
    });
    Ok(project)
}

/// Frameworks, routes, models, tasks and templates of the project.
#[tauri::command]
pub async fn python_web_project(refresh: bool, app: AppHandle) -> Result<WebProject> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(project(&state, refresh)?.info.clone())
    })
    .await
}

/// Go to Declaration on a template name, `{% url %}` name or `url_for` endpoint.
#[tauri::command]
pub async fn python_web_goto(
    path: String,
    content: String,
    offset: usize,
    app: AppHandle,
) -> Result<Vec<Location>> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(templates::goto(
            &*project(&state, false)?,
            &path,
            &content,
            offset,
        ))
    })
    .await
}

/// Template names, URL names or endpoints at `offset` (UTF-16).
#[tauri::command]
pub async fn python_web_complete(
    path: String,
    content: String,
    offset: usize,
    app: AppHandle,
) -> Result<Option<Completions>> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(templates::complete(
            &*project(&state, false)?,
            &path,
            &content,
            offset,
        ))
    })
    .await
}

/// Gutter marks and problems (unresolved templates and URL names) of an editor.
#[tauri::command]
pub async fn python_web_marks(path: String, content: String, app: AppHandle) -> Result<FileMarks> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(templates::marks(&*project(&state, false)?, &path, &content))
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManageCommands {
    pub commands: Vec<ManageCommand>,
    /// `manage.py` (from `help --commands`) or `static` (built in plus the project's).
    pub source: String,
    /// Why the commands are not the project's own list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The commands of Tools | Run manage.py Task….
#[tauri::command]
pub async fn python_manage_commands(refresh: bool, app: AppHandle) -> Result<ManageCommands> {
    blocking(move || {
        let state = app.state::<AppState>();
        manage_commands(&state, refresh)
    })
    .await
}

fn manage_commands(state: &AppState, refresh: bool) -> Result<ManageCommands> {
    let p = project(state, false)?;
    let known = django::static_commands(&p);
    let fallback = |message: Option<String>| ManageCommands {
        commands: known.clone(),
        source: "static".into(),
        message,
    };
    let Some(d) = p.info.django.clone() else {
        return Ok(fallback(Some("No manage.py found".into())));
    };
    if !setting_bool(state, "django.manageCommandsFromProject", true) {
        return Ok(fallback(None));
    }
    if state.safe_mode.is_on() {
        return Ok(fallback(Some(
            "Safe Mode: the project's own commands are listed once you trust the project".into(),
        )));
    }
    let backend = state.backend()?;
    let key = backend.info().display_root();
    if !refresh {
        if let Some(c) = COMMANDS.lock().ok().and_then(|m| m.get(&key).cloned()) {
            return Ok(ManageCommands {
                commands: c,
                source: "manage.py".into(),
                message: None,
            });
        }
    }
    let Some((interp, _)) = python::resolve(state, "") else {
        return Ok(fallback(Some("No Python interpreter is configured".into())));
    };
    let script = d.manage.rsplit('/').next().unwrap_or(&d.manage).to_string();
    let mut cmd = match python::python_command(
        state,
        backend.as_ref(),
        &interp,
        &[script.as_str(), "help", "--commands"],
    ) {
        Ok(c) => c,
        Err(e) => return Ok(fallback(Some(e.to_string()))),
    };
    cmd.cwd = python::working_dir(&backend.info().root, &d.root);
    if let Some(s) = &d.settings_module {
        cmd.env
            .entry("DJANGO_SETTINGS_MODULE".into())
            .or_insert_with(|| s.clone());
    }
    match run_captured(backend.as_ref(), &cmd, HELP_TIMEOUT) {
        Ok(out) => {
            let commands = django::parse_help_commands(&out, &known);
            if commands.is_empty() {
                return Ok(fallback(Some("manage.py listed no commands".into())));
            }
            if let Ok(mut m) = COMMANDS.lock() {
                m.insert(key, commands.clone());
            }
            Ok(ManageCommands {
                commands,
                source: "manage.py".into(),
                message: None,
            })
        }
        Err(e) => Ok(fallback(Some(format!(
            "manage.py help --commands failed: {e}"
        )))),
    }
}

/// What a web run configuration starts (the URL to open, the port), for the UI.
#[tauri::command]
pub async fn python_web_run_info(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<PythonRun> {
    blocking(move || {
        let state = app.state::<AppState>();
        let p = project(&state, false)?;
        run::build(&test_frameworks::to_runtools(&configuration)?, &p.info)
            .map_err(ForgeError::Other)
    })
    .await
}

/// A web configuration as the `python` configuration that runs it, and the server's port.
pub(crate) fn to_python(
    state: &AppState,
    cfg: &RunConfiguration,
) -> Result<(RunConfiguration, Option<u16>)> {
    let p = project(state, false)?;
    let run =
        run::build(&test_frameworks::to_runtools(cfg)?, &p.info).map_err(ForgeError::Other)?;
    let mut out = cfg.clone();
    out.type_id = "python".into();
    out.options = BTreeMap::from([
        ("target".to_string(), run.target),
        ("script".to_string(), run.script),
    ]);
    let mut args: Vec<String> = run
        .args
        .iter()
        .map(|a| forge_runtools::args::quote_arg(a))
        .collect();
    if !cfg.arguments.trim().is_empty() {
        args.push(cfg.arguments.trim().to_string());
    }
    out.arguments = args.join(" ");
    let mut env = run.env;
    env.extend(cfg.env.clone());
    out.env = env;
    if out.working_directory.trim().is_empty() {
        if let Some(dir) = run.working_directory {
            out.working_directory = dir;
        }
    }
    Ok((out, run.port))
}

/// Django tests: the project's `manage.py`, settings and folder when the configuration
/// leaves them empty.
pub(crate) fn fill_django_tests(state: &AppState, cfg: &mut RunConfiguration) -> Result<()> {
    let p = project(state, false)?;
    let Some(d) = p.info.django.as_ref() else {
        return Ok(());
    };
    let root = state.backend()?.info().root.clone();
    if cfg.option("manage").trim().is_empty() {
        cfg.options.insert(
            "manage".into(),
            root.join(&d.manage).to_string_lossy().replace('\\', "/"),
        );
    }
    if cfg.option("settings").trim().is_empty() {
        if let Some(s) = &d.settings_module {
            cfg.options.insert("settings".into(), s.clone());
        }
    }
    if cfg.working_directory.trim().is_empty() && !d.root.is_empty() {
        cfg.working_directory = d.root.clone();
    }
    Ok(())
}
