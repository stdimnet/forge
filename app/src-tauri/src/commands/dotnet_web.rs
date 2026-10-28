//! DotNetForge for web and cloud projects (`packs/dotnetforge/lang-dotnet`: `aspnet`,
//! `razor`, `efcore`, `aspire`, `docker`, `watch`, `secrets`): ASP.NET Core endpoints and
//! their `.http` requests, Blazor components, EF Core (`dotnet ef` command lines and their
//! output, the migrations of the sources), .NET Aspire AppHosts and their resources, the
//! output of running AppHosts and `dotnet watch` runs, Dockerfiles and the ".NET Docker" /
//! ".NET Aspire" run configurations, and user secrets.
//!
//! Reading sources runs nothing. `dotnet ef`, `docker build` and the run configurations
//! are started by the UI through `run_start`, which refuses them in Safe Mode. User
//! secrets live in the user profile (never in the project); Forge reads and writes them
//! only for local projects.

use std::collections::BTreeMap;
use std::path::PathBuf;

use dotnetforge_lang_dotnet::aspire::{self, AppHost, AspireOutput, DOTNET_ASPIRE};
use dotnetforge_lang_dotnet::aspnet::{self, Endpoint};
use dotnetforge_lang_dotnet::docker::{self, GeneratedDocker, DOTNET_DOCKER};
use dotnetforge_lang_dotnet::efcore::{self, EfModel, EfOutput, EfRequest};
use dotnetforge_lang_dotnet::razor::{self, RazorComponent};
use dotnetforge_lang_dotnet::secrets::{self, UserSecrets};
use dotnetforge_lang_dotnet::watch::{parse_watch_output, WatchEvent};
use dotnetforge_lang_dotnet::DotnetWorkspace;
use forge_core::{ForgeError, Result};
use forge_process::{CommandLine, RunConfiguration};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::dotnet::{context, is_local, other, to_command_line, to_runtools, workspace};
use crate::commands::workspace::blocking;
use crate::state::AppState;

fn collect(state: &AppState, extensions: &[&str]) -> Result<BTreeMap<String, String>> {
    let backend = state.backend()?;
    let req = forge_core::search::CollectRequest {
        extensions: extensions.iter().map(|s| s.to_string()).collect(),
        max_files: 20_000,
        max_file_size: 2 << 20,
        max_total: 256 << 20,
    };
    Ok(backend
        .collect_files(&req)?
        .into_iter()
        .filter(|f| {
            !f.path
                .split('/')
                .any(|s| s == "bin" || s == "obj" || s == "node_modules")
        })
        .map(|f| (f.path, f.text))
        .collect())
}

/// The files below the folders of some projects.
fn in_projects<'a>(
    files: &'a BTreeMap<String, String>,
    ws: &'a DotnetWorkspace,
    keep: impl Fn(&dotnetforge_lang_dotnet::msbuild::DotnetProject) -> bool + 'a,
) -> impl Iterator<Item = (&'a String, &'a String)> + 'a {
    let dirs: Vec<String> = ws
        .projects
        .iter()
        .filter(|p| keep(p))
        .map(|p| p.dir())
        .collect();
    files.iter().filter(move |(p, _)| {
        dirs.iter()
            .any(|d| d.is_empty() || p.starts_with(&format!("{d}/")))
    })
}

// ---- ASP.NET Core ------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointsPayload {
    pub endpoints: Vec<Endpoint>,
    /// Project file → the URL it listens on (launch profiles).
    pub base_urls: BTreeMap<String, String>,
}

/// The endpoints of the solution's web projects (Endpoints tool window).
#[tauri::command]
pub async fn dotnet_endpoints(app: AppHandle) -> Result<EndpointsPayload> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ws = workspace(&state)?;
        let files = collect(&state, &["cs", "razor", "cshtml"])?;
        let mut endpoints = Vec::new();
        for (path, text) in in_projects(&files, &ws, |p| p.web) {
            for mut e in aspnet::scan_file(path, text) {
                e.project = ws.project_of_file(path).map(|p| p.path.clone());
                endpoints.push(e);
            }
        }
        aspnet::sort(&mut endpoints);
        let base_urls = ws
            .projects
            .iter()
            .filter(|p| p.web)
            .map(|p| (p.path.clone(), aspnet::base_url(&p.launch_profiles)))
            .collect();
        Ok(EndpointsPayload {
            endpoints,
            base_urls,
        })
    })
    .await
}

/// "Generate HTTP Request": an `.http` request for an endpoint.
#[tauri::command]
pub async fn dotnet_http_request(endpoint: Endpoint, app: AppHandle) -> Result<String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ws = workspace(&state)?;
        let base = endpoint
            .project
            .as_deref()
            .and_then(|p| ws.project(p))
            .map(|p| aspnet::base_url(&p.launch_profiles))
            .unwrap_or_else(|| aspnet::base_url(&[]));
        Ok(aspnet::http_request(&endpoint, &base))
    })
    .await
}

