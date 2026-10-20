//! The keymap as the IDE sees it: the PHPStorm catalog, actions contributed by plugins and
//! commands, the user's overrides from the settings, and the resolver built from all of it.
//!
//! The selected keymap and the overrides are user settings (`keymap.active`,
//! `keymap.overrides.default`, `keymap.overrides.macOS`, declared in `plugins/core.toml`).
//! Overrides of one keymap are stored as a list: one entry per action, the action id
//! followed by its shortcuts, tab separated (shortcuts may contain commas and spaces). An
//! entry with only the id removes all shortcuts of the action.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::{aliases_of, catalog, ActionContext, ActionDef, KeymapId, ACTION_GROUP_ORDER};
use crate::engine::{
    build_bindings, find_conflicts, Binding, ConflictKind, KeyInput, Resolution, Resolver,
};
use crate::shortcut::{canonical_shortcut, format_shortcut_text, try_parse_shortcut, Platform};

/// Setting that selects the keymap: "auto", "$default" or "macOS".
pub const KEYMAP_SETTING: &str = "keymap.active";

/// Setting that holds the overrides of `km`.
pub fn overrides_setting(km: KeymapId) -> &'static str {
    match km {
        KeymapId::Default => "keymap.overrides.default",
        KeymapId::MacOs => "keymap.overrides.macOS",
    }
}

/// Every settings key the keymap reads.
pub const SETTING_KEYS: [&str; 3] = [
    KEYMAP_SETTING,
    "keymap.overrides.default",
    "keymap.overrides.macOS",
];

/// Action id → shortcut strings, keeping the order in which ids were first inserted (like a
/// JavaScript object, so lookups that pick "the first binding" stay stable).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcutMap {
    entries: Vec<(String, Vec<String>)>,
    index: HashMap<String, usize>,
}

impl ShortcutMap {
    pub fn insert(&mut self, id: &str, shortcuts: Vec<String>) {
        match self.index.get(id) {
            Some(&i) => self.entries[i].1 = shortcuts,
            None => {
                self.index.insert(id.to_string(), self.entries.len());
                self.entries.push((id.to_string(), shortcuts));
            }
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let Some(i) = self.index.remove(id) else {
            return false;
        };
        self.entries.remove(i);
        for v in self.index.values_mut() {
            if *v > i {
                *v -= 1;
            }
        }
        true
    }

    pub fn get(&self, id: &str) -> Option<&Vec<String>> {
        self.index.get(id).map(|&i| &self.entries[i].1)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    pub fn entries(&self) -> &[(String, Vec<String>)] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn extend_from(&mut self, other: &[(String, Vec<String>)]) {
        for (id, list) in other {
            self.insert(id, list.clone());
        }
    }
}

/// The settings list format of one keymap's overrides, sorted by action id.
pub fn encode_overrides(map: &ShortcutMap) -> Vec<String> {
    let mut entries: Vec<&(String, Vec<String>)> = map.entries().iter().collect();
    entries.sort_by(|(a, _), (b, _)| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| b.cmp(a))
    });
    entries
        .into_iter()
        .map(|(id, shortcuts)| {
            std::iter::once(id.as_str())
                .chain(shortcuts.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect()
}

/// Read a settings list written by [`encode_overrides`]; anything else yields no overrides.
pub fn decode_overrides(value: &Value) -> ShortcutMap {
    let mut out = ShortcutMap::default();
    let Some(list) = value.as_array() else {
        return out;
    };
    for entry in list.iter().filter_map(Value::as_str) {
        let mut parts = entry.split('\t');
        let Some(id) = parts.next().filter(|id| !id.is_empty()) else {
            continue;
        };
        out.insert(
            id,
            parts
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        );
    }
    out
}

/// The keymap settings: selected keymap and per-keymap overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapSettings {
    /// "auto" (platform default), "$default" or "macOS".
    pub active: String,
    pub overrides: [ShortcutMap; 2],
}

impl Default for KeymapSettings {
    fn default() -> Self {
        Self {
            active: "auto".into(),
            overrides: Default::default(),
        }
    }
}

fn slot(km: KeymapId) -> usize {
    match km {
        KeymapId::Default => 0,
        KeymapId::MacOs => 1,
    }
}

impl KeymapSettings {
    /// Build from a settings lookup (effective values).
    pub fn from_values(get: impl Fn(&str) -> Option<Value>) -> Self {
        let active = get(KEYMAP_SETTING)
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "auto".into());
        let read = |km| {
            get(overrides_setting(km)).map_or_else(ShortcutMap::default, |v| decode_overrides(&v))
        };
        Self {
            active,
            overrides: [read(KeymapId::Default), read(KeymapId::MacOs)],
        }
    }

    /// Build from a JSON object of settings values (`{ "keymap.active": "macOS", … }`).
    pub fn from_json(values: &Value) -> Self {
        Self::from_values(|k| values.get(k).cloned())
    }

    /// The keymap in use: the selected one, or the platform default for "auto".
    pub fn keymap(&self, host: Platform) -> KeymapId {
        KeymapId::parse(&self.active).unwrap_or_else(|| KeymapId::for_platform(host))
    }

    pub fn overrides(&self, km: KeymapId) -> &ShortcutMap {
        &self.overrides[slot(km)]
    }
}

/// An action contributed by a feature or plugin (`registerKeymapActions`), with optional
/// default shortcuts per keymap.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRegistration {
    pub id: String,
    pub title: String,
    pub group: String,
    #[serde(default)]
    pub context: ActionContext,
    #[serde(default)]
    pub native: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Keymap id ("$default" / "macOS") → shortcuts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcuts: Option<BTreeMap<String, Vec<String>>>,
}

