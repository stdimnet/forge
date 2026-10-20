//! Translation keys for `__()`, `trans()`, `trans_choice()` and `@lang`: grouped PHP files
//! (`lang/en/messages.php` → `messages.welcome`) and JSON files (`lang/de.json`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::php;
use crate::project::{files_under, read, Location};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub key: String,
    /// Text in the preferred locale (or the first locale defining it).
    pub value: String,
    /// Locales that define the key.
    pub locales: Vec<String>,
    pub location: Location,
}

fn lang_dir(root: &Path) -> Option<&'static str> {
    ["lang", "resources/lang"]
        .into_iter()
        .find(|d| root.join(d).is_dir())
}

/// The application locale from `config/app.php` (`'locale' => env('APP_LOCALE', 'en')`).
pub fn app_locale(root: &Path) -> String {
    let Some(src) = read(root, "config/app.php") else {
        return "en".into();
    };
    if let Some(i) = src.find("'locale'") {
        let line = src[i..].lines().next().unwrap_or("");
        let quoted: Vec<&str> = line.split('\'').collect();
        // 'locale' => env('APP_LOCALE', 'en'),  or  'locale' => 'en',
        if let Some(last) = quoted.iter().rev().nth(1) {
            if last.len() <= 5
                && !last.is_empty()
                && last.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
            {
                return (*last).to_string();
            }
        }
    }
    "en".into()
}

pub fn list_translations(root: &Path) -> Vec<Translation> {
    let Some(dir) = lang_dir(root) else {
        return Vec::new();
    };
    let preferred = app_locale(root);
    let mut out: Vec<Translation> = Vec::new();
    let mut add = |key: String, value: String, locale: &str, loc: Location| {
        if let Some(t) = out.iter_mut().find(|t| t.key == key) {
            t.locales.push(locale.to_string());
            if locale == preferred {
                t.value = value;
                t.location = loc;
            }
        } else {
            out.push(Translation {
                key,
                value,
                locales: vec![locale.to_string()],
                location: loc,
            });
        }
    };
    let mut files = files_under(root, dir, ".php");
    // Preferred locale first, so its location wins ties.
    files.sort_by_key(|f| !f.starts_with(&format!("{dir}/{preferred}/")));
    for path in files {
        let inner = path[dir.len() + 1..].to_string();
        let Some((locale, group)) = inner.split_once('/') else {
            continue;
        };
        if locale == "vendor" {
            continue;
        }
        let group = group.trim_end_matches(".php").replace('/', ".");
        let Some(src) = read(root, &path) else {
            continue;
        };
        let Some(tree) = php::parse(&src) else {
            continue;
        };
        let Some(value) = php::returned_value(&tree, &src) else {
            continue;
        };
        php::flatten(value.entries(), &group, &mut |key, entry| {
            if let Some(s) = entry.value.as_str() {
                add(
                    key,
                    s.to_string(),
                    locale,
                    Location::at(&path, &src, entry.offset),
                );
            }
        });
    }
    for path in files_under(root, dir, ".json") {
        let inner = &path[dir.len() + 1..];
        if inner.contains('/') {
            continue;
        }
        let locale = inner.trim_end_matches(".json").to_string();
        let Some(src) = read(root, &path) else {
            continue;
        };
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&src)
        else {
            continue;
        };
        for (key, value) in map {
            let needle = serde_json::to_string(&key).unwrap_or_default();
            let at = src.find(&needle).unwrap_or(0);
            add(
                key,
                value.as_str().unwrap_or_default().to_string(),
                &locale,
                Location::at(&path, &src, at),
            );
        }
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}
