//! Spring Boot and Jakarta EE IPC (`javaforge-fw-spring`): the project model (beans and
//! injection points, endpoints, JPA entities, configuration properties), gutter markers,
//! `application.properties` / `application.yml` completion, inspections, hover and
//! navigation with the configuration metadata of the dependency jars, "Open in HTTP Client"
//! requests and the run dashboard: Spring Boot runs are followed through their console
//! output and, while they run, their actuator on localhost.
//!
//! Static analysis only reads files; starting an application goes through `run_start`
//! (refused in Safe Mode).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use forge_backend::Backend;
use forge_core::search::CollectRequest;
use forge_core::{ForgeError, Result};
use forge_process::{RunConfiguration, RunConfigurations, RunEvent};
use javaforge_fw_spring::actuator::{self, Snapshot};
use javaforge_fw_spring::assist::{self, Completion, Context, Hover, Problem};
use javaforge_fw_spring::dashboard::{self, DashboardApp};
use javaforge_fw_spring::jars::{self, Caches, Coordinate};
use javaforge_fw_spring::metadata::Metadata;
use javaforge_fw_spring::model::{
    self, Application, ConfigClass, ConfigFileInfo, Entity, Marker, Model, Project,
};
use javaforge_fw_spring::run::{self, Observed};
use javaforge_fw_spring::views::{self, BeanRow, EndpointRow};
use javaforge_fw_spring::Location;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::java::{flag, setting};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Emitted when a Spring Boot run changes (started, port known, stopped).
pub const DASHBOARD_EVENT: &str = "spring://dashboard";

/// Files read per scan.
const MAX_FILES: usize = 20_000;
/// The model is scanned again after this long (or on Refresh, or after file changes).
const MAX_AGE: Duration = Duration::from_secs(10);

/// Runs a UI-side observer gets the events of (see `run::start_built`).
pub type RunObserver = Arc<dyn Fn(&RunEvent) + Send + Sync>;

struct ProjectCache {
    root: PathBuf,
    at: Instant,
    project: Project,
    model: Arc<Model>,
}

struct JarCache {
    key: Vec<(PathBuf, Option<SystemTime>)>,
    metadata: Arc<Metadata>,
    problems: Vec<String>,
}

/// A Spring Boot run the dashboard follows.
#[derive(Clone, Default)]
struct AppRun {
    run_id: String,
    main_class: String,
    profiles: Vec<String>,
    overrides: Vec<(String, String)>,
    running: bool,
    debug: bool,
    exit_code: Option<i32>,
    observed: Observed,
    /// The unfinished last line of the output.
    pending: String,
    health: Option<String>,
}

#[derive(Default)]
pub struct SpringState {
    project: Mutex<Option<ProjectCache>>,
    jars: Mutex<Option<JarCache>>,
    /// By run configuration name.
    apps: Mutex<BTreeMap<String, AppRun>>,
    /// Actuator answers by configuration name.
    snapshots: Mutex<HashMap<String, (Instant, Snapshot)>>,
}

