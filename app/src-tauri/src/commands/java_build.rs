//! JavaForge's Gradle and Maven tool windows, the Gradle wrapper and build script editing.
//! The logic lives in `javaforge-build` (`model`, `wrapper`, `dsl`, `deps`, `net`); this
//! module runs the build tool and talks to the network.
//!
//! - `java_build_model`: projects, tasks, Maven plugins and profiles, and the dependency
//!   trees of the last sync (cached in `<cache>/forge/javaforge/sync`, never in the
//!   project).
//! - `java_sync_project`: Reload All Gradle / Maven Projects. Gradle runs the init script
//!   `forge-model.gradle` (`forgeModel`), Maven `dependency:tree`; the output streams on
//!   `java://sync`. Build scripts are project code, so Safe Mode refuses it.
//! - `java_wrapper_info`, `java_wrapper_verify`, `java_wrapper_update`: the Gradle wrapper
//!   (`gradle-wrapper.properties`, `distributionSha256Sum` against services.gradle.org).
//! - `java_build_complete`, `java_build_outdated`: completion in `*.gradle(.kts)` and
//!   `pom.xml` (DSL model, Maven Central coordinates and versions) and the "newer version
//!   available" inspection.
//! - `java_build_workspace`: Build Project with "Build and run using: IntelliJ IDEA"
//!   (Eclipse JDT LS compiles).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Duration;

use forge_core::{ForgeError, Result};
use forge_jsonschema::CompletionOut;
use forge_process::CommandLine;
use javaforge_build::deps::{self, FileKind, OutdatedMarker};
use javaforge_build::dsl::{self, Script};
use javaforge_build::model::{self, BuildModel};
use javaforge_build::net::{self as central_net, MavenCentral};
use javaforge_build::wrapper::{self, Checksum, GradleWrapper};
use javaforge_build::{central, BuildProject, BuildTool};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::java::{
    exec_options, flag, gradle_java_home, read_build, setting, short_hash, SERVER_ID,
};
use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// `{ line, stderr }` while a sync runs.
pub const SYNC_EVENT: &str = "java://sync";

fn javaforge_cache() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|d| d.join("forge").join("javaforge"))
        .ok_or_else(|| ForgeError::Other("no user cache directory".into()))
}

/// Write an init script into the cache dir (only when it changed) and return its path.
pub(crate) fn init_script(name: &str, text: &str) -> Result<PathBuf> {
    let dir = javaforge_cache()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| ForgeError::Other(format!("cannot create {}: {e}", dir.display())))?;
    let path = dir.join(name);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
        std::fs::write(&path, text)
            .map_err(|e| ForgeError::Other(format!("cannot write {}: {e}", path.display())))?;
    }
    Ok(path)
}

fn sync_cache(root: &std::path::Path) -> Result<PathBuf> {
    Ok(javaforge_cache()?
        .join("sync")
        .join(format!("{}.json", short_hash(&root.display().to_string()))))
}

/// The last sync of the project, when there was one.
fn cached_sync(root: &std::path::Path) -> Option<BuildModel> {
    serde_json::from_str(&std::fs::read_to_string(sync_cache(root).ok()?).ok()?).ok()
}

/// The resolved dependencies of the last sync (Spring Boot's configuration metadata).
pub(crate) fn synced_model(root: &std::path::Path) -> Option<BuildModel> {
    cached_sync(root)
}

fn static_model(state: &AppState) -> Result<Option<(BuildProject, BuildModel, PathBuf)>> {
    let backend = state.backend()?;
    let Some(build) = read_build(state, backend.as_ref()) else {
        return Ok(None);
    };
    let base = model::static_model(&build, &|p| backend.read_optional(p).ok().flatten());
    Ok(Some((build, base, backend.info().root.clone())))
}

