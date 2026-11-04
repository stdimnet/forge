//! Localization of plugin manifests.
//!
//! The English text of a manifest is its source. Translations come from the bundled catalogs
//! (`crates/forge-i18n/locales/<locale>/plugins.json`) or from the manifest itself:
//!
//! ```toml
//! [l10n.de]
//! "plugin.acme.tools.name" = "Acme-Werkzeuge"
//! "setting.acme.enabled.title" = "Acme aktivieren"
//! ```
//!
//! Keys are derived from ids, so manifests need no keys of their own:
//!
//! | Text | Key |
//! | --- | --- |
//! | plugin name / description | `plugin.<id>.name`, `plugin.<id>.description` |
//! | command title | `action.<command id>` (shared with the keymap) |
//! | tool window title | `toolwindow.<id>.title` |
//! | setting title / description | `setting.<key>.title`, `setting.<key>.description` |
//! | enum option label | `setting.<key>.option.<value>` |
//! | settings section | `setting.section.<page id>.<section slug>` |
//! | run configuration type | `runconfig.<id>.name`, `runconfig.<id>.description`, `runconfig.<id>.field.<key>` |
//! | main menu labels | `menu.group.<slug>`, `menu.<action id>` (resolved by `forge-keymap`) |

use std::collections::BTreeMap;

use forge_i18n::{slug, Localizer};
use forge_keymap::MenuEntry;

use crate::contributions::{SettingSpec, SettingType};
use crate::registry::PluginInfo;
use crate::PluginManifest;

pub fn plugin_name_key(id: &str) -> String {
    format!("plugin.{id}.name")
}
pub fn plugin_description_key(id: &str) -> String {
    format!("plugin.{id}.description")
}
pub fn tool_window_key(id: &str) -> String {
    format!("toolwindow.{id}.title")
}
pub fn setting_title_key(key: &str) -> String {
    format!("setting.{key}.title")
}
pub fn setting_description_key(key: &str) -> String {
    format!("setting.{key}.description")
}
pub fn setting_option_key(key: &str, value: &str) -> String {
    format!("setting.{key}.option.{}", option_segment(value))
}
pub fn setting_section_key(group: &str, section: &str) -> String {
    format!("setting.section.{group}.{}", slug(section))
}

/// An option value as a key segment: dots and empty values would break dotted keys.
fn option_segment(value: &str) -> String {
    if value.is_empty() {
        "none".into()
    } else {
        value.replace('.', "-")
    }
}

/// Looks up translations: the bundled catalogs first, then the manifest's own `[l10n]`.
struct Translator<'a> {
    l10n: &'a Localizer,
    own: Vec<&'a BTreeMap<String, String>>,
}

impl<'a> Translator<'a> {
    fn new(l10n: &'a Localizer, manifest: &'a PluginManifest) -> Self {
        // The manifest's tables along the fallback chain: `de-AT`, then `de`.
        let mut own = Vec::new();
        let mut loc = l10n.locale().to_string();
        loop {
            if let Some(t) = manifest.l10n.get(&loc) {
                own.push(t);
            }
            match loc.rfind('-') {
                Some(i) => loc.truncate(i),
                None => break,
            }
        }
        Self { l10n, own }
    }

    fn tr(&self, key: &str, english: &mut String) {
        if let Some(t) = self.l10n.translate(key) {
            *english = t;
        } else if let Some(t) = self.own.iter().find_map(|m| m.get(key)) {
            *english = self
                .l10n
                .format_message(t, self.l10n.locale(), &Default::default());
        }
    }
}

fn localize_spec(t: &Translator, s: &mut SettingSpec) {
    t.tr(&setting_title_key(&s.key), &mut s.title);
    if !s.description.is_empty() {
        t.tr(&setting_description_key(&s.key), &mut s.description);
    }
    if let Some(section) = &mut s.section {
        t.tr(&setting_section_key(&s.group, section), section);
    }
    if let SettingType::Enum { options } = &mut s.kind {
        for o in options {
            t.tr(&setting_option_key(&s.key, &o.value), &mut o.label);
        }
    }
}

/// The plugin as shown in the language of `l10n`.
pub fn localize_plugin(info: &PluginInfo, l10n: &Localizer) -> PluginInfo {
    let mut out = info.clone();
    if l10n.locale() == forge_i18n::SOURCE_LOCALE {
        return out;
    }
    let t = Translator::new(l10n, &info.manifest);
    let id = &info.manifest.id;
    t.tr(&plugin_name_key(id), &mut out.manifest.name);
    if !out.manifest.description.is_empty() {
        t.tr(&plugin_description_key(id), &mut out.manifest.description);
    }
    let c = &mut out.contributions;
    for cmd in &mut c.commands {
        t.tr(&format!("action.{}", cmd.id), &mut cmd.title);
    }
    for w in &mut c.tool_windows {
        t.tr(&tool_window_key(&w.id), &mut w.title);
    }
    for s in &mut c.settings {
        localize_spec(&t, s);
    }
    for r in &mut c.run_configuration_types {
        t.tr(&format!("runconfig.{}.name", r.id), &mut r.name);
        if !r.description.is_empty() {
            t.tr(
                &format!("runconfig.{}.description", r.id),
                &mut r.description,
            );
        }
        for f in &mut r.fields {
            t.tr(&format!("runconfig.{}.field.{}", r.id, f.key), &mut f.label);
        }
    }
    out
}

