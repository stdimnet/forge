//! RubyForge (`packs/rubyforge/*`): the project's Ruby (interpreters of version managers,
//! the version file), Bundler (Gemfile, Gemfile.lock, gems for External Libraries), Rake
//! tasks, Ruby and Rake run configurations, the RSpec / Minitest provider's context,
//! RuboCop checks and autocorrect, the IRB console, Ruby LSP's configuration and consented
//! install, and debugging with rdbg.
//!
//! Commands that run project code call `require_trusted`: loading Rake tasks with Rake
//! (falls back to reading the Rakefile), RuboCop (it loads the project's configuration and
//! plugins), the IRB console (`.irbrc`, the bundle). Command lines the UI starts (Bundler,
//! gem installs, Rake tasks) go through `run_start`, which refuses them in Safe Mode. The
//! interpreter probe runs the interpreter only, never project files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_lsp::{LspManager, LspServerConfig};
use forge_process::{CommandLine, RunConfiguration};
use rubyforge_lang_ruby::{Bundle, RakeTask, RubyCommand, RubyContext, RubyProblem};
use rubyforge_sdk::detect::{system_runner, Interpreter};
use rubyforge_sdk::{RubyEnv, RubyStatus};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::debug::DebugState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PLUGIN: &str = "forge.ruby";
/// The run environment variable that asks for a debug run under rdbg on this port.
pub const RDBG_PORT_ENV: &str = "FORGE_RDBG_PORT";
const RAKE_TIMEOUT: Duration = Duration::from_secs(120);
const RUBOCOP_TIMEOUT: Duration = Duration::from_secs(60);

// ---- Settings ------------------------------------------------------------------------------

