//! User schemes in the settings, scheme resolution, the page snapshot and its changes.
//!
//! Settings (declared in `plugins/core.toml`):
//! - `editor.colorScheme.active`: the scheme in use; empty follows the theme (Ember for dark
//!   themes, Anvil for light ones, and the theme's own token colors).
//! - `editor.colorScheme.schemes`: user schemes, tables with `name`, `parent` ("Ember" or
//!   "Anvil") and `attributes` (one changed attribute per line: the key, then its
//!   [`TextAttributes::encode`] text).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{attribute, AttributeSpec, Bundled, CssStyle, Effect, Fields, TextAttributes, ATTRIBUTES};
use crate::preview::{preview, PreviewLine};

pub const ACTIVE_SETTING: &str = "editor.colorScheme.active";
pub const SCHEMES_SETTING: &str = "editor.colorScheme.schemes";
pub const SETTING_KEYS: [&str; 2] = [ACTIVE_SETTING, SCHEMES_SETTING];

/// One settings value to store (`null` resets it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingWrite {
    pub key: String,
    pub value: Value,
}

/// A scheme the user made (Duplicate or Import), storing only the attributes it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserScheme {
    pub name: String,
    pub parent: Bundled,
    pub attributes: BTreeMap<String, TextAttributes>,
}

impl UserScheme {
    fn encode_attributes(&self) -> String {
        ATTRIBUTES
            .iter()
            .filter_map(|a| {
                let v = self.attributes.get(a.key)?;
                Some(format!("{} {}", a.key, v.encode()).trim_end().to_string())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The `editor.colorScheme.schemes` value.
pub fn encode_schemes(schemes: &[UserScheme]) -> Value {
    Value::Array(
        schemes
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "parent": s.parent.name(),
                    "attributes": s.encode_attributes(),
                })
            })
            .collect(),
    )
}

/// Read `editor.colorScheme.schemes`; unknown attributes and nameless entries are skipped.
pub fn decode_schemes(value: &Value) -> Vec<UserScheme> {
    let mut out: Vec<UserScheme> = Vec::new();
    for entry in value.as_array().into_iter().flatten() {
        let field = |k: &str| entry.get(k).and_then(Value::as_str).unwrap_or_default();
        let name = field("name").trim();
        if name.is_empty() || out.iter().any(|s| s.name == name) {
            continue;
        }
        let mut attributes = BTreeMap::new();
        for line in field("attributes").lines() {
            let (key, rest) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
            if attribute(key).is_some() {
                attributes.insert(key.to_string(), TextAttributes::decode(rest));
            }
        }
        out.push(UserScheme {
            name: name.to_string(),
            parent: Bundled::parse(field("parent")).unwrap_or(Bundled::Ember),
            attributes,
        });
    }
    out
}

/// The color scheme settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemeSettings {
    /// Empty: follow the theme.
    pub active: String,
    pub schemes: Vec<UserScheme>,
}

impl SchemeSettings {
    pub fn from_values(get: impl Fn(&str) -> Option<Value>) -> Self {
        Self {
            active: get(ACTIVE_SETTING)
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            schemes: get(SCHEMES_SETTING)
                .map(|v| decode_schemes(&v))
                .unwrap_or_default(),
        }
    }

    pub fn from_json(values: &Value) -> Self {
        Self::from_values(|k| values.get(k).cloned())
    }

    pub fn user(&self, name: &str) -> Option<&UserScheme> {
        self.schemes.iter().find(|s| s.name == name)
    }

    /// Whether the active scheme follows the theme (no explicit or an unknown scheme).
    pub fn follows_theme(&self) -> bool {
        Bundled::parse(&self.active).is_none() && self.user(&self.active).is_none()
    }

    /// Name of the scheme in use; `dark` is whether the UI theme is a dark one.
    pub fn active_name(&self, dark: bool) -> String {
        if self.follows_theme() {
            return if dark { Bundled::Ember } else { Bundled::Anvil }
                .name()
                .to_string();
        }
        self.active.clone()
    }

