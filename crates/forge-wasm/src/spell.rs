//! Spelling and grammar (`forge-spell`) for the browser-mode mocks of the `spell_*` IPC
//! commands. Browser mode has no custom dictionaries; the bundled English dictionary is
//! parsed on first use.

use std::sync::OnceLock;

use forge_spell::{ActionsRequest, DictionaryInfo, InspectRequest, Speller};
use serde::de::DeserializeOwned;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn speller() -> &'static Speller {
    static SPELLER: OnceLock<Speller> = OnceLock::new();
    SPELLER.get_or_init(Speller::default)
}

fn run<T: DeserializeOwned, R: serde::Serialize>(json: &str, f: impl FnOnce(T) -> R) -> String {
    match serde_json::from_str::<T>(json) {
        Ok(v) => to_json(&f(v)),
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

/// `InspectRequest` → `Problem[]`, like `spell_inspect`.
#[wasm_bindgen]
pub fn spell_inspect(request_json: &str) -> String {
    run(request_json, |r: InspectRequest| {
        forge_spell::inspect(speller(), &r)
    })
}

/// `ActionsRequest` → `ContextAction[]`, like `spell_context_actions`.
#[wasm_bindgen]
pub fn spell_context_actions(request_json: &str) -> String {
    run(request_json, |r: ActionsRequest| {
        forge_spell::context_actions(speller(), &r)
    })
}

/// The bundled dictionaries, like `spell_dictionaries` without custom ones.
#[wasm_bindgen]
pub fn spell_dictionaries() -> String {
    let infos: Vec<DictionaryInfo> = speller()
        .dictionaries()
        .iter()
        .map(|d| DictionaryInfo::builtin(d))
        .collect();
    to_json(&infos)
}