/// A registered UI command; commands without a keymap entry are listed as plugin actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// One settings value to store (`null` resets it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingWrite {
    pub key: String,
    pub value: Value,
}

/// A change of the keymap selection or the shortcuts of the active keymap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum KeymapChange {
    /// Select "auto", "$default" or "macOS".
    SelectKeymap { keymap: String },
    /// Replace all shortcuts of an action.
    Set {
        action_id: String,
        shortcuts: Vec<String>,
    },
    /// Add a shortcut (no-op when an equivalent spelling exists). With `remove_conflicts`,
    /// actions bound to the identical shortcut lose it.
    Add {
        action_id: String,
        shortcut: String,
        #[serde(default)]
        remove_conflicts: bool,
    },
    /// Remove a shortcut, matched by canonical form.
    Remove { action_id: String, shortcut: String },
    /// Back to the default shortcuts of one action.
    Reset { action_id: String },
    /// Back to the defaults of the whole active keymap.
    ResetAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeymapInfo {
    pub id: KeymapId,
    pub title: String,
}

/// An action with its shortcuts in the active keymap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionInfo {
    #[serde(flatten)]
    pub def: ActionDef,
    /// Shortcut strings as stored (defaults or overrides).
    pub shortcuts: Vec<String>,
    /// The same shortcuts formatted for the keymap's platform ("Ctrl+Shift+A" / "⇧⌘A").
    pub display: Vec<String>,
    /// Whether the user changed the shortcuts of this action.
    #[serde(default)]
    pub modified: bool,
    /// Other command ids that implement this action.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

/// Everything the UI shows about the keymap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeymapSnapshot {
    pub keymap: KeymapId,
    /// The setting as stored ("auto", "$default", "macOS").
    pub selected: String,
    pub platform: Platform,
    pub keymaps: Vec<KeymapInfo>,
    pub group_order: Vec<String>,
    pub modified_count: usize,
    pub actions: Vec<ActionInfo>,
}

impl KeymapSnapshot {
    /// Bindings of the non-native actions, for a resolver.
    pub fn bindings(&self) -> Vec<Binding> {
        let shortcuts: Vec<(String, Vec<String>)> = self
            .actions
            .iter()
            .filter(|a| !a.def.native)
            .map(|a| (a.def.id.clone(), a.shortcuts.clone()))
            .collect();
        let contexts: HashMap<&str, ActionContext> = self
            .actions
            .iter()
            .map(|a| (a.def.id.as_str(), a.def.context))
            .collect();
        build_bindings(
            &shortcuts,
            |id| contexts.get(id).copied().unwrap_or_default(),
            self.platform,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub action_id: String,
    pub title: String,
    pub group: String,
    pub kind: ConflictKind,
    /// First shortcut of the conflicting action, formatted.
    pub display: String,
}

/// Result of checking a shortcut before it is assigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCheck {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
    pub display: String,
    pub conflicts: Vec<ConflictInfo>,
}

/// Plugin actions and registered commands.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    actions: Vec<ActionDef>,
    defaults: [ShortcutMap; 2],
    commands: Vec<CommandInfo>,
}

