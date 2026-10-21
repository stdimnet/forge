//! LuaForge IPC: the project's Lua index (`luaforge-lang-lua`) for Search Everywhere, the
//! Structure tool window, hover documentation, Go to Declaration and gutter run icons, plus
//! Reformat Code through StyLua. Files are read through the project's backend, so remote
//! projects work too; offsets crossing the boundary are UTF-16 (JS strings).

use std::sync::{Arc, Mutex};

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::CommandLine;
use luaforge_lang_lua::hover::Hover;
use luaforge_lang_lua::run::RunMarker;
use luaforge_lang_lua::structure::FileStructure;
use luaforge_lang_lua::{hover, require, run, structure};
use luaforge_lang_lua::{IndexStats, Location, ProjectIndex, SearchKind, SymbolHit};
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Extra `package.path` templates (`lua.requirePaths`).
pub const REQUIRE_PATHS_KEY: &str = "lua.requirePaths";
pub const STYLUA_ENABLED_KEY: &str = "lua.stylua.enabled";
pub const STYLUA_PATH_KEY: &str = "lua.stylua.path";

/// Lua files beyond this many are not indexed.
const MAX_FILES: usize = 20_000;
/// Larger files (generated code, bundled libraries) are skipped.
const MAX_FILE_SIZE: usize = 2 * 1024 * 1024;

/// The index of the open project, keyed by its root.
#[derive(Default)]
pub struct LuaState {
    index: Arc<Mutex<Option<(String, ProjectIndex)>>>,
}

fn root_key(backend: &dyn Backend) -> String {
    backend.info().root.display().to_string()
}

fn require_paths(state: &AppState) -> Vec<String> {
    state
        .read_settings()
        .get_as::<Vec<String>>(REQUIRE_PATHS_KEY)
        .unwrap_or_default()
}

/// Read every Lua file of the project into a fresh index.
fn build_index(backend: &dyn Backend, mut paths: Vec<String>) -> Result<ProjectIndex> {
    if let Ok(luarc) = backend.read_file(".luarc.json") {
        paths.extend(require::luarc_paths(&luarc));
    }
    let mut index = ProjectIndex::new(&paths);
    index.set_name(&backend.info().name);
    let files = backend.find_files(".lua", MAX_FILES)?;
    for f in files.iter().filter(|f| f.path.ends_with(".lua")) {
        match backend.read_file(&f.path) {
            Ok(text) if text.len() <= MAX_FILE_SIZE => index.set_file(&f.path, &text),
            Ok(_) => {}
            Err(e) => tracing::debug!("lua index: cannot read {}: {e}", f.path),
        }
    }
    Ok(index)
}

/// Run `f` on the index of the open project, building it first when needed and updating
/// `path` with the editor's `content`.
async fn with_index<T: Send + 'static>(
    state: &AppState,
    lua: &LuaState,
    path: Option<(String, Option<String>)>,
    f: impl FnOnce(&ProjectIndex) -> T + Send + 'static,
) -> Result<T> {
    let backend = state.backend()?;
    let paths = require_paths(state);
    let cell = lua.index.clone();
    blocking(move || {
        let key = root_key(backend.as_ref());
        let mut guard = cell
            .lock()
            .map_err(|_| ForgeError::Other("lua index poisoned".into()))?;
        if guard.as_ref().is_none_or(|(root, _)| *root != key) {
            *guard = Some((key, build_index(backend.as_ref(), paths)?));
        }
        let (_, index) = guard.as_mut().expect("index");
        if let Some((path, content)) = path {
            match content {
                Some(c) => index.update(&path, Some(&c)),
                None if index.file(&path).is_none() => {
                    if let Ok(text) = backend.read_file(&path) {
                        index.set_file(&path, &text);
                    }
                }
                None => {}
            }
        }
        Ok(f(index))
    })
    .await
}

/// (Re)index every Lua file of the project.
#[tauri::command]
pub async fn lua_index_workspace(
    state: State<'_, AppState>,
    lua: State<'_, LuaState>,
) -> Result<IndexStats> {
    let backend = state.backend()?;
    let paths = require_paths(&state);
    let cell = lua.index.clone();
    blocking(move || {
        let index = build_index(backend.as_ref(), paths)?;
        let stats = index.stats();
        *cell
            .lock()
            .map_err(|_| ForgeError::Other("lua index poisoned".into()))? =
            Some((root_key(backend.as_ref()), index));
        Ok(stats)
    })
    .await
}