fn spring(app: &AppHandle) -> &SpringState {
    app.state::<SpringState>().inner()
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

// ---- The project model --------------------------------------------------------------------

fn scan(backend: &dyn Backend) -> Result<Project> {
    let files = backend.collect_files(&CollectRequest {
        extensions: vec![
            "java".into(),
            "kt".into(),
            "properties".into(),
            "yml".into(),
            "yaml".into(),
        ],
        max_files: MAX_FILES,
        max_file_size: 2 * 1024 * 1024,
        max_total: 256 * 1024 * 1024,
    })?;
    let mut project = Project::default();
    for f in files.iter().filter(|f| model::is_relevant(&f.path)) {
        project.set_file(&f.path, &f.text);
    }
    for meta in [
        "src/main/resources/META-INF/additional-spring-configuration-metadata.json",
        "src/main/resources/META-INF/spring-configuration-metadata.json",
    ] {
        if let Ok(Some(text)) = backend.read_optional(meta) {
            project.set_file(meta, &text);
        }
    }
    Ok(project)
}

/// The model of the open project, with `edit` (an editor's unsaved text) applied.
fn model_of(
    state: &AppState,
    spring: &SpringState,
    refresh: bool,
    edit: Option<(&str, &str)>,
) -> Result<Arc<Model>> {
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let mut guard = spring
        .project
        .lock()
        .map_err(|_| err("spring model poisoned"))?;
    let stale = guard
        .as_ref()
        .is_none_or(|c| c.root != root || refresh || c.at.elapsed() > MAX_AGE);
    if stale {
        let project = scan(backend.as_ref())?;
        let model = Arc::new(project.model());
        *guard = Some(ProjectCache {
            root,
            at: Instant::now(),
            project,
            model,
        });
    }
    let cache = guard.as_mut().expect("just filled");
    if let Some((path, text)) = edit {
        if model::is_relevant(path) {
            cache.project.set_file(path, text);
            cache.model = Arc::new(cache.project.model());
        }
    }
    Ok(cache.model.clone())
}

/// The Spring Boot version the build declares.
fn boot_version(backend: &dyn Backend) -> Option<String> {
    ["build.gradle.kts", "build.gradle", "pom.xml"]
        .iter()
        .filter_map(|f| backend.read_optional(f).ok().flatten())
        .find_map(|b| forge_project::detect::spring_boot_version(&b))
}

fn caches(state: &AppState) -> Caches {
    let home = dirs::home_dir().unwrap_or_default();
    let mut c = Caches::under_home(&home);
    let maven = setting(state, "spring.metadata.mavenRepository");
    if !maven.is_empty() {
        c.maven = Some(PathBuf::from(maven));
    }
    let gradle = match setting(state, "spring.metadata.gradleUserHome") {
        g if !g.is_empty() => Some(PathBuf::from(g)),
        _ => std::env::var_os("GRADLE_USER_HOME").map(PathBuf::from),
    };
    if let Some(g) = gradle {
        c.gradle = Some(g.join("caches").join("modules-2").join("files-2.1"));
    }
    c
}

/// Resolved dependencies of the last Gradle / Maven sync.
fn synced_dependencies(root: &Path) -> Vec<Coordinate> {
    let Some(model) = crate::commands::java_build::synced_model(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    fn walk(nodes: &[javaforge_build::model::DependencyNode], out: &mut Vec<Coordinate>) {
        for n in nodes {
            if !n.version.is_empty() && !n.failed {
                out.push(Coordinate {
                    group: n.group.clone(),
                    artifact: n.artifact.clone(),
                    version: n.version.clone(),
                });
            }
            walk(&n.children, out);
        }
    }
    for p in &model.projects {
        for scope in &p.dependencies {
            walk(&scope.dependencies, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The jars' metadata (cached while the jars do not change).
fn jar_metadata(state: &AppState, spring: &SpringState) -> (Arc<Metadata>, Vec<String>) {
    let Ok(backend) = state.backend() else {
        return (Arc::default(), Vec::new());
    };
    if !flag(state, "spring.metadata.fromJars", true) || backend.info().remote.is_some() {
        return (Arc::default(), Vec::new());
    }
    let root = backend.info().root.clone();
    let deps = synced_dependencies(&root);
    let jars = jars::jars_for(
        &deps,
        boot_version(backend.as_ref()).as_deref(),
        &caches(state),
    );
    let key: Vec<(PathBuf, Option<SystemTime>)> = jars
        .iter()
        .map(|j| {
            (
                j.clone(),
                std::fs::metadata(j).and_then(|m| m.modified()).ok(),
            )
        })
        .collect();
    let mut guard = match spring.jars.lock() {
        Ok(g) => g,
        Err(_) => return (Arc::default(), Vec::new()),
    };
    if let Some(c) = guard.as_ref().filter(|c| c.key == key) {
        return (c.metadata.clone(), c.problems.clone());
    }
    let mut meta = Metadata::default();
    let problems = jars::load(&jars, &mut meta);
    let metadata = Arc::new(meta);
    *guard = Some(JarCache {
        key,
        metadata: metadata.clone(),
        problems: problems.clone(),
    });
    (metadata, problems)
}

/// The jars' metadata with the project's (`@ConfigurationProperties`, its own metadata
/// files) and the project's enums.
fn metadata(state: &AppState, spring: &SpringState, model: &Model) -> (Metadata, Vec<String>) {
    let (jars, problems) = jar_metadata(state, spring);
    let mut m = (*jars).clone();
    for p in model.metadata.properties.values() {
        m.add_property(p.clone());
    }
    for (k, h) in &model.metadata.hints {
        m.hints.entry(k.clone()).or_insert_with(|| h.clone());
    }
    for (k, v) in &model.enums {
        m.enums.entry(k.clone()).or_insert_with(|| v.clone());
    }
    if !model.metadata.is_empty() {
        m.sources.push("project".into());
    }
    m.sources.extend(model.metadata.sources.iter().cloned());
    (m, problems)
}

fn profiles(model: &Model) -> Vec<String> {
    let mut p = model.profiles.clone();
    if !p.iter().any(|x| x == "default") {
        p.push("default".into());
    }
    p
}

// ---- Commands: model, gutter, configuration files ---------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataStatus {
    pub properties: usize,
    pub sources: Vec<String>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub beans: bool,
    pub mappings: bool,
}

/// What the Spring tool windows show.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringOverview {
    pub spring: bool,
    pub jakarta: bool,
    pub beans: Vec<BeanRow>,
    pub endpoints: Vec<EndpointRow>,
    pub entities: Vec<Entity>,
    pub config_classes: Vec<ConfigClass>,
    pub applications: Vec<Application>,
    pub apis: Vec<model::Api>,
    pub profiles: Vec<String>,
    pub config_files: Vec<ConfigFileInfo>,
    pub metadata: MetadataStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeInfo>,
}

#[tauri::command]
pub async fn spring_model(refresh: Option<bool>, app: AppHandle) -> Result<SpringOverview> {
    blocking(move || {
        let state = app.state::<AppState>();
        let sp = spring(&app);
        let model = model_of(&state, sp, refresh.unwrap_or(false), None)?;
        let (meta, problems) = metadata(&state, sp, &model);
        // A running application enriches beans and endpoints.
        let running = running_snapshot(&app, &model);
        let (snap, runtime) = match &running {
            Some((name, url, snap)) => (
                Some(snap),
                Some(RuntimeInfo {
                    name: name.clone(),
                    url: url.clone(),
                    beans: snap.beans.is_some(),
                    mappings: snap.mappings.is_some(),
                }),
            ),
            None => (None, None),
        };
        Ok(SpringOverview {
            spring: model.spring,
            jakarta: model.jakarta,
            beans: views::bean_rows(&model.beans, snap.and_then(|s| s.beans.as_deref())),
            endpoints: views::endpoint_rows(
                &model.endpoints,
                snap.and_then(|s| s.mappings.as_deref()),
            ),
            entities: model.entities.clone(),
            config_classes: model.config_classes.clone(),
            applications: model.applications.clone(),
            apis: model.apis.clone(),
            profiles: profiles(&model),
            config_files: model.config_files.clone(),
            metadata: MetadataStatus {
                properties: meta.properties.len(),
                sources: meta.sources.clone(),
                problems,
            },
            runtime,
        })
    })
    .await
}

/// Gutter markers of a Java / Kotlin file.
#[tauri::command]
pub async fn spring_gutter(path: String, content: String, app: AppHandle) -> Result<Vec<Marker>> {
    blocking(move || {
        let state = app.state::<AppState>();
        if !flag(&state, "spring.gutterIcons", true) {
            return Ok(Vec::new());
        }
        let model = model_of(&state, spring(&app), false, Some((&path, &content)))?;
        Ok(model::gutter(&model, &path))
    })
    .await
}

fn with_assist<T: Send + 'static>(
    app: AppHandle,
    path: String,
    content: String,
    f: impl FnOnce(&Context, &Model) -> T + Send + 'static,
) -> impl std::future::Future<Output = Result<T>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let sp = spring(&app);
        let model = model_of(&state, sp, false, Some((&path, &content)))?;
        let (meta, _) = metadata(&state, sp, &model);
        let profiles = profiles(&model);
        let ctx = Context {
            metadata: &meta,
            profiles: &profiles,
        };
        Ok(f(&ctx, &model))
    })
}

