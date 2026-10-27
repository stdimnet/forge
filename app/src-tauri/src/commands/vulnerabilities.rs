//! Vulnerable dependency check (`forge-vulns`): runs each ecosystem's audit tool on the
//! project's machine, caches the result per manifest and lock file in
//! `<cache dir>/forge/vulnerabilities/<project>/`, and answers the inspection and the
//! Upgrade quick fix of the UI.
//!
//! Audits run project tooling (Composer plugins, Yarn plugins, the project's Python
//! environment) and send package lists to advisory databases, so they are refused in Safe
//! Mode and skipped while `vulnerabilities.enabled` is off.
//!
//! IPC: `vulnerabilities_check({ force, tools? }) -> { enabled, results }`,
//! `vulnerabilities_problems({ path, text, vulnerabilities, notices }) -> FileProblem[]`,
//! `vulnerabilities_upgrade({ path, text, package, fixed }) -> string | null`,
//! `vulnerabilities_install_command({ tool, dir, package?, fixed? }) -> CommandLine`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::CommandLine;
use forge_vulns::problems::{self, FileProblem};
use forge_vulns::report::{self, CacheEntry, TargetResult};
use forge_vulns::tool::{self, DetectOptions, Target, Tool};
use forge_vulns::{Notice, Vulnerability, CACHE_HOURS_KEY, DEPENDENCY_CHECK_KEY, ENABLED_KEY};
use pyforge_packages::Action;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::workspace::blocking;
use crate::commands::{composer, python};
use crate::state::AppState;

/// One check at a time (project open, lock file changes and the action may overlap).
#[derive(Default)]
pub struct VulnerabilitiesState {
    running: tokio::sync::Mutex<()>,
}

const RUN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    /// `vulnerabilities.enabled`: nothing was checked while it is off.
    pub enabled: bool,
    pub results: Vec<TargetResult>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `<cache dir>/forge/vulnerabilities/<project hash>`.
fn cache_dir(state: &AppState) -> Option<PathBuf> {
    let info = state.project_info()?;
    let key = crate::state::trust_key(&info);
    Some(
        dirs::cache_dir()?
            .join("forge")
            .join("vulnerabilities")
            .join(tool::fnv1a(&[&key])),
    )
}

fn cache_file(dir: &Path, target: &Target) -> PathBuf {
    dir.join(format!(
        "{}-{}.json",
        target.tool.id(),
        tool::fnv1a(&[&target.dir])
    ))
}