/// Changed or removed files reported by the file watcher.
#[tauri::command]
pub async fn lua_files_changed(
    changed: Vec<String>,
    removed: Vec<String>,
    state: State<'_, AppState>,
    lua: State<'_, LuaState>,
) -> Result<()> {
    let backend = state.backend()?;
    let cell = lua.index.clone();
    blocking(move || {
        let mut guard = cell
            .lock()
            .map_err(|_| ForgeError::Other("lua index poisoned".into()))?;
        let Some((root, index)) = guard.as_mut() else {
            return Ok(());
        };
        if *root != root_key(backend.as_ref()) {
            return Ok(());
        }
        for path in removed.iter().filter(|p| p.ends_with(".lua")) {
            index.remove_file(path);
        }
        for path in changed.iter().filter(|p| p.ends_with(".lua")) {
            match backend.read_file(path) {
                Ok(text) if text.len() <= MAX_FILE_SIZE => index.set_file(path, &text),
                _ => index.remove_file(path),
            }
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn lua_search_symbols(
    query: String,
    kind: Option<SearchKind>,
    limit: Option<usize>,
    state: State<'_, AppState>,
    lua: State<'_, LuaState>,
) -> Result<Vec<SymbolHit>> {
    with_index(&state, &lua, None, move |index| {
        index.search(&query, kind.unwrap_or_default(), limit.unwrap_or(100))
    })
    .await
}

#[tauri::command]
pub async fn lua_file_structure(
    path: String,
    content: Option<String>,
    state: State<'_, AppState>,
    lua: State<'_, LuaState>,
) -> Result<Option<FileStructure>> {
    let p = path.clone();
    with_index(&state, &lua, Some((path, content)), move |index| {
        structure::file_structure(index, &p)
    })
    .await
}

#[tauri::command]
pub async fn lua_hover(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
    lua: State<'_, LuaState>,
) -> Result<Option<Hover>> {
    let p = path.clone();
    with_index(&state, &lua, Some((path, content)), move |index| {
        hover::hover(index, &p, offset)
    })
    .await
}

#[tauri::command]
pub async fn lua_goto(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
    lua: State<'_, LuaState>,
) -> Result<Vec<Location>> {
    let p = path.clone();
    with_index(&state, &lua, Some((path, content)), move |index| {
        index.goto(&p, offset)
    })
    .await
}

#[tauri::command]
pub async fn lua_run_markers(
    path: String,
    content: Option<String>,
    state: State<'_, AppState>,
    lua: State<'_, LuaState>,
) -> Result<Vec<RunMarker>> {
    let p = path.clone();
    with_index(&state, &lua, Some((path, content)), move |index| {
        run::run_markers(index, &p)
    })
    .await
}

/// Reformat a Lua file with StyLua (`stylua --stdin-filepath <file> -`), which finds the
/// project's `stylua.toml`. StyLua runs where the project lives (on the host for a remote
/// project). `None` when StyLua is turned off or not installed; errors carry StyLua's
/// message.
#[tauri::command]
pub async fn lua_format(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    let (enabled, exe) = {
        let s = state.read_settings();
        (
            s.get_as::<bool>(STYLUA_ENABLED_KEY).unwrap_or(true),
            s.get_as::<String>(STYLUA_PATH_KEY)
                .filter(|p| !p.trim().is_empty())
                .unwrap_or_else(|| "stylua".into()),
        )
    };
    if !enabled {
        return Ok(None);
    }
    let backend = state.backend()?;
    blocking(move || format_with_stylua(backend.as_ref(), &exe, &path, &content)).await
}

/// StyLua finishes in milliseconds; a hanging one is stopped.
const STYLUA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn format_with_stylua(
    backend: &dyn Backend,
    exe: &str,
    path: &str,
    content: &str,
) -> Result<Option<String>> {
    let file = backend.resolve(path)?;
    // A bare name is looked up like other tools (PATH, cargo, ...); a path must exist.
    let found = if exe.contains('/') || exe.contains('\\') {
        backend.find_executable(&[], Some(exe))?
    } else {
        backend.find_executable(&[exe.to_string()], None)?
    };
    let Some(program) = found else {
        return Ok(None);
    };
    let mut cmd = CommandLine::new(program.to_string_lossy())
        .arg("--stdin-filepath")
        .arg(file.to_string_lossy())
        .arg("-");
    cmd.cwd = backend.info().root.clone();
    let out = capture(backend, &cmd, content, STYLUA_TIMEOUT)?;
    match out.code {
        Some(0) => Ok(Some(out.stdout)),
        None => Err(ForgeError::Other("StyLua did not finish".into())),
        Some(_) => {
            let msg = out.stderr.trim().to_string();
            Err(ForgeError::Other(if msg.is_empty() {
                "StyLua failed".into()
            } else {
                msg
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_backend::LocalBackend;

    #[test]
    fn missing_stylua_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let out = format_with_stylua(&backend, "forge-no-such-stylua", "a.lua", "x=1").unwrap();
        assert_eq!(out, None);
        let out = format_with_stylua(&backend, "/no/such/stylua", "a.lua", "x=1").unwrap();
        assert_eq!(out, None);
    }

    #[cfg(unix)]
    #[test]
    fn formats_through_the_backend() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("stylua");
        std::fs::write(
            &fake,
            "#!/bin/sh\n[ \"$1\" = --stdin-filepath ] || exit 9\ncase \"$2\" in */a.lua) ;; *) exit 8 ;; esac\nsed 's/=/ = /'\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let exe = fake.to_string_lossy().into_owned();
        let out = format_with_stylua(&backend, &exe, "a.lua", "x=1\n").unwrap();
        assert_eq!(out.as_deref(), Some("x = 1\n"));
    }
}