/// Completion in `application*.properties` / `application*.yml` (UTF-16 `offset`).
#[tauri::command]
pub async fn spring_config_complete(
    path: String,
    content: String,
    offset: usize,
    app: AppHandle,
) -> Result<Option<Completion>> {
    let (p, c) = (path.clone(), content.clone());
    with_assist(app, path, content, move |ctx, _| {
        assist::complete(&p, &c, offset, ctx)
    })
    .await
}

/// Unknown / deprecated keys and invalid values.
#[tauri::command]
pub async fn spring_config_inspect(
    path: String,
    content: String,
    app: AppHandle,
) -> Result<Vec<Problem>> {
    let unknown = flag(
        &app.state::<AppState>(),
        "spring.inspections.unknownProperties",
        true,
    );
    let (p, c) = (path.clone(), content.clone());
    with_assist(app, path, content, move |ctx, _| {
        let mut out = assist::inspect(&p, &c, ctx);
        if !unknown {
            out.retain(|x| x.code != "unknownProperty");
        }
        out
    })
    .await
}

#[tauri::command]
pub async fn spring_config_hover(
    path: String,
    content: String,
    offset: usize,
    app: AppHandle,
) -> Result<Option<Hover>> {
    let (p, c) = (path.clone(), content.clone());
    with_assist(app, path, content, move |ctx, _| {
        assist::hover(&p, &c, offset, ctx)
    })
    .await
}