/// The tool window's model: the build files' data with the last sync's over it.
#[tauri::command]
pub async fn java_build_model(state: State<'_, AppState>) -> Result<Option<BuildModel>> {
    let Some((_, base, root)) = static_model(&state)? else {
        return Ok(None);
    };
    Ok(Some(match cached_sync(&root) {
        Some(synced) => model::merge(base, &synced),
        None => base,
    }))
}

static SYNC_CANCEL: AtomicBool = AtomicBool::new(false);
static SYNC_RUNNING: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize)]
struct SyncLine {
    line: String,
    stderr: bool,
}

/// Reload All Gradle / Maven Projects: ask the build tool for tasks and dependency trees,
/// then let JDT LS re-import the build.
#[tauri::command]
pub async fn java_sync_project(app: AppHandle) -> Result<BuildModel> {
    let state = app.state::<AppState>();
    // Gradle and Maven run the build scripts and plugins of the project.
    state.require_trusted("Reloading Gradle and Maven projects")?;
    let backend = state.backend()?;
    if backend.info().remote.is_some() {
        return Err(ForgeError::Other(
            "Reloading Gradle and Maven projects on a remote host is not supported yet".into(),
        ));
    }
    let Some((build, base, root)) = static_model(&state)? else {
        return Err(ForgeError::Other(
            "no Maven or Gradle build in the project root".into(),
        ));
    };
    let opts = exec_options(&state, build.tool, &root);
    let java_home = match build.tool {
        BuildTool::Gradle => gradle_java_home(&state),
        BuildTool::Maven => {
            let jdks =
                crate::commands::java::all_jdks(&state, crate::commands::java::java_state(), false);
            crate::commands::java::project_jdk(&state, &jdks).map(|j| j.home)
        }
    };
    let out = javaforge_cache()?
        .join("sync")
        .join(format!("{}.out", short_hash(&root.display().to_string())));
    let cache = sync_cache(&root)?;
    let handle = app.clone();
    let synced = blocking(move || {
        let _guard = SYNC_RUNNING
            .try_lock()
            .map_err(|_| ForgeError::Other("a sync is already running".into()))?;
        SYNC_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
        if let Some(dir) = out.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::remove_file(&out);
        let out_s = out.display().to_string();
        let built = match build.tool {
            BuildTool::Gradle => {
                let script = init_script("forge-model.gradle", model::GRADLE_INIT_SCRIPT)?;
                javaforge_build::command::gradle_sync_command(
                    &build,
                    &opts,
                    &script.display().to_string(),
                    &out_s,
                )
            }
            BuildTool::Maven => javaforge_build::command::maven_sync_command(&build, &opts, &out_s),
        };
        let mut cmd = CommandLine::new(built.program).args(built.args);
        cmd.cwd = root.clone();
        if let Some(home) = java_home {
            cmd.env.insert("JAVA_HOME".into(), home);
        }
        let _ = handle.emit(
            SYNC_EVENT,
            SyncLine {
                line: format!("{} {}", cmd.program, cmd.args.join(" ")),
                stderr: false,
            },
        );
        let mut errors = String::new();
        let code = forge_backend::process_io::run_lines(
            backend.as_ref(),
            &cmd,
            &SYNC_CANCEL,
            &mut |line, stderr| {
                if stderr {
                    errors.push_str(&line);
                    errors.push('\n');
                }
                let _ = handle.emit(SYNC_EVENT, SyncLine { line, stderr });
            },
        )
        .map_err(|e| ForgeError::Other(format!("cannot run {}: {e}", build.tool.label())))?;
        let text = std::fs::read_to_string(&out).unwrap_or_default();
        if code != Some(0) || text.trim().is_empty() {
            let detail = errors
                .lines()
                .rev()
                .find(|l| {
                    l.contains("What went wrong") || l.starts_with("[ERROR]") || l.contains("rror")
                })
                .unwrap_or("")
                .trim()
                .to_string();
            return Err(ForgeError::Other(format!(
                "{} sync failed{}{}",
                build.tool.label(),
                code.map(|c| format!(" (exit code {c})"))
                    .unwrap_or_default(),
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            )));
        }
        let mut synced = match build.tool {
            BuildTool::Gradle => model::parse_gradle_model(&text).map_err(ForgeError::Other)?,
            BuildTool::Maven => model::maven_sync_model(&text),
        };
        synced.synced_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());
        if let Ok(json) = serde_json::to_string(&synced) {
            let _ = std::fs::write(&cache, json);
        }
        let _ = std::fs::remove_file(&out);
        Ok(synced)
    })
    .await?;
    // JDT LS re-imports the build too (when it runs).
    let _ = crate::commands::java::java_reload_project(app.clone()).await;
    Ok(model::merge(base, &synced))
}