impl Registry {
    /// Register actions contributed by features/plugins. Re-registering an id replaces it.
    pub fn register_actions(&mut self, actions: Vec<ActionRegistration>) {
        for a in actions {
            self.actions.retain(|p| p.id != a.id);
            if let Some(shortcuts) = &a.shortcuts {
                for km in KeymapId::ALL {
                    if let Some(list) = shortcuts.get(km.as_str()) {
                        self.defaults[slot(km)].insert(&a.id, list.clone());
                    }
                }
            }
            self.actions.push(ActionDef {
                id: a.id,
                title: a.title,
                group: a.group,
                context: a.context,
                native: a.native,
                description: a.description,
            });
        }
    }

    /// Replace the list of registered UI commands. Returns whether it changed.
    pub fn set_commands(&mut self, commands: Vec<CommandInfo>) -> bool {
        if self.commands == commands {
            return false;
        }
        self.commands = commands;
        true
    }
}

/// The keymap service behind the IPC commands and the WebAssembly bridge.
#[derive(Debug)]
pub struct KeymapService {
    pub registry: Registry,
    settings: KeymapSettings,
    host: Platform,
    resolver: Resolver,
    resolver_stale: bool,
}

impl KeymapService {
    pub fn new(host: Platform) -> Self {
        Self {
            registry: Registry::default(),
            settings: KeymapSettings::default(),
            host,
            resolver: Resolver::default(),
            resolver_stale: true,
        }
    }

    pub fn host(&self) -> Platform {
        self.host
    }

    pub fn set_host(&mut self, host: Platform) {
        if self.host != host {
            self.host = host;
            self.resolver_stale = true;
        }
    }

    pub fn settings(&self) -> &KeymapSettings {
        &self.settings
    }

    /// Adopt the current settings (cheap when nothing changed).
    pub fn update_settings(&mut self, settings: KeymapSettings) {
        if self.settings != settings {
            self.settings = settings;
            self.resolver_stale = true;
        }
    }

    pub fn register_actions(&mut self, actions: Vec<ActionRegistration>) {
        self.registry.register_actions(actions);
        self.resolver_stale = true;
    }

    pub fn set_commands(&mut self, commands: Vec<CommandInfo>) {
        if self.registry.set_commands(commands) {
            self.resolver_stale = true;
        }
    }

    pub fn keymap(&self) -> KeymapId {
        self.settings.keymap(self.host)
    }

    pub fn platform(&self) -> Platform {
        self.keymap().platform()
    }

    /// Every known action: PHPStorm catalog, plugin actions and bare registered commands.
    pub fn all_actions(&self) -> Vec<ActionDef> {
        let cat = catalog();
        let mut out = cat.actions.clone();
        let mut known: std::collections::HashSet<String> =
            out.iter().map(|a| a.id.clone()).collect();
        for a in &self.registry.actions {
            if known.insert(a.id.clone()) {
                out.push(a.clone());
            }
        }
        for c in &self.registry.commands {
            if known.insert(c.id.clone()) {
                out.push(ActionDef {
                    id: c.id.clone(),
                    title: c.title.clone(),
                    group: match &c.category {
                        Some(cat) if !cat.is_empty() => format!("Plugins/{cat}"),
                        _ => "Plugins".into(),
                    },
                    context: ActionContext::Any,
                    native: false,
                    description: None,
                });
            }
        }
        out
    }

    /// Action id → shortcut strings in the active keymap, overrides applied.
    pub fn effective_shortcuts(&self) -> ShortcutMap {
        let km = self.keymap();
        let mut map = ShortcutMap::default();
        map.extend_from(catalog().defaults(km));
        map.extend_from(self.registry.defaults[slot(km)].entries());
        map.extend_from(self.settings.overrides(km).entries());
        map
    }