/// Blazor components (`.razor` files with their parameters) for navigation and completion.
#[tauri::command]
pub async fn dotnet_razor_components(app: AppHandle) -> Result<Vec<RazorComponent>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let files = collect(&state, &["razor", "cs"])?;
        let razor_files: BTreeMap<String, String> = files
            .iter()
            .filter(|(p, _)| p.ends_with(".razor") || p.ends_with(".razor.cs"))
            .map(|(p, t)| (p.clone(), t.clone()))
            .collect();
        let ws = workspace(&state).ok();
        let mut out = razor::components(&razor_files);
        if let Some(ws) = ws {
            for c in &mut out {
                c.project = ws.project_of_file(&c.path).map(|p| p.path.clone());
            }
        }
        Ok(out)
    })
    .await
}

// ---- EF Core -----------------------------------------------------------------------------------

/// The EF Core projects, DbContexts and migrations of the sources.
#[tauri::command]
pub async fn dotnet_ef_model(app: AppHandle) -> Result<EfModel> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ws = workspace(&state)?;
        let projects = efcore::ef_projects(&ws.projects);
        let mut model = EfModel {
            local_tool: state
                .backend()?
                .read_optional(".config/dotnet-tools.json")?
                .is_some_and(|t| efcore::has_local_tool(&t)),
            ..EfModel::default()
        };
        if !projects.is_empty() {
            let paths: Vec<String> = projects.iter().map(|p| p.path.clone()).collect();
            let files = collect(&state, &["cs"])?;
            for (path, text) in in_projects(&files, &ws, |p| paths.contains(&p.path)) {
                let (contexts, migrations) = efcore::scan_source(path, text);
                let project = ws.project_of_file(path).map(|p| p.path.clone());
                model.contexts.extend(contexts.into_iter().map(|mut c| {
                    c.project = project.clone();
                    c
                }));
                model.migrations.extend(migrations.into_iter().map(|mut m| {
                    m.project = project.clone();
                    m
                }));
            }
            model.migrations.sort_by(|a, b| a.id.cmp(&b.id));
        }
        model.projects = projects;
        Ok(model)
    })
    .await
}

/// The `dotnet ef …` command line of an EF Core action (the UI runs it with `run_start`).
#[tauri::command]
pub async fn dotnet_ef_command(request: EfRequest, app: AppHandle) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ctx = context(&state);
        let c = efcore::ef_command(&request, &ctx).map_err(ForgeError::Other)?;
        Ok(to_command_line(c, &state.project_root()?))
    })
    .await
}

/// The migrations (with their applied state) or the failure of a `dotnet ef … list` run.
#[tauri::command]
pub fn dotnet_ef_output(output: String) -> Result<EfOutput> {
    Ok(efcore::parse_ef_output(&output))
}

// ---- Aspire, watch and Docker --------------------------------------------------------------------

/// The Aspire AppHosts of the solution with their resources.
#[tauri::command]
pub async fn dotnet_aspire_hosts(app: AppHandle) -> Result<Vec<AppHost>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ws = workspace(&state)?;
        let backend = state.backend()?;
        let read = |p: &str| backend.read_optional(p).ok().flatten();
        Ok(aspire::app_hosts(&ws.projects, &read))
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutputPayload {
    pub aspire: AspireOutput,
    pub watch: Vec<WatchEvent>,
}

/// What a chunk of run output says: the Aspire dashboard URL, Hot Reload events.
#[tauri::command]
pub fn dotnet_run_output(text: String) -> Result<RunOutputPayload> {
    Ok(RunOutputPayload {
        aspire: aspire::parse_output(&text),
        watch: parse_watch_output(&text),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerfilePayload {
    #[serde(flatten)]
    pub generated: GeneratedDocker,
    pub dockerfile_exists: bool,
    pub dockerignore_exists: bool,
}

/// "Add Docker Support": the Dockerfile and `.dockerignore` of a project (the UI shows and
/// writes them).
#[tauri::command]
pub async fn dotnet_dockerfile(
    project: String,
    port: Option<u16>,
    app: AppHandle,
) -> Result<DockerfilePayload> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ws = workspace(&state)?;
        let backend = state.backend()?;
        let root_files: Vec<String> = backend
            .list_dir("")
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.name)
            .collect();
        let generated =
            docker::generate(&ws, &project, port, &root_files).map_err(ForgeError::Other)?;
        Ok(DockerfilePayload {
            dockerfile_exists: backend.read_optional(&generated.dockerfile_path)?.is_some(),
            dockerignore_exists: backend
                .read_optional(&generated.dockerignore_path)?
                .is_some(),
            generated,
        })
    })
    .await
}

/// `docker build` of a ".NET Docker" configuration (run before the container starts).
#[tauri::command]
pub async fn dotnet_docker_build(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        let c = docker::docker_commands(&to_runtools(&configuration)).map_err(ForgeError::Other)?;
        Ok(to_command_line(c.build, &state.project_root()?))
    })
    .await
}