/// Stop a running sync.
#[tauri::command]
pub fn java_sync_cancel() {
    SYNC_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

// ---- Gradle wrapper --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WrapperInfo {
    pub wrapper: GradleWrapper,
    /// `gradle/wrapper/gradle-wrapper.properties`.
    pub path: String,
}

#[tauri::command]
pub async fn java_wrapper_info(state: State<'_, AppState>) -> Result<Option<WrapperInfo>> {
    let backend = state.backend()?;
    let text = blocking(move || backend.read_optional(wrapper::PROPERTIES)).await?;
    Ok(text
        .and_then(|t| wrapper::parse_properties(&t))
        .map(|w| WrapperInfo {
            wrapper: w,
            path: wrapper::PROPERTIES.into(),
        }))
}

fn services_url(state: &AppState) -> (String, bool) {
    match setting(state, "java.gradle.servicesUrl") {
        s if s.is_empty() => (wrapper::SERVICES.to_string(), false),
        // A self-hosted mirror (and the tests' local server) without the proxy.
        s => {
            let local = s.contains("127.0.0.1") || s.contains("localhost");
            (s, local)
        }
    }
}

/// `distributionSha256Sum` against the checksum Gradle publishes for the distribution.
#[tauri::command]
pub async fn java_wrapper_verify(state: State<'_, AppState>) -> Result<Checksum> {
    let Some(info) = java_wrapper_info(state.clone()).await? else {
        return Err(ForgeError::Other(
            "the project has no Gradle wrapper".into(),
        ));
    };
    let (_, direct) = services_url(&state);
    if info.wrapper.sha256.is_none() {
        return Ok(Checksum::NotSet);
    }
    let official = info
        .wrapper
        .distribution_url
        .starts_with(wrapper::DISTRIBUTIONS)
        || direct;
    let published = if official {
        Some(
            central_net::gradle_sha256(&info.wrapper.distribution_url, direct)
                .await
                .map_err(ForgeError::Other)?,
        )
    } else {
        None
    };
    Ok(wrapper::check(&info.wrapper, published.as_deref()))
}

/// Update Wrapper: point `distributionUrl` at `version` (the newest release when empty) and
/// write the checksum Gradle publishes for it. The wrapper downloads the new Gradle on its
/// next run and refuses it when the checksum does not match.
#[tauri::command]
pub async fn java_wrapper_update(
    version: Option<String>,
    state: State<'_, AppState>,
) -> Result<GradleWrapper> {
    let backend = state.backend()?;
    let b = backend.clone();
    let current = blocking(move || b.read_optional(wrapper::PROPERTIES))
        .await?
        .unwrap_or_default();
    let kind = wrapper::parse_properties(&current)
        .map(|w| w.distribution_type)
        .unwrap_or_else(|| "bin".into());
    let (services, direct) = services_url(&state);
    let version = match version
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        Some(v) => v,
        None => {
            central_net::gradle_current(&services, direct)
                .await
                .map_err(ForgeError::Other)?
                .version
        }
    };
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    {
        return Err(ForgeError::Other(format!(
            "{version} is not a Gradle version"
        )));
    }
    let url = if direct {
        format!(
            "{}/distributions/gradle-{version}-{kind}.zip",
            services.trim_end_matches('/')
        )
    } else {
        wrapper::distribution_url(&version, &kind)
    };
    let sum = central_net::gradle_sha256(&url, direct)
        .await
        .map_err(ForgeError::Other)?;
    let text = wrapper::update_properties(&current, &url, Some(&sum));
    let written = text.clone();
    blocking(move || backend.write_file(wrapper::PROPERTIES, &written)).await?;
    wrapper::parse_properties(&text)
        .ok_or_else(|| ForgeError::Other("the updated properties are unreadable".into()))
}

