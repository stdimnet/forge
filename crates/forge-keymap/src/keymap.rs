//! The keymap as the IDE sees it: the PHPStorm catalog, actions contributed by plugins and
//! commands, the user's overrides from the settings, and the resolver built from all of it.
//!
//! The selected keymap and the overrides are user settings (`keymap.active`,
//! `keymap.overrides.default`, `keymap.overrides.macOS`, `keymap.overrides.bundled`,
//! `keymap.custom`, declared in `plugins/core.toml`). Overrides of one keymap are stored as a
//! list: one entry per action, the action id followed by its shortcuts, tab separated
//! (shortcuts may contain commas and spaces). An entry with only the id removes all shortcuts
//! of the action. Overrides of the derived bundled keymaps (VS Code, Eclipse, …) share one list
//! whose entries start with the keymap id.
//!
//! Keymaps form a tree like in PHPStorm: the two PHPStorm keymaps are the roots, the other
//! bundled keymaps derive from one of them, and user keymaps (`keymap.custom`, made with
//! Duplicate or Derive in Settings | Keymap) derive from any keymap and store only the actions
//! they change. Changes made to a bundled keymap apply to that keymap alone; a keymap derived
//! from it starts from its bundled shortcuts.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::{
    aliases_of, bundled_keymap, bundled_keymaps, catalog, ActionContext, ActionDef, BundledKeymap,
    KeymapId, ACTION_GROUP_ORDER,
};
use crate::engine::{
    build_bindings, find_conflicts, Binding, ConflictKind, KeyInput, Resolution, Resolver,
};
use crate::menu::{resolve_main_menu_in, ActionLookup, MainMenu, MainMenuRequest};
use crate::shortcut::{canonical_shortcut, format_shortcut_text, try_parse_shortcut, Platform};

/// Setting that selects the keymap: "auto" or a keymap id ("$default", "macOS", "VSCode",
/// a user keymap's name).
pub const KEYMAP_SETTING: &str = "keymap.active";

/// Overrides of the derived bundled keymaps: "keymap id\taction id\tshortcut…" entries.
pub const BUNDLED_OVERRIDES_SETTING: &str = "keymap.overrides.bundled";

/// User keymaps: tables with `name`, `parent` and `shortcuts` (override entries, one per line).
pub const CUSTOM_SETTING: &str = "keymap.custom";

/// Setting that holds the overrides of `km`.
pub fn overrides_setting(km: KeymapId) -> &'static str {
    match km {
        KeymapId::Default => "keymap.overrides.default",
        KeymapId::MacOs => "keymap.overrides.macOS",
    }
}

