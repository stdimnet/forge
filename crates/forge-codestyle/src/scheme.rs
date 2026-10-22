//! Code style schemes and where they are stored, as in PHPStorm:
//!
//! - **Default** (IDE level): the user settings, under the code style keys every language
//!   page always used (`php.codeStyle.indent`, `editor.codeStyle.hardWrapAt`, …).
//! - **Project**: the same keys in the project settings (`.forge/settings.toml`). Options it
//!   does not set come from the Default scheme.
//! - More IDE schemes (Duplicate, Copy to IDE, Import): user settings under
//!   `editor.codeStyle.schemes.<id>.name` and `editor.codeStyle.schemes.<id>.values.<language>.<option>`.
//!
//! The project picks its scheme with `editor.codeStyle.scheme` (`default`, `project` or an
//! IDE scheme id); without it, a project with code style settings uses its Project scheme.
//! Only values that differ from the scheme's base are stored.
//!
//! Older versions had a single PHP brace option (`php.codeStyle.braces`: `psr12`, `allman`,
//! `kr`); it is read as the three brace options and removed on the next save.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::{model, sanitize, Values, GENERAL};

/// The selected scheme of the project (or of the IDE without a project).
pub const SCHEME_KEY: &str = "editor.codeStyle.scheme";
/// Prefix of the IDE schemes besides Default.
pub const SCHEMES_PREFIX: &str = "editor.codeStyle.schemes.";
/// The single PHP brace option of older versions.
pub const LEGACY_BRACES_KEY: &str = "php.codeStyle.braces";

pub const DEFAULT_ID: &str = "default";
pub const PROJECT_ID: &str = "project";

/// A settings layer: flat dotted keys (`forge_core::Settings::layer_values`).
pub type Layer = BTreeMap<String, Value>;

/// The settings layers code style schemes are stored in.
#[derive(Debug, Clone, Copy)]
pub struct Layers<'a> {
    pub user: &'a Layer,
    /// `None` without an open project.
    pub project: Option<&'a Layer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SchemeLevel {
    Ide,
    Project,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scheme {
    pub id: String,
    pub name: String,
    pub level: SchemeLevel,
    /// Every option's value (`<language>.<option>`).
    pub values: Values,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemesState {
    pub schemes: Vec<Scheme>,
    pub selected: String,
    pub has_project: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriteScope {
    User,
    Project,
}

/// One change to the settings; `value: None` removes the key from its layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingWrite {
    pub key: String,
    pub value: Option<Value>,
    pub scope: WriteScope,
}

/// Settings key of an option in the Default and Project schemes.
pub fn settings_key(full_key: &str) -> Option<String> {
    let (lang, key) = full_key.split_once('.')?;
    Some(if lang == GENERAL {
        format!("editor.codeStyle.{key}")
    } else {
        format!("{lang}.codeStyle.{key}")
    })
}

/// Settings keys of every option of the Default and Project schemes, by option key.
fn settings_keys() -> &'static BTreeMap<String, String> {
    static KEYS: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        model()
            .defaults()
            .into_keys()
            .filter_map(|k| settings_key(&k).map(|s| (k, s)))
            .collect()
    })
}

/// Whether `key` is a code style option of the Default/Project scheme (or the legacy key).
pub fn is_scheme_setting(key: &str) -> bool {
    key == LEGACY_BRACES_KEY || settings_keys().values().any(|k| k == key)
}

fn custom_key(id: &str, rest: &str) -> String {
    format!("{SCHEMES_PREFIX}{id}.{rest}")
}

/// Values stored in a layer, read through `key_of` (option key → settings key).
fn explicit(layer: &Layer, key_of: impl Fn(&str) -> String) -> Values {
    let m = model();
    let mut out = Values::new();
    for full in settings_keys().keys() {
        if let Some(v) = layer.get(&key_of(full)) {
            if let Some((_, spec)) = m.spec(full) {
                if let Some(v) = sanitize(spec, v) {
                    out.insert(full.clone(), v);
                }
            }
        }
    }
    out
}

