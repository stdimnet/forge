//! Spelling and grammar (`forge-spell`): the "Typo" and "Grammar" inspections of the editor
//! and the commit message, their Alt+Enter entries and the dictionaries of
//! Settings | Editor | Natural Languages. Custom dictionaries (`spelling.customDictionaries`)
//! are loaded once per list of paths: absolute paths from this machine, relative ones from
//! the project through the backend.

use std::sync::{Arc, Mutex};

use forge_core::Result;
use forge_lsp::forge_codeintel::ContextAction;
use forge_spell::{ActionsRequest, Dictionary, DictionaryInfo, InspectRequest, Problem, Speller};
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Setting with the paths of custom dictionaries.
pub const CUSTOM_DICTIONARIES: &str = "spelling.customDictionaries";

#[derive(Default)]
pub struct SpellState {
    cache: Mutex<Option<Loaded>>,
}

#[derive(Clone)]
struct Loaded {
    paths: Vec<String>,
    speller: Arc<Speller>,
    infos: Vec<DictionaryInfo>,
}

fn read(state: &AppState, path: &str) -> std::result::Result<Vec<u8>, String> {
    if std::path::Path::new(path).is_absolute() {
        std::fs::read(path).map_err(|e| e.to_string())
    } else {
        let backend = state.backend().map_err(|e| e.to_string())?;
        backend.read_bytes(path).map_err(|e| e.to_string())
    }
}

fn load(state: &AppState, paths: &[String]) -> Loaded {
    let mut custom = Vec::new();
    let mut infos = Vec::new();
    for path in paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
        match read(state, path) {
            Ok(dic) => {
                let aff = forge_spell::aff_path_for(path).and_then(|a| read(state, &a).ok());
                let dict = Dictionary::from_hunspell(&name, aff.as_deref(), &dic);
                infos.push(DictionaryInfo {
                    name,
                    path: Some(path.to_string()),
                    words: dict.len(),
                    builtin: false,
                    error: None,
                });
                custom.push(Arc::new(dict));
            }
            Err(e) => infos.push(DictionaryInfo {
                name,
                path: Some(path.to_string()),
                words: 0,
                builtin: false,
                error: Some(e),
            }),
        }
    }
    let speller = Speller::new(custom);
    let mut all: Vec<DictionaryInfo> = speller
        .dictionaries()
        .iter()
        .take(2)
        .map(|d| DictionaryInfo::builtin(d))
        .collect();
    all.extend(infos);
    Loaded {
        paths: paths.to_vec(),
        speller: Arc::new(speller),
        infos: all,
    }
}

/// The speller for the current `spelling.customDictionaries`, loading them when they changed.
fn loaded(spell: &SpellState, state: &AppState) -> Loaded {
    let paths: Vec<String> = state
        .read_settings()
        .get_as(CUSTOM_DICTIONARIES)
        .unwrap_or_default();
    let mut cache = spell.cache.lock().expect("spell cache poisoned");
    if let Some(l) = cache.as_ref().filter(|l| l.paths == paths) {
        return l.clone();
    }
    let l = load(state, &paths);
    *cache = Some(l.clone());
    l
}

/// Spelling and grammar problems of the given segments of a document.
#[tauri::command]
pub async fn spell_inspect(
    request: InspectRequest,
    spell: State<'_, SpellState>,
    state: State<'_, AppState>,
) -> Result<Vec<Problem>> {
    let l = loaded(&spell, &state);
    blocking(move || Ok(forge_spell::inspect(&l.speller, &request))).await
}

/// Alt+Enter entries for the problems at the caret.
#[tauri::command]
pub async fn spell_context_actions(
    request: ActionsRequest,
    spell: State<'_, SpellState>,
    state: State<'_, AppState>,
) -> Result<Vec<ContextAction>> {
    let l = loaded(&spell, &state);
    blocking(move || Ok(forge_spell::context_actions(&l.speller, &request))).await
}

/// The bundled and custom dictionaries with their word counts (or load errors).
#[tauri::command]
pub async fn spell_dictionaries(
    spell: State<'_, SpellState>,
    state: State<'_, AppState>,
) -> Result<Vec<DictionaryInfo>> {
    Ok(loaded(&spell, &state).infos)
}
