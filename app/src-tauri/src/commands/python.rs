//! PyForge IPC: Python interpreters (detection, probe, new environments, the status bar
//! widget), the Python Packages tool window (installed packages, PyPI, install / uninstall
//! / upgrade commands), requirements inspections and completion, Reformat Code with ruff
//! format or Black, and the "Python" run configuration. The logic lives in
//! `packs/pyforge/*`; remote interpreters (Docker, Docker Compose, SSH, WSL) run through
//! the PHP interpreters' command lines (`php_interpreters::prepare`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::output::OutputPathMapping;
use forge_process::{CommandLine, RunConfiguration};
use phpforge_interpreters::Program;
use pyforge_interpreters::create::{self, NewEnvironment, ProjectFiles};
use pyforge_interpreters::detect::{self, Detected, SearchEnv};
use pyforge_interpreters::model::{GLOBAL_KEY, PROJECT_KEY, SELECTED_KEY};
use pyforge_interpreters::{probe, EnvKind, InterpreterInfo, InterpreterSet, PythonInterpreter};
use pyforge_lang_python::completion::{self, Context};
use pyforge_lang_python::deps::{self, DepFile};
use pyforge_lang_python::run::{self, RunMarker, Target};
use pyforge_lang_python::{ImportContext, InstalledPackage, Problem};
use pyforge_packages::net::Pypi;
use pyforge_packages::{listing, manager, Action, PackageInfo, SearchHit};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::php_interpreters;
use crate::commands::workspace::blocking;
use crate::state::AppState;

const FORMATTER_KEY: &str = "python.formatter";
const RUFF_PATH_KEY: &str = "python.ruff.path";
const BLACK_PATH_KEY: &str = "python.black.path";
const PYPI_KEY: &str = "python.packages.pypi";
const INDEX_KEY: &str = "python.packages.index";
const CHECK_REQUIREMENTS_KEY: &str = "python.inspections.requirements";
const CHECK_IMPORTS_KEY: &str = "python.inspections.imports";
const IGNORED_KEY: &str = "python.inspections.ignoredPackages";

/// Installed packages per interpreter (filled by `python_packages_list`, read by the
/// inspections) and the PyPI client.
#[derive(Default)]
pub struct PythonState {
    installed: Mutex<HashMap<String, Vec<InstalledPackage>>>,
    pypi: Mutex<Option<(String, Pypi)>>,
}

impl PythonState {
    fn pypi(&self, index: &str) -> Option<Pypi> {
        let mut guard = self.pypi.lock().expect("pypi lock");
        if let Some((i, p)) = guard.as_ref() {
            if i == index {
                return Some(p.clone());
            }
        }
        let p = Pypi::new(index, Pypi::default_cache_dir()?)
            .map_err(|e| tracing::warn!("no PyPI client: {e}"))
            .ok()?;
        *guard = Some((index.to_string(), p.clone()));
        Some(p)
    }
}

fn setting_bool(state: &AppState, key: &str, default: bool) -> bool {
    state.read_settings().get_as::<bool>(key).unwrap_or(default)
}

fn setting_string(state: &AppState, key: &str) -> String {
    state
        .read_settings()
        .get_as::<String>(key)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The configured interpreters of both settings layers.
fn interpreter_set(state: &AppState) -> InterpreterSet {
    let settings = state.read_settings();
    InterpreterSet::from_settings(settings.get(GLOBAL_KEY), settings.get(PROJECT_KEY))
}

/// The project's environments found without running anything (`.venv`, Poetry / Pipenv /
/// Hatch caches), for the automatic choice.
fn project_environments(root: &Path) -> Vec<Detected> {
    let env = SearchEnv {
        skip_system_dirs: true,
        path: None,
        ..SearchEnv::system(Some(root.to_path_buf()))
    };
    detect::detect(&env, &|_| None, &|_, _, _| None)
        .into_iter()
        .filter(|d| d.project)
        .collect()
}

fn from_detected(d: &Detected) -> PythonInterpreter {
    PythonInterpreter {
        name: d.path.clone(),
        kind: d.kind,
        path: d.path.clone(),
        env_dir: d.env_dir.clone(),
        version: d.version.clone(),
        ..Default::default()
    }
}

/// The interpreter in effect: `name` (a run configuration's own), else `python.interpreter`,
/// else the project's own environment. Details of a selected local path come from
/// detection of the project's environments when it is one of them.
pub(crate) fn resolve(state: &AppState, name: &str) -> Option<(PythonInterpreter, bool)> {
    let selected = match name.trim() {
        "" => setting_string(state, SELECTED_KEY),
        n => n.to_string(),
    };
    let local = state.project_info().is_some_and(|i| i.remote.is_none());
    let root = state.project_root().ok();
    let detected = || {
        root.as_deref()
            .filter(|_| local)
            .map(project_environments)
            .unwrap_or_default()
    };
    if let Some(mut interp) = interpreter_set(state).resolve(&selected) {
        if interp.kind == EnvKind::System && interp.version.is_empty() {
            if let Some(d) = detected().iter().find(|d| d.path == interp.path) {
                interp = from_detected(d);
            }
        }
        return Some((interp, false));
    }
    detected().first().map(|d| (from_detected(d), true))
}

/// What the status bar shows.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentInterpreter {
    pub interpreter: PythonInterpreter,
    /// `Python 3.12 (shop-api)`.
    pub label: String,
    /// Chosen automatically (the project's environment; nothing selected yet).
    pub automatic: bool,
}

