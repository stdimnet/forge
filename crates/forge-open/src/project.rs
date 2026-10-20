//! What the Browser Companion settings page needs to know about the open project to show
//! the relevant framework snippets first, and the one config change it may apply itself:
//! setting a variable in a dotenv file.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

/// Dotenv files the "Apply" buttons may write (project root only).
pub const WRITABLE_ENV_FILES: &[&str] = &[".env", ".env.local"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProfile {
    /// Package names from `composer.json` (`require` + `require-dev`).
    pub composer: Vec<String>,
    /// Package names from `package.json` (`dependencies` + `devDependencies`).
    pub npm: Vec<String>,
    /// For each existing dotenv file: the values of the requested keys that are set.
    pub env: BTreeMap<String, BTreeMap<String, String>>,
    /// Files present at the root among the probed ones (e.g. `config/ignition.php`).
    pub files: Vec<String>,
}

fn package_names(json: &str, sections: &[&str]) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return vec![];
    };
    let mut names: Vec<String> = sections
        .iter()
        .filter_map(|s| value.get(*s).and_then(|v| v.as_object()))
        .flat_map(|o| o.keys().cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Read the project's manifests, the given dotenv keys and which `probe_files` exist.
pub fn profile(root: &Path, env_keys: &[String], probe_files: &[String]) -> ProjectProfile {
    profile_with(
        |name| std::fs::read_to_string(root.join(name)).ok(),
        |name| root.join(name).is_file(),
        env_keys,
        probe_files,
    )
}

/// Like [`profile`], reading the project's files with `read` and checking them with
/// `is_file` (a remote project's files are on its host).
pub fn profile_with(
    read: impl Fn(&str) -> Option<String>,
    is_file: impl Fn(&str) -> bool,
    env_keys: &[String],
    probe_files: &[String],
) -> ProjectProfile {
    let mut env = BTreeMap::new();
    for file in WRITABLE_ENV_FILES {
        if let Some(content) = read(file) {
            let values: BTreeMap<String, String> = env_keys
                .iter()
                .filter_map(|k| get_env_var(&content, k).map(|v| (k.clone(), v)))
                .collect();
            env.insert(file.to_string(), values);
        }
    }
    ProjectProfile {
        composer: read("composer.json")
            .map(|s| package_names(&s, &["require", "require-dev"]))
            .unwrap_or_default(),
        npm: read("package.json")
            .map(|s| package_names(&s, &["dependencies", "devDependencies"]))
            .unwrap_or_default(),
        env,
        files: probe_files
            .iter()
            .filter(|f| is_plain_relative(f) && is_file(f))
            .cloned()
            .collect(),
    }
}

fn is_plain_relative(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && p.split('/').all(|s| !s.is_empty() && s != ".." && s != ".")
}

/// `UPPER_SNAKE` names only.
pub fn is_valid_env_key(key: &str) -> bool {
    key.starts_with(|c: char| c.is_ascii_uppercase())
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && key.len() <= 64
}

fn line_key(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    let (key, _) = line.split_once('=')?;
    let key = key.trim_end();
    (!key.is_empty() && !key.starts_with('#')).then_some(key)
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].to_string();
        }
    }
    // Strip an inline comment from unquoted values.
    v.split(" #").next().unwrap_or(v).trim_end().to_string()
}

pub fn get_env_var(content: &str, key: &str) -> Option<String> {
    content
        .lines()
        .rev()
        .find(|l| line_key(l) == Some(key))
        .and_then(|l| l.split_once('=').map(|(_, v)| unquote(v)))
}

/// Quote values that dotenv parsers (phpdotenv, Symfony Dotenv) could misread.
fn format_value(value: &str) -> String {
    let plain = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_-./:".contains(c));
    if plain && !value.is_empty() {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Set `key=value` in dotenv `content`: replaces the last existing assignment (keeping its
/// position) or appends a new line.
pub fn upsert_env_var(content: &str, key: &str, value: &str) -> String {
    let line = format!("{key}={}", format_value(value));
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    match lines.iter().rposition(|l| line_key(l) == Some(key)) {
        Some(i) => lines[i] = line,
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(line);
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn upserts_env_vars() {
        let env =
            "APP_NAME=Shop\n# IGNITION_EDITOR=vscode\nIGNITION_EDITOR=phpstorm\nAPP_DEBUG=true\n\n";
        let out = upsert_env_var(env, "IGNITION_EDITOR", "forge");
        assert_eq!(
            out,
            "APP_NAME=Shop\n# IGNITION_EDITOR=vscode\nIGNITION_EDITOR=forge\nAPP_DEBUG=true\n\n"
        );
        assert_eq!(
            upsert_env_var("A=1\n\n", "SYMFONY_IDE", "forge://open?file=%f&line=%l"),
            "A=1\n\nSYMFONY_IDE=\"forge://open?file=%f&line=%l\"\n"
        );
        assert_eq!(upsert_env_var("", "K", "v"), "K=v\n");
    }

    #[test]
    fn reads_env_vars() {
        let env = "export A=\"x y\"\nB='z' \nC=plain # comment\n#D=no\n";
        assert_eq!(get_env_var(env, "A").as_deref(), Some("x y"));
        assert_eq!(get_env_var(env, "B").as_deref(), Some("z"));
        assert_eq!(get_env_var(env, "C").as_deref(), Some("plain"));
        assert_eq!(get_env_var(env, "D"), None);
    }

    #[test]
    fn validates_keys() {
        assert!(is_valid_env_key("IGNITION_EDITOR"));
        assert!(!is_valid_env_key("ignition"));
        assert!(!is_valid_env_key("A=B"));
        assert!(!is_valid_env_key(""));
    }

    #[test]
    fn profiles_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("composer.json"),
            r#"{"require":{"laravel/framework":"^11"},"require-dev":{"spatie/laravel-ignition":"^2"}}"#,
        )
        .unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"devDependencies":{"vite":"^6","@vitejs/plugin-vue":"^5"},"dependencies":{"vue":"^3"}}"#,
        )
        .unwrap();
        fs::write(root.join(".env"), "IGNITION_EDITOR=forge\nAPP_KEY=secret\n").unwrap();
        fs::create_dir(root.join("config")).unwrap();
        fs::write(root.join("config/ignition.php"), "<?php").unwrap();

        let p = profile(
            root,
            &["IGNITION_EDITOR".into(), "SYMFONY_IDE".into()],
            &["config/ignition.php".into(), "../etc/passwd".into()],
        );
        assert_eq!(
            p.composer,
            vec!["laravel/framework", "spatie/laravel-ignition"]
        );
        assert_eq!(p.npm, vec!["@vitejs/plugin-vue", "vite", "vue"]);
        assert_eq!(
            p.env[".env"].get("IGNITION_EDITOR").map(String::as_str),
            Some("forge")
        );
        assert!(
            !p.env[".env"].contains_key("APP_KEY"),
            "only requested keys"
        );
        assert!(!p.env.contains_key(".env.local"));
        assert_eq!(p.files, vec!["config/ignition.php"]);
    }
}