fn read_cache(dir: Option<&Path>, target: &Target) -> Option<CacheEntry> {
    let text = std::fs::read_to_string(cache_file(dir?, target)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_cache(dir: Option<&Path>, target: &Target, entry: &CacheEntry) {
    let Some(dir) = dir else { return };
    let written = std::fs::create_dir_all(dir).and_then(|_| {
        std::fs::write(
            cache_file(dir, target),
            serde_json::to_string(entry).unwrap_or_default(),
        )
    });
    if let Err(e) = written {
        tracing::warn!("cannot cache the vulnerability check: {e}");
    }
}

fn find(backend: &Arc<dyn Backend>, name: &str) -> Option<String> {
    backend
        .find_executable(&[name.to_string()], None)
        .ok()
        .flatten()
        .map(|p| p.display().to_string())
}

/// The command line of a target's audit; `Ok(None)` when its tool is not installed.
fn audit_command(
    state: &AppState,
    backend: &Arc<dyn Backend>,
    target: &Target,
) -> Result<Option<CommandLine>> {
    let cwd = backend.info().root.join(&target.dir);
    let cmd = match target.tool {
        Tool::Composer => {
            let opts = composer::options(state);
            let (program, lead) = forge_process::composer_program(&opts.executable, &opts.php);
            let mut cmd = CommandLine::new(program)
                .args(lead)
                .args(target.tool.args().iter().copied());
            // The lock file when there is one, else what is installed in vendor/.
            if target.lockfile.is_some() {
                cmd = cmd.arg("--locked");
            }
            cmd
        }
        Tool::PipAudit => {
            let Some((interp, _)) = python::resolve(state, "") else {
                return Ok(None);
            };
            python::python_command(state, backend.as_ref(), &interp, target.tool.args())?
        }
        Tool::CargoAudit => {
            if find(backend, "cargo-audit").is_none() {
                return Ok(None);
            }
            let Some(cargo) = find(backend, "cargo") else {
                return Ok(None);
            };
            CommandLine::new(cargo).args(target.tool.args().iter().copied())
        }
        Tool::Npm | Tool::Pnpm | Tool::YarnClassic | Tool::YarnBerry | Tool::Govulncheck => {
            let Some(program) = target.tool.executable().and_then(|e| find(backend, e)) else {
                return Ok(None);
            };
            CommandLine::new(program).args(target.tool.args().iter().copied())
        }
        Tool::DependencyCheck => return Ok(None),
    };
    let mut cmd = cmd;
    if cmd.cwd.as_os_str().is_empty() {
        cmd.cwd = cwd;
    }
    cmd.env.insert("NO_COLOR".into(), "1".into());
    Ok(Some(cmd))
}

/// Check one target: the cache while it is fresh, else the tool.
fn check_target(
    state: &AppState,
    backend: &Arc<dyn Backend>,
    target: Target,
    cache: Option<&Path>,
    force: bool,
    max_age: u64,
) -> TargetResult {
    if target.blocked.is_some() {
        return TargetResult::blocked(target);
    }
    let read = |p: &str| backend.read_optional(p).ok().flatten();
    let key = tool::cache_key(&target, &read);
    if !force {
        if let Some(entry) = read_cache(cache, &target).filter(|e| e.is_fresh(&key, now(), max_age))
        {
            return entry.result(target);
        }
    }
    let result = if target.tool == Tool::DependencyCheck {
        // Only the report the build wrote is read.
        match target.lockfile.as_deref().and_then(read) {
            Some(text) => report::from_output(target, &text, "", now()),
            None => {
                let message = format!(
                    "{} not found: run dependency-check with the JSON format first",
                    target.lockfile.as_deref().unwrap_or("The report")
                );
                TargetResult::new(target, report::Status::Blocked, Some(message))
            }
        }
    } else {
        match audit_command(state, backend, &target) {
            Ok(None) => TargetResult::missing(target),
            Err(e) => TargetResult::new(target, report::Status::Failed, Some(e.to_string())),
            Ok(Some(cmd)) => match capture(backend.as_ref(), &cmd, "", RUN_TIMEOUT) {
                Ok(out) if out.code.is_none() => TargetResult::new(
                    target,
                    report::Status::Failed,
                    Some("The audit did not finish in time".into()),
                ),
                Ok(out) => report::from_output(target, &out.stdout, &out.stderr, now()),
                Err(e) => TargetResult::new(
                    target,
                    report::Status::Failed,
                    Some(format!("Cannot run {}: {e}", cmd.program)),
                ),
            },
        }
    };
    if result.status == report::Status::Ok {
        write_cache(
            cache,
            &result.target,
            &CacheEntry {
                key,
                checked_at: result.checked_at.unwrap_or_else(now),
                findings: result.findings.clone(),
            },
        );
    }
    result
}

/// Check the project's dependencies. `force`: run the tools even when the cached result of
/// an unchanged manifest and lock file is still fresh (the Check for Vulnerable
/// Dependencies action). `tools`: only these (`composer` for Composer | Audit).
#[tauri::command]
pub async fn vulnerabilities_check(
    force: bool,
    tools: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
    vulns: State<'_, VulnerabilitiesState>,
) -> Result<CheckResult> {
    let (enabled, max_age, report) = {
        let s = state.read_settings();
        (
            s.get_as::<bool>(ENABLED_KEY).unwrap_or(true),
            s.get_as::<i64>(CACHE_HOURS_KEY).unwrap_or(24).max(0) as u64 * 3600,
            s.get_as::<String>(DEPENDENCY_CHECK_KEY).unwrap_or_default(),
        )
    };
    if !enabled {
        return Ok(CheckResult {
            enabled,
            results: Vec::new(),
        });
    }
    // Composer and Yarn load the project's plugins; pip-audit runs its environment.
    state.require_trusted("Checking dependencies for vulnerabilities")?;
    let _guard = vulns.running.lock().await;
    let backend = state.backend()?;
    let opts = DetectOptions {
        composer_json: composer::options(&state).json,
        dependency_check_report: report,
    };
    let cache = cache_dir(&state);
    blocking(move || {
        let state = app.state::<AppState>();
        let files: Vec<String> = backend
            .list_dir("")?
            .into_iter()
            .map(|e| e.path.trim_start_matches("./").to_string())
            .collect();
        let read = |p: &str| backend.read_optional(p).ok().flatten();
        let targets = tool::detect(&files, &read, &opts);
        let results = targets
            .into_iter()
            .filter(|t| {
                tools
                    .as_ref()
                    .is_none_or(|only| only.iter().any(|x| x == t.tool.id()))
            })
            .map(|t| check_target(&state, &backend, t, cache.as_deref(), force, max_age))
            .collect();
        Ok(CheckResult { enabled, results })
    })
    .await
}

/// The "Vulnerable Dependencies" problems of a manifest or lock file in the editor.
#[tauri::command]
pub fn vulnerabilities_problems(
    path: String,
    text: String,
    vulnerabilities: Vec<Vulnerability>,
    notices: Vec<Notice>,
) -> Vec<FileProblem> {
    problems::problems(&path, &text, &vulnerabilities, &notices)
}

/// `text` with the constraint of `package` raised to `fixed` (Upgrade quick fix).
#[tauri::command]
pub fn vulnerabilities_upgrade(
    path: String,
    text: String,
    package: String,
    fixed: String,
) -> Option<String> {
    problems::upgrade(&path, &text, &package, &fixed)
}

/// The command that installs what a manifest now requires after an upgrade (`package` and
/// `fixed` set), or that installs the audit tool itself (pip-audit). The UI runs it in the
/// Run tool window (refused in Safe Mode).
#[tauri::command]
pub async fn vulnerabilities_install_command(
    tool: String,
    dir: String,
    package: Option<String>,
    fixed: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    let tool =
        Tool::parse(&tool).ok_or_else(|| ForgeError::Other(format!("unknown tool {tool}")))?;
    let backend = state.backend()?;
    if dir.split(['/', '\\']).any(|p| p == "..") {
        return Err(ForgeError::Other(
            "the folder is outside the project".into(),
        ));
    }
    let cwd = backend.info().root.join(&dir);
    let program = |name: &str| find(&backend, name).unwrap_or_else(|| name.to_string());
    let valid = |p: &str| !p.trim().is_empty() && !p.trim_start().starts_with('-');
    let mut cmd = match (tool, package.as_deref()) {
        (Tool::PipAudit, None) => {
            return python::packages_command(&state, Action::Install, &["pip-audit".into()], None)
        }
        (Tool::PipAudit, Some(p)) if valid(p) => {
            let spec = match fixed.as_deref() {
                Some(f) => format!("{p}>={f}"),
                None => p.to_string(),
            };
            return python::packages_command(&state, Action::Install, &[spec], None);
        }
        (Tool::Composer, Some(p)) if valid(p) => {
            let opts = composer::options(&state);
            let (program, lead) = forge_process::composer_program(&opts.executable, &opts.php);
            CommandLine::new(program).args(lead).args([
                "update",
                p,
                "--with-dependencies",
                "--no-interaction",
            ])
        }
        (Tool::Npm, Some(_)) => CommandLine::new(program("npm")).arg("install"),
        (Tool::Pnpm, Some(_)) => CommandLine::new(program("pnpm")).arg("install"),
        (Tool::YarnClassic | Tool::YarnBerry, Some(_)) => {
            CommandLine::new(program("yarn")).arg("install")
        }
        (Tool::CargoAudit, Some(p)) if valid(p) => {
            CommandLine::new(program("cargo")).args(["update", "-p", p])
        }
        (Tool::Govulncheck, Some(_)) => CommandLine::new(program("go")).args(["mod", "tidy"]),
        _ => {
            return Err(ForgeError::Other(format!(
                "{} has no install step in Forge",
                tool.label()
            )))
        }
    };
    cmd.cwd = cwd;
    Ok(cmd)
}