#[tauri::command]
pub async fn python_interpreter_current(
    state: State<'_, AppState>,
) -> Result<Option<CurrentInterpreter>> {
    Ok(
        resolve(&state, "").map(|(interpreter, automatic)| CurrentInterpreter {
            label: interpreter.label(),
            interpreter,
            automatic,
        }),
    )
}

/// Interpreters on this machine and the project's environments (none for remote
/// projects: their host is not searched). Tools (`poetry env info -p`, `pipenv --venv`) are
/// asked only outside Safe Mode.
#[tauri::command]
pub async fn python_interpreters_detect(state: State<'_, AppState>) -> Result<Vec<Detected>> {
    if state.is_remote() {
        return Ok(Vec::new());
    }
    let root = state.project_root().ok();
    let safe_mode = state.safe_mode.is_on();
    blocking(move || {
        let env = SearchEnv::system(root);
        let timeout = Duration::from_secs(5);
        let tools = |program: &str, args: &[&str], cwd: &Path| -> Option<String> {
            if safe_mode {
                return None;
            }
            let exe = forge_langservers::Detector::system().find(program)?;
            detect::run_with_timeout(exe.as_os_str(), args, Some(cwd), Duration::from_secs(20))
        };
        Ok(detect::detect(
            &env,
            &|p| detect::version_of(p, timeout),
            &tools,
        ))
    })
    .await
}

/// A Python command line through the interpreter, with the output mappings of remote ones.
fn through(
    state: &AppState,
    backend: &dyn Backend,
    interp: &PythonInterpreter,
    program: Option<&str>,
    mut cmd: CommandLine,
) -> Result<(CommandLine, Vec<OutputPathMapping>)> {
    if cmd.cwd.as_os_str().is_empty() {
        cmd.cwd = backend.info().root.clone();
    }
    if !interp.kind.is_remote() {
        cmd.program = program.unwrap_or(interp.python()).to_string();
        return Ok((cmd, Vec::new()));
    }
    let prepared = php_interpreters::prepare(state, backend, interp.wrap_target());
    let program = program.unwrap_or(interp.python()).to_string();
    let wrapped = prepared.wrap(Program::Other(program), &cmd)?;
    Ok((wrapped, prepared.output_mappings()))
}

/// `python <args>` through the interpreter; refused in Safe Mode for an interpreter inside
/// the project (a project `.venv` runs the project's `.pth` files and `sitecustomize`) or a
/// Docker Compose service built from it.
fn python_command(
    state: &AppState,
    backend: &dyn Backend,
    interp: &PythonInterpreter,
    args: &[&str],
) -> Result<CommandLine> {
    let mut cmd = CommandLine::new("python").args(args.iter().copied());
    cmd.env.insert("PYTHONIOENCODING".into(), "utf-8".into());
    let (cmd, _) = through(state, backend, interp, None, cmd)?;
    if state.safe_mode.is_on()
        && (cmd.args.iter().any(|a| a == "compose")
            || forge_trust::runs_project_program(&backend.info().root, &cmd.program, &cmd.args))
    {
        return Err(ForgeError::Other(forge_trust::refusal(
            "Running the project's Python environment",
        )));
    }
    Ok(cmd)
}

