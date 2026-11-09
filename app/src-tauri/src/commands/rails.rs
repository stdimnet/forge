//! Ruby on Rails IPC (RubyMine's Rails support); the analysis lives in
//! `packs/rubyforge/fw-rails`:
//!
//! - the project model for the Endpoints view (routes from `config/routes.rb`, or `bin/rails
//!   routes` in trusted projects when `rails.routesFromRails` asks for it), models, tables,
//!   locales, Stimulus controllers and the databases of `config/database.yml`, read in one
//!   round trip (also for remote projects) and cached for a few seconds;
//! - Go to Declaration, completion, gutter marks, inspections and folds on the editor's text;
//! - the generators of the Run Rails Generator dialog (`bin/rails generate` in trusted
//!   projects) and the command lines of generators, the console and the database console
//!   (trusted projects only: they boot the application);
//! - data sources from `config/database.yml` (passwords go to the keychain);
//! - the "Rails" run configuration (`bin/rails server`), debugged through rdbg.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{CollectRequest, ForgeError, Result};
use forge_db::config::{DataSource, Driver, Scope};
use forge_db::DataSourceInfo;
use forge_process::{CommandLine, RunConfiguration};
use rubyforge_fw_rails::editor::{self, Completions, FileMarks};
use rubyforge_fw_rails::generators::{self, Generator};
use rubyforge_fw_rails::{command, Location, Options, Project, RailsInfo};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::db::DbState;
use crate::commands::ruby::{context, flag, setting, to_command_line, to_runtools};
use crate::commands::workspace::blocking;
use crate::state::AppState;

const TTL: Duration = Duration::from_secs(5);
const MAX_FILES: usize = 20_000;
const MAX_FILE_SIZE: u64 = 1024 * 1024;
const MAX_TOTAL_SIZE: u64 = 48 * 1024 * 1024;
const RAILS_TIMEOUT: Duration = Duration::from_secs(90);

struct Cached {
    key: String,
    options: Options,
    at: Instant,
    project: Arc<Project>,
}

static CACHE: LazyLock<Mutex<Option<Cached>>> = LazyLock::new(|| Mutex::new(None));
/// `bin/rails routes` output per project (it boots the application: slow).
static ROUTES: LazyLock<Mutex<BTreeMap<String, std::result::Result<String, String>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
/// `bin/rails generate` generators per project.
static GENERATORS: LazyLock<Mutex<BTreeMap<String, Vec<Generator>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn key(state: &AppState) -> Result<String> {
    Ok(state.backend()?.info().display_root())
}

/// Read the files the analysis needs.
fn load(backend: &dyn Backend, options: Options) -> Result<Project> {
    let mut files: BTreeMap<String, String> = backend
        .collect_files(&CollectRequest {
            extensions: rubyforge_fw_rails::EXTENSIONS
                .iter()
                .map(|e| e.to_string())
                .collect(),
            max_files: MAX_FILES,
            max_file_size: MAX_FILE_SIZE,
            max_total: MAX_TOTAL_SIZE,
        })?
        .into_iter()
        .filter(|f| rubyforge_fw_rails::wanted(&f.path))
        .map(|f| (f.path, f.text))
        .collect();
    for name in rubyforge_fw_rails::FILE_NAMES {
        if let Ok(Some(text)) = backend.read_optional(name) {
            files.insert(name.to_string(), text);
        }
    }
    Ok(Project::new(files, options))
}

fn options(state: &AppState, key: &str) -> Options {
    let mut env: BTreeMap<String, String> = std::env::vars().collect();
    env.extend(context(state).env);
    let routes_output = if flag(state, "rails.routesFromRails", false) && !state.safe_mode.is_on() {
        ROUTES
            .lock()
            .ok()
            .and_then(|m| m.get(key).cloned())
            .and_then(|r| r.ok())
    } else {
        None
    };
    Options {
        routes_output,
        env,
        // Windows variable names are case-insensitive (`Path`, `ComSpec`).
        env_case_insensitive: cfg!(windows),
    }
}

