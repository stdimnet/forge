//! Web tooling: File Watchers (`forge_runtools::watchers`), OpenAPI and GraphQL files,
//! Nx workspaces, Nuxt pages and Deno tasks (`webforge-web-tooling`). The project's files
//! are read through its backend, so remote projects work too; watcher runs start through
//! the usual run commands (and are refused in Safe Mode here, before anything is planned).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_runtools::watchers::{
    self, FileWatcher, PlanInput, Trigger, WatcherProblem, WatcherRun, WatcherTemplate,
};
use forge_scopes::{Scope, ScopeContext, ScopeFilter};
use serde::Serialize;
use tauri::State;
use webforge_web_tooling::{deno, graphql, nuxt, nx, openapi};

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Project files looked at by the scans of this module.
const FILE_LIMIT: usize = 20_000;
/// Files read per scan (OpenAPI candidates, schema files, project.json files).
const READ_LIMIT: usize = 400;

pub const WATCHERS_KEY: &str = "tools.fileWatchers";

fn files_of(backend: &dyn Backend) -> Vec<String> {
    backend
        .list_files(FILE_LIMIT)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !p.contains("node_modules/") && !p.starts_with("vendor/"))
        .collect()
}

// ---- File Watchers --------------------------------------------------------------------

/// The watcher templates of the "+" menu.
#[tauri::command]
pub fn file_watchers_templates() -> Vec<WatcherTemplate> {
    watchers::templates()
}

/// Problems that keep the Edit Watcher dialog from closing.
#[tauri::command]
pub fn file_watchers_validate(watcher: FileWatcher, others: Vec<FileWatcher>) -> Vec<String> {
    watchers::validate(&watcher, &others)
}

fn configured_watchers(state: &AppState) -> Vec<FileWatcher> {
    state
        .read_settings()
        .get_as::<Vec<BTreeMap<String, serde_json::Value>>>(WATCHERS_KEY)
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let strings: BTreeMap<String, String> = m
                .iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), s)
                })
                .collect();
            FileWatcher::from_object(&strings)
        })
        .collect()
}

/// The watcher runs for saved, typed-in or externally changed files (project-relative).
/// Refused in Safe Mode: watchers run project tools.
#[tauri::command]
pub async fn file_watchers_plan(
    paths: Vec<String>,
    trigger: Trigger,
    state: State<'_, AppState>,
) -> Result<Vec<WatcherRun>> {
    let list = configured_watchers(&state);
    if list.iter().all(|w| !w.enabled) || paths.is_empty() {
        return Ok(Vec::new());
    }
    state.require_trusted("File Watchers")?;
    let backend = state.backend()?;
    let scopes = crate::inspections::scope_set(&state.read_settings());
    blocking(move || {
        let info = backend.info().clone();
        let root = info.root.to_string_lossy().into_owned();
        let files = std::cell::OnceCell::new();
        let all = || files.get_or_init(|| files_of(backend.as_ref()));
        let in_scope = |scope: &str, path: &str| {
            let Some(spec) = scopes.resolve(scope) else {
                return true;
            };
            Scope::new(&ScopeFilter {
                scope: spec,
                context: ScopeContext::default(),
            })
            .map(|s| s.contains(path))
            .unwrap_or(true)
        };
        let sources = |w: &FileWatcher| -> BTreeMap<String, String> {
            all()
                .iter()
                .filter(|p| w.accepts(p))
                .take(READ_LIMIT)
                .filter_map(|p| Some((p.clone(), backend.read_file(p).ok()?)))
                .collect()
        };
        let resolve_program = |name: &str| -> Option<String> {
            let rel = format!("node_modules/.bin/{name}");
            let found = backend.read_optional(&rel).ok().flatten().is_some()
                || (info.remote.is_none() && info.root.join(&rel).exists());
            found.then(|| info.root.join(&rel).to_string_lossy().into_owned())
        };
        Ok(watchers::plan(
            &list,
            &PlanInput {
                paths: &paths,
                trigger,
                root: &root,
                project_name: &info.name,
                in_scope: &in_scope,
                sources: &sources,
                resolve_program: &resolve_program,
            },
        ))
    })
    .await
}