/// Run a command and fail with its last error line unless it succeeds.
fn run_captured(backend: &dyn Backend, cmd: &CommandLine, timeout: Duration) -> Result<String> {
    let out = capture(backend, cmd, "", timeout)
        .map_err(|e| ForgeError::Other(format!("Cannot run {}: {e}", cmd.program)))?;
    if out.code != Some(0) {
        return Err(failure(out.code, out.stdout, out.stderr));
    }
    Ok(out.stdout)
}

fn failure(code: Option<i32>, stdout: String, stderr: String) -> ForgeError {
    let message = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    ForgeError::Other(match message.trim() {
        "" => format!("The interpreter exited with code {code:?}"),
        m => m.lines().last().unwrap_or(m).to_string(),
    })
}

/// Version, prefix and implementation of an interpreter (Docker images may be pulled).
#[tauri::command]
pub async fn python_interpreter_info(
    interpreter: PythonInterpreter,
    state: State<'_, AppState>,
) -> Result<InterpreterInfo> {
    let backend = state.backend()?;
    let cmd = python_command(
        &state,
        backend.as_ref(),
        &interpreter,
        &["-c", probe::PROBE_SCRIPT],
    )?;
    blocking(move || {
        let stdout = run_captured(backend.as_ref(), &cmd, Duration::from_secs(180))?;
        probe::parse_probe(&stdout).map_err(ForgeError::Other)
    })
    .await
}

/// The dependency files of the project root.
fn project_files(backend: &dyn Backend) -> ProjectFiles {
    let names: Vec<String> = backend
        .list_dir("")
        .map(|e| e.into_iter().map(|f| f.name).collect())
        .unwrap_or_default();
    let has = |n: &str| names.iter().any(|x| x == n);
    let mut requirements: Vec<String> = names
        .iter()
        .filter(|n| {
            deps::is_requirements_file(n) && !n.starts_with("constraints") && !n.ends_with(".in")
        })
        .cloned()
        .collect();
    requirements.sort_by_key(|n| (n != "requirements.txt", n.clone()));
    requirements.truncate(1);
    ProjectFiles {
        pyproject: has("pyproject.toml"),
        uv_lock: has("uv.lock"),
        poetry_lock: has("poetry.lock"),
        requirements,
    }
}

/// The commands that create a new environment ("Add Python Interpreter | Generate new");
/// the UI runs them one after another in the Run tool window and selects `python`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPlan {
    pub steps: Vec<CommandLine>,
    /// The new environment's python (empty for Poetry, which picks its own folder).
    pub python: String,
}

#[tauri::command]
pub async fn python_environment_plan(
    request: NewEnvironment,
    state: State<'_, AppState>,
) -> Result<EnvironmentPlan> {
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let files = project_files(backend.as_ref());
    let windows = cfg!(windows) && backend.info().remote.is_none();
    let mut request = request;
    if !request.location.trim().is_empty() && !Path::new(request.location.trim()).is_absolute() {
        request.location = root.join(request.location.trim()).display().to_string();
    }
    let steps = create::plan(&request, &files, windows).map_err(ForgeError::Other)?;
    let python = match request.kind {
        EnvKind::Poetry => String::new(),
        k => create::python_in(&request.location, windows, k == EnvKind::Conda),
    };
    Ok(EnvironmentPlan {
        steps: steps
            .into_iter()
            .map(|s| {
                let mut c = CommandLine::new(s.program).args(s.args);
                c.cwd = root.clone();
                c
            })
            .collect(),
        python,
    })
}

/// The installed packages of an interpreter.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageListing {
    pub interpreter: String,
    pub manager: manager::Manager,
    pub packages: Vec<InstalledPackage>,
}

fn cache_key(interp: &PythonInterpreter) -> String {
    format!("{}|{}|{}", interp.kind.id(), interp.name, interp.path)
}

/// List the packages of `interpreter` (the project's when empty) and remember them for the
/// requirements inspections.
#[tauri::command]
pub async fn python_packages_list(
    interpreter: Option<String>,
    state: State<'_, AppState>,
    python: State<'_, PythonState>,
) -> Result<Option<PackageListing>> {
    let Some((interp, _)) = resolve(&state, interpreter.as_deref().unwrap_or("")) else {
        return Ok(None);
    };
    let backend = state.backend()?;
    let cmd = python_command(
        &state,
        backend.as_ref(),
        &interp,
        &["-c", listing::LIST_SCRIPT],
    )?;
    let b = backend.clone();
    let packages = blocking(move || {
        let stdout = run_captured(b.as_ref(), &cmd, Duration::from_secs(120))?;
        listing::parse(&stdout).map_err(ForgeError::Other)
    })
    .await?;
    python
        .installed
        .lock()
        .expect("installed lock")
        .insert(cache_key(&interp), packages.clone());
    let uv_lock = matches!(backend.read_optional("uv.lock"), Ok(Some(_)));
    Ok(Some(PackageListing {
        interpreter: interp.label(),
        manager: manager::choose(interp.kind, uv_lock),
        packages,
    }))
}