/// Settings specs in the language of `l10n`; `plugins` supplies each plugin's own `[l10n]`.
pub fn localize_settings(
    specs: impl IntoIterator<Item = SettingSpec>,
    plugins: &[PluginInfo],
    l10n: &Localizer,
) -> Vec<SettingSpec> {
    let empty = PluginManifest::default();
    specs
        .into_iter()
        .map(|mut s| {
            if l10n.locale() != forge_i18n::SOURCE_LOCALE {
                let owner = plugins
                    .iter()
                    .find(|p| p.contributions.settings.iter().any(|o| o.key == s.key))
                    .map_or(&empty, |p| &p.manifest);
                localize_spec(&Translator::new(l10n, owner), &mut s);
            }
            s
        })
        .collect()
}

/// The English source messages of manifests (`locales/en/plugins.json`). Keys the keymap
/// already owns (`action.<PHPStorm id>`) are left out.
pub fn manifest_messages(
    plugins: &[PluginInfo],
    skip: impl Fn(&str) -> bool,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut put = |k: String, v: &str| {
        if !v.is_empty() && !skip(&k) {
            out.entry(k).or_insert_with(|| v.to_string());
        }
    };
    fn menu(entries: &[MenuEntry], put: &mut dyn FnMut(String, &str)) {
        for e in entries {
            match e {
                MenuEntry::Action {
                    id, label: Some(l), ..
                } => put(forge_keymap::menu::label_key(id, l), l),
                MenuEntry::Group { label, items } => {
                    put(forge_keymap::menu::group_key(label), label);
                    menu(items, put);
                }
                _ => {}
            }
        }
    }
    for p in plugins {
        let m = &p.manifest;
        put(plugin_name_key(&m.id), &m.name);
        put(plugin_description_key(&m.id), &m.description);
        let c = &p.contributions;
        for cmd in &c.commands {
            put(format!("action.{}", cmd.id), &cmd.title);
        }
        for w in &c.tool_windows {
            put(tool_window_key(&w.id), &w.title);
        }
        for s in &c.settings {
            if s.hidden {
                continue;
            }
            put(setting_title_key(&s.key), &s.title);
            put(setting_description_key(&s.key), &s.description);
            if let Some(section) = &s.section {
                put(setting_section_key(&s.group, section), section);
            }
            if let SettingType::Enum { options } = &s.kind {
                for o in options {
                    put(setting_option_key(&s.key, &o.value), &o.label);
                }
            }
        }
        for r in &c.run_configuration_types {
            put(format!("runconfig.{}.name", r.id), &r.name);
            put(format!("runconfig.{}.description", r.id), &r.description);
            for f in &r.fields {
                put(format!("runconfig.{}.field.{}", r.id, f.key), &f.label);
            }
        }
        for mm in &c.main_menu {
            for part in mm.path.split('/').filter(|p| !p.trim().is_empty()) {
                put(forge_keymap::menu::group_key(part.trim()), part.trim());
            }
            menu(&mm.items, &mut put);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::PluginDescriptor;

    const MANIFEST: &str = r#"
id = "acme.tools"
name = "Acme Tools"
version = "1.0.0"
pack = "core"
description = "Tools by Acme."

[l10n.de]
"plugin.acme.tools.description" = "Werkzeuge von Acme."
"setting.acme.mode.option.fast" = "Schnell"

[[contributions.toolWindows]]
id = "acme"
title = "Acme"
anchor = "left"

[[contributions.settings]]
key = "acme.mode"
title = "Mode"
type = "enum"
options = [{ value = "fast", label = "Fast" }, { value = "slow", label = "Slow" }]
default = "fast"
group = "tools.acme"
section = "Behavior"
"#;

    fn plugin() -> PluginInfo {
        let d = PluginDescriptor::from_toml(MANIFEST).unwrap();
        PluginInfo {
            manifest: d.manifest,
            enabled: true,
            contributions: d.contributions,
        }
    }

    #[test]
    fn manifest_and_bundled_translations() {
        let mut de = Localizer::empty("de");
        de.add_messages(
            "de",
            [
                (
                    "plugin.acme.tools.name".to_string(),
                    "Acme-Werkzeuge".to_string(),
                ),
                ("setting.acme.mode.title".to_string(), "Modus".to_string()),
                (
                    "setting.section.tools.acme.behavior".to_string(),
                    "Verhalten".to_string(),
                ),
            ],
        );
        let p = localize_plugin(&plugin(), &de);
        assert_eq!(p.manifest.name, "Acme-Werkzeuge");
        assert_eq!(p.manifest.description, "Werkzeuge von Acme.");
        // Untranslated text stays English.
        assert_eq!(p.contributions.tool_windows[0].title, "Acme");
        let s = &p.contributions.settings[0];
        assert_eq!(s.title, "Modus");
        assert_eq!(s.section.as_deref(), Some("Verhalten"));
        let SettingType::Enum { options } = &s.kind else {
            panic!()
        };
        assert_eq!(options[0].label, "Schnell");
        assert_eq!(options[1].label, "Slow");
        let specs = localize_settings(p.contributions.settings.clone(), &[plugin()], &de);
        assert_eq!(specs[0].title, "Modus");
        // English leaves everything as written.
        assert_eq!(
            localize_plugin(&plugin(), &Localizer::empty("en")),
            plugin()
        );
    }

    #[test]
    fn source_messages() {
        let m = manifest_messages(&[plugin()], |_| false);
        assert_eq!(m["plugin.acme.tools.name"], "Acme Tools");
        assert_eq!(m["toolwindow.acme.title"], "Acme");
        assert_eq!(m["setting.acme.mode.option.slow"], "Slow");
        assert_eq!(m["setting.section.tools.acme.behavior"], "Behavior");
    }
}