/// The three brace options of the single legacy one.
pub fn legacy_braces(value: &str) -> Option<[(&'static str, &'static str); 3]> {
    let (class, function, other) = match value {
        "psr12" => ("nextLine", "nextLine", "endOfLine"),
        "allman" => ("nextLine", "nextLine", "nextLine"),
        "kr" => ("endOfLine", "endOfLine", "endOfLine"),
        _ => return None,
    };
    Some([
        ("php.wrapping.classBraces", class),
        ("php.wrapping.functionBraces", function),
        ("php.wrapping.otherBraces", other),
    ])
}

/// Values of the Default or Project scheme stored in `layer`, with the legacy brace option
/// migrated.
fn layer_values(layer: &Layer) -> Values {
    let mut out = explicit(layer, |k| settings_key(k).unwrap_or_default());
    if let Some(braces) = layer
        .get(LEGACY_BRACES_KEY)
        .and_then(Value::as_str)
        .and_then(legacy_braces)
    {
        for (k, v) in braces {
            out.entry(k.to_string()).or_insert_with(|| json!(v));
        }
    }
    out
}

fn with_base(base: &Values, explicit: Values) -> Values {
    let mut out = base.clone();
    out.extend(explicit);
    out
}

/// Ids of the IDE schemes besides Default, in key order.
fn custom_ids(user: &Layer) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for key in user.keys() {
        if let Some(rest) = key.strip_prefix(SCHEMES_PREFIX) {
            if let Some((id, _)) = rest.split_once('.') {
                if valid_id(id) && !ids.iter().any(|i| i == id) {
                    ids.push(id.to_string());
                }
            }
        }
    }
    ids
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The schemes and the selection, as the Code Style pages show them.
pub fn load(layers: Layers<'_>) -> SchemesState {
    let defaults = model().defaults();
    let default_values = with_base(&defaults, layer_values(layers.user));
    let mut schemes = vec![Scheme {
        id: DEFAULT_ID.into(),
        name: "Default".into(),
        level: SchemeLevel::Ide,
        values: default_values.clone(),
    }];
    for id in custom_ids(layers.user) {
        let prefix = custom_key(&id, "values.");
        let values = explicit(layers.user, |k| format!("{prefix}{k}"));
        let name = layers
            .user
            .get(&custom_key(&id, "name"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| id.clone());
        schemes.push(Scheme {
            id,
            name,
            level: SchemeLevel::Ide,
            values: with_base(&defaults, values),
        });
    }
    if let Some(project) = layers.project {
        schemes.push(Scheme {
            id: PROJECT_ID.into(),
            name: "Project".into(),
            level: SchemeLevel::Project,
            values: with_base(&default_values, layer_values(project)),
        });
    }
    let selected_layer = layers.project.unwrap_or(layers.user);
    let implicit = if layers.project.is_some_and(|p| !layer_values(p).is_empty()) {
        PROJECT_ID
    } else {
        DEFAULT_ID
    };
    let selected = selected_layer
        .get(SCHEME_KEY)
        .and_then(Value::as_str)
        .filter(|id| schemes.iter().any(|s| s.id == *id))
        .unwrap_or(implicit)
        .to_string();
    SchemesState {
        schemes,
        selected,
        has_project: layers.project.is_some(),
    }
}

/// The values of the selected scheme: what formatting and the editor use.
pub fn selected_values(layers: Layers<'_>) -> Values {
    let state = load(layers);
    state
        .schemes
        .into_iter()
        .find(|s| s.id == state.selected)
        .map(|s| s.values)
        .unwrap_or_else(|| model().defaults())
}

/// The effective settings with the code style keys of the Default/Project scheme replaced by
/// the selected scheme's values, for readers of `<language>.codeStyle.*` keys
/// (`forge_text::resolve_format`, the PHP code style).
pub fn effective_settings(
    effective: &serde_json::Map<String, Value>,
    layers: Layers<'_>,
) -> serde_json::Map<String, Value> {
    let mut out: serde_json::Map<String, Value> = effective
        .iter()
        .filter(|(k, _)| !is_scheme_setting(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (k, v) in selected_values(layers) {
        if let Some(key) = settings_key(&k) {
            out.insert(key, v);
        }
    }
    out
}

/// Values that differ from `base`.
fn sparse(values: &Values, base: &Values) -> Values {
    values
        .iter()
        .filter(|(k, v)| base.get(*k) != Some(*v) && settings_keys().contains_key(*k))
        .filter_map(|(k, v)| {
            let (_, spec) = model().spec(k)?;
            sanitize(spec, v).map(|v| (k.clone(), v))
        })
        .collect()
}

struct Plan {
    writes: Vec<SettingWrite>,
}

impl Plan {
    fn set(&mut self, layer: &Layer, scope: WriteScope, key: String, value: Option<Value>) {
        if layer.get(&key) != value.as_ref() {
            self.writes.push(SettingWrite { key, value, scope });
        }
    }
}

/// The settings changes that store `state` (as edited on the Code Style pages).
pub fn plan_save(layers: Layers<'_>, state: &SchemesState) -> Result<Vec<SettingWrite>, String> {
    let defaults = model().defaults();
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for s in &state.schemes {
        if !ids.insert(s.id.as_str()) {
            return Err(format!("duplicate scheme id `{}`", s.id));
        }
        let level = if s.level == SchemeLevel::Project {
            "p"
        } else {
            "i"
        };
        if !names.insert((level, s.name.trim().to_lowercase())) {
            return Err(format!(
                "A scheme named \"{}\" already exists",
                s.name.trim()
            ));
        }
        if s.name.trim().is_empty() {
            return Err("A scheme needs a name".into());
        }
        if s.id != DEFAULT_ID && s.id != PROJECT_ID && !valid_id(&s.id) {
            return Err(format!("invalid scheme id `{}`", s.id));
        }
    }
    let find = |id: &str| state.schemes.iter().find(|s| s.id == id);
    let default = find(DEFAULT_ID).ok_or("the Default scheme is missing")?;
    if !state.schemes.iter().any(|s| s.id == state.selected) {
        return Err(format!("unknown scheme `{}`", state.selected));
    }

    let mut plan = Plan { writes: Vec::new() };
    // Default: the user layer under the language keys.
    let default_sparse = sparse(&default.values, &defaults);
    for (full, key) in settings_keys() {
        plan.set(
            layers.user,
            WriteScope::User,
            key.clone(),
            default_sparse.get(full).cloned(),
        );
    }
    plan.set(
        layers.user,
        WriteScope::User,
        LEGACY_BRACES_KEY.into(),
        None,
    );

    // Project: the project layer, relative to the (new) Default scheme.
    let mut project_has_values = false;
    if let Some(project) = layers.project {
        if let Some(p) = find(PROJECT_ID) {
            let project_sparse = sparse(&p.values, &default.values);
            project_has_values = !project_sparse.is_empty();
            for (full, key) in settings_keys() {
                plan.set(
                    project,
                    WriteScope::Project,
                    key.clone(),
                    project_sparse.get(full).cloned(),
                );
            }
            plan.set(project, WriteScope::Project, LEGACY_BRACES_KEY.into(), None);
        }
    }

    // Other IDE schemes.
    let mut wanted: BTreeMap<String, Value> = BTreeMap::new();
    for s in state
        .schemes
        .iter()
        .filter(|s| s.id != DEFAULT_ID && s.id != PROJECT_ID)
    {
        wanted.insert(custom_key(&s.id, "name"), json!(s.name.trim()));
        for (k, v) in sparse(&s.values, &defaults) {
            wanted.insert(custom_key(&s.id, &format!("values.{k}")), v);
        }
    }
    for key in layers.user.keys().filter(|k| k.starts_with(SCHEMES_PREFIX)) {
        if !wanted.contains_key(key) {
            plan.set(layers.user, WriteScope::User, key.clone(), None);
        }
    }
    for (key, value) in wanted {
        plan.set(layers.user, WriteScope::User, key, Some(value));
    }

    // The selection.
    let (sel_layer, sel_scope) = match layers.project {
        Some(p) => (p, WriteScope::Project),
        None => (layers.user, WriteScope::User),
    };
    let implicit = if project_has_values {
        PROJECT_ID
    } else {
        DEFAULT_ID
    };
    let stored = sel_layer.get(SCHEME_KEY).and_then(Value::as_str);
    if stored != Some(state.selected.as_str()) && !(stored.is_none() && state.selected == implicit)
    {
        plan.set(
            sel_layer,
            sel_scope,
            SCHEME_KEY.into(),
            Some(json!(state.selected)),
        );
    }
    Ok(plan.writes)
}

/// Apply a preset (`Set from…`) to the values of one language: the language's defaults,
/// then the preset's values.
pub fn apply_preset(values: &Values, language: &str, preset: &str) -> Option<Values> {
    let lang = model().language(language)?;
    let p = lang.preset(preset)?;
    let mut out = values.clone();
    out.extend(lang.defaults());
    for (k, v) in &p.values {
        out.insert(format!("{language}.{k}"), v.clone());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(v: Value) -> Layer {
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    #[test]
    fn reads_the_existing_keys_and_migrates_the_brace_option() {
        let user = layer(json!({
            "php.codeStyle.indent": 2,
            "php.codeStyle.braces": "kr",
            "editor.codeStyle.hardWrapAt": 100,
            "editor.fontSize": 14,
        }));
        let project =
            layer(json!({ "php.codeStyle.useTabs": true, "php.codeStyle.braces": "allman" }));
        let state = load(Layers {
            user: &user,
            project: Some(&project),
        });
        assert_eq!(state.selected, PROJECT_ID);
        let default = &state.schemes[0];
        assert_eq!(default.values["php.indent"], json!(2));
        assert_eq!(
            default.values["php.wrapping.functionBraces"],
            json!("endOfLine")
        );
        assert_eq!(default.values["general.hardWrapAt"], json!(100));
        assert_eq!(default.values["php.tabSize"], json!(4));
        let project = state.schemes.last().unwrap();
        assert_eq!(project.level, SchemeLevel::Project);
        // The project inherits from Default and overrides.
        assert_eq!(project.values["php.indent"], json!(2));
        assert_eq!(project.values["php.useTabs"], json!(true));
        assert_eq!(
            project.values["php.wrapping.otherBraces"],
            json!("nextLine")
        );

        // Saving the migrated state removes the legacy key and stores the new ones.
        let writes = plan_save(
            Layers {
                user: &user,
                project: Some(&project_layer()),
            },
            &state,
        )
        .unwrap();
        let find = |k: &str, s: WriteScope| writes.iter().find(|w| w.key == k && w.scope == s);
        assert_eq!(
            find(LEGACY_BRACES_KEY, WriteScope::User).unwrap().value,
            None
        );
        assert_eq!(
            find("php.codeStyle.wrapping.classBraces", WriteScope::User)
                .unwrap()
                .value,
            Some(json!("endOfLine"))
        );
        // Unchanged keys are not written again.
        assert!(find("php.codeStyle.indent", WriteScope::User).is_none());
        assert!(find("editor.fontSize", WriteScope::User).is_none());
    }

    fn project_layer() -> Layer {
        layer(json!({ "php.codeStyle.useTabs": true, "php.codeStyle.braces": "allman" }))
    }

    #[test]
    fn without_project_settings_the_default_scheme_is_used() {
        let user = layer(json!({ "php.codeStyle.indent": 3 }));
        let project = Layer::new();
        let state = load(Layers {
            user: &user,
            project: Some(&project),
        });
        assert_eq!(state.selected, DEFAULT_ID);
        assert_eq!(state.schemes.len(), 2);
        let values = selected_values(Layers {
            user: &user,
            project: Some(&project),
        });
        assert_eq!(values["php.indent"], json!(3));
        let none = load(Layers {
            user: &user,
            project: None,
        });
        assert!(!none.has_project);
        assert_eq!(none.schemes.len(), 1);
    }

    #[test]
    fn stores_ide_schemes_and_the_selection() {
        let user = Layer::new();
        let project = Layer::new();
        let layers = Layers {
            user: &user,
            project: Some(&project),
        };
        let mut state = load(layers);
        let mut copy = state.schemes[0].clone();
        copy.id = "wp".into();
        copy.name = "WordPress team".into();
        copy.values = apply_preset(&copy.values, "php", "wordpress").unwrap();
        state.schemes.push(copy);
        state.selected = "wp".into();
        let writes = plan_save(layers, &state).unwrap();
        let get = |k: &str| {
            writes
                .iter()
                .find(|w| w.key == k)
                .and_then(|w| w.value.clone())
        };
        assert_eq!(
            get("editor.codeStyle.schemes.wp.name"),
            Some(json!("WordPress team"))
        );
        assert_eq!(
            get("editor.codeStyle.schemes.wp.values.php.useTabs"),
            Some(json!(true))
        );
        assert_eq!(get(SCHEME_KEY), Some(json!("wp")));
        assert!(writes
            .iter()
            .all(|w| w.key == SCHEME_KEY || w.scope == WriteScope::User));

        // Read back what was written.
        let mut user = Layer::new();
        let mut project = Layer::new();
        for w in writes {
            let l = if w.scope == WriteScope::User {
                &mut user
            } else {
                &mut project
            };
            if let Some(v) = w.value {
                l.insert(w.key, v);
            }
        }
        let layers = Layers {
            user: &user,
            project: Some(&project),
        };
        let back = load(layers);
        assert_eq!(back.selected, "wp");
        assert_eq!(back.schemes[1].name, "WordPress team");
        assert_eq!(
            selected_values(layers)["php.codeConversion.arraySyntax"],
            json!("long")
        );

        // Removing the scheme deletes its keys.
        let mut state = back.clone();
        state.schemes.retain(|s| s.id != "wp");
        state.selected = DEFAULT_ID.into();
        let writes = plan_save(layers, &state).unwrap();
        assert!(writes
            .iter()
            .filter(|w| w.key.starts_with(SCHEMES_PREFIX))
            .all(|w| w.value.is_none()));
        assert!(writes
            .iter()
            .any(|w| w.key == "editor.codeStyle.schemes.wp.name"));
    }

    #[test]
    fn rejects_duplicate_names() {
        let user = Layer::new();
        let layers = Layers {
            user: &user,
            project: None,
        };
        let mut state = load(layers);
        let mut copy = state.schemes[0].clone();
        copy.id = "x".into();
        copy.name = " default ".into();
        state.schemes.push(copy);
        assert!(plan_save(layers, &state)
            .unwrap_err()
            .contains("already exists"));
    }

    #[test]
    fn effective_settings_carry_the_selected_scheme() {
        let user = layer(json!({ "php.codeStyle.indent": 2 }));
        let project = layer(json!({ SCHEME_KEY: "default", "php.codeStyle.indent": 8 }));
        let effective: serde_json::Map<String, Value> = json!({
            "php.codeStyle.indent": 8, "php.codeStyle.braces": "kr", "editor.codeStyle.useEditorConfig": true,
        })
        .as_object()
        .unwrap()
        .clone();
        let out = effective_settings(
            &effective,
            Layers {
                user: &user,
                project: Some(&project),
            },
        );
        assert_eq!(out["php.codeStyle.indent"], json!(2));
        assert_eq!(out["editor.codeStyle.useEditorConfig"], json!(true));
        assert!(!out.contains_key(LEGACY_BRACES_KEY));
        assert_eq!(out["json.codeStyle.indent"], json!(2));
        assert_eq!(out["php.codeStyle.wrapping.classBraces"], json!("nextLine"));
    }
}