/// The command line that installs, uninstalls or upgrades `packages` (requirement specs) in
/// the interpreter's environment; the UI starts it with `run_start` (refused in Safe Mode).
#[tauri::command]
pub async fn python_packages_command(
    action: Action,
    packages: Vec<String>,
    interpreter: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    let (interp, _) = resolve(&state, interpreter.as_deref().unwrap_or("")).ok_or_else(|| {
        ForgeError::Other("No Python interpreter is configured for the project".into())
    })?;
    if packages
        .iter()
        .any(|p| p.trim().is_empty() || p.trim_start().starts_with('-'))
    {
        return Err(ForgeError::Other("not a package requirement".into()));
    }
    let backend = state.backend()?;
    let uv_lock = matches!(backend.read_optional("uv.lock"), Ok(Some(_)));
    let mgr = if interp.kind.is_remote() {
        manager::Manager::Pip
    } else {
        manager::choose(interp.kind, uv_lock)
    };
    let (program, args) = manager::command(mgr, action, &packages, interp.python());
    let mut cmd = CommandLine::new("python").args(args);
    cmd.env
        .insert("PIP_DISABLE_PIP_VERSION_CHECK".into(), "1".into());
    let tool = (mgr != manager::Manager::Pip).then_some(program.as_str());
    through(&state, backend.as_ref(), &interp, tool, cmd).map(|(c, _)| c)
}

fn index(state: &AppState) -> Option<String> {
    if !setting_bool(state, PYPI_KEY, true) {
        return None;
    }
    Some(match setting_string(state, INDEX_KEY) {
        s if s.is_empty() => pyforge_packages::pypi::PYPI_URL.to_string(),
        s => s,
    })
}

/// The latest PyPI release of each package (unknown or unreachable ones are left out).
#[tauri::command]
pub async fn python_packages_latest(
    names: Vec<String>,
    state: State<'_, AppState>,
    python: State<'_, PythonState>,
) -> Result<HashMap<String, String>> {
    let Some(pypi) = index(&state).and_then(|i| python.pypi(&i)) else {
        return Ok(HashMap::new());
    };
    let mut tasks = Vec::new();
    for name in names.into_iter().take(500) {
        let p = pypi.clone();
        tasks.push(tauri::async_runtime::spawn(async move {
            p.package(&name).await.ok().map(|info| (name, info.version))
        }));
    }
    let mut out = HashMap::new();
    for t in tasks {
        if let Ok(Some((name, version))) = t.await {
            out.insert(name, version);
        }
    }
    Ok(out)
}

/// Search PyPI.
#[tauri::command]
pub async fn python_packages_search(
    query: String,
    state: State<'_, AppState>,
    python: State<'_, PythonState>,
) -> Result<Vec<SearchHit>> {
    let pypi = index(&state).and_then(|i| python.pypi(&i)).ok_or_else(|| {
        ForgeError::Other("PyPI lookups are turned off in Settings | Python".into())
    })?;
    pypi.search(&query).await.map_err(ForgeError::Other)
}

/// A package's summary and releases.
#[tauri::command]
pub async fn python_package_info(
    name: String,
    state: State<'_, AppState>,
    python: State<'_, PythonState>,
) -> Result<PackageInfo> {
    let pypi = index(&state).and_then(|i| python.pypi(&i)).ok_or_else(|| {
        ForgeError::Other("PyPI lookups are turned off in Settings | Python".into())
    })?;
    pypi.package(&name).await.map_err(ForgeError::Other)
}