// ---- Build scripts in the editor -----------------------------------------------------

/// The Maven Central client (cached; `None` when turned off).
fn central(state: &AppState) -> Option<MavenCentral> {
    if !flag(state, "java.mavenCentral.enabled", true) {
        return None;
    }
    let search = match setting(state, "java.mavenCentral.searchUrl") {
        s if s.is_empty() => central::SEARCH_URL.to_string(),
        s => s,
    };
    let repo = match setting(state, "java.mavenCentral.repositoryUrl") {
        s if s.is_empty() => central::REPOSITORY_URL.to_string(),
        s => s,
    };
    static CLIENT: Mutex<Option<(String, String, MavenCentral)>> = Mutex::new(None);
    let mut c = CLIENT.lock().ok()?;
    if let Some((s, r, client)) = c.as_ref() {
        if *s == search && *r == repo {
            return Some(client.clone());
        }
    }
    let client = MavenCentral::new(&search, &repo, MavenCentral::default_cache_dir()?).ok()?;
    *c = Some((search, repo, client.clone()));
    Some(client)
}

/// Completion in a Gradle build script or `pom.xml` at UTF-16 `offset`.
#[tauri::command]
pub async fn java_build_complete(
    path: String,
    text: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<Option<CompletionOut>> {
    let Some(script) = Script::of_path(&path) else {
        return Ok(None);
    };
    let data = match (dsl::request(script, &text, offset), central(&state)) {
        (Some(req), Some(c)) => tokio::time::timeout(Duration::from_secs(8), c.answer(&req))
            .await
            .unwrap_or_default(),
        _ => dsl::CentralData::default(),
    };
    Ok(dsl::complete(script, &text, offset, &data))
}

/// "A newer version is available" in a build file.
#[tauri::command]
pub async fn java_build_outdated(
    path: String,
    text: String,
    state: State<'_, AppState>,
) -> Result<Vec<OutdatedMarker>> {
    let Some(kind) = FileKind::of_path(&path) else {
        return Ok(Vec::new());
    };
    let Some(c) = central(&state) else {
        return Ok(Vec::new());
    };
    let coords = deps::versioned_coordinates(&text, kind);
    let versions = c.versions_of(&coords).await;
    Ok(deps::outdated(&text, kind, &versions))
}

/// Build Project with "Build and run using: IntelliJ IDEA": Eclipse JDT LS compiles the
/// workspace (`java/buildWorkspace`, full build when `full`). Returns JDT LS's status
/// (0 failed, 1 succeeded, 2 has errors, 3 cancelled).
#[tauri::command]
pub async fn java_build_workspace(full: Option<bool>, app: AppHandle) -> Result<i64> {
    let state = app.state::<AppState>();
    let manager = app.state::<LspState>().manager(&app, &state)?;
    let Some(client) = manager
        .running_clients()
        .into_iter()
        .find(|c| c.id() == SERVER_ID)
    else {
        return Err(ForgeError::Other(
            "Eclipse JDT LS is not running: open a Java file first".into(),
        ));
    };
    let status = client
        .request_raw(
            javaforge_lang_java::jdtls::commands::BUILD_WORKSPACE,
            json!(full.unwrap_or(false)),
            Duration::from_secs(600),
        )
        .await?;
    Ok(status.as_i64().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_scripts_land_in_the_cache_dir() {
        let path = init_script("forge-test.gradle", "// test\n").unwrap();
        assert!(path.starts_with(dirs::cache_dir().unwrap()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "// test\n");
        let _ = std::fs::remove_file(path);
    }
}