    /// The bundled scheme `name` is (or derives from).
    pub fn base_of(&self, name: &str) -> Bundled {
        Bundled::parse(name)
            .or_else(|| self.user(name).map(|s| s.parent))
            .unwrap_or(Bundled::Ember)
    }

    /// An attribute of scheme `name` and whether it comes from the bundled parent.
    pub fn attribute_of(&self, name: &str, key: &str) -> (TextAttributes, bool) {
        if let Some(v) = self.user(name).and_then(|s| s.attributes.get(key)) {
            return (v.clone(), false);
        }
        (self.base_of(name).attribute(key), true)
    }

    /// Whether `name` can name a new or renamed scheme (`except`: its current name).
    pub fn name_problem(&self, name: &str, except: Option<&str>) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return Some("Enter a name".into());
        }
        if name.contains(['\t', '\n']) {
            return Some("The name cannot contain tabs or line breaks".into());
        }
        let taken = Bundled::parse(name).is_some()
            || self
                .schemes
                .iter()
                .any(|s| s.name == name && Some(s.name.as_str()) != except);
        taken.then(|| format!("A scheme named \"{name}\" already exists"))
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

    /// CSS variables that make the editor show scheme `name`.
    pub fn css(&self, name: &str) -> Vec<CssVar> {
        let text_fg = self
            .attribute_of(name, "TEXT")
            .0
            .foreground
            .unwrap_or_else(|| "inherit".into());
        ATTRIBUTES
            .iter()
            .flat_map(|a| self.attribute_of(name, a.key).0.css_vars(a.target, &text_fg))
            .map(|(name, value)| CssVar { name, value })
            .collect()
    }

    fn write_schemes(schemes: &[UserScheme]) -> SettingWrite {
        SettingWrite {
            key: SCHEMES_SETTING.into(),
            value: encode_schemes(schemes),
        }
    }

    fn select(name: &str) -> SettingWrite {
        SettingWrite {
            key: ACTIVE_SETTING.into(),
            value: Value::String(name.to_string()),
        }
    }

    /// Settings writes that apply `change` (`dark`: the UI theme, for "follow the theme").
    /// Invalid names and changes of bundled schemes write nothing.
    pub fn apply(&self, change: SchemeChange, dark: bool) -> Vec<SettingWrite> {
        let active = self.active_name(dark);
        match change {
            SchemeChange::Select { scheme } => {
                if scheme == self.active {
                    return Vec::new();
                }
                vec![Self::select(&scheme)]
            }
            SchemeChange::Duplicate { name } => {
                let name = match name.map(|n| n.trim().to_string()) {
                    Some(n) if self.name_problem(&n, None).is_none() => n,
                    Some(_) => return Vec::new(),
                    None => self.free_name(&format!("{active} copy")),
                };
                let source = self.user(&active);
                let mut schemes = self.schemes.clone();
                schemes.push(UserScheme {
                    name: name.clone(),
                    parent: self.base_of(&active),
                    attributes: source.map(|s| s.attributes.clone()).unwrap_or_default(),
                });
                vec![Self::write_schemes(&schemes), Self::select(&name)]
            }
            SchemeChange::Rename { scheme, name } => {
                let name = name.trim();
                if name == scheme
                    || self.user(&scheme).is_none()
                    || self.name_problem(name, Some(&scheme)).is_some()
                {
                    return Vec::new();
                }
                let schemes: Vec<UserScheme> = self
                    .schemes
                    .iter()
                    .cloned()
                    .map(|mut s| {
                        if s.name == scheme {
                            s.name = name.to_string();
                        }
                        s
                    })
                    .collect();
                let mut out = vec![Self::write_schemes(&schemes)];
                if self.active == scheme {
                    out.push(Self::select(name));
                }
                out
            }
            SchemeChange::Delete { scheme } => {
                let Some(gone) = self.user(&scheme) else {
                    return Vec::new();
                };
                let schemes: Vec<UserScheme> = self
                    .schemes
                    .iter()
                    .filter(|s| s.name != scheme)
                    .cloned()
                    .collect();
                let mut out = vec![Self::write_schemes(&schemes)];
                if self.active == scheme {
                    out.push(Self::select(gone.parent.name()));
                }
                out
            }
            SchemeChange::SetAttribute { key, value } => {
                if attribute(&key).is_none() || self.user(&active).is_none() {
                    return Vec::new();
                }
                let parent = self.base_of(&active).attribute(&key);
                let schemes: Vec<UserScheme> = self
                    .schemes
                    .iter()
                    .cloned()
                    .map(|mut s| {
                        if s.name == active {
                            match value.clone().map(TextAttributes::normalized) {
                                // The parent's value is no change.
                                Some(v) if v != parent => {
                                    s.attributes.insert(key.clone(), v);
                                }
                                _ => {
                                    s.attributes.remove(&key);
                                }
                            }
                        }
                        s
                    })
                    .collect();
                if schemes == self.schemes {
                    return Vec::new();
                }
                vec![Self::write_schemes(&schemes)]
            }
            SchemeChange::ResetAll => {
                let Some(user) = self.user(&active) else {
                    return Vec::new();
                };
                if user.attributes.is_empty() {
                    return Vec::new();
                }
                let schemes: Vec<UserScheme> = self
                    .schemes
                    .iter()
                    .cloned()
                    .map(|mut s| {
                        if s.name == active {
                            s.attributes.clear();
                        }
                        s
                    })
                    .collect();
                vec![Self::write_schemes(&schemes)]
            }
        }
    }

    /// Writes that add an imported scheme (renamed when the name is taken) and select it.
    pub fn add_imported(&self, mut scheme: UserScheme) -> Vec<SettingWrite> {
        scheme.name = self.free_name(scheme.name.trim());
        let name = scheme.name.clone();
        let mut schemes = self.schemes.clone();
        schemes.push(scheme);
        vec![Self::write_schemes(&schemes), Self::select(&name)]
    }

    /// Everything Settings | Editor | Color Scheme shows.
    pub fn snapshot(&self, dark: bool) -> SchemesSnapshot {
        let active = self.active_name(dark);
        let base = self.base_of(&active);
        let text = self.attribute_of(&active, "TEXT").0;
        let text_fg = text.foreground.clone().unwrap_or_default();
        let mut schemes: Vec<SchemeInfo> = Bundled::ALL
            .iter()
            .map(|b| SchemeInfo {
                name: b.name().into(),
                parent: None,
                bundled: true,
            })
            .collect();
        schemes.extend(self.schemes.iter().map(|s| SchemeInfo {
            name: s.name.clone(),
            parent: Some(s.parent.name().into()),
            bundled: false,
        }));
        let attributes = ATTRIBUTES
            .iter()
            .map(|spec| {
                let (value, inherited) = self.attribute_of(&active, spec.key);
                AttributeView::new(spec, value, inherited, base.attribute(spec.key), &text_fg)
            })
            .collect();
        let follows_theme = self.follows_theme();
        SchemesSnapshot {
            selected: self.active.clone(),
            follows_theme,
            editable: self.user(&active).is_some(),
            modified_count: self.user(&active).map_or(0, |s| s.attributes.len()),
            active: active.clone(),
            base: base.name().into(),
            schemes,
            attributes,
            effects: Effect::ALL
                .iter()
                .map(|e| EffectInfo {
                    id: e.as_str().into(),
                    title: e.title().into(),
                })
                .collect(),
            preview: preview(),
            css: if follows_theme {
                Vec::new()
            } else {
                self.css(&active)
            },
        }
    }
}