/// The declared dependencies of the project root (pyproject.toml and requirements files),
/// the file "Add requirement" writes to, and the project's own name.
fn declared(backend: &dyn Backend) -> (Vec<String>, Option<String>, Option<String>) {
    let mut names = Vec::new();
    let mut target = None;
    let mut project = None;
    let entries = backend.list_dir("").unwrap_or_default();
    if let Ok(Some(text)) = backend.read_optional("pyproject.toml") {
        let f = deps::pyproject(&text);
        if f.has_pep621 || f.has_poetry {
            target = Some("pyproject.toml".to_string());
        }
        project = f.project_name.clone();
        names.extend(f.deps.into_iter().map(|d| d.normalized));
    }
    let mut req_files: Vec<String> = entries
        .iter()
        .map(|e| e.name.clone())
        .filter(|n| deps::is_requirements_file(n))
        .collect();
    if entries.iter().any(|e| e.name == "requirements") {
        if let Ok(list) = backend.list_dir("requirements") {
            req_files.extend(
                list.into_iter()
                    .filter(|e| e.name.ends_with(".txt"))
                    .map(|e| e.path),
            );
        }
    }
    req_files.sort_by_key(|n| (n != "requirements.txt", n.clone()));
    for f in &req_files {
        if let Ok(Some(text)) = backend.read_optional(f) {
            names.extend(
                deps::requirements_txt(&text)
                    .deps
                    .into_iter()
                    .map(|d| d.normalized),
            );
        }
    }
    if target.is_none() {
        target = req_files.first().cloned();
    }
    (names, target, project)
}

/// Top-level modules of the project itself: `*.py` files and packages in the root and in
/// `src/`.
fn local_modules(backend: &dyn Backend) -> Vec<String> {
    let mut out = Vec::new();
    for dir in ["", "src"] {
        for e in backend.list_dir(dir).unwrap_or_default() {
            if let Some(m) = e.name.strip_suffix(".py") {
                out.push(m.to_string());
            } else if e.kind == forge_core::FileKind::Dir && !e.name.starts_with('.') {
                // Namespace packages have no __init__.py; any folder may be imported.
                out.push(e.name.clone());
            }
        }
    }
    out
}

/// Requirements inspections of a file: unsatisfied dependencies in requirements files and
/// pyproject.toml, undeclared imports in Python files. Uses the packages of the last
/// `python_packages_list` (nothing about the environment before that).
#[tauri::command]
pub async fn python_inspect(
    path: String,
    content: String,
    state: State<'_, AppState>,
    python: State<'_, PythonState>,
) -> Result<Vec<Problem>> {
    let check_reqs = setting_bool(&state, CHECK_REQUIREMENTS_KEY, true);
    let check_imports = setting_bool(&state, CHECK_IMPORTS_KEY, true);
    let ignored: Vec<String> = state
        .read_settings()
        .get_as::<Vec<String>>(IGNORED_KEY)
        .unwrap_or_default();
    let installed = resolve(&state, "").and_then(|(i, _)| {
        python
            .installed
            .lock()
            .expect("installed lock")
            .get(&cache_key(&i))
            .cloned()
    });
    let backend = state.backend()?;
    blocking(move || {
        let is_python = path.ends_with(".py") || path.ends_with(".pyw");
        let ctx = if is_python && check_imports {
            let (declared, target, project_name) = declared(backend.as_ref());
            ImportContext {
                declared,
                installed: installed.clone(),
                local_modules: local_modules(backend.as_ref()),
                project_name,
                target,
                ignored: ignored.clone(),
            }
        } else {
            ImportContext {
                ignored: ignored.clone(),
                ..Default::default()
            }
        };
        if is_python && !check_imports {
            return Ok(Vec::new());
        }
        let installed = if check_reqs {
            installed.as_deref()
        } else {
            None
        };
        Ok(pyforge_lang_python::inspect(&path, &content, installed, &ctx).unwrap_or_default())
    })
    .await
}

/// "Add requirement": the dependency file's new text (the UI applies it to an open editor
/// or saves it).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEdit {
    pub path: String,
    pub text: String,
}

#[tauri::command]
pub async fn python_add_requirement(
    requirement: String,
    target: String,
    current: Option<String>,
    state: State<'_, AppState>,
) -> Result<FileEdit> {
    if pyforge_lang_python::pep508::parse(&requirement).is_err() {
        return Err(ForgeError::Other(format!(
            "'{requirement}' is not a requirement"
        )));
    }
    let backend = state.backend()?;
    let text = match current {
        Some(t) => t,
        None => backend.read_optional(&target)?.unwrap_or_default(),
    };
    let name = target.rsplit('/').next().unwrap_or(&target);
    let new_text = if name == "pyproject.toml" {
        deps::add_to_pyproject(&text, &requirement).ok_or_else(|| {
            ForgeError::Other("pyproject.toml has no [project] or Poetry dependencies".into())
        })?
    } else {
        deps::add_to_requirements(&text, &requirement)
    };
    Ok(FileEdit {
        path: target,
        text: new_text,
    })
}