/// Problems in a watcher's output (its output filters), for the Problems tool window.
#[tauri::command]
pub fn file_watchers_problems(
    output: String,
    filters: Vec<String>,
    cwd: String,
    state: State<'_, AppState>,
) -> Vec<WatcherProblem> {
    let root = state
        .project_root()
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_default();
    watchers::problems(&output, &filters, &cwd, &root)
}

// ---- OpenAPI ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiFile {
    pub path: String,
    pub title: String,
    pub spec: String,
    pub endpoints: Vec<openapi::Endpoint>,
}

fn is_spec_candidate(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let ext_ok = [".yaml", ".yml", ".json"]
        .iter()
        .any(|e| lower.ends_with(e));
    ext_ok
        && !lower.ends_with("package.json")
        && !lower.ends_with("composer.json")
        && !lower.ends_with("lock.json")
        && !lower.contains("tsconfig")
        && !lower.starts_with(".github/")
}

/// The OpenAPI / Swagger documents of the project with their endpoints (Endpoints tool
/// window). Files whose name says `openapi` / `swagger` / `api` are read first.
#[tauri::command]
pub async fn openapi_endpoints(state: State<'_, AppState>) -> Result<Vec<OpenApiFile>> {
    let backend = state.backend()?;
    blocking(move || {
        let mut candidates: Vec<String> = files_of(backend.as_ref())
            .into_iter()
            .filter(|p| is_spec_candidate(p))
            .collect();
        candidates.sort_by_key(|p| {
            let l = p.to_ascii_lowercase();
            !(l.contains("openapi") || l.contains("swagger") || l.contains("api"))
        });
        let mut out = Vec::new();
        for path in candidates.into_iter().take(READ_LIMIT) {
            let Ok(text) = backend.read_file(&path) else {
                continue;
            };
            if !openapi::is_openapi(&text) {
                continue;
            }
            if let Some(p) = openapi::preview(&path, &text) {
                out.push(OpenApiFile {
                    endpoints: openapi::endpoints(&path, &text),
                    title: p.title,
                    spec: p.spec,
                    path,
                });
            }
        }
        Ok(out)
    })
    .await
}

/// The preview panel's model of an OpenAPI document (its editor text).
#[tauri::command]
pub fn openapi_preview(path: String, content: String) -> Option<openapi::Preview> {
    openapi::preview(&path, &content)
}

/// "Generate HTTP Request" for an operation.
#[tauri::command]
pub fn openapi_http_request(
    path: String,
    content: String,
    method: String,
    op_path: String,
) -> Option<String> {
    openapi::http_request(&path, &content, &method, &op_path)
}

// ---- GraphQL ---------------------------------------------------------------------------

/// The project's schema, read at most every few seconds.
#[derive(Default)]
pub struct GraphqlState {
    cache: Mutex<Option<(Instant, String, Arc<SchemaFiles>)>>,
}

pub struct SchemaFiles {
    pub config: Option<graphql::Config>,
    pub files: Vec<(String, String)>,
}

const SCHEMA_TTL: Duration = Duration::from_secs(3);

fn glob_match(pattern: &str, path: &str) -> bool {
    forge_jsonschema::mapping::glob_match(pattern.trim_start_matches("./"), path)
}