/// The analysed project, read again after [`TTL`] or with `refresh`.
pub(crate) fn project(state: &AppState, refresh: bool) -> Result<Arc<Project>> {
    let backend = state.backend()?;
    let key = backend.info().display_root();
    let options = options(state, &key);
    let mut guard = CACHE
        .lock()
        .map_err(|_| ForgeError::Other("rails cache poisoned".into()))?;
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

fn root_command(state: &AppState, c: rubyforge_lang_ruby::RubyCommand) -> Result<CommandLine> {
    Ok(to_command_line(c, &state.project_root()?))
}

/// Run `bin/rails routes` and keep its output (or the error) for the project.
fn refresh_routes(state: &AppState, key: &str, binstub: bool) {
    let result = (|| -> std::result::Result<String, String> {
        state
            .require_trusted("Reading routes with bin/rails routes")
            .map_err(|e| e.to_string())?;
        let backend = state.backend().map_err(|e| e.to_string())?;
        let cmd = root_command(state, command::routes_command(&context(state), binstub))
            .map_err(|e| e.to_string())?;
        let out = capture(backend.as_ref(), &cmd, "", RAILS_TIMEOUT).map_err(|e| e.to_string())?;
        if out.code != Some(0) {
            return Err(out
                .stderr
                .lines()
                .find(|l| l.contains("Error") || l.contains("error"))
                .or_else(|| out.stderr.lines().next())
                .unwrap_or("bin/rails routes failed")
                .trim()
                .to_string());
        }
        Ok(out.stdout)
    })();
    if let Ok(mut m) = ROUTES.lock() {
        m.insert(key.to_string(), result);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RailsProject {
    #[serde(flatten)]
    pub info: RailsInfo,
    /// Why `bin/rails routes` was not used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routes_error: Option<String>,
}

/// Routes, controllers, models, tables, locales, Stimulus controllers and databases.
#[tauri::command]
pub async fn rails_project(refresh: bool, app: AppHandle) -> Result<RailsProject> {
    blocking(move || {
        let state = app.state::<AppState>();
        let key = key(&state)?;
        let mut routes_error = None;
        if flag(&state, "rails.routesFromRails", false) {
            if state.safe_mode.is_on() {
                routes_error = Some("Safe Mode: routes are read from config/routes.rb".into());
            } else {
                let known = ROUTES.lock().ok().is_some_and(|m| m.contains_key(&key));
                if refresh || !known {
                    let binstub = state
                        .backend()?
                        .read_optional("bin/rails")
                        .ok()
                        .flatten()
                        .is_some();
                    refresh_routes(&state, &key, binstub);
                }
                if let Some(Err(e)) = ROUTES.lock().ok().and_then(|m| m.get(&key).cloned()) {
                    routes_error = Some(format!("bin/rails routes failed: {e}"));
                }
            }
        }
        let p = project(&state, refresh)?;
        Ok(RailsProject {
            info: p.info.clone(),
            routes_error,
        })
    })
    .await
}

/// Go to Declaration on route helpers, partials, translation keys, Stimulus and Turbo names.
#[tauri::command]
pub async fn rails_goto(
    path: String,
    content: String,
    offset: usize,
    app: AppHandle,
) -> Result<Vec<Location>> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(editor::goto(
            &*project(&state, false)?,
            &path,
            &content,
            offset,
        ))
    })
    .await
}

/// Completions at `offset` (UTF-16).
#[tauri::command]
pub async fn rails_complete(
    path: String,
    content: String,
    offset: usize,
    app: AppHandle,
) -> Result<Option<Completions>> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(editor::complete(
            &*project(&state, false)?,
            &path,
            &content,
            offset,
        ))
    })
    .await
}

