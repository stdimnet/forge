//! Message catalogs: flat JSON objects of `key → ICU message`, one file per area and locale
//! in `locales/<locale>/<area>.json`, compiled into the binary.
//!
//! English (`en`) is the source catalog. `menu.json`, `actions.json` and `plugins.json` of
//! `en` are generated from the code that owns the English text (the main menu structure,
//! the PHPStorm action catalog and the plugin manifests) by snapshot tests; every other file
//! is written by hand.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::message::Message;

/// The source language every other catalog falls back to.
pub const SOURCE_LOCALE: &str = "en";

/// A language the IDE can be shown in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocaleInfo {
    /// BCP 47 id, e.g. `de`.
    pub id: String,
    /// English name.
    pub name: String,
    /// The language's own name, as the language picker shows it.
    pub native_name: String,
}

/// Languages with a bundled catalog, English first.
pub fn locales() -> Vec<LocaleInfo> {
    [("en", "English", "English"), ("de", "German", "Deutsch")]
        .iter()
        .map(|(id, name, native)| LocaleInfo {
            id: id.to_string(),
            name: name.to_string(),
            native_name: native.to_string(),
        })
        .collect()
}

macro_rules! builtin {
    ($($locale:literal / $area:literal),* $(,)?) => {
        /// Bundled catalog files: (locale, area, JSON).
        pub const BUILTIN: &[(&str, &str, &str)] = &[
            $(($locale, $area, include_str!(concat!("../locales/", $locale, "/", $area, ".json")))),*
        ];
    };
}

builtin!(
    "en" / "common",
    "en" / "shell",
    "en" / "settings",
    "en" / "search",
    "en" / "vcs",
    "en" / "run",
    "en" / "dialogs",
    "en" / "welcome",
    "en" / "errors",
    "en" / "git",
    "en" / "debug",
    "en" / "database",
    "en" / "menu",
    "en" / "actions",
    "en" / "plugins",
    "de" / "common",
    "de" / "shell",
    "de" / "settings",
    "de" / "search",
    "de" / "vcs",
    "de" / "run",
    "de" / "dialogs",
    "de" / "welcome",
    "de" / "errors",
    "de" / "git",
    "de" / "debug",
    "de" / "database",
    "de" / "menu",
    "de" / "actions",
    "de" / "plugins",
);

/// Catalog areas whose English file is generated from code (do not edit by hand).
pub const GENERATED_AREAS: &[&str] = &["menu", "actions", "plugins"];

/// Parse one catalog file (a flat JSON object of strings).
pub fn parse_catalog(json: &str) -> Result<BTreeMap<String, String>, String> {
    serde_json::from_str::<BTreeMap<String, String>>(json).map_err(|e| e.to_string())
}

/// All bundled messages of `locale` (exact id, no fallback).
pub fn builtin_messages(locale: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (loc, area, json) in BUILTIN {
        if *loc == locale {
            let map = parse_catalog(json)
                .unwrap_or_else(|e| panic!("locales/{loc}/{area}.json is invalid: {e}"));
            out.extend(map);
        }
    }
    out
}

/// A problem of a translation against the source catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Problem {
    /// The key does not exist in the source catalog.
    UnknownKey { key: String },
    /// The message does not parse.
    Syntax { key: String, error: String },
    /// The translation uses other arguments than the source.
    Arguments {
        key: String,
        source: Vec<String>,
        translation: Vec<String>,
    },
}

fn arguments(msg: &str) -> Result<BTreeSet<String>, String> {
    Message::parse(msg)
        .map(|m| m.arguments())
        .map_err(|e| e.to_string())
}

/// Check `translation` against `source`: every key exists in the source, every message parses
/// and uses the same arguments as its source message.
pub fn check_translation(
    source: &HashMap<String, String>,
    translation: &HashMap<String, String>,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut keys: Vec<&String> = translation.keys().collect();
    keys.sort();
    for key in keys {
        let msg = &translation[key];
        let Some(src) = source.get(key) else {
            problems.push(Problem::UnknownKey { key: key.clone() });
            continue;
        };
        let theirs = match arguments(msg) {
            Ok(a) => a,
            Err(error) => {
                problems.push(Problem::Syntax {
                    key: key.clone(),
                    error,
                });
                continue;
            }
        };
        // A source message that is plain text with braces (generated from code) has none.
        let ours = arguments(src).unwrap_or_default();
        if ours != theirs {
            problems.push(Problem::Arguments {
                key: key.clone(),
                source: ours.into_iter().collect(),
                translation: theirs.into_iter().collect(),
            });
        }
    }
    problems
}