fn load_schema(backend: &dyn Backend) -> SchemaFiles {
    let files = files_of(backend);
    let config = graphql::CONFIG_FILES.iter().find_map(|name| {
        let text = backend.read_optional(name).ok().flatten()?;
        Some(graphql::parse_config(name, &text))
    });
    let mut schema_files: Vec<(String, String)> = Vec::new();
    let is_gql = |p: &str| {
        let l = p.to_ascii_lowercase();
        l.ends_with(".graphql") || l.ends_with(".gql") || l.ends_with(".graphqls")
    };
    match &config {
        Some(c) if !c.schema.is_empty() => {
            for p in files
                .iter()
                .filter(|p| c.schema.iter().any(|g| glob_match(g, p) || *p == g))
            {
                if let Ok(text) = backend.read_file(p) {
                    let sdl = if p.ends_with(".json") {
                        graphql::introspection_to_sdl(&text).unwrap_or_default()
                    } else {
                        text
                    };
                    schema_files.push((p.clone(), sdl));
                }
            }
        }
        _ => {
            for p in files.iter().filter(|p| is_gql(p)).take(READ_LIMIT) {
                if let Ok(text) = backend.read_file(p) {
                    if graphql::is_schema_file(&text) {
                        schema_files.push((p.clone(), text));
                    }
                }
            }
        }
    }
    SchemaFiles {
        config,
        files: schema_files,
    }
}