/// Gutter marks, problems and folds of an editor.
#[tauri::command]
pub async fn rails_marks(path: String, content: String, app: AppHandle) -> Result<FileMarks> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(editor::marks(&*project(&state, false)?, &path, &content))
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generators {
    pub generators: Vec<Generator>,
    /// `rails` (bin/rails generate) or `static` (Rails' own and lib/generators).
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The generators of the Run Rails Generator dialog.
#[tauri::command]
pub async fn rails_generators(refresh: bool, app: AppHandle) -> Result<Generators> {
    blocking(move || {
        let state = app.state::<AppState>();
        list_generators(&state, refresh)
    })
    .await
}

fn list_generators(state: &AppState, refresh: bool) -> Result<Generators> {
    let p = project(state, false)?;
    let builtin = generators::built_in();
    let own = generators::project_generators(
        p.files()
            .map(|(path, _)| path)
            .filter(|f| f.starts_with("lib/generators/")),
    );
    let fallback = |message: Option<String>| {
        let mut list = builtin.clone();
        list.extend(own.clone());
        Generators {
            generators: list,
            source: "static".into(),
            message,
        }
    };
    if !flag(state, "rails.generatorsFromRails", true) {
        return Ok(fallback(None));
    }
    if state.safe_mode.is_on() {
        return Ok(fallback(Some(
            "Safe Mode: the generators of the application's gems are listed once you trust the project".into(),
        )));
    }
    let key = key(state)?;
    if !refresh {
        if let Some(g) = GENERATORS.lock().ok().and_then(|m| m.get(&key).cloned()) {
            return Ok(Generators {
                generators: g,
                source: "rails".into(),
                message: None,
            });
        }
    }
    let backend = state.backend()?;
    let cmd = root_command(
        state,
        command::generators_command(&context(state), p.info.binstub),
    )?;
    match capture(backend.as_ref(), &cmd, "", RAILS_TIMEOUT) {
        Ok(out) => {
            let mut known = builtin.clone();
            known.extend(own.clone());
            let mut list = generators::parse_generate_output(&out.stdout, &known);
            if list.is_empty() {
                return Ok(fallback(Some(
                    "bin/rails generate listed no generators".into(),
                )));
            }
            for g in own {
                if !list.iter().any(|x| x.name == g.name) {
                    list.push(g);
                }
            }
            if let Ok(mut m) = GENERATORS.lock() {
                m.insert(key, list.clone());
            }
            Ok(Generators {
                generators: list,
                source: "rails".into(),
                message: None,
            })
        }
        Err(e) => Ok(fallback(Some(format!("bin/rails generate failed: {e}")))),
    }
}

/// `bin/rails generate <generator> <arguments> [options]` (or `destroy`), run by the UI in
/// the Run tool window. Generators load the application: trusted projects only.
#[tauri::command]
pub fn rails_generate_command(
    generator: String,
    arguments: String,
    options: Vec<String>,
    destroy: bool,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    state.require_trusted("Running Rails generators")?;
    let binstub = state
        .backend()?
        .read_optional("bin/rails")
        .ok()
        .flatten()
        .is_some();
    let c = command::generate_command(
        &context(&state),
        binstub,
        &generator,
        &arguments,
        &options,
        destroy,
    )
    .map_err(ForgeError::Other)?;
    root_command(&state, c)
}

/// The Rails console (`console`) or database console (`dbconsole`) for a Terminal tab.
#[tauri::command]
pub fn rails_console_command(kind: String, state: State<'_, AppState>) -> Result<CommandLine> {
    state.require_trusted("Starting the Rails console")?;
    let binstub = state
        .backend()?
        .read_optional("bin/rails")
        .ok()
        .flatten()
        .is_some();
    let env = setting(&state, "rails.console.environment");
    let ctx = context(&state);
    let c = if kind == "dbconsole" {
        command::dbconsole_command(&ctx, binstub, &env)
    } else {
        command::console_command(
            &ctx,
            binstub,
            &env,
            flag(&state, "rails.console.sandbox", false),
        )
    }
    .map_err(ForgeError::Other)?;
    root_command(&state, c)
}

fn driver(name: &str) -> Option<Driver> {
    serde_json::from_value(serde_json::Value::String(name.to_string())).ok()
}

/// Add the databases of `config/database.yml` (`environments`; empty: all that the
/// Database tool window does not have yet) as project data sources.
#[tauri::command]
pub async fn rails_add_data_sources(
    environments: Vec<String>,
    app: AppHandle,
    db: State<'_, DbState>,
) -> Result<Vec<DataSourceInfo>> {
    let a = app.clone();
    let p = blocking(move || {
        let state = a.state::<AppState>();
        project(&state, true)
    })
    .await?;
    let existing: Vec<DataSource> = db.service.list().into_iter().map(|i| i.source).collect();
    let mut added = Vec::new();
    for c in &p.info.databases {
        if !environments.is_empty() && !environments.contains(&c.environment) {
            continue;
        }
        let Some(driver) = c.driver.as_deref().and_then(driver) else {
            continue;
        };
        let mut ds = DataSource {
            driver,
            host: c.host.clone(),
            port: c.port,
            database: c.database.clone(),
            user: c.username.clone(),
            file: c.file.clone(),
            comment: format!("From config/database.yml ({})", c.environment),
            scope: Scope::Project,
            ..DataSource::default()
        };
        if ds.host.is_empty() && driver != Driver::Sqlite {
            ds.host = "localhost".into();
        }
        // Production databases ask before unfiltered changes.
        if c.environment.starts_with("production") {
            ds.options.color = Some(forge_db::config::PRODUCTION_COLOR.to_string());
        }
        if existing
            .iter()
            .chain(added.iter().map(|i: &DataSourceInfo| &i.source))
            .any(|d| d.same_target(&ds))
        {
            continue;
        }
        ds.name = format!("{} ({})", forge_db::env::default_name(&ds), c.environment);
        added.push(db.service.save(ds, c.password.clone()).await?);
    }
    crate::commands::db::emit_sources(&app, &db.service);
    Ok(added)
}

/// The command line of a "Rails" run configuration (`run.rs`).
pub(crate) fn run_command(state: &AppState, cfg: &RunConfiguration) -> Result<CommandLine> {
    let binstub = state
        .backend()?
        .read_optional("bin/rails")
        .ok()
        .flatten()
        .is_some();
    let c = command::server_command(&to_runtools(cfg), &context(state), binstub)
        .map_err(ForgeError::Other)?;
    let mut cmd = root_command(state, c)?;
    cmd.env.extend(cfg.env.clone());
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_drivers() {
        assert_eq!(driver("postgres"), Some(Driver::Postgres));
        assert_eq!(driver("sqlite"), Some(Driver::Sqlite));
        assert_eq!(driver("mysql"), Some(Driver::Mysql));
        assert_eq!(driver("db2"), None);
    }
}