/// One completion item.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub detail: String,
    /// `package` or `version`.
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    pub from: u32,
    pub items: Vec<CompletionItem>,
}

/// Package names and versions in requirements files and pyproject.toml.
#[tauri::command]
pub async fn python_complete(
    path: String,
    content: String,
    offset: u32,
    state: State<'_, AppState>,
    python: State<'_, PythonState>,
) -> Result<Option<Completion>> {
    let ctx = if path.ends_with("pyproject.toml") {
        completion::pyproject_context(&content, offset)
    } else if deps::is_requirements_file(&path) {
        completion::requirements_context(&content, offset)
    } else {
        None
    };
    let Some(ctx) = ctx else {
        return Ok(None);
    };
    let pypi = index(&state).and_then(|i| python.pypi(&i));
    let installed: Vec<InstalledPackage> = resolve(&state, "")
        .and_then(|(i, _)| {
            python
                .installed
                .lock()
                .expect("installed lock")
                .get(&cache_key(&i))
                .cloned()
        })
        .unwrap_or_default();
    match ctx {
        Context::Name { from, prefix } => {
            let lower = prefix.to_ascii_lowercase();
            let mut items: Vec<CompletionItem> = installed
                .iter()
                .filter(|p| p.name.to_ascii_lowercase().starts_with(&lower))
                .map(|p| CompletionItem {
                    label: p.name.clone(),
                    detail: format!("{} (installed)", p.version),
                    kind: "package".into(),
                })
                .collect();
            if let (Some(pypi), true) = (pypi, prefix.len() >= 2) {
                if let Ok(hits) = pypi.search(&prefix).await {
                    for h in hits {
                        if !items.iter().any(|i| i.label.eq_ignore_ascii_case(&h.name)) {
                            items.push(CompletionItem {
                                label: h.name,
                                detail: h.summary,
                                kind: "package".into(),
                            });
                        }
                    }
                }
            }
            Ok(Some(Completion { from, items }))
        }
        Context::Version {
            from,
            prefix,
            package,
        } => {
            let mut versions: Vec<String> = Vec::new();
            if let Some(pypi) = pypi {
                if let Ok(info) = pypi.package(&package).await {
                    versions = info.versions;
                }
            }
            let installed_version = installed
                .iter()
                .find(|p| {
                    pyforge_lang_python::normalize(&p.name)
                        == pyforge_lang_python::normalize(&package)
                })
                .map(|p| p.version.clone());
            if let Some(v) = &installed_version {
                if !versions.contains(v) {
                    versions.insert(0, v.clone());
                }
            }
            let items = versions
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .take(200)
                .map(|v| CompletionItem {
                    detail: if Some(&v) == installed_version.as_ref() {
                        "installed".into()
                    } else {
                        String::new()
                    },
                    label: v,
                    kind: "version".into(),
                })
                .collect();
            Ok(Some(Completion { from, items }))
        }
    }
}

/// `if __name__ == "__main__":` lines of a file (gutter ▶).
#[tauri::command]
pub fn python_run_markers(content: String) -> Vec<RunMarker> {
    run::run_markers(&content)
}

/// Formatters finish quickly; a hanging one is stopped.
const FORMAT_TIMEOUT: Duration = Duration::from_secs(30);