    pub fn shortcuts_of(&self, action_id: &str) -> Vec<String> {
        self.effective_shortcuts()
            .get(action_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn is_modified(&self, action_id: &str) -> bool {
        self.settings.overrides(self.keymap()).contains(action_id)
    }

    /// Bindings of all non-native actions in the active keymap.
    pub fn bindings(&self) -> Vec<Binding> {
        let actions: HashMap<String, ActionDef> = self
            .all_actions()
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();
        let effective = self.effective_shortcuts();
        let shortcuts: Vec<(String, Vec<String>)> = effective
            .entries()
            .iter()
            .filter(|(id, _)| !actions.get(id).is_some_and(|a| a.native))
            .cloned()
            .collect();
        build_bindings(
            &shortcuts,
            |id| actions.get(id).map(|a| a.context).unwrap_or_default(),
            self.platform(),
        )
    }

    pub fn snapshot(&self) -> KeymapSnapshot {
        let km = self.keymap();
        let platform = km.platform();
        let effective = self.effective_shortcuts();
        let overrides = self.settings.overrides(km);
        let actions = self
            .all_actions()
            .into_iter()
            .map(|def| {
                let shortcuts = effective.get(&def.id).cloned().unwrap_or_default();
                let display = shortcuts
                    .iter()
                    .map(|s| format_shortcut_text(s, platform))
                    .collect();
                ActionInfo {
                    modified: overrides.contains(&def.id),
                    aliases: aliases_of(&def.id).iter().map(|s| s.to_string()).collect(),
                    def,
                    shortcuts,
                    display,
                }
            })
            .collect();
        KeymapSnapshot {
            keymap: km,
            selected: self.settings.active.clone(),
            platform,
            keymaps: KeymapId::ALL
                .iter()
                .map(|&id| KeymapInfo {
                    id,
                    title: id.title().into(),
                })
                .collect(),
            group_order: ACTION_GROUP_ORDER.iter().map(|s| s.to_string()).collect(),
            modified_count: overrides.len(),
            actions,
        }
    }

    /// Check `shortcut` for validity and clashes with other actions' shortcuts.
    pub fn conflicts(&self, shortcut: &str, action_id: Option<&str>) -> ShortcutCheck {
        let platform = self.platform();
        let Some(parsed) = try_parse_shortcut(shortcut, platform) else {
            return ShortcutCheck {
                valid: false,
                canonical: None,
                display: shortcut.to_string(),
                conflicts: Vec::new(),
            };
        };
        let actions: HashMap<String, ActionDef> = self
            .all_actions()
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();
        let effective = self.effective_shortcuts();
        let conflicts = find_conflicts(&self.bindings(), &parsed, action_id)
            .into_iter()
            .map(|c| {
                let def = actions.get(&c.action_id);
                let first = effective
                    .get(&c.action_id)
                    .and_then(|l| l.iter().find(|s| try_parse_shortcut(s, platform).is_some()))
                    .map(|s| format_shortcut_text(s, platform))
                    .unwrap_or_default();
                ConflictInfo {
                    title: def.map_or_else(|| c.action_id.clone(), |d| d.title.clone()),
                    group: def.map(|d| d.group.clone()).unwrap_or_default(),
                    action_id: c.action_id,
                    kind: c.kind,
                    display: first,
                }
            })
            .collect();
        ShortcutCheck {
            valid: true,
            canonical: Some(parsed.id()),
            display: parsed.format(platform),
            conflicts,
        }
    }

    /// Ids of actions bound to `shortcut`. A single keystroke also finds chords starting
    /// with it ("Find Actions by Shortcut").
    pub fn find_by_shortcut(&self, shortcut: &str) -> Vec<String> {
        let platform = self.platform();
        let Some(sc) = canonical_shortcut(shortcut, platform) else {
            return Vec::new();
        };
        let prefix = format!("{sc} ");
        let effective = self.effective_shortcuts();
        self.all_actions()
            .into_iter()
            .filter(|a| {
                effective.get(&a.id).is_some_and(|list| {
                    list.iter().any(|s| {
                        canonical_shortcut(s, platform)
                            .is_some_and(|c| c == sc || c.starts_with(&prefix))
                    })
                })
            })
            .map(|a| a.id)
            .collect()
    }

    /// Settings writes that apply `change`. Only changed values are returned.
    pub fn apply(&self, change: KeymapChange) -> Vec<SettingWrite> {
        let km = self.keymap();
        let platform = km.platform();
        let mut overrides = self.settings.overrides(km).clone();
        let effective = self.effective_shortcuts();
        let current = |overrides: &ShortcutMap, id: &str| -> Vec<String> {
            overrides
                .get(id)
                .or_else(|| effective.get(id))
                .cloned()
                .unwrap_or_default()
        };
        let canon_or_raw =
            |s: &str| canonical_shortcut(s, platform).unwrap_or_else(|| s.to_string());
        match change {
            KeymapChange::SelectKeymap { keymap } => {
                if keymap == self.settings.active {
                    return Vec::new();
                }
                return vec![SettingWrite {
                    key: KEYMAP_SETTING.into(),
                    value: Value::String(keymap),
                }];
            }
            KeymapChange::Set {
                action_id,
                shortcuts,
            } => overrides.insert(&action_id, shortcuts),
            KeymapChange::Add {
                action_id,
                shortcut,
                remove_conflicts,
            } => {
                let canon = canonical_shortcut(&shortcut, platform);
                if remove_conflicts {
                    if let Some(parsed) = try_parse_shortcut(&shortcut, platform) {
                        for c in find_conflicts(&self.bindings(), &parsed, Some(&action_id)) {
                            if c.kind != ConflictKind::Same {
                                continue;
                            }
                            let kept: Vec<String> = current(&overrides, &c.action_id)
                                .into_iter()
                                .filter(|s| canonical_shortcut(s, platform) != canon)
                                .collect();
                            overrides.insert(&c.action_id, kept);
                        }
                    }
                }
                let mut list = current(&overrides, &action_id);
                let exists = canon.is_some()
                    && list
                        .iter()
                        .any(|s| canonical_shortcut(s, platform) == canon);
                if !exists {
                    list.push(shortcut);
                    overrides.insert(&action_id, list);
                }
            }
            KeymapChange::Remove {
                action_id,
                shortcut,
            } => {
                let target = canon_or_raw(&shortcut);
                let kept = current(&overrides, &action_id)
                    .into_iter()
                    .filter(|s| canon_or_raw(s) != target)
                    .collect();
                overrides.insert(&action_id, kept);
            }
            KeymapChange::Reset { action_id } => {
                overrides.remove(&action_id);
            }
            KeymapChange::ResetAll => overrides = ShortcutMap::default(),
        }
        let before = encode_overrides(self.settings.overrides(km));
        let after = encode_overrides(&overrides);
        if before == after {
            return Vec::new();
        }
        vec![SettingWrite {
            key: overrides_setting(km).into(),
            value: Value::from(after),
        }]
    }

    /// Feed one key event to the resolver.
    pub fn resolve(&mut self, input: &KeyInput) -> Resolution {
        if self.resolver_stale {
            self.resolver.set_bindings(self.bindings());
            self.resolver_stale = false;
        }
        self.resolver.handle(input)
    }
}

/// Settings to write for a keymap saved by an earlier version in the browser storage:
/// `active` is the JSON-encoded keymap id, `overrides` a JSON object keymap → action →
/// shortcuts. Corrupt values are ignored.
pub fn legacy_changes(active: Option<&str>, overrides: Option<&str>) -> Vec<SettingWrite> {
    let mut out = Vec::new();
    if let Some(Value::String(id)) = active.and_then(|a| serde_json::from_str(a).ok()) {
        if KeymapId::parse(&id).is_some() {
            out.push(SettingWrite {
                key: KEYMAP_SETTING.into(),
                value: Value::String(id),
            });
        }
    }
    if let Some(Value::Object(old)) = overrides.and_then(|o| serde_json::from_str(o).ok()) {
        for km in KeymapId::ALL {
            let Some(Value::Object(map)) = old.get(km.as_str()) else {
                continue;
            };
            let mut m = ShortcutMap::default();
            for (id, list) in map {
                let shortcuts = list
                    .as_array()
                    .map(|l| {
                        l.iter()
                            .filter_map(|s| s.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                m.insert(id, shortcuts);
            }
            if !m.is_empty() {
                out.push(SettingWrite {
                    key: overrides_setting(km).into(),
                    value: Value::from(encode_overrides(&m)),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::FocusContext;
    use crate::shortcut::KeyEvent;
    use serde_json::json;

    fn service() -> KeymapService {
        let mut s = KeymapService::new(Platform::Other);
        s.update_settings(KeymapSettings {
            active: "$default".into(),
            ..Default::default()
        });
        s
    }

    /// Apply a change and adopt the written settings, like the settings backend would.
    fn change(s: &mut KeymapService, c: KeymapChange) {
        let mut values = json!({
            KEYMAP_SETTING: s.settings().active,
            "keymap.overrides.default": encode_overrides(s.settings().overrides(KeymapId::Default)),
            "keymap.overrides.macOS": encode_overrides(s.settings().overrides(KeymapId::MacOs)),
        });
        for w in s.apply(c) {
            values[w.key.as_str()] = w.value;
        }
        s.update_settings(KeymapSettings::from_json(&values));
    }

    fn select(s: &mut KeymapService, km: &str) {
        change(s, KeymapChange::SelectKeymap { keymap: km.into() });
    }

    fn label(s: &KeymapService, id: &str) -> String {
        s.snapshot()
            .actions
            .into_iter()
            .find(|a| a.def.id == id)
            .and_then(|a| a.display.into_iter().next())
            .unwrap_or_default()
    }

    #[test]
    fn round_trips_overrides_including_removed_shortcuts_and_commas() {
        let mut map = ShortcutMap::default();
        map.insert("GotoClass", vec!["Ctrl+N".into(), "Ctrl+K Ctrl+C".into()]);
        map.insert("ShowSettings", vec!["Meta+,".into()]);
        map.insert("Cut", vec![]);
        let list = encode_overrides(&map);
        assert_eq!(
            list,
            vec![
                "Cut",
                "GotoClass\tCtrl+N\tCtrl+K Ctrl+C",
                "ShowSettings\tMeta+,"
            ]
        );
        let decoded = decode_overrides(&json!(list));
        assert_eq!(decoded.get("GotoClass"), map.get("GotoClass"));
        assert_eq!(decoded.get("Cut"), Some(&vec![]));
        assert_eq!(decoded.len(), 3);
        assert!(decode_overrides(&json!("nope")).is_empty());
    }

    #[test]
    fn migrates_the_old_local_storage_keymap() {
        let changes = legacy_changes(
            Some("\"macOS\""),
            Some(r#"{"macOS":{"GotoClass":["Cmd+O"]},"$default":{}}"#),
        );
        assert_eq!(
            changes,
            vec![
                SettingWrite {
                    key: "keymap.active".into(),
                    value: json!("macOS")
                },
                SettingWrite {
                    key: "keymap.overrides.macOS".into(),
                    value: json!(["GotoClass\tCmd+O"])
                },
            ]
        );
        assert!(legacy_changes(Some("garbage"), Some("{")).is_empty());
    }

    #[test]
    fn serves_phpstorm_defaults_per_keymap() {
        let mut s = service();
        assert_eq!(s.shortcuts_of("GotoAction"), vec!["Ctrl+Shift+A"]);
        assert_eq!(label(&s, "GotoAction"), "Ctrl+Shift+A");
        select(&mut s, "macOS");
        assert_eq!(s.keymap(), KeymapId::MacOs);
        assert_eq!(label(&s, "GotoAction"), "⇧⌘A");
    }

    #[test]
    fn auto_follows_the_host_platform() {
        let mut s = KeymapService::new(Platform::Mac);
        assert_eq!(s.keymap(), KeymapId::MacOs);
        s.set_host(Platform::Other);
        assert_eq!(s.keymap(), KeymapId::Default);
        select(&mut s, "macOS");
        assert_eq!(s.keymap(), KeymapId::MacOs);
    }

    #[test]
    fn layers_overrides_on_top_and_persists_them() {
        let mut s = service();
        let writes = s.apply(KeymapChange::Add {
            action_id: "GotoClass".into(),
            shortcut: "Ctrl+Alt+Shift+K".into(),
            remove_conflicts: false,
        });
        assert_eq!(
            writes,
            vec![SettingWrite {
                key: "keymap.overrides.default".into(),
                value: json!(["GotoClass\tCtrl+N\tCtrl+Alt+Shift+K"])
            }]
        );
        change(
            &mut s,
            KeymapChange::Add {
                action_id: "GotoClass".into(),
                shortcut: "Ctrl+Alt+Shift+K".into(),
                remove_conflicts: false,
            },
        );
        assert_eq!(
            s.shortcuts_of("GotoClass"),
            vec!["Ctrl+N", "Ctrl+Alt+Shift+K"]
        );
        assert!(s.is_modified("GotoClass"));
        assert_eq!(s.snapshot().modified_count, 1);
        // Overrides are per keymap.
        select(&mut s, "macOS");
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Cmd+O"]);
    }

    #[test]
    fn does_not_add_the_same_shortcut_twice_in_another_spelling() {
        let s = service();
        let writes = s.apply(KeymapChange::Add {
            action_id: "GotoClass".into(),
            shortcut: "ctrl+n".into(),
            remove_conflicts: false,
        });
        assert!(writes.is_empty());
    }

    #[test]
    fn removes_shortcuts_by_canonical_form_and_resets() {
        let mut s = service();
        change(
            &mut s,
            KeymapChange::Remove {
                action_id: "GotoClass".into(),
                shortcut: "ctrl+n".into(),
            },
        );
        assert!(s.shortcuts_of("GotoClass").is_empty());
        change(
            &mut s,
            KeymapChange::Reset {
                action_id: "GotoClass".into(),
            },
        );
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Ctrl+N"]);
        assert!(!s.is_modified("GotoClass"));
    }

    #[test]
    fn reset_all_clears_the_active_keymap_only() {
        let mut s = service();
        change(
            &mut s,
            KeymapChange::Set {
                action_id: "GotoClass".into(),
                shortcuts: vec![],
            },
        );
        let writes = s.apply(KeymapChange::ResetAll);
        assert_eq!(
            writes,
            vec![SettingWrite {
                key: "keymap.overrides.default".into(),
                value: json!([])
            }]
        );
    }

    #[test]
    fn add_can_remove_the_shortcut_from_conflicting_actions() {
        let mut s = service();
        change(
            &mut s,
            KeymapChange::Add {
                action_id: "GotoFile".into(),
                shortcut: "ctrl+n".into(),
                remove_conflicts: true,
            },
        );
        assert!(s.shortcuts_of("GotoClass").is_empty());
        assert_eq!(s.shortcuts_of("GotoFile"), vec!["Ctrl+Shift+N", "ctrl+n"]);
    }

    #[test]
    fn feeds_overrides_into_the_bindings() {
        let mut s = service();
        let has = |s: &KeymapService, id: &str, sc: &str| {
            s.bindings()
                .iter()
                .any(|b| b.action_id == id && b.shortcut.id() == sc)
        };
        assert!(has(&s, "GotoClass", "Ctrl+N"));
        change(
            &mut s,
            KeymapChange::Remove {
                action_id: "GotoClass".into(),
                shortcut: "Ctrl+N".into(),
            },
        );
        assert!(!has(&s, "GotoClass", "Ctrl+N"));
    }

    #[test]
    fn leaves_native_actions_out_of_the_bindings() {
        assert!(!service().bindings().iter().any(|b| b.action_id == "$Copy"));
    }

    #[test]
    fn accepts_plugin_actions_with_default_shortcuts() {
        let mut s = service();
        let reg: Vec<ActionRegistration> = serde_json::from_value(json!([{
            "id": "php.RunComposer", "title": "Run Composer", "group": "Plugins/PHP",
            "shortcuts": { "$default": ["Ctrl+Alt+Shift+R"], "macOS": ["Cmd+Alt+Shift+R"] }
        }]))
        .unwrap();
        s.register_actions(reg);
        let a = s
            .all_actions()
            .into_iter()
            .find(|a| a.id == "php.RunComposer")
            .unwrap();
        assert_eq!(a.context, ActionContext::Any);
        assert_eq!(s.shortcuts_of("php.RunComposer"), vec!["Ctrl+Alt+Shift+R"]);
        select(&mut s, "macOS");
        assert_eq!(s.shortcuts_of("php.RunComposer"), vec!["Cmd+Alt+Shift+R"]);
    }

    #[test]
    fn lists_registered_commands_that_have_no_keymap_entry() {
        let mut s = service();
        s.set_commands(vec![
            CommandInfo {
                id: "demo.hello".into(),
                title: "Say Hello".into(),
                category: Some("Demo".into()),
            },
            CommandInfo {
                id: "GotoClass".into(),
                title: "Class".into(),
                category: None,
            },
        ]);
        let all = s.all_actions();
        let hello = all.iter().find(|a| a.id == "demo.hello").unwrap();
        assert_eq!(hello.group, "Plugins/Demo");
        assert_eq!(all.iter().filter(|a| a.id == "GotoClass").count(), 1);
    }

    #[test]
    fn snapshot_carries_aliases_display_and_modified_state() {
        let snap = service().snapshot();
        assert_eq!(snap.keymap, KeymapId::Default);
        assert_eq!(snap.keymaps.len(), 2);
        let run = snap.actions.iter().find(|a| a.def.id == "Run").unwrap();
        assert_eq!(run.aliases, vec!["run.run"]);
        assert_eq!(run.display, vec!["Shift+F10"]);
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["keymap"], "$default");
        assert_eq!(json["platform"], "other");
        assert_eq!(json["actions"][0]["id"], "SearchEverywhere");
        assert_eq!(json["actions"][0]["display"][0], "Double Shift");
        let back: KeymapSnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(back, snap);
    }

    #[test]
    fn checks_shortcuts_for_conflicts() {
        let s = service();
        let check = s.conflicts("ctrl+n", Some("GotoFile"));
        assert!(check.valid);
        assert_eq!(check.canonical.as_deref(), Some("Ctrl+N"));
        assert_eq!(check.display, "Ctrl+N");
        assert_eq!(check.conflicts.len(), 1);
        assert_eq!(check.conflicts[0].action_id, "GotoClass");
        assert_eq!(check.conflicts[0].kind, ConflictKind::Same);
        assert_eq!(check.conflicts[0].display, "Ctrl+N");
        assert!(!s.conflicts("Ctrl+Nope", None).valid);
    }

    #[test]
    fn finds_actions_by_shortcut_including_chord_prefixes() {
        let mut s = service();
        change(
            &mut s,
            KeymapChange::Set {
                action_id: "CommentByLineComment".into(),
                shortcuts: vec!["Ctrl+Alt+Shift+K Ctrl+C".into()],
            },
        );
        assert!(s
            .find_by_shortcut("Ctrl+Shift+A")
            .contains(&"GotoAction".to_string()));
        assert_eq!(
            s.find_by_shortcut("ctrl+alt+shift+k"),
            vec!["CommentByLineComment"]
        );
        assert!(s.find_by_shortcut("garbage!").is_empty());
    }

    #[test]
    fn resolves_with_the_current_overrides() {
        let mut s = service();
        let down = |code: &str, key: &str| KeyInput {
            event: KeyEvent {
                code: code.into(),
                key: key.into(),
                ctrl_key: true,
                ..Default::default()
            },
            focus: FocusContext::Global,
            ..Default::default()
        };
        assert_eq!(
            s.resolve(&down("KeyN", "n")).action.as_deref(),
            Some("GotoClass")
        );
        change(
            &mut s,
            KeymapChange::Set {
                action_id: "GotoClass".into(),
                shortcuts: vec!["Ctrl+Alt+Shift+K".into()],
            },
        );
        assert_eq!(s.resolve(&down("KeyN", "n")).action, None);
    }

    #[test]
    fn deserializes_changes_from_the_ui() {
        let c: KeymapChange = serde_json::from_value(
            json!({ "op": "add", "actionId": "GotoClass", "shortcut": "Ctrl+M", "removeConflicts": true }),
        )
        .unwrap();
        assert_eq!(
            c,
            KeymapChange::Add {
                action_id: "GotoClass".into(),
                shortcut: "Ctrl+M".into(),
                remove_conflicts: true
            }
        );
        let r: KeymapChange = serde_json::from_value(json!({ "op": "resetAll" })).unwrap();
        assert_eq!(r, KeymapChange::ResetAll);
    }
}
