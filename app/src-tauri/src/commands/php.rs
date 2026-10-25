//! PHPForge IPC: workspace index, symbol search, structure, hover, code generation and
//! language server configuration. The index lives with the project (in the remote agent for
//! remote projects); offsets crossing the boundary are UTF-16 (JS strings).

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use forge_core::Result;
use phpforge_lang_php::composer::{self, OutdatedPackage};
use phpforge_lang_php::generate::{GenerateKind, GenerateOptions, GenerateTargets, TextEdit};
use phpforge_lang_php::hover::Hover;
use phpforge_lang_php::index::{SearchKind, SymbolHit};
use phpforge_lang_php::lsp::{self, DetectedServers, LspServerConfig, PhpLspSettings};
use phpforge_lang_php::structure::FileStructure;
use phpforge_lang_php::style::{tidy_blank_lines, PhpCodeStyle};
use phpforge_lang_php::IndexStats;

use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

#[derive(Clone, Serialize)]
struct IndexProgress {
    done: usize,
    total: usize,
}

#[tauri::command]
pub async fn php_index_workspace(app: AppHandle, state: State<'_, AppState>) -> Result<IndexStats> {
    let backend = state.backend()?;
    // Include paths and source roots first: a changed include path indexes everything again.
    crate::commands::php_meta::configure(&state)?;
    blocking(move || {
        backend.php().index_workspace(Arc::new(move |done, total| {
            let _ = app.emit("php://index-progress", IndexProgress { done, total });
        }))
    })
    .await
}

#[tauri::command]
pub async fn php_search_symbols(
    query: String,
    kind: Option<SearchKind>,
    limit: Option<usize>,
    exact: Option<bool>,
    state: State<'_, AppState>,
    lib: State<'_, crate::commands::library::LibraryState>,
) -> Result<Vec<SymbolHit>> {
    let backend = state.backend()?;
    let mut hits = blocking(move || {
        backend.php().search(
            &query,
            kind.unwrap_or_default(),
            limit.unwrap_or(100),
            exact.unwrap_or(false),
        )
    })
    .await?;
    // Include path classes open as read-only library files.
    for h in &mut hits {
        h.path = lib.ui_path(&h.path);
    }
    Ok(hits)
}

#[tauri::command]
pub async fn php_file_structure(
    path: String,
    content: Option<String>,
    inherited: Option<bool>,
    state: State<'_, AppState>,
) -> Result<FileStructure> {
    let backend = state.backend()?;
    blocking(move || {
        backend
            .php()
            .file_structure(&path, content, inherited.unwrap_or(false))
    })
    .await
}