/// Reformat Code: the file's text formatted by ruff format or Black (`None` when the
/// formatter is off, set to the language server, or not installed).
#[tauri::command]
pub async fn python_format(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    let formatter = setting_string(&state, FORMATTER_KEY);
    let (names, custom, label): (Vec<String>, String, &str) = match formatter.as_str() {
        "black" => (
            vec!["black".into()],
            setting_string(&state, BLACK_PATH_KEY),
            "Black",
        ),
        "lsp" | "none" => return Ok(None),
        _ => (
            vec!["ruff".into()],
            setting_string(&state, RUFF_PATH_KEY),
            "ruff",
        ),
    };
    // The selected environment's tools first (`.venv/bin/ruff`).
    let env_bin = resolve(&state, "")
        .filter(|(i, _)| {
            !i.kind.is_remote() && setting_bool(&state, "python.lsp.useEnvironmentTools", true)
        })
        .and_then(|(i, _)| Path::new(i.python()).parent().map(Path::to_path_buf));
    let backend = state.backend()?;
    let safe_mode = state.safe_mode.is_on();
    let black = formatter == "black";
    blocking(move || {
        let file = backend.resolve(&path)?;
        let program = if !custom.is_empty() {
            backend.find_executable(&[], Some(&custom))?
        } else {
            let in_env = env_bin
                .filter(|_| backend.info().remote.is_none())
                .map(|d| {
                    d.join(if cfg!(windows) {
                        format!("{}.exe", names[0])
                    } else {
                        names[0].clone()
                    })
                })
                .filter(|p| p.is_file());
            match in_env {
                Some(p) => Some(p),
                None => backend
                    .find_executable(&names, None)?
                    .or_else(|| managed_tool(&names[0])),
            }
        };
        let Some(program) = program else {
            return Ok(None);
        };
        if safe_mode
            && forge_trust::runs_project_program(
                &backend.info().root,
                &program.to_string_lossy(),
                &[],
            )
        {
            return Err(ForgeError::Other(forge_trust::refusal(&format!(
                "Running the project's {label}"
            ))));
        }
        let mut cmd = CommandLine::new(program.to_string_lossy());
        cmd = if black {
            cmd.args(["-q", "--stdin-filename"])
                .arg(file.to_string_lossy())
                .arg("-")
        } else {
            cmd.args(["format", "--stdin-filename"])
                .arg(file.to_string_lossy())
                .arg("-")
        };
        cmd.cwd = backend.info().root.clone();
        let out = capture(backend.as_ref(), &cmd, &content, FORMAT_TIMEOUT)?;
        match out.code {
            Some(0) => Ok(Some(out.stdout)),
            None => Err(ForgeError::Other(format!("{label} did not finish"))),
            Some(_) => {
                let msg = out.stderr.trim();
                Err(ForgeError::Other(if msg.is_empty() {
                    format!("{label} failed")
                } else {
                    msg.lines().next().unwrap_or(msg).to_string()
                }))
            }
        }
    })
    .await
}

/// ruff downloaded by Forge (`<data dir>/forge/langservers/bin`).
fn managed_tool(name: &str) -> Option<PathBuf> {
    let dir = forge_langservers::managed::pypi::bin_dir(
        &forge_langservers::detect::default_managed_dir()?,
    );
    let p = dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    p.is_file().then_some(p)
}

/// The "Python" run configuration: `python [options] (<script> | -m <module>) [parameters]`
/// through the configuration's interpreter (or the project's).
pub(crate) fn build_run(
    state: &AppState,
    cfg: &RunConfiguration,
) -> Result<(CommandLine, Vec<OutputPathMapping>)> {
    let (interp, _) = resolve(state, &cfg.interpreter).ok_or_else(|| {
        ForgeError::Other("No Python interpreter is configured: choose one in Settings | Project | Python Interpreter".into())
    })?;
    let target = Target::parse(cfg.option("target"));
    let args = run::arguments(
        target,
        cfg.option("script"),
        &forge_process::split_args(&cfg.interpreter_options),
        &forge_process::split_args(&cfg.arguments),
    )
    .map_err(ForgeError::Other)?;
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let mut cmd = CommandLine::new("python").args(args);
    cmd.cwd = match cfg.working_directory.trim() {
        "" => root.clone(),
        d if Path::new(d).is_absolute() => PathBuf::from(d),
        d => root.join(d),
    };
    cmd.env.insert("PYTHONUNBUFFERED".into(), "1".into());
    cmd.env.insert("PYTHONIOENCODING".into(), "utf-8".into());
    for (k, v) in &cfg.env {
        cmd.env.insert(k.clone(), v.clone());
    }
    through(state, backend.as_ref(), &interp, None, cmd)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepFileRequest {
    pub path: String,
    pub content: String,
}

/// The dependencies of a requirements file or pyproject.toml (the Python Packages tool
/// window's "declared" marks and Install Requirements).
#[tauri::command]
pub fn python_dependencies(request: DepFileRequest) -> DepFile {
    if request.path.ends_with("pyproject.toml") {
        deps::pyproject(&request.content)
    } else {
        deps::requirements_txt(&request.content)
    }
}
