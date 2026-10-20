//! PHPForge IPC: workspace index, symbol search, structure, hover, code generation and
//! language server configuration. Offsets crossing the boundary are UTF-16 (JS strings);
//! the index works in bytes, so every command converts.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use forge_core::{ForgeError, Result};
use phpforge_lang_php::generate::{self, GenerateKind, GenerateOptions, GenerateTargets, TextEdit};
use phpforge_lang_php::hover::{self, Hover};
use phpforge_lang_php::index::{SearchKind, SymbolHit};
use phpforge_lang_php::lsp::{self, DetectedServers, LspServerConfig, PhpLspSettings};
use phpforge_lang_php::offsets::{byte_to_utf16, utf16_to_byte};
use phpforge_lang_php::parser;
use phpforge_lang_php::structure::{FileStructure, StructureNode};
use phpforge_lang_php::{IndexConfig, IndexStats, ProjectIndex};

use crate::state::AppState;

/// The PHP index of the open workspace (created lazily, persisted under `.forge/cache`).
#[derive(Default)]
pub struct PhpState {
    index: Mutex<Option<ProjectIndex>>,
}

#[derive(Clone, Serialize)]
struct IndexProgress {
    done: usize,
    total: usize,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn workspace_root(app: &AppState) -> Result<PathBuf> {
    app.with_workspace(|ws| Ok(ws.root().to_path_buf()))
}

/// Run `f` with the index for the current workspace, creating it (from cache) if needed.
fn with_index<T>(
    app: &AppState,
    php: &PhpState,
    f: impl FnOnce(&mut ProjectIndex) -> T,
) -> Result<T> {
    let root = workspace_root(app)?;
    let mut guard = php.index.lock().map_err(err)?;
    if guard.as_ref().is_none_or(|i| i.root() != root) {
        let mut idx = ProjectIndex::new(&root, IndexConfig::default());
        idx.load_cache();
        *guard = Some(idx);
    }
    Ok(f(guard.as_mut().expect("index initialised above")))
}

/// Current buffer content, or the file on disk.
fn source(app: &AppState, path: &str, content: Option<String>) -> Result<String> {
    match content {
        Some(c) => Ok(c),
        None => app.with_workspace(|ws| ws.read_file(path)),
    }
}

#[tauri::command]
pub async fn php_index_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    php: State<'_, PhpState>,
) -> Result<IndexStats> {
    let root = workspace_root(&state)?;
    // Take the index out so other commands are not blocked while indexing.
    let previous = php.index.lock().map_err(err)?.take();
    let mut idx = match previous {
        Some(i) if i.root() == root => i,
        _ => {
            let mut i = ProjectIndex::new(&root, IndexConfig::default());
            i.load_cache();
            i
        }
    };
    let emitter = app.clone();
    let (idx, stats) = tauri::async_runtime::spawn_blocking(move || {
        let stats = idx.index_workspace(|done, total| {
            let _ = emitter.emit("php://index-progress", IndexProgress { done, total });
        });
        if let Err(e) = idx.save_cache() {
            tracing::warn!("could not save PHP index cache: {e}");
        }
        (idx, stats)
    })
    .await
    .map_err(err)?;
    *php.index.lock().map_err(err)? = Some(idx);
    Ok(stats)
}

#[tauri::command]
pub fn php_search_symbols(
    query: String,
    kind: Option<SearchKind>,
    limit: Option<usize>,
    state: State<'_, AppState>,
    php: State<'_, PhpState>,
) -> Result<Vec<SymbolHit>> {
    with_index(&state, &php, |idx| {
        idx.search(&query, kind.unwrap_or_default(), limit.unwrap_or(100))
    })
}

fn convert_nodes(text: &str, nodes: &mut [StructureNode]) {
    for n in nodes {
        // Inherited members live in other files; the UI navigates to them by line.
        if n.path.is_none() {
            n.offset = byte_to_utf16(text, n.offset);
        }
        convert_nodes(text, &mut n.children);
    }
}

#[tauri::command]
pub fn php_file_structure(
    path: String,
    content: Option<String>,
    inherited: Option<bool>,
    state: State<'_, AppState>,
    php: State<'_, PhpState>,
) -> Result<FileStructure> {
    let text = source(&state, &path, content)?;
    let file = parser::index_source(&path, &text);
    let mut s = with_index(&state, &php, |idx| {
        idx.file_structure(&file, inherited.unwrap_or(false))
    })?;
    convert_nodes(&text, &mut s.nodes);
    Ok(s)
}

#[tauri::command]
pub fn php_hover(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
    php: State<'_, PhpState>,
) -> Result<Option<Hover>> {
    let text = source(&state, &path, content)?;
    let byte = utf16_to_byte(&text, offset);
    let h = with_index(&state, &php, |idx| hover::hover(idx, &path, &text, byte))?;
    Ok(h.map(|h| Hover {
        start: byte_to_utf16(&text, h.start),
        end: byte_to_utf16(&text, h.end),
        markdown: h.markdown,
    }))
}

#[tauri::command]
pub fn php_generate_targets(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
    php: State<'_, PhpState>,
) -> Result<GenerateTargets> {
    let text = source(&state, &path, content)?;
    let file = parser::index_source(&path, &text);
    let byte = utf16_to_byte(&text, offset);
    with_index(&state, &php, |idx| {
        generate::targets(&file, &text, byte, idx)
    })
}

#[tauri::command]
pub fn php_generate(
    path: String,
    offset: usize,
    kind: GenerateKind,
    options: Option<GenerateOptions>,
    content: Option<String>,
    state: State<'_, AppState>,
    php: State<'_, PhpState>,
) -> Result<Vec<TextEdit>> {
    let text = source(&state, &path, content)?;
    let file = parser::index_source(&path, &text);
    let byte = utf16_to_byte(&text, offset);
    let opts = options.unwrap_or_default();
    let edits = with_index(&state, &php, |idx| {
        generate::generate(&file, &text, byte, kind, &opts, idx)
    })?
    .map_err(err)?;
    Ok(edits
        .into_iter()
        .map(|e| TextEdit {
            start: byte_to_utf16(&text, e.start),
            end: byte_to_utf16(&text, e.end),
            new_text: e.new_text,
        })
        .collect())
}

/// Language servers installed on this machine (for Settings > PHP).
#[tauri::command]
pub fn php_detect_servers(settings: Option<PhpLspSettings>) -> DetectedServers {
    let settings = settings.unwrap_or_default();
    let path_env = std::env::var("PATH").ok();
    let dirs = lsp::search_dirs(path_env.as_deref(), dirs_home().as_deref());
    lsp::detect(&settings, &dirs, &lsp::FsProbe)
}

/// The language server to start for the open workspace, according to the PHP settings.
#[tauri::command]
pub fn php_lsp_config(
    settings: Option<PhpLspSettings>,
    state: State<'_, AppState>,
) -> Result<Option<LspServerConfig>> {
    let root = workspace_root(&state)?;
    let mut settings = settings.unwrap_or_default();
    // The licence key lives in the OS keychain only; free mode sends none.
    settings.licence_key = crate::commands::langservers::intelephense_licence_key();
    Ok(lsp::resolve_for_workspace(&settings, &root))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}