/// Go to Declaration on a key: its `@ConfigurationProperties` field or class.
#[tauri::command]
pub async fn spring_config_goto(
    path: String,
    content: String,
    offset: usize,
    app: AppHandle,
) -> Result<Vec<Location>> {
    let (p, c) = (path.clone(), content.clone());
    with_assist(app, path, content, move |ctx, model| {
        assist::goto(&p, &c, offset, ctx, &|class| {
            model.classes.get(class).cloned()
        })
    })
    .await
}

/// The base URL requests go to: the running application, else `server.port` and the
/// context path of the configuration files.
fn base_url(app: &AppHandle, model: &Model) -> String {
    if let Ok(apps) = spring(app).apps.lock() {
        if let Some(a) = apps
            .values()
            .find(|a| a.running && a.observed.port.is_some())
        {
            let ctx = a.observed.context_path.clone().unwrap_or_default();
            return format!("http://localhost:{}{ctx}", a.observed.port.unwrap_or(8080));
        }
    }
    let port = model::setting(model, "server.port", &[])
        .unwrap_or("8080")
        .trim()
        .to_string();
    let ctx = model::setting(model, "server.servlet.context-path", &[])
        .or_else(|| model::setting(model, "spring.webflux.base-path", &[]))
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string();
    format!(
        "http://localhost:{}{ctx}",
        if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() {
            port
        } else {
            "8080".into()
        }
    )
}

/// "Open in HTTP Client": an `.http` request for an endpoint.
#[tauri::command]
pub async fn spring_http_request(endpoint: EndpointRow, app: AppHandle) -> Result<String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let model = model_of(&state, spring(&app), false, None)?;
        Ok(views::http_request(&endpoint, &base_url(&app, &model)))
    })
    .await
}

// ---- Runs and the run dashboard -------------------------------------------------------------

/// Turn a Spring Boot configuration into the Application configuration it runs as.
pub(crate) fn to_application(cfg: &mut RunConfiguration) {
    if cfg.type_id != run::TYPE {
        return;
    }
    let l = run::launch(&cfg.options, &cfg.interpreter_options, &cfg.arguments);
    cfg.type_id = crate::commands::java::APPLICATION.into();
    cfg.interpreter_options = l.vm_options;
    cfg.arguments = l.arguments;
}

fn notify(app: &AppHandle) {
    let _ = app.emit(DASHBOARD_EVENT, ());
}

/// Follow a Spring Boot run: returns the observer its output goes to.
pub(crate) fn track_run(
    app: &AppHandle,
    run_id: &str,
    cfg: &RunConfiguration,
    debug: bool,
) -> RunObserver {
    let name = cfg.name.clone();
    if let Ok(mut apps) = spring(app).apps.lock() {
        apps.insert(
            name.clone(),
            AppRun {
                run_id: run_id.to_string(),
                main_class: cfg.option("mainClass").to_string(),
                profiles: run::profiles(&cfg.options),
                overrides: run::overrides(&cfg.options),
                running: true,
                debug,
                ..AppRun::default()
            },
        );
    }
    if let Ok(mut s) = spring(app).snapshots.lock() {
        s.remove(&name);
    }
    notify(app);
    let handle = app.clone();
    let run_id = run_id.to_string();
    Arc::new(move |e: &RunEvent| {
        let changed = {
            let Ok(mut apps) = spring(&handle).apps.lock() else {
                return;
            };
            let Some(a) = apps.get_mut(&name).filter(|a| a.run_id == run_id) else {
                return;
            };
            match e {
                RunEvent::Output { text, .. } => {
                    a.pending.push_str(text);
                    let mut changed = false;
                    while let Some(nl) = a.pending.find('\n') {
                        let line: String = a.pending.drain(..=nl).collect();
                        changed |= a.observed.observe(&line);
                    }
                    if a.pending.len() > 64 * 1024 {
                        a.pending.clear();
                    }
                    changed
                }
                RunEvent::Exited { code, .. } => {
                    if !a.pending.is_empty() {
                        let rest = std::mem::take(&mut a.pending);
                        a.observed.observe(&rest);
                    }
                    a.running = false;
                    a.exit_code = *code;
                    true
                }
                _ => false,
            }
        };
        if changed {
            notify(&handle);
        }
    })
}

