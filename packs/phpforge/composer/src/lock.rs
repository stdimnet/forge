//! Installed packages: `composer.lock` (`packages`, `packages-dev`) and
//! `vendor/composer/installed.json` (Composer 1: an array, Composer 2: `{"packages": …}`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A package Composer installed (or locked).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// The source repository (`source.url`, without `.git`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// From `packages-dev` (or `dev-package-names` of installed.json).
    #[serde(default)]
    pub dev: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abandoned: Option<String>,
}

fn package(v: &Value, dev: bool) -> Option<InstalledPackage> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let source = v
        .get("source")
        .and_then(|s| s.get("url"))
        .and_then(Value::as_str)
        .map(|u| u.trim_end_matches(".git").to_string());
    let abandoned = match v.get("abandoned") {
        Some(Value::String(r)) => Some(r.clone()),
        Some(Value::Bool(true)) => Some(String::new()),
        _ => None,
    };
    Some(InstalledPackage {
        name: s("name")?,
        version: s("version").unwrap_or_default(),
        description: s("description").unwrap_or_default(),
        homepage: s("homepage").filter(|h| !h.is_empty()),
        source,
        dev,
        abandoned,
    })
}

/// The packages of a composer.lock.
pub fn parse_lock(text: &str) -> Vec<InstalledPackage> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, dev) in [("packages", false), ("packages-dev", true)] {
        for p in v.get(key).and_then(Value::as_array).into_iter().flatten() {
            out.extend(package(p, dev));
        }
    }
    out
}

/// The packages of vendor/composer/installed.json.
pub fn parse_installed_json(text: &str) -> Vec<InstalledPackage> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let dev_names: Vec<&str> = v
        .get("dev-package-names")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let list = match &v {
        Value::Array(items) => items.as_slice(),
        _ => v
            .get("packages")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    };
    list.iter()
        .filter_map(|p| {
            let name = p.get("name").and_then(Value::as_str)?;
            package(p, dev_names.contains(&name))
        })
        .collect()
}

/// Packages of the lock file, else of installed.json (whichever is there).
pub fn installed(lock: Option<&str>, installed_json: Option<&str>) -> Vec<InstalledPackage> {
    let from_lock = lock.map(parse_lock).unwrap_or_default();
    if !from_lock.is_empty() {
        return from_lock;
    }
    installed_json.map(parse_installed_json).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_composer_lock() {
        let lock = r#"{
            "packages": [
                {"name": "laravel/framework", "version": "v12.1.0", "description": "The Laravel Framework.",
                 "homepage": "https://laravel.com", "source": {"type": "git", "url": "https://github.com/laravel/framework.git"}},
                {"name": "swiftmailer/swiftmailer", "version": "v6.3.0", "abandoned": "symfony/mailer"}
            ],
            "packages-dev": [{"name": "phpunit/phpunit", "version": "11.4.0"}]
        }"#;
        let p = parse_lock(lock);
        assert_eq!(p.len(), 3);
        assert_eq!(
            p[0].source.as_deref(),
            Some("https://github.com/laravel/framework")
        );
        assert_eq!(p[0].homepage.as_deref(), Some("https://laravel.com"));
        assert_eq!(p[1].abandoned.as_deref(), Some("symfony/mailer"));
        assert!(p[2].dev && !p[0].dev);
        assert!(parse_lock("{broken").is_empty());
    }

    #[test]
    fn reads_both_installed_json_formats() {
        let v2 = r#"{"packages": [{"name": "a/b", "version": "1.0.0"}, {"name": "c/d", "version": "2.0.0"}],
                     "dev": true, "dev-package-names": ["c/d"]}"#;
        let p = parse_installed_json(v2);
        assert_eq!(p.len(), 2);
        assert!(!p[0].dev && p[1].dev);
        let v1 = r#"[{"name": "a/b", "version": "1.0.0"}]"#;
        assert_eq!(parse_installed_json(v1)[0].name, "a/b");
        assert_eq!(installed(Some("{}"), Some(v1)).len(), 1);
    }
}