/// Share of source keys that `translation` covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub translated: usize,
    pub total: usize,
}

pub fn coverage(
    source: &HashMap<String, String>,
    translation: &HashMap<String, String>,
) -> Coverage {
    Coverage {
        translated: source
            .keys()
            .filter(|k| translation.contains_key(*k))
            .count(),
        total: source.len(),
    }
}

/// Turn free text into a key segment: "Recent Projects" → "recent-projects".
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c == '_' || c == '&' {
            continue;
        }
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn every_bundled_file_parses_and_is_listed() {
        for (loc, area, json) in BUILTIN {
            let m = parse_catalog(json).unwrap_or_else(|e| panic!("{loc}/{area}: {e}"));
            for (k, v) in &m {
                assert!(
                    k.split('.').all(|p| !p.is_empty()),
                    "{loc}/{area}: bad key `{k}`"
                );
                if !(*loc == SOURCE_LOCALE && GENERATED_AREAS.contains(area)) {
                    Message::parse(v).unwrap_or_else(|e| panic!("{loc}/{area} `{k}`: {e}"));
                }
            }
        }
        // Files on disk and the list agree.
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/locales");
        for entry in std::fs::read_dir(dir).unwrap() {
            let loc_dir = entry.unwrap().path();
            let loc = loc_dir.file_name().unwrap().to_string_lossy().to_string();
            for f in std::fs::read_dir(&loc_dir).unwrap() {
                let f = f.unwrap().path();
                let area = f.file_stem().unwrap().to_string_lossy().to_string();
                assert!(
                    BUILTIN.iter().any(|(l, a, _)| *l == loc && *a == area),
                    "locales/{loc}/{area}.json is not listed in BUILTIN"
                );
            }
        }
    }

    /// The key-parity test: every German key exists in English with the same arguments.
    #[test]
    fn german_matches_english() {
        let en = builtin_messages("en");
        let de = builtin_messages("de");
        let problems = check_translation(&en, &de);
        assert!(
            problems.is_empty(),
            "German catalog problems: {problems:#?}"
        );
        // No key is defined twice across the area files of one locale.
        for loc in ["en", "de"] {
            let mut seen = HashMap::new();
            for (l, area, json) in BUILTIN.iter().filter(|(l, ..)| *l == loc) {
                for k in parse_catalog(json).unwrap().keys() {
                    if let Some(prev) = seen.insert(k.clone(), *area) {
                        panic!("{l}: `{k}` is in both {prev}.json and {area}.json");
                    }
                }
            }
        }
    }

    #[test]
    fn hand_written_areas_are_fully_translated() {
        let de = builtin_messages("de");
        for (loc, area, json) in BUILTIN {
            if *loc != "en" || GENERATED_AREAS.contains(area) {
                continue;
            }
            let missing: Vec<String> = parse_catalog(json)
                .unwrap()
                .into_keys()
                .filter(|k| !de.contains_key(k))
                .collect();
            assert!(missing.is_empty(), "de is missing {area} keys: {missing:?}");
        }
    }

    #[test]
    fn checks_find_problems() {
        let en = map(&[("a", "{n} files"), ("b", "Plain")]);
        let de = map(&[("a", "{count} Dateien"), ("c", "x"), ("b", "{oops")]);
        let p = check_translation(&en, &de);
        assert_eq!(p.len(), 3);
        assert!(matches!(&p[0], Problem::Arguments { key, .. } if key == "a"));
        assert!(matches!(&p[1], Problem::Syntax { key, .. } if key == "b"));
        assert!(matches!(&p[2], Problem::UnknownKey { key } if key == "c"));
        assert_eq!(
            coverage(&en, &map(&[("a", "{n} Dateien")])),
            Coverage {
                translated: 1,
                total: 2
            }
        );
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("_Recent Projects"), "recent-projects");
        assert_eq!(slug("File Propert_ies"), "file-properties");
        assert_eq!(slug("Appearance & Behavior"), "appearance-behavior");
        assert_eq!(slug("E_xport…"), "export");
    }
}