/// A run that never started (the build of its command line failed).
pub(crate) fn forget_run(app: &AppHandle, run_id: &str) {
    if let Ok(mut apps) = spring(app).apps.lock() {
        apps.retain(|_, a| a.run_id != run_id);
    }
    notify(app);
}

fn saved_configurations(state: &AppState) -> Vec<RunConfiguration> {
    let Ok(backend) = state.backend() else {
        return Vec::new();
    };
    let loaded = if backend.info().remote.is_none() {
        RunConfigurations::load(&backend.info().root)
    } else {
        let shared = backend.read_file(forge_process::config::PROJECT_FILE).ok();
        let local = backend.read_file(forge_process::config::LOCAL_FILE).ok();
        RunConfigurations::from_texts(shared.as_deref(), local.as_deref())
    };
    loaded
        .map(|c| {
            c.configurations
                .into_iter()
                .filter(|c| c.type_id == run::TYPE)
                .collect()
        })
        .unwrap_or_default()
}

fn port_of(
    model: Option<&Model>,
    profiles: &[String],
    overrides: &[(String, String)],
    key: &str,
) -> Option<String> {
    overrides
        .iter()
        .find(|(k, _)| javaforge_fw_spring::config::canonical(k) == key)
        .map(|(_, v)| v.clone())
        .or_else(|| {
            model
                .and_then(|m| model::setting(m, key, profiles))
                .map(str::to_string)
        })
}

/// The configurations and runs of the dashboard.
fn dashboard_apps(app: &AppHandle) -> Vec<DashboardApp> {
    let state = app.state::<AppState>();
    let sp = spring(app);
    let model = model_of(&state, sp, false, None).ok();
    let runs = sp.apps.lock().map(|a| a.clone()).unwrap_or_default();
    let mut out: Vec<DashboardApp> = Vec::new();
    let mut add = |name: &str,
                   main: &str,
                   profiles: Vec<String>,
                   overrides: Vec<(String, String)>,
                   run: Option<&AppRun>| {
        let port = port_of(model.as_deref(), &profiles, &overrides, "server.port")
            .and_then(|p| p.trim().parse().ok());
        let ctx = port_of(
            model.as_deref(),
            &profiles,
            &overrides,
            "server.servlet.context-path",
        );
        out.push(DashboardApp {
            name: name.to_string(),
            main_class: run
                .map(|r| r.main_class.clone())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| main.to_string()),
            profiles,
            run_id: run.map(|r| r.run_id.clone()),
            running: run.is_some_and(|r| r.running),
            debug: run.is_some_and(|r| r.debug),
            exit_code: run.and_then(|r| r.exit_code),
            observed: run.map(|r| r.observed.clone()).unwrap_or_default(),
            configured_port: port.or(Some(8080)),
            configured_context_path: ctx,
            health: run.and_then(|r| r.health.clone()),
        });
    };
    let saved = saved_configurations(&state);
    for c in &saved {
        add(
            &c.name,
            c.option("mainClass"),
            run::profiles(&c.options),
            run::overrides(&c.options),
            runs.get(&c.name),
        );
    }
    // Temporary configurations (gutter, Services) that are not saved.
    for (name, r) in &runs {
        if !saved.iter().any(|c| &c.name == name) {
            add(
                name,
                &r.main_class,
                r.profiles.clone(),
                r.overrides.clone(),
                Some(r),
            );
        }
    }
    out
}