#[tauri::command]
pub async fn php_hover(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<Hover>> {
    let backend = state.backend()?;
    blocking(move || backend.php().hover(&path, offset, content)).await
}

#[tauri::command]
pub async fn php_generate_targets(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<GenerateTargets> {
    let backend = state.backend()?;
    blocking(move || backend.php().generate_targets(&path, offset, content)).await
}

#[tauri::command]
pub async fn php_generate(
    path: String,
    offset: usize,
    kind: GenerateKind,
    options: Option<GenerateOptions>,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<TextEdit>> {
    let mut options = options.unwrap_or_default();
    // Braces and blank lines of Settings | Editor | Code Style | PHP.
    options.style = php_code_style(&state);
    // Settings | Editor | File and Code Templates | Code.
    options.code_templates = crate::commands::templates::code_templates(&state);
    let backend = state.backend()?;
    blocking(move || {
        backend
            .php()
            .generate(&path, offset, kind, options, content)
    })
    .await
}

/// The PHP options of the selected code style scheme.
pub(crate) fn php_code_style(state: &AppState) -> PhpCodeStyle {
    let settings = crate::commands::code_style::code_style_settings(state);
    PhpCodeStyle::from_settings(|k| settings.get(k).cloned())
}

/// The PHP language level in effect and whether it comes from composer.json
/// (Settings | PHP | Composer, "Synchronize IDE settings with composer.json").
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageLevel {
    pub level: String,
    pub from_composer: bool,
    /// The level follows the CLI interpreter's version (`php.languageLevel` = `interpreter`).
    pub from_interpreter: bool,
}

#[tauri::command]
pub async fn php_language_level(state: State<'_, AppState>) -> Result<LanguageLevel> {
    let settings = lsp_settings(&state);
    let backend = state.backend().ok();
    blocking(move || {
        let composer = match &backend {
            Some(b) => b.read_optional(&settings.composer_json).ok().flatten(),
            None => None,
        };
        let (level, from_composer) = settings.effective_level(composer.as_deref());
        Ok(LanguageLevel {
            level,
            from_composer,
            from_interpreter: !from_composer && settings.level_from_interpreter,
        })
    })
    .await
}

/// "Check for package updates on project open": `composer outdated --direct` in the folder
/// of the project's composer.json, on the project's machine.
#[tauri::command]
pub async fn php_composer_outdated(state: State<'_, AppState>) -> Result<Vec<OutdatedPackage>> {
    // Composer loads the project's plugins.
    state.require_trusted("Composer")?;
    let (program, lead, json_path) = {
        let settings = state.read_settings();
        let executable = settings
            .get_as::<String>(composer::EXECUTABLE_KEY)
            .unwrap_or_default();
        let php = crate::commands::php_interpreters::local_php(&state);
        let (program, lead) = forge_process::composer_program(&executable, &php);
        let json = lsp_settings(&state).composer_json;
        (program, lead, json)
    };
    let backend = state.backend()?;
    blocking(move || {
        if backend.read_optional(&json_path)?.is_none() {
            return Ok(Vec::new());
        }
        let dir = Path::new(&json_path).parent().unwrap_or(Path::new(""));
        let mut cmd = forge_process::CommandLine::new(program).args(lead).args([
            "outdated",
            "--direct",
            "--format=json",
            "--no-interaction",
        ]);
        cmd.cwd = Path::new(&backend.info().root).join(dir);
        let mut out = String::new();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        forge_backend::process_io::run_lines(
            backend.as_ref(),
            &cmd,
            &cancel,
            &mut |line, stderr| {
                if !stderr {
                    out.push_str(&line);
                    out.push('\n');
                }
            },
        )?;
        Ok(composer::parse_outdated(&out))
    })
    .await
}

/// Reformat Code, after the language server: apply the blank line settings of the PHP code
/// style to `text` (the editor's buffer). `None` when nothing changes.
#[tauri::command]
pub async fn php_tidy_blank_lines(
    text: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    let style = php_code_style(&state);
    blocking(move || Ok(tidy_blank_lines(&text, &style))).await
}

/// Language servers installed on the project's machine (this one when no project is open),
/// for Settings > PHP.
#[tauri::command]
pub async fn php_detect_servers(
    settings: Option<PhpLspSettings>,
    state: State<'_, AppState>,
) -> Result<DetectedServers> {
    let settings = settings.unwrap_or_default();
    match state.backend() {
        Ok(backend) => {
            let remote = backend.info().remote.is_some();
            blocking(move || backend.php().detect_servers(for_host(settings, remote))).await
        }
        Err(_) => {
            let settings = forge_backend::php::with_managed_intelephense(settings);
            let path_env = std::env::var("PATH").ok();
            let dirs = lsp::search_dirs(path_env.as_deref(), dirs::home_dir().as_deref());
            Ok(lsp::detect(&settings, &dirs, &lsp::FsProbe))
        }
    }
}

/// The language server to start for the open project, according to the PHP settings
/// (the given ones, else Forge's settings store).
#[tauri::command]
pub async fn php_lsp_config(
    settings: Option<PhpLspSettings>,
    state: State<'_, AppState>,
) -> Result<Option<LspServerConfig>> {
    let backend = state.backend()?;
    let mut settings = settings.unwrap_or_else(|| lsp_settings(&state));
    // The licence key lives in the OS keychain only; free mode sends none.
    settings.licence_key = intelephense_licence_key();
    let remote = backend.info().remote.is_some();
    blocking(move || backend.php().lsp_config(for_host(settings, remote))).await
}

/// Settings for the machine the project lives on: local executable and cache paths mean
/// nothing on a remote host, where the agent finds Intelephense / phpactor itself. Locally,
/// Forge's managed Intelephense install comes before anything on `PATH`.
/// Local paths do not apply on a remote host. The backend adds the managed Intelephense of
/// the project's machine (`forge_backend::php`).
fn for_host(mut settings: PhpLspSettings, remote: bool) -> PhpLspSettings {
    if remote {
        settings.intelephense_path = None;
        settings.intelephense_node = None;
        settings.phpactor_path = None;
        settings.storage_path = None;
    }
    settings
}

/// Id under which the PHP language server (Intelephense or phpactor) is registered.
pub const PHP_SERVER_ID: &str = "php";

/// The Intelephense licence key from the OS keychain (`None` = free mode).
pub fn intelephense_licence_key() -> Option<String> {
    crate::commands::langservers::intelephense_licence_key()
}

/// The PHP language server settings from Forge's settings store.
fn lsp_settings(state: &AppState) -> PhpLspSettings {
    // Code style keys (brace placement) come from the selected scheme.
    let settings = crate::commands::code_style::code_style_settings(state);
    let mut s = PhpLspSettings::from_settings(|k| settings.get(k).cloned());
    s.licence_key = intelephense_licence_key();
    s
}

fn to_lsp_config(cfg: LspServerConfig) -> forge_lsp::LspServerConfig {
    forge_lsp::LspServerConfig {
        id: PHP_SERVER_ID.into(),
        name: Some(cfg.name),
        language_ids: cfg.language_ids,
        file_extensions: cfg.file_extensions,
        command: cfg.command.to_string_lossy().into_owned(),
        args: cfg.args,
        initialization_options: cfg.initialization_options,
        settings: cfg.settings,
    }
}

/// Register (or remove) the PHP language server for the open project: PHP projects with
/// the PHP plugin enabled get Intelephense or phpactor according to Settings > PHP, found
/// on the project's machine (a remote project's server runs next to its files and indexes
/// them there). Call it after a project was detected and when PHP settings change; an
/// unchanged config keeps the running server. Blocking (file system lookups, remote calls),
/// so call it off the main thread.
///
/// `detected` is a detection result for a root; it is ignored if another project is open
/// by now, and detection runs again when it is missing.
pub fn sync_lsp_server(app: &AppHandle, detected: Option<(&Path, &forge_project::ProjectInfo)>) {
    let state = app.state::<AppState>();
    let lsp_state = app.state::<LspState>();
    let Ok(backend) = state.backend() else {
        lsp_state.unregister_server(PHP_SERVER_ID);
        return;
    };
    let root = backend.info().root.clone();
    let fresh;
    let info = match detected {
        Some((r, _)) if r != root => return,
        Some((_, info)) => info,
        None => {
            fresh = match backend.detect_project() {
                Ok(i) => i,
                Err(e) => {
                    tracing::warn!("project detection failed: {e}");
                    return;
                }
            };
            &fresh
        }
    };
    let enabled = state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled("forge.php");
    let remote = backend.info().remote.is_some();
    let config = if enabled && info.uses("php") {
        backend
            .php()
            .lsp_config(for_host(lsp_settings(&state), remote))
            .unwrap_or_else(|e| {
                tracing::warn!("cannot look up the PHP language server: {e}");
                None
            })
    } else {
        None
    };
    match config {
        Some(cfg) => {
            tracing::info!("PHP language server: {}", cfg.command.display());
            lsp_state.register_server(to_lsp_config(cfg));
        }
        None => lsp_state.unregister_server(PHP_SERVER_ID),
    }
}

/// [`sync_lsp_server`] on a blocking thread.
pub fn spawn_sync_lsp_server(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&app, None));
}