/// Every settings key the keymap reads.
pub const SETTING_KEYS: [&str; 5] = [
    KEYMAP_SETTING,
    "keymap.overrides.default",
    "keymap.overrides.macOS",
    BUNDLED_OVERRIDES_SETTING,
    CUSTOM_SETTING,
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

/// Plugin default shortcuts are written for PHPStorm's keymaps. A derived bundled keymap that
/// gives one of those strokes to another action (VS Code's Ctrl+Shift+O: File Structure) wins:
/// the plugin action loses that stroke there instead of conflicting with it.
fn shadow_plugin_defaults(
    map: &mut ShortcutMap,
    plugins: &ShortcutMap,
    changes: &[(String, Vec<String>)],
    platform: Platform,
) {
    let claimed: HashSet<String> = changes
        .iter()
        .flat_map(|(_, list)| list)
        .filter_map(|s| canonical_shortcut(s, platform))
        .collect();
    if claimed.is_empty() {
        return;
    }
    for (id, _) in plugins.entries() {
        if changes.iter().any(|(changed, _)| changed == id) {
            continue;
        }
        let Some(list) = map.get(id) else { continue };
        let kept: Vec<String> = list
            .iter()
            .filter(|s| canonical_shortcut(s, platform).is_none_or(|c| !claimed.contains(&c)))
            .cloned()
            .collect();
        if kept.len() != list.len() {
            map.insert(id, kept);
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
    let Some(list) = value.as_array() else {
        return ShortcutMap::default();
    };
    decode_entries(list.iter().filter_map(Value::as_str))
}

fn decode_entries<'a>(entries: impl Iterator<Item = &'a str>) -> ShortcutMap {
    let mut out = ShortcutMap::default();
    for entry in entries {
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

/// The overrides of the derived bundled keymaps as one settings list, sorted by keymap.
pub fn encode_bundled_overrides(maps: &BTreeMap<String, ShortcutMap>) -> Vec<String> {
    maps.iter()
        .flat_map(|(km, map)| {
            encode_overrides(map)
                .into_iter()
                .map(move |entry| format!("{km}\t{entry}"))
        })
        .collect()
}

/// Read a list written by [`encode_bundled_overrides`]; entries of unknown keymaps are kept.
pub fn decode_bundled_overrides(value: &Value) -> BTreeMap<String, ShortcutMap> {
    let mut out: BTreeMap<String, ShortcutMap> = BTreeMap::new();
    for entry in value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let Some((km, rest)) = entry.split_once('\t').filter(|(km, _)| !km.is_empty()) else {
            continue;
        };
        let one = decode_entries(std::iter::once(rest));
        let map = out.entry(km.to_string()).or_default();
        for (id, list) in one.entries() {
            map.insert(id, list.clone());
        }
    }
    out
}

/// A keymap the user made (Duplicate / Derive in Settings | Keymap).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserKeymap {
    /// Shown name, also the keymap id.
    pub name: String,
    /// Id of the keymap it derives from.
    pub parent: String,
    /// The actions whose shortcuts differ from the parent.
    pub shortcuts: ShortcutMap,
}

/// The `keymap.custom` value of `keymaps`.
pub fn encode_custom(keymaps: &[UserKeymap]) -> Value {
    Value::Array(
        keymaps
            .iter()
            .map(|k| {
                serde_json::json!({
                    "name": k.name,
                    "parent": k.parent,
                    "shortcuts": encode_overrides(&k.shortcuts).join("\n"),
                })
            })
            .collect(),
    )
}

/// Read `keymap.custom`; entries without a name are skipped.
pub fn decode_custom(value: &Value) -> Vec<UserKeymap> {
    let mut out: Vec<UserKeymap> = Vec::new();
    for entry in value.as_array().into_iter().flatten() {
        let field = |k: &str| entry.get(k).and_then(Value::as_str).unwrap_or_default();
        let name = field("name").trim();
        if name.is_empty() || out.iter().any(|k| k.name == name) {
            continue;
        }
        out.push(UserKeymap {
            name: name.to_string(),
            parent: field("parent").trim().to_string(),
            shortcuts: decode_entries(field("shortcuts").lines()),
        });
    }
    out
}

/// The keymap settings: selected keymap, per-keymap overrides and the user keymaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapSettings {
    /// "auto" (platform default) or a keymap id.
    pub active: String,
    /// Overrides of the two PHPStorm keymaps.
    pub overrides: [ShortcutMap; 2],
    /// Overrides of the derived bundled keymaps by keymap id.
    pub bundled: BTreeMap<String, ShortcutMap>,
    pub custom: Vec<UserKeymap>,
}

impl Default for KeymapSettings {
    fn default() -> Self {
        Self {
            active: "auto".into(),
            overrides: Default::default(),
            bundled: BTreeMap::new(),
            custom: Vec::new(),
        }
    }
}

/// A keymap of the tree.
#[derive(Debug, Clone, Copy)]
enum Node<'a> {
    Root(KeymapId),
    Bundled(&'static BundledKeymap),
    Custom(&'a UserKeymap),
}

impl Node<'_> {
    fn id(&self) -> String {
        match self {
            Self::Root(km) => km.as_str().to_string(),
            Self::Bundled(b) => b.id.clone(),
            Self::Custom(u) => u.name.clone(),
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Root(km) => km.title().to_string(),
            Self::Bundled(b) => b.title.clone(),
            Self::Custom(u) => u.name.clone(),
        }
    }
}

/// Deepest chain of user keymaps that is followed (cycles end there).
const MAX_DEPTH: usize = 16;

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
            bundled: get(BUNDLED_OVERRIDES_SETTING)
                .map(|v| decode_bundled_overrides(&v))
                .unwrap_or_default(),
            custom: get(CUSTOM_SETTING)
                .map(|v| decode_custom(&v))
                .unwrap_or_default(),
        }
    }

    fn node(&self, id: &str) -> Option<Node<'_>> {
        if let Some(km) = KeymapId::parse(id) {
            return Some(Node::Root(km));
        }
        if let Some(b) = bundled_keymap(id) {
            return Some(Node::Bundled(b));
        }
        self.custom.iter().find(|k| k.name == id).map(Node::Custom)
    }

    /// Id of the keymap in use: the selected one, or the platform default for "auto" and
    /// unknown ids.
    pub fn active_id(&self, host: Platform) -> String {
        match self.node(&self.active) {
            Some(n) => n.id(),
            None => KeymapId::for_platform(host).as_str().to_string(),
        }
    }

    /// The keymaps from the root down to `id` (the root alone for unknown ids). A user keymap
    /// with an unknown parent or a cycle derives from the platform default.
    fn chain(&self, id: &str, host: Platform) -> (KeymapId, Vec<Node<'_>>) {
        let mut nodes: Vec<Node<'_>> = Vec::new();
        let mut current = self.node(id);
        let root = loop {
            match current {
                Some(Node::Root(km)) => break km,
                Some(Node::Bundled(b)) => {
                    nodes.push(Node::Bundled(b));
                    break b.parent;
                }
                Some(Node::Custom(u)) if nodes.len() < MAX_DEPTH => {
                    nodes.push(Node::Custom(u));
                    current = self.node(&u.parent);
                }
                _ => break KeymapId::for_platform(host),
            }
        };
        nodes.reverse();
        (root, nodes)
    }

    /// The overrides the user made to the keymap `id` (for a user keymap: all its changes).
    pub fn changes_of(&self, id: &str) -> ShortcutMap {
        match self.node(id) {
            Some(Node::Root(km)) => self.overrides(km).clone(),
            Some(Node::Bundled(b)) => self.bundled.get(&b.id).cloned().unwrap_or_default(),
            Some(Node::Custom(u)) => u.shortcuts.clone(),
            None => ShortcutMap::default(),
        }
    }

    /// Whether `name` can name a new or renamed user keymap (`except`: its current name).
    pub fn name_problem(&self, name: &str, except: Option<&str>) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return Some("Enter a name".into());
        }
        if name.contains(['\t', '\n']) {
            return Some("The name cannot contain tabs or line breaks".into());
        }
        let reserved = name == "auto"
            || KeymapId::ALL
                .iter()
                .any(|k| k.as_str() == name || k.title() == name)
            || bundled_keymaps()
                .iter()
                .any(|b| b.id == name || b.title == name);
        let taken = self
            .custom
            .iter()
            .any(|k| k.name == name && Some(k.name.as_str()) != except);
        (reserved || taken).then(|| format!("A keymap named \"{name}\" already exists"))
    }

    /// `base` or "base 2", "base 3", … whichever is free.
    pub fn free_name(&self, base: &str) -> String {
        if self.name_problem(base, None).is_none() {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|n| self.name_problem(n, None).is_none())
            .expect("a free name")
    }

    /// Build from a JSON object of settings values (`{ "keymap.active": "macOS", … }`).
    pub fn from_json(values: &Value) -> Self {
        Self::from_values(|k| values.get(k).cloned())
    }

    /// The PHPStorm keymap the keymap in use derives from (decides the platform of its
    /// shortcuts and which plugin defaults apply).
    pub fn keymap(&self, host: Platform) -> KeymapId {
        self.chain(&self.active_id(host), host).0
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
    /// Copy the active keymap with all its shortcuts into a new user keymap and select it.
    Duplicate {
        #[serde(default)]
        name: Option<String>,
    },
    /// Make a user keymap that derives from the active keymap (it follows the parent's
    /// shortcuts and stores only its own changes) and select it.
    Derive {
        #[serde(default)]
        name: Option<String>,
    },
    /// Rename a user keymap; keymaps deriving from it follow.
    Rename { keymap: String, name: String },
    /// Delete a user keymap; keymaps deriving from it take over its parent and changes.
    Delete { keymap: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeymapInfo {
    /// "$default", "macOS", a bundled keymap id ("VSCode") or a user keymap's name.
    pub id: String,
    pub title: String,
    /// The keymap it derives from (none for the PHPStorm keymaps).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The PHPStorm keymap at the root of its tree.
    pub base: KeymapId,
    /// Whether the user made it (it can be renamed and deleted).
    #[serde(default)]
    pub user: bool,
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
    /// Id of the keymap in use.
    pub keymap: String,
    /// The PHPStorm keymap it derives from.
    pub base: KeymapId,
    /// The setting as stored ("auto" or a keymap id).
    pub selected: String,
    pub platform: Platform,
    pub keymaps: Vec<KeymapInfo>,
    pub group_order: Vec<String>,
    pub modified_count: usize,
    pub actions: Vec<ActionInfo>,
    /// The other PHPStorm keymap's default shortcuts, for the Presentation Assistant's
    /// "alternate keymap" (macOS shortcuts next to Windows/Linux ones and vice versa).
    #[serde(default)]
    pub alternate: AlternateKeymap,
}

/// Default shortcuts of the PHPStorm keymap the active one does not derive from, formatted for
/// that keymap's platform.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlternateKeymap {
    pub keymap: KeymapId,
    pub title: String,
    pub platform: Platform,
    /// Action id → display strings ("⇧⌘A"); actions without shortcuts are left out.
    pub display: BTreeMap<String, Vec<String>>,
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
                title_en: None,
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

    /// The PHPStorm keymap the active keymap derives from.
    pub fn keymap(&self) -> KeymapId {
        self.settings.keymap(self.host)
    }

    /// Id of the active keymap.
    pub fn active_id(&self) -> String {
        self.settings.active_id(self.host)
    }

    /// Every keymap: the PHPStorm ones, the other bundled ones and the user's, in that order.
    pub fn keymaps(&self) -> Vec<KeymapInfo> {
        let s = &self.settings;
        let mut nodes: Vec<Node<'_>> = KeymapId::ALL.iter().map(|&k| Node::Root(k)).collect();
        nodes.extend(bundled_keymaps().iter().map(Node::Bundled));
        nodes.extend(s.custom.iter().map(Node::Custom));
        nodes
            .into_iter()
            .map(|n| {
                let id = n.id();
                KeymapInfo {
                    title: n.title(),
                    parent: match n {
                        Node::Root(_) => None,
                        Node::Bundled(b) => Some(b.parent.as_str().to_string()),
                        Node::Custom(u) => Some(u.parent.clone()),
                    },
                    base: s.chain(&id, self.host).0,
                    user: matches!(n, Node::Custom(_)),
                    id,
                }
            })
            .collect()
    }

    /// The changes of the active keymap that the user can reset.
    fn own_changes(&self) -> ShortcutMap {
        self.settings.changes_of(&self.active_id())
    }

    pub fn platform(&self) -> Platform {
        self.keymap().platform()
    }

    /// Every known action: PHPStorm catalog, plugin actions and bare registered commands,
    /// titled in the language of the process-wide localizer.
    pub fn all_actions(&self) -> Vec<ActionDef> {
        self.all_actions_in(&forge_i18n::current())
    }

    /// [`KeymapService::all_actions`] with the titles of `l10n` (`action.<id>` keys).
    pub fn all_actions_in(&self, l10n: &forge_i18n::Localizer) -> Vec<ActionDef> {
        let mut out = self.all_actions_en();
        for a in &mut out {
            a.localize(l10n);
        }
        out
    }

    fn all_actions_en(&self) -> Vec<ActionDef> {
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
                    title_en: None,
                });
            }
        }
        out
    }

    /// Action id → shortcut strings in the active keymap, overrides applied.
    pub fn effective_shortcuts(&self) -> ShortcutMap {
        self.effective_with(&self.own_changes())
    }

    /// The active keymap's shortcuts with `own` as its own changes: the PHPStorm defaults and
    /// plugin defaults of the root, then every derived keymap's changes down to the active
    /// one.
    fn effective_with(&self, own: &ShortcutMap) -> ShortcutMap {
        let (root, nodes) = self.settings.chain(&self.active_id(), self.host);
        let mut map = ShortcutMap::default();
        map.extend_from(catalog().defaults(root));
        map.extend_from(self.registry.defaults[slot(root)].entries());
        let last = nodes.len().checked_sub(1);
        for (i, node) in nodes.iter().enumerate() {
            match node {
                Node::Bundled(b) => {
                    let plugins = &self.registry.defaults[slot(root)];
                    shadow_plugin_defaults(&mut map, plugins, &b.changes, root.platform());
                    map.extend_from(&b.changes)
                }
                Node::Custom(u) if Some(i) != last => map.extend_from(u.shortcuts.entries()),
                _ => {}
            }
        }
        map.extend_from(own.entries());
        map
    }

    pub fn shortcuts_of(&self, action_id: &str) -> Vec<String> {
        self.effective_shortcuts()
            .get(action_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn is_modified(&self, action_id: &str) -> bool {
        self.own_changes().contains(action_id)
    }

    /// Bindings of all non-native actions in the active keymap.
    pub fn bindings(&self) -> Vec<Binding> {
        let actions: HashMap<String, ActionDef> = self
            .all_actions_en()
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
        let overrides = &self.own_changes();
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
            keymap: self.active_id(),
            base: km,
            selected: self.settings.active.clone(),
            platform,
            keymaps: self.keymaps(),
            group_order: ACTION_GROUP_ORDER.iter().map(|s| s.to_string()).collect(),
            modified_count: overrides.len(),
            actions,
            alternate: self.alternate_keymap(),
        }
    }

    /// The other PHPStorm keymap's defaults (catalog and plugin defaults, no user changes).
    pub fn alternate_keymap(&self) -> AlternateKeymap {
        let other = match self.keymap() {
            KeymapId::Default => KeymapId::MacOs,
            KeymapId::MacOs => KeymapId::Default,
        };
        let platform = other.platform();
        let mut map = ShortcutMap::default();
        map.extend_from(catalog().defaults(other));
        map.extend_from(self.registry.defaults[slot(other)].entries());
        let display = map
            .entries()
            .iter()
            .filter(|(_, list)| !list.is_empty())
            .map(|(id, list)| {
                let shown = list
                    .iter()
                    .map(|s| format_shortcut_text(s, platform))
                    .collect();
                (id.clone(), shown)
            })
            .collect();
        AlternateKeymap {
            keymap: other,
            title: other.title().to_string(),
            platform,
            display,
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

    /// Settings writes that apply `change`. Only changed values are returned; invalid names
    /// and unknown keymaps change nothing.
    pub fn apply(&self, change: KeymapChange) -> Vec<SettingWrite> {
        let km = self.keymap();
        let platform = km.platform();
        let active = self.active_id();
        let mut overrides = self.own_changes();
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
            KeymapChange::Duplicate { name } => return self.duplicate(name, true),
            KeymapChange::Derive { name } => return self.duplicate(name, false),
            KeymapChange::Rename { keymap, name } => return self.rename(&keymap, &name),
            KeymapChange::Delete { keymap } => return self.delete(&keymap),
        }
        if encode_overrides(&self.own_changes()) == encode_overrides(&overrides) {
            return Vec::new();
        }
        self.write_changes(&active, overrides)
    }

    /// Store `changes` as the own changes of keymap `id`.
    fn write_changes(&self, id: &str, changes: ShortcutMap) -> Vec<SettingWrite> {
        let s = &self.settings;
        match s.node(id) {
            Some(Node::Root(km)) => vec![SettingWrite {
                key: overrides_setting(km).into(),
                value: Value::from(encode_overrides(&changes)),
            }],
            Some(Node::Bundled(b)) => {
                let mut all = s.bundled.clone();
                if changes.is_empty() {
                    all.remove(&b.id);
                } else {
                    all.insert(b.id.clone(), changes);
                }
                vec![SettingWrite {
                    key: BUNDLED_OVERRIDES_SETTING.into(),
                    value: Value::from(encode_bundled_overrides(&all)),
                }]
            }
            Some(Node::Custom(_)) => {
                let mut custom = s.custom.clone();
                if let Some(k) = custom.iter_mut().find(|k| k.name == id) {
                    k.shortcuts = changes;
                }
                vec![custom_write(&custom)]
            }
            None => Vec::new(),
        }
    }

    /// Duplicate (`copy`: same parent, all changes copied; a bundled keymap's shortcuts are
    /// kept by deriving from it) or derive (a child with no changes) the active keymap.
    fn duplicate(&self, name: Option<String>, copy: bool) -> Vec<SettingWrite> {
        let s = &self.settings;
        let active = self.active_id();
        let Some(node) = s.node(&active) else {
            return Vec::new();
        };
        let name = match name.map(|n| n.trim().to_string()) {
            Some(n) if s.name_problem(&n, None).is_none() => n,
            Some(_) => return Vec::new(),
            None => s.free_name(&format!("{} copy", node.title())),
        };
        let (parent, shortcuts) = match node {
            Node::Custom(u) if copy => (u.parent.clone(), u.shortcuts.clone()),
            Node::Custom(_) => (active.clone(), ShortcutMap::default()),
            // A bundled keymap's own user changes are copied in both cases: they apply to that
            // keymap alone, so the child would not see them otherwise.
            _ => (active.clone(), self.own_changes()),
        };
        let mut custom = s.custom.clone();
        custom.push(UserKeymap {
            name: name.clone(),
            parent,
            shortcuts,
        });
        vec![
            custom_write(&custom),
            SettingWrite {
                key: KEYMAP_SETTING.into(),
                value: Value::String(name),
            },
        ]
    }

    fn rename(&self, keymap: &str, name: &str) -> Vec<SettingWrite> {
        let s = &self.settings;
        let name = name.trim();
        if name == keymap
            || !s.custom.iter().any(|k| k.name == keymap)
            || s.name_problem(name, Some(keymap)).is_some()
        {
            return Vec::new();
        }
        let custom: Vec<UserKeymap> = s
            .custom
            .iter()
            .cloned()
            .map(|mut k| {
                if k.name == keymap {
                    k.name = name.to_string();
                }
                if k.parent == keymap {
                    k.parent = name.to_string();
                }
                k
            })
            .collect();
        let mut out = vec![custom_write(&custom)];
        if s.active == keymap {
            out.push(SettingWrite {
                key: KEYMAP_SETTING.into(),
                value: Value::String(name.to_string()),
            });
        }
        out
    }

    fn delete(&self, keymap: &str) -> Vec<SettingWrite> {
        let s = &self.settings;
        let Some(gone) = s.custom.iter().find(|k| k.name == keymap) else {
            return Vec::new();
        };
        let custom: Vec<UserKeymap> = s
            .custom
            .iter()
            .filter(|k| k.name != keymap)
            .cloned()
            .map(|mut k| {
                if k.parent == keymap {
                    // Keep its shortcuts: take over the deleted keymap's changes it did not
                    // override itself.
                    let mut merged = gone.shortcuts.clone();
                    merged.extend_from(k.shortcuts.entries());
                    k.parent = gone.parent.clone();
                    k.shortcuts = merged;
                }
                k
            })
            .collect();
        let mut out = vec![custom_write(&custom)];
        if s.active == keymap {
            out.push(SettingWrite {
                key: KEYMAP_SETTING.into(),
                value: Value::String(gone.parent.clone()),
            });
        }
        out
    }

    /// The main menu with labels, shortcuts and enabled states from this keymap.
    pub fn main_menu(&self, request: &MainMenuRequest) -> MainMenu {
        self.main_menu_in(request, &forge_i18n::current())
    }

    /// [`KeymapService::main_menu`] in the language of `l10n`.
    pub fn main_menu_in(
        &self,
        request: &MainMenuRequest,
        l10n: &forge_i18n::Localizer,
    ) -> MainMenu {
        let titles: HashMap<String, String> = self
            .all_actions_in(l10n)
            .into_iter()
            .map(|a| (a.id, a.title))
            .collect();
        let lookup = MenuLookup {
            titles,
            shortcuts: self.effective_shortcuts(),
            commands: self
                .registry
                .commands
                .iter()
                .map(|c| c.id.as_str())
                .collect(),
            platform: self.platform(),
        };
        resolve_main_menu_in(request, &lookup, l10n)
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

fn custom_write(custom: &[UserKeymap]) -> SettingWrite {
    SettingWrite {
        key: CUSTOM_SETTING.into(),
        value: encode_custom(custom),
    }
}

/// Action data the main menu is resolved against.
struct MenuLookup<'a> {
    titles: HashMap<String, String>,
    shortcuts: ShortcutMap,
    commands: HashSet<&'a str>,
    platform: Platform,
}

impl ActionLookup for MenuLookup<'_> {
    fn title(&self, id: &str) -> Option<String> {
        self.titles.get(id).cloned()
    }

    fn shortcut(&self, id: &str) -> Option<String> {
        self.shortcuts
            .get(id)
            .and_then(|list| list.first())
            .map(|s| format_shortcut_text(s, self.platform))
    }

    fn is_available(&self, id: &str) -> bool {
        self.commands.contains(id) || aliases_of(id).iter().any(|a| self.commands.contains(a))
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
            BUNDLED_OVERRIDES_SETTING: encode_bundled_overrides(&s.settings().bundled),
            CUSTOM_SETTING: encode_custom(&s.settings().custom),
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
    fn snapshot_carries_the_alternate_keymap() {
        let svc = KeymapService::new(Platform::Other);
        let alt = svc.snapshot().alternate;
        assert_eq!(alt.keymap, KeymapId::MacOs);
        assert_eq!(alt.platform, Platform::Mac);
        assert_eq!(alt.display["GotoAction"], vec!["⇧⌘A".to_string()]);

        let mac = KeymapService::new(Platform::Mac);
        let alt = mac.snapshot().alternate;
        assert_eq!(alt.keymap, KeymapId::Default);
        assert_eq!(alt.display["GotoAction"], vec!["Ctrl+Shift+A".to_string()]);
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
    fn derived_keymaps_take_plugin_default_strokes_they_bind_elsewhere() {
        let mut s = service();
        let reg: Vec<ActionRegistration> = serde_json::from_value(json!([{
            "id": "ForceOthersToFollowAction", "title": "Make Everyone Follow You",
            "group": "Plugins/Code With Me",
            "shortcuts": { "$default": ["Ctrl+Shift+O", "Ctrl+Alt+Shift+F"], "macOS": ["Ctrl+Shift+O"] }
        }]))
        .unwrap();
        s.register_actions(reg);
        assert_eq!(
            s.shortcuts_of("ForceOthersToFollowAction"),
            vec!["Ctrl+Shift+O", "Ctrl+Alt+Shift+F"]
        );
        // VS Code: Ctrl+Shift+O is File Structure; Eclipse: Optimize Imports.
        for (keymap, owner) in [
            ("VSCode", "FileStructurePopup"),
            ("Eclipse", "OptimizeImports"),
        ] {
            select(&mut s, keymap);
            assert_eq!(s.shortcuts_of(owner), vec!["Ctrl+Shift+O"], "{keymap}");
            assert_eq!(
                s.shortcuts_of("ForceOthersToFollowAction"),
                vec!["Ctrl+Alt+Shift+F"],
                "{keymap}"
            );
            let check = s.conflicts("Ctrl+Shift+O", Some(owner));
            assert!(
                check.conflicts.is_empty(),
                "{keymap}: {:?}",
                check.conflicts
            );
        }
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
        assert_eq!(snap.keymap, "$default");
        assert_eq!(snap.base, KeymapId::Default);
        assert_eq!(snap.keymaps.len(), 2 + bundled_keymaps().len());
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

    fn set(s: &mut KeymapService, id: &str, shortcuts: &[&str]) {
        change(
            s,
            KeymapChange::Set {
                action_id: id.into(),
                shortcuts: shortcuts.iter().map(|x| x.to_string()).collect(),
            },
        );
    }

    #[test]
    fn derived_bundled_keymaps_start_from_their_phpstorm_parent() {
        let mut s = service();
        select(&mut s, "VSCode");
        assert_eq!(s.active_id(), "VSCode");
        assert_eq!(s.keymap(), KeymapId::Default);
        assert_eq!(s.shortcuts_of("GotoAction"), vec!["Ctrl+Shift+P", "F1"]);
        // Unchanged actions keep the PHPStorm shortcut.
        assert_eq!(s.shortcuts_of("ShowSettings"), vec!["Ctrl+,"]);
        assert_eq!(s.shortcuts_of("SearchEverywhere"), vec!["Double Shift"]);
        select(&mut s, "VSCode OSX");
        assert_eq!(s.keymap(), KeymapId::MacOs);
        assert_eq!(label(&s, "GotoAction"), "⇧⌘P");
        let snap = s.snapshot();
        let info = snap.keymaps.iter().find(|k| k.id == "VSCode OSX").unwrap();
        assert_eq!(info.parent.as_deref(), Some("macOS"));
        assert_eq!(info.base, KeymapId::MacOs);
        assert!(!info.user);
    }

    #[test]
    fn changes_to_a_bundled_keymap_apply_to_it_alone() {
        let mut s = service();
        select(&mut s, "Eclipse");
        set(&mut s, "GotoClass", &["Ctrl+Alt+Shift+K"]);
        assert_eq!(
            s.settings().bundled["Eclipse"].get("GotoClass"),
            Some(&vec!["Ctrl+Alt+Shift+K".to_string()])
        );
        assert!(s.is_modified("GotoClass"));
        assert_eq!(s.snapshot().modified_count, 1);
        select(&mut s, "$default");
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Ctrl+N"]);
        assert!(s.settings().overrides(KeymapId::Default).is_empty());
        select(&mut s, "Eclipse");
        change(&mut s, KeymapChange::ResetAll);
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Ctrl+Shift+T"]);
        assert!(s.settings().bundled.is_empty());
    }

    #[test]
    fn round_trips_bundled_overrides_and_user_keymaps() {
        let mut maps = BTreeMap::new();
        let mut m = ShortcutMap::default();
        m.insert("GotoFile", vec!["Ctrl+P".into(), "Ctrl+K Ctrl+P".into()]);
        m.insert("Cut", vec![]);
        maps.insert("VSCode OSX".to_string(), m.clone());
        let list = encode_bundled_overrides(&maps);
        assert_eq!(
            list,
            vec![
                "VSCode OSX\tCut",
                "VSCode OSX\tGotoFile\tCtrl+P\tCtrl+K Ctrl+P"
            ]
        );
        assert_eq!(
            encode_bundled_overrides(&decode_bundled_overrides(&json!(list))),
            list
        );
        let custom = vec![UserKeymap {
            name: "Mine".into(),
            parent: "VSCode".into(),
            shortcuts: m,
        }];
        let value = encode_custom(&custom);
        assert_eq!(
            value[0]["shortcuts"],
            "Cut\nGotoFile\tCtrl+P\tCtrl+K Ctrl+P"
        );
        assert_eq!(encode_custom(&decode_custom(&value)), value);
        assert_eq!(decode_custom(&value)[0].parent, "VSCode");
        assert!(decode_custom(&json!([{ "name": " " }, "x"])).is_empty());
    }

    #[test]
    fn duplicates_a_keymap_with_its_changes_and_selects_the_copy() {
        let mut s = service();
        select(&mut s, "VSCode");
        set(&mut s, "GotoClass", &["Ctrl+Alt+Shift+K"]);
        change(&mut s, KeymapChange::Duplicate { name: None });
        assert_eq!(s.active_id(), "VS Code copy");
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Ctrl+Alt+Shift+K"]);
        assert_eq!(s.shortcuts_of("GotoFile"), vec!["Ctrl+P"]);
        let info = s
            .keymaps()
            .into_iter()
            .find(|k| k.id == "VS Code copy")
            .unwrap();
        assert_eq!(info.parent.as_deref(), Some("VSCode"));
        assert!(info.user);
        // Its own changes: the copied one.
        assert!(s.is_modified("GotoClass"));
        // A second copy gets a free name.
        change(&mut s, KeymapChange::Duplicate { name: None });
        assert_eq!(s.active_id(), "VS Code copy copy");
        select(&mut s, "VSCode");
        change(&mut s, KeymapChange::Duplicate { name: None });
        assert_eq!(s.active_id(), "VS Code copy 2");
    }

    #[test]
    fn derived_user_keymaps_follow_their_parent() {
        let mut s = service();
        change(
            &mut s,
            KeymapChange::Derive {
                name: Some("Base".into()),
            },
        );
        set(&mut s, "GotoClass", &["Ctrl+Alt+Shift+K"]);
        change(
            &mut s,
            KeymapChange::Derive {
                name: Some("Child".into()),
            },
        );
        assert_eq!(s.active_id(), "Child");
        assert!(!s.is_modified("GotoClass"));
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Ctrl+Alt+Shift+K"]);
        set(&mut s, "GotoFile", &[]);
        // The parent changes later: the child follows.
        select(&mut s, "Base");
        set(&mut s, "GotoClass", &["Ctrl+Alt+Shift+J"]);
        select(&mut s, "Child");
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Ctrl+Alt+Shift+J"]);
        assert!(s.shortcuts_of("GotoFile").is_empty());
        // Duplicating a user keymap keeps its parent.
        change(
            &mut s,
            KeymapChange::Duplicate {
                name: Some("Twin".into()),
            },
        );
        let info = s.keymaps().into_iter().find(|k| k.id == "Twin").unwrap();
        assert_eq!(info.parent.as_deref(), Some("Base"));
        assert!(s.shortcuts_of("GotoFile").is_empty());
    }

    #[test]
    fn renames_and_deletes_user_keymaps() {
        let mut s = service();
        change(
            &mut s,
            KeymapChange::Derive {
                name: Some("A".into()),
            },
        );
        set(&mut s, "GotoClass", &["Ctrl+Alt+Shift+K"]);
        change(
            &mut s,
            KeymapChange::Derive {
                name: Some("B".into()),
            },
        );
        change(
            &mut s,
            KeymapChange::Rename {
                keymap: "A".into(),
                name: "Renamed".into(),
            },
        );
        let b = s.keymaps().into_iter().find(|k| k.id == "B").unwrap();
        assert_eq!(b.parent.as_deref(), Some("Renamed"));
        // Names of bundled keymaps and taken names are refused.
        for name in ["VS Code", "Renamed", "", "auto"] {
            assert!(s
                .apply(KeymapChange::Rename {
                    keymap: "B".into(),
                    name: name.into()
                })
                .is_empty());
        }
        assert!(s
            .apply(KeymapChange::Delete {
                keymap: "VSCode".into()
            })
            .is_empty());
        // Deleting the parent keeps the child's shortcuts.
        change(
            &mut s,
            KeymapChange::Delete {
                keymap: "Renamed".into(),
            },
        );
        assert_eq!(s.active_id(), "B");
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Ctrl+Alt+Shift+K"]);
        let b = s.keymaps().into_iter().find(|k| k.id == "B").unwrap();
        assert_eq!(b.parent.as_deref(), Some("$default"));
        // Deleting the active keymap selects its parent.
        change(&mut s, KeymapChange::Delete { keymap: "B".into() });
        assert_eq!(s.active_id(), "$default");
        assert!(s.settings().custom.is_empty());
    }

    #[test]
    fn unknown_keymaps_and_cycles_fall_back_to_the_platform_default() {
        let mut s = KeymapService::new(Platform::Mac);
        let cyclic = vec![
            UserKeymap {
                name: "X".into(),
                parent: "Y".into(),
                shortcuts: ShortcutMap::default(),
            },
            UserKeymap {
                name: "Y".into(),
                parent: "X".into(),
                shortcuts: ShortcutMap::default(),
            },
        ];
        s.update_settings(KeymapSettings {
            active: "X".into(),
            custom: cyclic,
            ..Default::default()
        });
        assert_eq!(s.keymap(), KeymapId::MacOs);
        assert_eq!(s.shortcuts_of("GotoClass"), vec!["Cmd+O"]);
        s.update_settings(KeymapSettings {
            active: "Gone".into(),
            ..Default::default()
        });
        assert_eq!(s.active_id(), "macOS");
    }

    #[test]
    fn deserializes_keymap_tree_changes() {
        let c: KeymapChange = serde_json::from_value(json!({ "op": "duplicate" })).unwrap();
        assert_eq!(c, KeymapChange::Duplicate { name: None });
        let r: KeymapChange =
            serde_json::from_value(json!({ "op": "rename", "keymap": "A", "name": "B" })).unwrap();
        assert_eq!(
            r,
            KeymapChange::Rename {
                keymap: "A".into(),
                name: "B".into()
            }
        );
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