/// The actuator's answer for a running application (cached for the refresh interval).
fn snapshot_for(app: &AppHandle, a: &DashboardApp, model: Option<&Model>) -> Option<Snapshot> {
    let state = app.state::<AppState>();
    if !a.running || !flag(&state, "spring.dashboard.actuator", true) {
        return None;
    }
    let refresh = state
        .read_settings()
        .get_as::<u64>("spring.dashboard.refreshSeconds")
        .unwrap_or(5)
        .max(2);
    let sp = spring(app);
    if let Some((at, s)) = sp.snapshots.lock().ok()?.get(&a.name) {
        if at.elapsed() < Duration::from_secs(refresh) {
            return Some(s.clone());
        }
    }
    let port = a.observed.port?;
    let overrides = sp
        .apps
        .lock()
        .ok()?
        .get(&a.name)
        .map(|r| r.overrides.clone())
        .unwrap_or_default();
    let profiles = if a.observed.profiles.is_empty() {
        a.profiles.clone()
    } else {
        a.observed.profiles.clone()
    };
    let mgmt_port = port_of(model, &profiles, &overrides, "management.server.port")
        .and_then(|p| p.trim().parse::<u16>().ok());
    let base = port_of(
        model,
        &profiles,
        &overrides,
        "management.endpoints.web.base-path",
    )
    .unwrap_or_else(|| "/actuator".into());
    // The management server has its own context path; the application's applies otherwise.
    let base = match mgmt_port {
        Some(_) => base,
        None => format!(
            "{}{}",
            a.context_path(),
            if base.starts_with('/') {
                base
            } else {
                format!("/{base}")
            }
        ),
    };
    let snap = actuator::probe(
        mgmt_port.unwrap_or(port),
        &base,
        Duration::from_millis(1500),
    );
    if let Ok(mut apps) = sp.apps.lock() {
        if let Some(r) = apps.get_mut(&a.name) {
            let health = snap.health.as_ref().map(|h| h.status.clone());
            if r.health != health {
                r.health = health;
                notify(app);
            }
        }
    }
    if let Ok(mut s) = sp.snapshots.lock() {
        s.insert(a.name.clone(), (Instant::now(), snap.clone()));
    }
    Some(snap)
}

/// The first running application with an actuator answer: (name, url, snapshot).
fn running_snapshot(app: &AppHandle, model: &Model) -> Option<(String, Option<String>, Snapshot)> {
    let apps = dashboard_apps(app);
    let a = apps.iter().find(|a| a.running && a.observed.started)?;
    let snap = snapshot_for(app, a, Some(model))?;
    Some((a.name.clone(), a.url(), snap))
}

/// The run dashboard's nodes in the Services tool window.
#[tauri::command]
pub async fn spring_dashboard(app: AppHandle) -> Result<Vec<Value>> {
    blocking(move || Ok(dashboard::tree(&dashboard_apps(&app)))).await
}

/// Details of a run dashboard node (asks the actuator of a running application).
#[tauri::command]
pub async fn spring_dashboard_details(node_id: String, app: AppHandle) -> Result<Option<Value>> {
    blocking(move || {
        let apps = dashboard_apps(&app);
        let state = app.state::<AppState>();
        let model = model_of(&state, spring(&app), false, None).ok();
        let snap = dashboard::app_name(&node_id)
            .and_then(|n| apps.iter().find(|a| a.name == n))
            .and_then(|a| snapshot_for(&app, a, model.as_deref()));
        let mut apps = apps;
        // The probe may have updated the health.
        if let Some(n) = dashboard::app_name(&node_id) {
            if let Some(r) = spring(&app)
                .apps
                .lock()
                .ok()
                .and_then(|a| a.get(&n).cloned())
            {
                if let Some(a) = apps.iter_mut().find(|a| a.name == n) {
                    a.health = r.health;
                }
            }
        }
        Ok(dashboard::details(&node_id, &apps, snap.as_ref()))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_boot_configurations_run_as_applications() {
        let mut cfg = RunConfiguration {
            name: "ShopApplication".into(),
            type_id: run::TYPE.into(),
            interpreter_options: "-Xmx1g".into(),
            options: BTreeMap::from([
                (
                    "mainClass".to_string(),
                    "com.acme.ShopApplication".to_string(),
                ),
                ("activeProfiles".to_string(), "dev".to_string()),
                ("exposeActuator".to_string(), "false".to_string()),
            ]),
            ..RunConfiguration::default()
        };
        to_application(&mut cfg);
        assert_eq!(cfg.type_id, crate::commands::java::APPLICATION);
        assert_eq!(
            cfg.interpreter_options,
            "-Xmx1g -Dspring.profiles.active=dev -Dspring.output.ansi.enabled=always"
        );
        assert_eq!(cfg.option("mainClass"), "com.acme.ShopApplication");
        let before = cfg.clone();
        to_application(&mut cfg);
        assert_eq!(cfg, before, "only Spring Boot configurations change");
    }
}