fn schema(state: &GraphqlState, backend: &dyn Backend) -> Arc<SchemaFiles> {
    let root = backend.info().root.to_string_lossy().into_owned();
    let mut guard = state.cache.lock().expect("graphql cache");
    if let Some((at, r, s)) = guard.as_ref() {
        if at.elapsed() < SCHEMA_TTL && *r == root {
            return s.clone();
        }
    }
    let loaded = Arc::new(load_schema(backend));
    *guard = Some((Instant::now(), root, loaded.clone()));
    loaded
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlInfo {
    pub config: Option<graphql::Config>,
    pub schema_files: Vec<String>,
    pub types: Vec<String>,
}

/// The GraphQL config and schema of the project (status and the endpoints list).
#[tauri::command]
pub async fn graphql_info(
    state: State<'_, AppState>,
    gql: State<'_, Arc<GraphqlState>>,
) -> Result<GraphqlInfo> {
    let backend = state.backend()?;
    let gql = gql.inner().clone();
    blocking(move || {
        let s = schema(&gql, backend.as_ref());
        let set = graphql::SchemaSet::new(&s.files);
        Ok(GraphqlInfo {
            config: s.config.clone(),
            schema_files: s.files.iter().map(|(p, _)| p.clone()).collect(),
            types: set.type_names(),
        })
    })
    .await
}

/// Problems of a GraphQL file: schema files against each other, operations against the
/// project's schema (syntax only without one).
#[tauri::command]
pub async fn graphql_validate(
    path: String,
    content: String,
    state: State<'_, AppState>,
    gql: State<'_, Arc<GraphqlState>>,
) -> Result<Vec<graphql::Problem>> {
    let backend = state.backend()?;
    let gql = gql.inner().clone();
    blocking(move || {
        let s = schema(&gql, backend.as_ref());
        if graphql::is_schema_file(&content) {
            let mut files: Vec<(String, String)> = s
                .files
                .iter()
                .filter(|(p, _)| *p != path)
                .cloned()
                .collect();
            files.push((path.clone(), content));
            let set = graphql::SchemaSet::new(&files);
            return Ok(set.problems.get(&path).cloned().unwrap_or_default());
        }
        if s.files.is_empty() {
            return Ok(graphql::syntax_problems(&path, &content));
        }
        Ok(graphql::SchemaSet::new(&s.files).validate(&path, &content))
    })
    .await
}

/// Completion in a GraphQL operation (also the query of a `GRAPHQL` request).
#[tauri::command]
pub async fn graphql_complete(
    content: String,
    offset: usize,
    state: State<'_, AppState>,
    gql: State<'_, Arc<GraphqlState>>,
) -> Result<graphql::Completion> {
    let backend = state.backend()?;
    let gql = gql.inner().clone();
    blocking(move || {
        let s = schema(&gql, backend.as_ref());
        Ok(graphql::SchemaSet::new(&s.files).complete(&content, offset))
    })
    .await
}

/// "Get GraphQL Schema from Endpoint": run the introspection query against `url` and save
/// the schema as SDL to `target` (project-relative, `schema.graphql` by default).
#[tauri::command]
pub async fn graphql_introspect(
    url: String,
    headers: Option<BTreeMap<String, String>>,
    target: Option<String>,
    state: State<'_, AppState>,
    gql: State<'_, Arc<GraphqlState>>,
) -> Result<String> {
    let backend = state.backend()?;
    let body = graphql::request_body(graphql::INTROSPECTION_QUERY, None, None)
        .map_err(ForgeError::Other)?;
    let mut req = forge_scratches::http::HttpRequest {
        index: 0,
        line: 0,
        end_line: 0,
        name: None,
        method: "POST".into(),
        url: url.clone(),
        version: None,
        headers: vec![forge_scratches::http::Header {
            name: "Content-Type".into(),
            value: "application/json".into(),
        }],
        body: Some(body),
        handler: None,
        handler_file: None,
    };
    for (k, v) in headers.unwrap_or_default() {
        req.headers
            .push(forge_scratches::http::Header { name: k, value: v });
    }
    let resp = forge_scratches::client::send(&req, Duration::from_secs(30))
        .await
        .map_err(ForgeError::Other)?;
    if !(200..300).contains(&resp.status) {
        return Err(ForgeError::Other(format!(
            "{url} answered {} {}",
            resp.status, resp.reason
        )));
    }
    let sdl = graphql::introspection_to_sdl(&resp.body).map_err(ForgeError::Other)?;
    let target = target.unwrap_or_else(|| "schema.graphql".into());
    let gql = gql.inner().clone();
    blocking(move || {
        backend.write_file(&target, &sdl)?;
        *gql.cache.lock().expect("graphql cache") = None;
        Ok(target)
    })
    .await
}

// ---- Nx, Nuxt, Deno --------------------------------------------------------------------

/// The Nx workspace: projects, targets and the project graph.
#[tauri::command]
pub async fn nx_workspace(state: State<'_, AppState>) -> Result<nx::Workspace> {
    let backend = state.backend()?;
    blocking(move || {
        let mut texts = BTreeMap::new();
        for p in files_of(backend.as_ref()) {
            let name = p.rsplit('/').next().unwrap_or(&p);
            if matches!(name, "nx.json" | "project.json" | "package.json")
                && texts.len() < READ_LIMIT
            {
                if let Ok(t) = backend.read_file(&p) {
                    texts.insert(p, t);
                }
            }
        }
        Ok(nx::workspace(&texts))
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NuxtProject {
    pub root: String,
    pub routes: Vec<nuxt::Route>,
    pub components: Vec<nuxt::Component>,
}

/// Pages (routes) and auto-imported components of the Nuxt app in `root` (the folder the
/// detection found it in).
#[tauri::command]
pub async fn nuxt_project(root: Option<String>, state: State<'_, AppState>) -> Result<NuxtProject> {
    let backend = state.backend()?;
    blocking(move || {
        let root = root.unwrap_or_default();
        let files = files_of(backend.as_ref());
        let paths = files.iter().map(String::as_str);
        Ok(NuxtProject {
            routes: nuxt::routes(&root, paths.clone()),
            components: nuxt::components(&root, paths),
            root,
        })
    })
    .await
}

/// Tasks of the root `deno.json` / `deno.jsonc`.
#[tauri::command]
pub async fn deno_tasks(state: State<'_, AppState>) -> Result<Vec<deno::Task>> {
    let backend = state.backend()?;
    blocking(move || {
        for name in deno::CONFIG_FILES {
            if let Some(text) = backend.read_optional(name)? {
                return Ok(deno::tasks(&text));
            }
        }
        Ok(Vec::new())
    })
    .await
}