/// The process of a ".NET Aspire" or ".NET Docker" configuration (`run.rs`).
pub(crate) fn run_command_line(state: &AppState, cfg: &RunConfiguration) -> Result<CommandLine> {
    let built = if cfg.type_id == DOTNET_DOCKER {
        docker::docker_commands(&to_runtools(cfg)).map(|c| c.run)
    } else {
        aspire::run_command(&to_runtools(cfg), &context(state))
    }
    .map_err(ForgeError::Other)?;
    let mut cmd = to_command_line(built, &state.project_root()?);
    cmd.env.extend(cfg.env.clone());
    Ok(cmd)
}

/// Whether `run.rs` builds a configuration's command line here.
pub(crate) fn handles(type_id: &str) -> bool {
    type_id == DOTNET_ASPIRE || type_id == DOTNET_DOCKER
}

// ---- User secrets --------------------------------------------------------------------------------

fn secrets_file(id: &str) -> Result<PathBuf> {
    if !secrets::valid_id(id) {
        return Err(ForgeError::Other(format!(
            "'{id}' is not a valid UserSecretsId"
        )));
    }
    let home = dirs::home_dir().ok_or_else(|| other("No home folder"))?;
    let appdata = std::env::var("APPDATA").ok();
    Ok(PathBuf::from(secrets::secrets_path(
        id,
        std::env::consts::OS,
        appdata.as_deref(),
        &home.to_string_lossy(),
    )))
}

fn require_local(state: &AppState) -> Result<()> {
    if is_local(state) {
        Ok(())
    } else {
        Err(other(
            "User secrets of remote projects live on the remote machine; use dotnet user-secrets in its terminal",
        ))
    }
}

fn user_secrets(project: &str, id: Option<String>) -> Result<UserSecrets> {
    let path = match id.as_deref() {
        Some(i) => Some(secrets_file(i)?),
        None => None,
    };
    Ok(UserSecrets {
        project: project.to_string(),
        exists: path.as_ref().is_some_and(|p| p.is_file()),
        path: path.map(|p| p.to_string_lossy().into_owned()),
        id,
    })
}

/// A project's `UserSecretsId` and where its `secrets.json` lives.
#[tauri::command]
pub async fn dotnet_user_secrets(project: String, app: AppHandle) -> Result<UserSecrets> {
    blocking(move || {
        let state = app.state::<AppState>();
        require_local(&state)?;
        let ws = workspace(&state)?;
        let p = ws
            .project(&project)
            .ok_or_else(|| other(format!("No project '{project}'")))?;
        user_secrets(&p.path, p.properties.get("UserSecretsId").cloned())
    })
    .await
}

fn random_bytes() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 16];
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        h.write_usize(i);
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    out
}

/// `dotnet user-secrets init` without running the project: a `UserSecretsId` in the
/// project file and an empty `secrets.json` in the user profile.
#[tauri::command]
pub async fn dotnet_user_secrets_init(project: String, app: AppHandle) -> Result<UserSecrets> {
    blocking(move || {
        let state = app.state::<AppState>();
        require_local(&state)?;
        let ws = workspace(&state)?;
        let p = ws
            .project(&project)
            .ok_or_else(|| other(format!("No project '{project}'")))?;
        let id = match p.properties.get("UserSecretsId") {
            Some(id) => id.clone(),
            None => {
                let backend = state.backend()?;
                let id = secrets::new_id(random_bytes());
                let xml = backend.read_file(&p.path)?;
                let updated = secrets::add_user_secrets_id(&xml, &id).map_err(ForgeError::Other)?;
                backend.write_file(&p.path, &updated)?;
                id
            }
        };
        let file = secrets_file(&id)?;
        if !file.exists() {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
            }
            std::fs::write(&file, "{\n}\n").map_err(|e| ForgeError::io(&file, e))?;
            restrict(&file);
        }
        user_secrets(&p.path, Some(id))
    })
    .await
}

#[cfg(unix)]
fn restrict(file: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_file: &std::path::Path) {}

/// The `secrets.json` of an id (empty object when it does not exist yet).
#[tauri::command]
pub async fn dotnet_user_secrets_read(id: String, app: AppHandle) -> Result<String> {
    blocking(move || {
        require_local(&app.state::<AppState>())?;
        let file = secrets_file(&id)?;
        match std::fs::read_to_string(&file) {
            Ok(t) => Ok(t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("{\n}\n".into()),
            Err(e) => Err(ForgeError::io(&file, e)),
        }
    })
    .await
}

/// Save an edited `secrets.json` (the editor tab of Manage User Secrets).
#[tauri::command]
pub async fn dotnet_user_secrets_write(id: String, content: String, app: AppHandle) -> Result<()> {
    blocking(move || {
        require_local(&app.state::<AppState>())?;
        let file = secrets_file(&id)?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
        }
        std::fs::write(&file, content).map_err(|e| ForgeError::io(&file, e))?;
        restrict(&file);
        Ok(())
    })
    .await
}