pub(crate) fn setting(state: &AppState, key: &str) -> String {
    state
        .read_settings()
        .get_as::<String>(key)
        .map(|v| v.trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn flag(state: &AppState, key: &str, default: bool) -> bool {
    state.read_settings().get_bool(key).unwrap_or(default)
}

fn list(state: &AppState, key: &str) -> Vec<String> {
    state
        .read_settings()
        .get_as::<Vec<String>>(key)
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_local(state: &AppState) -> bool {
    state
        .backend()
        .map(|b| b.info().remote.is_none())
        .unwrap_or(true)
}

fn enabled(state: &AppState) -> bool {
    state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled(PLUGIN)
}

fn has_file(backend: &dyn Backend, path: &str) -> bool {
    backend.read_optional(path).ok().flatten().is_some()
}

// ---- Interpreters ---------------------------------------------------------------------------

/// Detected status per project root and setting, looked up again after a minute.
type Cached = (PathBuf, String, Instant, RubyStatus);
static STATUS: Mutex<Option<Cached>> = Mutex::new(None);

/// The interpreters added in Settings (`ruby.interpreters`), probed for their version.
fn custom_interpreters(state: &AppState) -> Vec<Interpreter> {
    list(state, "ruby.interpreters")
        .into_iter()
        .filter(|p| Path::new(p).is_file())
        .map(|p| probe_interpreter(&p))
        .collect()
}

fn probe_interpreter(path: &str) -> Interpreter {
    let ruby = Path::new(path);
    let version = system_runner(
        ruby,
        &["-e".to_string(), rubyforge_sdk::detect::PROBE.to_string()],
    )
    .and_then(|o| rubyforge_sdk::detect::parse_probe(&o))
    .map(|p| p.version)
    .unwrap_or_default();
    Interpreter {
        path: path.to_string(),
        name: if version.is_empty() {
            format!("custom: {path}")
        } else {
            format!("custom: {version}")
        },
        version,
        manager: "custom".into(),
        bin_dir: ruby
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        gem_paths: Vec::new(),
    }
}

/// The Ruby status of the project (local projects; remote ones get an empty status).
pub(crate) fn status(state: &AppState, refresh: bool) -> RubyStatus {
    let Ok(backend) = state.backend() else {
        return RubyStatus::default();
    };
    if !is_local(state) {
        return RubyStatus::default();
    }
    let root = backend.info().root.clone();
    let chosen = setting(state, "ruby.interpreter");
    {
        let cached = STATUS.lock().expect("ruby status");
        if let Some((r, c, at, s)) = cached.as_ref() {
            if !refresh && *r == root && *c == chosen && at.elapsed() < Duration::from_secs(60) {
                return s.clone();
            }
        }
    }
    let read = |p: &str| backend.read_optional(p).ok().flatten();
    let request = rubyforge_lang_ruby::requested_version(&read);
    let env = RubyEnv::system();
    let mut s = rubyforge_sdk::detect(&env, &chosen, request.clone(), &system_runner);
    let custom = custom_interpreters(state);
    if !custom.is_empty() {
        s.interpreters.extend(custom);
        let global = rubyforge_sdk::detect::global_version(&env);
        if let Some(c) = rubyforge_sdk::detect::choose(
            &s.interpreters,
            &chosen,
            request.as_ref(),
            global.as_deref(),
        ) {
            if s.chosen.as_ref() != Some(&c) {
                s.chosen = Some(c);
            }
        }
    }
    // A Ruby inside an untrusted project must not run, not even for its version.
    if state.safe_mode.is_on() {
        if let Some(c) = &s.chosen {
            if Path::new(&c.interpreter.path).starts_with(&root) {
                s.chosen = None;
            }
        }
    }
    *STATUS.lock().expect("ruby status") = Some((root, chosen, Instant::now(), s.clone()));
    s
}

/// The PATH of this process (the base of Ruby commands' PATH).
fn base_path() -> String {
    std::env::var("PATH").unwrap_or_default()
}

/// `KEY=VALUE` entries of `ruby.environment`.
fn environment(state: &AppState) -> BTreeMap<String, String> {
    list(state, "ruby.environment")
        .iter()
        .filter_map(|e| e.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// The project's bundle (Gemfile and Gemfile.lock, read without running Bundler).
fn bundle_of(backend: &dyn Backend) -> Option<Bundle> {
    let gemfile = backend
        .read_optional("Gemfile")
        .ok()
        .flatten()
        .or_else(|| backend.read_optional("gems.rb").ok().flatten())?;
    let lock = backend
        .read_optional("Gemfile.lock")
        .ok()
        .flatten()
        .or_else(|| backend.read_optional("gems.locked").ok().flatten());
    Some(rubyforge_lang_ruby::bundle(&gemfile, lock.as_deref()))
}

/// The project's Ruby context for command lines and the test provider.
pub(crate) fn context(state: &AppState) -> RubyContext {
    let local = is_local(state);
    let s = if local {
        status(state, false)
    } else {
        RubyStatus::default()
    };
    let backend = state.backend().ok();
    let bundle = backend.as_ref().and_then(|b| bundle_of(b.as_ref()));
    let mut env = rubyforge_sdk::command_env(&s, &base_path(), cfg!(windows) && local);
    env.extend(environment(state));
    let bundle_setting = setting(state, "ruby.bundler.path");
    RubyContext {
        ruby: s
            .chosen
            .as_ref()
            .map(|c| c.interpreter.path.clone())
            .unwrap_or_default(),
        bundle: bundle_setting,
        use_bundler: bundle.is_some() && flag(state, "ruby.bundler.useBundleExec", true),
        tools: s.tools.clone(),
        env,
        gems: bundle
            .map(|b| b.gems.into_iter().map(|g| g.name).collect())
            .unwrap_or_default(),
        windows: cfg!(windows) && local,
    }
}

pub(crate) fn to_command_line(c: RubyCommand, root: &Path) -> CommandLine {
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = if c.cwd.is_empty() {
        root.to_path_buf()
    } else if Path::new(&c.cwd).is_absolute() {
        PathBuf::from(&c.cwd)
    } else {
        root.join(&c.cwd)
    };
    cmd.env = c.env;
    cmd
}

pub(crate) fn to_runtools(cfg: &RunConfiguration) -> forge_runtools::Configuration {
    let mut options = cfg.options.clone();
    // The configuration's own interpreter (Ruby configurations' "Ruby interpreter" field).
    if !cfg.interpreter.trim().is_empty() {
        options.insert("interpreter".into(), cfg.interpreter.trim().to_string());
    }
    forge_runtools::Configuration {
        name: cfg.name.clone(),
        type_id: cfg.type_id.clone(),
        working_directory: cfg.working_directory.clone(),
        interpreter_options: cfg.interpreter_options.clone(),
        arguments: cfg.arguments.clone(),
        options,
        before_launch: Vec::new(),
    }
}

/// What Settings | Ruby and the status bar show about Ruby LSP.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspInfo {
    /// The ruby-lsp Forge uses (setting, the interpreter's, Forge's install, PATH).
    pub path: Option<String>,
    /// Installed by Forge into its data folder.
    pub managed: bool,
    /// Where an install would go (`<data>/forge/langservers/ruby/<version>`).
    pub install_dir: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RubyStatusPayload {
    #[serde(flatten)]
    pub status: RubyStatus,
    /// The interpreter chosen in Settings (empty: automatic).
    pub setting: String,
    /// The project has a Gemfile.
    pub bundler: bool,
    pub lsp: LspInfo,
}

fn lsp_path(state: &AppState, s: &RubyStatus) -> Option<String> {
    let custom = setting(state, "ruby.lsp.path");
    if !custom.is_empty() {
        return Some(custom);
    }
    s.tools.get("ruby-lsp").cloned()
}

/// Interpreters, the chosen one and why, the version file, gem executables, Ruby LSP.
#[tauri::command]
pub async fn ruby_status(refresh: Option<bool>, app: AppHandle) -> Result<RubyStatusPayload> {
    let a = app.clone();
    blocking(move || {
        let state = a.state::<AppState>();
        let s = status(&state, refresh.unwrap_or(false));
        let bundler = state
            .backend()
            .map(|b| has_file(b.as_ref(), "Gemfile") || has_file(b.as_ref(), "gems.rb"))
            .unwrap_or(false);
        let path = lsp_path(&state, &s);
        let base = rubyforge_sdk::lsp::default_dir();
        let lsp = LspInfo {
            managed: match (&path, &base) {
                (Some(p), Some(b)) => rubyforge_sdk::lsp::is_managed(b, Path::new(p)),
                _ => false,
            },
            path,
            install_dir: base.zip(s.chosen.as_ref()).map(|(b, c)| {
                rubyforge_sdk::lsp::install_dir(&b, &c.interpreter.version)
                    .to_string_lossy()
                    .into_owned()
            }),
        };
        Ok(RubyStatusPayload {
            setting: setting(&state, "ruby.interpreter"),
            status: s,
            bundler,
            lsp,
        })
    })
    .await
}

/// Probe a Ruby executable the user picked (Add Interpreter…).
#[tauri::command]
pub async fn ruby_interpreter_probe(path: String) -> Result<Interpreter> {
    blocking(move || {
        if !Path::new(&path).is_file() {
            return Err(ForgeError::Other(format!("{path} is not a file")));
        }
        let i = probe_interpreter(&path);
        if i.version.is_empty() {
            return Err(ForgeError::Other(format!(
                "{path} is not a Ruby interpreter"
            )));
        }
        Ok(i)
    })
    .await
}

// ---- Bundler and gems --------------------------------------------------------------------------

/// The project's bundle: Gemfile declarations merged with Gemfile.lock (Bundler tool
/// window, External Libraries). Read without running Bundler.
#[tauri::command]
pub fn ruby_bundle(state: State<'_, AppState>) -> Result<Option<Bundle>> {
    Ok(bundle_of(state.backend()?.as_ref()))
}

/// A Bundler action (`install`, `update [gem]`, `outdated`, `check`, `lock`, `init`) as a
/// command line (started by the UI with `run_start`).
#[tauri::command]
pub fn ruby_bundle_command(
    action: String,
    gem: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    let ctx = context(&state);
    let c = rubyforge_lang_ruby::bundler_action(&action, gem.as_deref(), &ctx)
        .map_err(ForgeError::Other)?;
    Ok(to_command_line(c, &state.project_root()?))
}

/// `gem install <gem>` with the project interpreter's gem (`debug`, `rubocop`), run by the
/// UI after the user agreed.
#[tauri::command]
pub fn ruby_gem_install_command(gem: String, state: State<'_, AppState>) -> Result<CommandLine> {
    let ctx = context(&state);
    // A system Ruby's gem folder belongs to root: install into the user's gem folder.
    let user = status(&state, false)
        .chosen
        .as_ref()
        .is_some_and(|c| matches!(c.interpreter.manager.as_str(), "system" | "homebrew"));
    let c = rubyforge_lang_ruby::gem_install(&gem, &ctx, user).map_err(ForgeError::Other)?;
    let mut cmd = to_command_line(c, &std::env::temp_dir());
    cmd.cwd = std::env::temp_dir();
    Ok(cmd)
}

/// The command line that installs Ruby LSP with the project interpreter's gem into Forge's
/// data folder (the UI asks first and starts it with `run_start`).
#[tauri::command]
pub fn ruby_lsp_install_command(state: State<'_, AppState>) -> Result<CommandLine> {
    let s = status(&state, false);
    let chosen = s
        .chosen
        .as_ref()
        .ok_or_else(|| ForgeError::Other("No Ruby interpreter found: install Ruby first".into()))?;
    let base = rubyforge_sdk::lsp::default_dir()
        .ok_or_else(|| ForgeError::Other("no user data folder for Ruby LSP".into()))?;
    let gem = s.tools.get("gem").cloned().unwrap_or_default();
    let c = rubyforge_sdk::lsp::install_command(&gem, &base, &chosen.interpreter.version);
    let mut cmd = to_command_line(c, &std::env::temp_dir());
    cmd.env = rubyforge_sdk::command_env(&s, &base_path(), cfg!(windows));
    Ok(cmd)
}

// ---- Rake ---------------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RakeTasks {
    pub tasks: Vec<RakeTask>,
    /// `rake` (rake --tasks --all) or `files` (read from the Rakefile and .rake files).
    pub source: String,
    /// Why Rake was not asked or failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

static RAKE: Mutex<Option<(PathBuf, RakeTasks)>> = Mutex::new(None);

/// Tasks found in the Rakefile, `rakelib/*.rake` and `lib/tasks/**/*.rake`.
fn static_tasks(backend: &dyn Backend) -> Vec<RakeTask> {
    let mut files: Vec<String> = ["Rakefile", "rakefile", "Rakefile.rb"]
        .iter()
        .filter(|f| has_file(backend, f))
        .map(|f| f.to_string())
        .collect();
    let req = forge_core::search::CollectRequest {
        extensions: vec!["rake".into()],
        max_files: 2000,
        max_file_size: 1 << 20,
        max_total: 32 << 20,
    };
    if let Ok(found) = backend.collect_files(&req) {
        files.extend(
            found
                .into_iter()
                .map(|f| f.path)
                .filter(|p| p.starts_with("rakelib/") || p.starts_with("lib/tasks/")),
        );
    }
    let mut out: Vec<RakeTask> = Vec::new();
    for f in files {
        if let Ok(Some(text)) = backend.read_optional(&f) {
            for t in rubyforge_lang_ruby::scan_rakefile(&f, &text) {
                if !out.iter().any(|x| x.name == t.name) {
                    out.push(t);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn load_rake_tasks(state: &AppState) -> Result<RakeTasks> {
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let fallback = |error: String| RakeTasks {
        tasks: static_tasks(backend.as_ref()),
        source: "files".into(),
        error: Some(error),
    };
    if !flag(state, "ruby.rake.runRake", true) {
        return Ok(RakeTasks {
            tasks: static_tasks(backend.as_ref()),
            source: "files".into(),
            error: None,
        });
    }
    if let Err(e) = state.require_trusted("Loading Rake tasks (rake --tasks)") {
        return Ok(fallback(e.to_string()));
    }
    let ctx = context(state);
    let list = to_command_line(rubyforge_lang_ruby::rake_tasks_command(&ctx, false), &root);
    let out = capture(backend.as_ref(), &list, "", RAKE_TIMEOUT)?;
    if out.code != Some(0) {
        let err = out
            .stderr
            .lines()
            .find(|l| l.contains("Error") || l.contains("aborted"))
            .or_else(|| out.stderr.lines().next())
            .unwrap_or("rake --tasks failed")
            .trim()
            .to_string();
        return Ok(fallback(err));
    }
    let mut tasks = rubyforge_lang_ruby::parse_tasks(&out.stdout);
    let wh = to_command_line(rubyforge_lang_ruby::rake_tasks_command(&ctx, true), &root);
    if let Ok(w) = capture(backend.as_ref(), &wh, "", RAKE_TIMEOUT) {
        rubyforge_lang_ruby::rake::merge_locations(&mut tasks, &w.stdout, &root.to_string_lossy());
    }
    Ok(RakeTasks {
        tasks,
        source: "rake".into(),
        error: None,
    })
}

/// The project's Rake tasks (Rake tool window; `refresh` asks Rake again).
#[tauri::command]
pub async fn ruby_rake_tasks(refresh: Option<bool>, app: AppHandle) -> Result<RakeTasks> {
    let a = app.clone();
    blocking(move || {
        let state = a.state::<AppState>();
        let root = state.project_root()?;
        if !refresh.unwrap_or(false) {
            if let Some((r, t)) = RAKE.lock().expect("rake tasks").as_ref() {
                if *r == root {
                    return Ok(RakeTasks {
                        tasks: t.tasks.clone(),
                        source: t.source.clone(),
                        error: t.error.clone(),
                    });
                }
            }
        }
        let t = load_rake_tasks(&state)?;
        *RAKE.lock().expect("rake tasks") = Some((
            root,
            RakeTasks {
                tasks: t.tasks.clone(),
                source: t.source.clone(),
                error: t.error.clone(),
            },
        ));
        Ok(t)
    })
    .await
}

/// The command line of a Rake task (Rake tool window, Run Anything).
#[tauri::command]
pub fn ruby_rake_command(
    task: String,
    args: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    let cfg = forge_runtools::Configuration::new(task.clone(), rubyforge_lang_ruby::RAKE)
        .with_option("task", task)
        .with_option("taskArgs", args.unwrap_or_default());
    let c = rubyforge_lang_ruby::rake_command(&cfg, &context(&state)).map_err(ForgeError::Other)?;
    Ok(to_command_line(c, &state.project_root()?))
}

// ---- Run configurations and tests ------------------------------------------------------------

/// Whether a run configuration type is RubyForge's (`ruby`, `rake`, `rspec`, `minitest`).
pub(crate) fn is_ruby_type(type_id: &str) -> bool {
    matches!(
        type_id,
        rubyforge_lang_ruby::RUBY
            | rubyforge_lang_ruby::RAKE
            | rubyforge_lang_ruby::RSPEC
            | rubyforge_lang_ruby::MINITEST
            | rubyforge_fw_rails::command::RAILS
    )
}

/// The command line of a Ruby or Rake configuration (`run.rs`).
pub(crate) fn run_command(state: &AppState, cfg: &RunConfiguration) -> Result<CommandLine> {
    let ctx = context(state);
    let rt = to_runtools(cfg);
    let c = if cfg.type_id == rubyforge_lang_ruby::RAKE {
        rubyforge_lang_ruby::rake_command(&rt, &ctx)
    } else {
        rubyforge_lang_ruby::script_command(&rt, &ctx)
    }
    .map_err(ForgeError::Other)?;
    let mut cmd = to_command_line(c, &state.project_root()?);
    cmd.env.extend(cfg.env.clone());
    Ok(cmd)
}

/// Hand the Ruby context to the RSpec / Minitest provider of a test run (`run.rs`).
pub(crate) fn extend_test_context(
    state: &AppState,
    type_id: &str,
    ctx: &mut forge_testing::CommandContext,
) {
    if type_id != rubyforge_lang_ruby::RSPEC && type_id != rubyforge_lang_ruby::MINITEST {
        return;
    }
    if let Ok(json) = serde_json::to_string(&context(state)) {
        ctx.settings
            .insert(rubyforge_lang_ruby::CONTEXT_KEY.to_string(), json);
    }
}

/// A ▶ of a Ruby script (`#!/usr/bin/env ruby`, `if __FILE__ == $0`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RubyRunTarget {
    pub line: u32,
    pub configuration: RunConfiguration,
}

/// The script ▶ of a Ruby file (tests get theirs from the test runner).
#[tauri::command]
pub fn ruby_file_targets(path: String, content: String) -> Vec<RubyRunTarget> {
    let Some(line) = rubyforge_lang_ruby::script_entry(&content) else {
        return Vec::new();
    };
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    let mut cfg = RunConfiguration {
        name,
        type_id: rubyforge_lang_ruby::RUBY.into(),
        ..RunConfiguration::default()
    };
    cfg.options.insert("script".into(), path);
    vec![RubyRunTarget {
        line,
        configuration: cfg,
    }]
}

/// The IRB console's command line: `bundle exec irb` or the interpreter's `irb` with
/// `lib` on the load path (opened in a Terminal tab by the UI). IRB reads the project's
/// `.irbrc` and Bundler loads its Gemfile: trusted projects only.
#[tauri::command]
pub fn ruby_irb_command(state: State<'_, AppState>) -> Result<CommandLine> {
    state.require_trusted("Starting the IRB console")?;
    let backend = state.backend()?;
    let has_lib = backend
        .list_dir("")
        .map(|e| {
            e.iter()
                .any(|f| f.name == "lib" && f.kind == forge_core::FileKind::Dir)
        })
        .unwrap_or(false);
    let c = rubyforge_lang_ruby::irb_command(&context(&state), has_lib);
    Ok(to_command_line(c, &state.project_root()?))
}

// ---- RuboCop ------------------------------------------------------------------------------------

/// How RuboCop runs: the setting, `bundle exec rubocop` when the bundle has it, the
/// interpreter's, PATH.
fn rubocop_command(state: &AppState, ctx: &RubyContext, args: Vec<String>) -> RubyCommand {
    let custom = setting(state, "ruby.rubocop.path");
    let mut ctx = ctx.clone();
    ctx.use_bundler = ctx.use_bundler && ctx.has_gem("rubocop");
    ctx.gem_command("rubocop", &custom, args)
}

/// RuboCop's offenses for a file: the editor `text` on stdin (else the saved file).
/// RuboCop loads the project's configuration and plugins: trusted projects only.
#[tauri::command]
pub async fn ruby_rubocop(
    path: String,
    text: Option<String>,
    app: AppHandle,
) -> Result<Vec<RubyProblem>> {
    let a = app.clone();
    blocking(move || {
        let state = a.state::<AppState>();
        state.require_trusted("Running RuboCop")?;
        let backend = state.backend()?;
        let root = backend.info().root.clone();
        let text = match text {
            Some(t) => t,
            None => backend.read_file(&path)?,
        };
        let ctx = context(&state);
        let extra = forge_process::split_args(&setting(&state, "ruby.rubocop.options"));
        let c = rubocop_command(
            &state,
            &ctx,
            rubyforge_lang_ruby::rubocop::check_args(&path, &extra),
        );
        let cmd = to_command_line(c, &root);
        let out = capture(backend.as_ref(), &cmd, &text, RUBOCOP_TIMEOUT)?;
        // 0: no offenses, 1: offenses, 2: RuboCop failed.
        if out.code == Some(2) || out.code.is_none() {
            let err = out
                .stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("RuboCop failed");
            return Err(ForgeError::Other(err.trim().to_string()));
        }
        if out.stdout.trim().is_empty() {
            return Err(ForgeError::Other(
                out.stderr
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("RuboCop printed nothing")
                    .trim()
                    .to_string(),
            ));
        }
        rubyforge_lang_ruby::parse_rubocop(&out.stdout, &root.to_string_lossy(), Some(&path))
            .map_err(ForgeError::Other)
    })
    .await
}

/// Reformat (or autocorrect) a Ruby file with RuboCop (`ruby.formatter`; `mode`: `safe` /
/// `all` overrides it). `None` when formatting is off, left to Ruby LSP, or the project is
/// not trusted.
#[tauri::command]
pub async fn ruby_format(
    path: String,
    content: String,
    mode: Option<String>,
    app: AppHandle,
) -> Result<Option<String>> {
    let a = app.clone();
    blocking(move || {
        let state = a.state::<AppState>();
        let formatter = match mode.as_deref() {
            Some("all") => "rubocopAll".to_string(),
            Some("safe") => "rubocop".to_string(),
            _ => match setting(&state, "ruby.formatter") {
                f if f.is_empty() => "rubocop".to_string(),
                f => f,
            },
        };
        if !matches!(formatter.as_str(), "rubocop" | "rubocopAll") {
            return Ok(None);
        }
        if state.safe_mode.is_on() {
            if mode.is_some() {
                state.require_trusted("Running RuboCop")?;
            }
            return Ok(None);
        }
        let backend = state.backend()?;
        let root = backend.info().root.clone();
        let ctx = context(&state);
        let extra = forge_process::split_args(&setting(&state, "ruby.rubocop.options"));
        let args =
            rubyforge_lang_ruby::rubocop::autocorrect_args(&path, formatter == "rubocop", &extra);
        let cmd = to_command_line(rubocop_command(&state, &ctx, args), &root);
        let out = capture(backend.as_ref(), &cmd, &content, RUBOCOP_TIMEOUT)?;
        match out.code {
            Some(0) | Some(1) if !out.stdout.is_empty() => Ok(Some(
                rubyforge_lang_ruby::rubocop::corrected_source(&out.stdout),
            )),
            None => Err(ForgeError::Other("RuboCop did not finish".into())),
            _ => Err(ForgeError::Other(
                out.stderr
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("RuboCop failed")
                    .trim()
                    .to_string(),
            )),
        }
    })
    .await
}

// ---- Debugging with rdbg ------------------------------------------------------------------------

/// The rdbg of the project: the setting, the interpreter's (or `bundle exec rdbg` when the
/// bundle has `debug`).
fn rdbg_program(state: &AppState, ctx: &RubyContext) -> RubyCommand {
    let custom = setting(state, "ruby.debugger.rdbgPath");
    let mut c = ctx.clone();
    c.use_bundler = c.use_bundler && c.has_gem("debug");
    c.gem_command("rdbg", &custom, Vec::new())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugPlan {
    /// The port rdbg listens on (pass it as `FORGE_RDBG_PORT` with the run).
    pub port: u16,
    /// rdbg was found (else the UI offers to install the debug gem).
    pub found: bool,
    pub rdbg: String,
}

/// A port for a debug run and whether rdbg is there (Debug on a Ruby configuration).
#[tauri::command]
pub async fn ruby_debug_plan(app: AppHandle) -> Result<DebugPlan> {
    let a = app.clone();
    blocking(move || {
        let state = a.state::<AppState>();
        let ctx = context(&state);
        let custom = setting(&state, "ruby.debugger.rdbgPath");
        let via_bundle = ctx.use_bundler && ctx.has_gem("debug");
        let found = !custom.is_empty() || via_bundle || ctx.tools.contains_key("rdbg");
        let port = forge_ports::free_port().map_err(|e| ForgeError::Other(e.to_string()))?;
        let rdbg = rdbg_program(&state, &ctx);
        Ok(DebugPlan {
            port,
            found,
            rdbg: std::iter::once(rdbg.program)
                .chain(rdbg.args)
                .collect::<Vec<_>>()
                .join(" "),
        })
    })
    .await
}

/// The rdbg port a run request asks for, removed from its environment.
pub(crate) fn take_rdbg_port(env: &mut BTreeMap<String, String>) -> Option<u16> {
    env.remove(RDBG_PORT_ENV)
        .and_then(|p| p.trim().parse().ok())
}

/// Run `cmd` under rdbg listening on `port` (debug runs of Ruby, Rake, RSpec, Minitest).
pub(crate) fn wrap_rdbg(state: &AppState, cmd: CommandLine, port: u16) -> CommandLine {
    let ctx = context(state);
    let rdbg = rdbg_program(state, &ctx);
    let inner = RubyCommand {
        program: cmd.program.clone(),
        args: cmd.args.clone(),
        cwd: String::new(),
        env: BTreeMap::new(),
    };
    let wrapped = rubyforge_lang_ruby::rdbg_command(&rdbg.program, &inner);
    let mut args = rdbg.args;
    args.extend(wrapped.args.into_iter().map(|a| {
        a.replace(
            rubyforge_lang_ruby::command::PORT_PLACEHOLDER,
            &port.to_string(),
        )
    }));
    let mut out = CommandLine::new(wrapped.program).args(args);
    out.cwd = cmd.cwd;
    out.env = cmd.env;
    // rdbg prints its own banner on stderr; keep the program's colors off its protocol.
    out.env.insert("RUBY_DEBUG_NO_COLOR".into(), "1".into());
    out
}

/// Register the `rdbg` adapter with the DAP manager while the Ruby plugin is enabled.
pub(crate) fn sync_debug(state: &AppState, debug: &DebugState) {
    if !enabled(state) {
        debug.dap.unregister_adapter(rubyforge_sdk::ADAPTER);
        return;
    }
    let rdbg = STATUS
        .lock()
        .expect("ruby status")
        .as_ref()
        .and_then(|(_, _, _, s)| s.tools.get("rdbg").cloned())
        .unwrap_or_else(|| "rdbg".into());
    debug
        .dap
        .register_adapter(Arc::new(rubyforge_sdk::RdbgAdapter::new(rdbg)));
}

// ---- Ruby LSP -------------------------------------------------------------------------------------

/// Configure the planned Ruby LSP from Settings | Ruby, or register it when only the
/// interpreter or Forge's install has one (called after the catalog servers are planned).
pub(crate) fn configure_lsp(manager: &LspManager, state: &AppState) {
    if !enabled(state) || !is_local(state) {
        return;
    }
    let existing = manager
        .configs()
        .into_iter()
        .find(|c| c.id == rubyforge_sdk::lsp::SERVER_ID);
    let uses_ruby = existing.is_some()
        || state
            .backend()
            .ok()
            .is_some_and(|b| has_file(b.as_ref(), "Gemfile") || has_file(b.as_ref(), "Rakefile"));
    if !uses_ruby {
        return;
    }
    let s = status(state, false);
    let command = lsp_path(state, &s);
    let mut config = match existing {
        Some(c) => c,
        None => match &command {
            Some(_) => LspServerConfig {
                id: rubyforge_sdk::lsp::SERVER_ID.into(),
                name: Some("Ruby LSP".into()),
                language_ids: vec!["ruby".into(), "erb".into()],
                file_extensions: vec![
                    "rb".into(),
                    "rake".into(),
                    "gemspec".into(),
                    "ru".into(),
                    "erb".into(),
                    "=Gemfile".into(),
                    "=Rakefile".into(),
                ],
                ..LspServerConfig::default()
            },
            None => return,
        },
    };
    let before = config.clone();
    if let Some(c) = &command {
        config.command = c.clone();
    }
    let mut env = rubyforge_sdk::command_env(&s, &base_path(), cfg!(windows));
    env.extend(environment(state));
    if let (Some(c), Some(base)) = (&command, rubyforge_sdk::lsp::default_dir()) {
        if rubyforge_sdk::lsp::is_managed(&base, Path::new(c)) {
            let gem_path = s
                .probe
                .as_ref()
                .map(|p| p.gem_path.clone())
                .unwrap_or_default();
            env = rubyforge_sdk::lsp::launch_env(Path::new(c), &gem_path, &env, cfg!(windows));
        }
    }
    config.env = env;
    let formatter = match setting(state, "ruby.lsp.formatter").as_str() {
        "" => "auto".to_string(),
        f => f.to_string(),
    };
    let mut init = if config.initialization_options.is_object() {
        config.initialization_options.clone()
    } else {
        serde_json::json!({})
    };
    init["formatter"] = serde_json::json!(formatter);
    config.initialization_options = init;
    if config != before {
        manager.register_server(config);
    }
}

/// Whether a changed setting reconfigures Ruby LSP.
pub fn affects_lsp(key: &str) -> bool {
    key.starts_with("ruby.lsp.")
        || matches!(
            key,
            "ruby.interpreter" | "ruby.interpreters" | "ruby.environment"
        )
}

/// [`configure_lsp`] for the open project, on a blocking thread (after an install or a
/// settings change).
pub fn spawn_sync_lsp(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        status(&state, true);
        let lsp = app.state::<crate::commands::lsp::LspState>();
        if let Ok(m) = lsp.manager(&app, &state) {
            configure_lsp(&m, &state);
        }
    });
}

/// Reconfigure Ruby LSP after Forge installed it (the UI calls this when the install's
/// run finished).
#[tauri::command]
pub fn ruby_lsp_refresh(app: AppHandle) {
    spawn_sync_lsp(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_that_restart_ruby_lsp_and_debug_ports() {
        assert!(affects_lsp("ruby.lsp.formatter"));
        assert!(affects_lsp("ruby.interpreter"));
        assert!(!affects_lsp("ruby.formatter"));
        let mut env = BTreeMap::new();
        env.insert(RDBG_PORT_ENV.to_string(), "40123".to_string());
        env.insert("A".to_string(), "b".to_string());
        assert_eq!(take_rdbg_port(&mut env), Some(40123));
        assert!(!env.contains_key(RDBG_PORT_ENV));
        assert!(is_ruby_type("rspec") && is_ruby_type("rake") && !is_ruby_type("cargo"));
    }

    #[test]
    fn script_targets() {
        let t = ruby_file_targets("bin/report".into(), "#!/usr/bin/env ruby\nputs 1\n".into());
        assert_eq!(t[0].line, 0);
        assert_eq!(t[0].configuration.options["script"], "bin/report");
        assert!(ruby_file_targets("lib/a.rb".into(), "class A; end\n".into()).is_empty());
    }
}