/// A change made on the Color Scheme page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SchemeChange {
    /// Select a scheme; "" follows the theme.
    Select { scheme: String },
    /// Copy the active scheme into a new user scheme and select it.
    Duplicate {
        #[serde(default)]
        name: Option<String>,
    },
    Rename { scheme: String, name: String },
    /// Delete a user scheme; when it was active its parent is selected.
    Delete { scheme: String },
    /// Change one attribute of the active user scheme; `None` resets it to the parent's.
    SetAttribute {
        key: String,
        #[serde(default)]
        value: Option<TextAttributes>,
    },
    /// Reset every attribute of the active user scheme.
    ResetAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemeInfo {
    pub name: String,
    /// The bundled scheme a user scheme derives from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub bundled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeView {
    pub key: String,
    pub title: String,
    pub group: String,
    pub fields: Fields,
    pub value: TextAttributes,
    /// Whether the value is the parent scheme's.
    pub inherited: bool,
    /// The parent scheme's value (shown by "Reset").
    pub parent_value: TextAttributes,
    /// Inline CSS of the value, for the preview and the list.
    pub style: CssStyle,
}

impl AttributeView {
    fn new(
        spec: &AttributeSpec,
        value: TextAttributes,
        inherited: bool,
        parent_value: TextAttributes,
        text_fg: &str,
    ) -> Self {
        Self {
            key: spec.key.into(),
            title: spec.title.into(),
            group: spec.group.into(),
            fields: spec.fields,
            style: value.style(text_fg),
            value,
            inherited,
            parent_value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectInfo {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CssVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemesSnapshot {
    /// The setting as stored ("" follows the theme).
    pub selected: String,
    pub follows_theme: bool,
    /// The scheme shown and edited.
    pub active: String,
    /// The bundled scheme it is or derives from.
    pub base: String,
    /// Only user schemes can be changed (bundled ones are duplicated first).
    pub editable: bool,
    pub modified_count: usize,
    pub schemes: Vec<SchemeInfo>,
    pub attributes: Vec<AttributeView>,
    pub effects: Vec<EffectInfo>,
    pub preview: Vec<PreviewLine>,
    /// CSS variables the UI sets on the document; empty while following the theme (the
    /// theme's own variables apply).
    pub css: Vec<CssVar>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Apply a change and adopt the writes, like the settings backend.
    fn change(s: &mut SchemeSettings, c: SchemeChange) {
        let mut values = json!({ ACTIVE_SETTING: s.active, SCHEMES_SETTING: encode_schemes(&s.schemes) });
        for w in s.apply(c, true) {
            values[w.key.as_str()] = w.value;
        }
        *s = SchemeSettings::from_json(&values);
    }

    fn red() -> TextAttributes {
        TextAttributes {
            foreground: Some("#FF0000".into()),
            bold: true,
            ..Default::default()
        }
    }

    #[test]
    fn follows_the_theme_by_default() {
        let s = SchemeSettings::default();
        assert!(s.follows_theme());
        assert_eq!(s.active_name(true), "Ember");
        assert_eq!(s.active_name(false), "Anvil");
        let snap = s.snapshot(false);
        assert!(snap.css.is_empty());
        assert_eq!(snap.active, "Anvil");
        assert!(!snap.editable);
        assert_eq!(snap.schemes.len(), 2);
    }

    #[test]
    fn an_explicit_bundled_scheme_sets_every_variable() {
        let s = SchemeSettings {
            active: "Ember".into(),
            ..Default::default()
        };
        let css = s.snapshot(false).css;
        let get = |n: &str| css.iter().find(|v| v.name == n).map(|v| v.value.as_str());
        assert_eq!(get("--syn-keyword"), Some("#ff9a57"));
        assert_eq!(get("--syn-comment-style"), Some("italic"));
        assert_eq!(get("--syn-keyword-weight"), Some("normal"));
        assert_eq!(get("--bg-editor"), Some("#141417"));
        assert_eq!(get("--editor-caret"), Some("#ff7a1a"));
    }

    #[test]
    fn duplicates_edits_and_resets_a_scheme() {
        let mut s = SchemeSettings::default();
        // Bundled schemes cannot be changed.
        assert!(s
            .apply(
                SchemeChange::SetAttribute {
                    key: "KEYWORD".into(),
                    value: Some(red())
                },
                true
            )
            .is_empty());
        change(&mut s, SchemeChange::Duplicate { name: None });
        assert_eq!(s.active, "Ember copy");
        change(
            &mut s,
            SchemeChange::SetAttribute {
                key: "KEYWORD".into(),
                value: Some(red()),
            },
        );
        let (kw, inherited) = s.attribute_of("Ember copy", "KEYWORD");
        assert_eq!(kw.foreground.as_deref(), Some("#ff0000"));
        assert!(!inherited);
        let snap = s.snapshot(true);
        assert!(snap.editable);
        assert_eq!(snap.modified_count, 1);
        let get = |n: &str| snap.css.iter().find(|v| v.name == n).map(|v| v.value.clone());
        assert_eq!(get("--syn-keyword").as_deref(), Some("#ff0000"));
        assert_eq!(get("--syn-keyword-weight").as_deref(), Some("bold"));
        // Setting the parent's value removes the change.
        change(
            &mut s,
            SchemeChange::SetAttribute {
                key: "KEYWORD".into(),
                value: Some(Bundled::Ember.attribute("KEYWORD")),
            },
        );
        assert!(s.user("Ember copy").unwrap().attributes.is_empty());
        change(
            &mut s,
            SchemeChange::SetAttribute {
                key: "STRING".into(),
                value: Some(red()),
            },
        );
        change(&mut s, SchemeChange::ResetAll);
        assert!(s.user("Ember copy").unwrap().attributes.is_empty());
        // A copy of a user scheme keeps its changes and parent.
        change(
            &mut s,
            SchemeChange::SetAttribute {
                key: "STRING".into(),
                value: Some(red()),
            },
        );
        change(
            &mut s,
            SchemeChange::Duplicate {
                name: Some("Twin".into()),
            },
        );
        assert_eq!(s.user("Twin").unwrap().attributes.len(), 1);
        assert_eq!(s.user("Twin").unwrap().parent, Bundled::Ember);
    }

    #[test]
    fn renames_and_deletes_user_schemes() {
        let mut s = SchemeSettings {
            active: "Anvil".into(),
            ..Default::default()
        };
        change(
            &mut s,
            SchemeChange::Duplicate {
                name: Some("Mine".into()),
            },
        );
        for bad in ["", "Ember", "Anvil"] {
            assert!(s
                .apply(
                    SchemeChange::Rename {
                        scheme: "Mine".into(),
                        name: bad.into()
                    },
                    true
                )
                .is_empty());
        }
        change(
            &mut s,
            SchemeChange::Rename {
                scheme: "Mine".into(),
                name: "Ours".into(),
            },
        );
        assert_eq!(s.active, "Ours");
        change(
            &mut s,
            SchemeChange::Delete {
                scheme: "Ours".into(),
            },
        );
        assert_eq!(s.active, "Anvil");
        assert!(s.schemes.is_empty());
    }

    #[test]
    fn round_trips_user_schemes_through_the_settings() {
        let mut attributes = BTreeMap::new();
        attributes.insert("KEYWORD".to_string(), red().normalized());
        attributes.insert("TEXT".to_string(), TextAttributes::bg("#000000"));
        let schemes = vec![UserScheme {
            name: "Mine".into(),
            parent: Bundled::Anvil,
            attributes,
        }];
        let value = encode_schemes(&schemes);
        assert_eq!(value[0]["attributes"], "TEXT bg=#000000\nKEYWORD fg=#ff0000 bold");
        assert_eq!(decode_schemes(&value), schemes);
        assert!(decode_schemes(&json!([{ "name": "" }, 3])).is_empty());
    }

    #[test]
    fn imported_schemes_get_a_free_name_and_are_selected() {
        let s = SchemeSettings::default();
        let writes = s.add_imported(UserScheme {
            name: "Ember".into(),
            parent: Bundled::Ember,
            attributes: BTreeMap::new(),
        });
        assert_eq!(writes[1].value, json!("Ember 2"));
    }

    #[test]
    fn deserializes_changes_from_the_ui() {
        let c: SchemeChange = serde_json::from_value(json!({
            "op": "setAttribute", "key": "KEYWORD",
            "value": { "foreground": "#ff0000", "effect": "waveUnderline" }
        }))
        .unwrap();
        let SchemeChange::SetAttribute { value: Some(v), .. } = c else {
            panic!()
        };
        assert_eq!(v.effect, Some(Effect::WaveUnderline));
    }
}
