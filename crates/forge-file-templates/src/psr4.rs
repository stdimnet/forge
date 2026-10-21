//! Directory ↔ namespace mapping from Composer's PSR-4 autoload rules (`autoload.psr-4` and
//! `autoload-dev.psr-4`), as the New PHP Class dialog fills its Namespace and Directory
//! fields. Paths are project-relative with `/` separators.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One PSR-4 rule: classes under `prefix` live in `dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Psr4Root {
    /// Namespace prefix without the trailing backslash (`App`; empty for the global one).
    pub prefix: String,
    /// Project-relative folder without a trailing slash (empty for the project root).
    pub dir: String,
    /// From `autoload-dev`.
    pub dev: bool,
}

fn normalize_dir(dir: &str) -> String {
    let parts: Vec<&str> = dir
        .split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    parts.join("/")
}

fn join(base: &str, rel: &str) -> String {
    normalize_dir(&format!("{base}/{rel}"))
}

/// The PSR-4 rules of a composer.json that lives in the project folder `composer_dir`.
pub fn roots(composer_json: &str, composer_dir: &str) -> Vec<Psr4Root> {
    let Ok(json) = serde_json::from_str::<Value>(composer_json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (section, dev) in [("autoload", false), ("autoload-dev", true)] {
        let Some(map) = json[section]["psr-4"].as_object() else {
            continue;
        };
        for (prefix, dirs) in map {
            let dirs: Vec<&str> = match dirs {
                Value::String(s) => vec![s.as_str()],
                Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            for d in dirs {
                out.push(Psr4Root {
                    prefix: prefix.trim_matches('\\').to_string(),
                    dir: join(composer_dir, d),
                    dev,
                });
            }
        }
    }
    out
}

/// `dir` relative to `root`, `None` when it is not inside.
fn relative<'a>(root: &str, dir: &'a str) -> Option<&'a str> {
    if root.is_empty() {
        return Some(dir);
    }
    if dir == root {
        return Some("");
    }
    dir.strip_prefix(root)?.strip_prefix('/')
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// The namespace of classes in `dir`: from the rule with the deepest folder containing it
/// (non-dev rules win ties). `None` without a matching rule or when a folder name below it
/// cannot be part of a namespace (`resources/views`, `2024-backup`).
pub fn namespace_for_dir(roots: &[Psr4Root], dir: &str) -> Option<String> {
    let dir = normalize_dir(dir);
    let (root, rest) = roots
        .iter()
        .filter_map(|r| relative(&r.dir, &dir).map(|rest| (r, rest)))
        .max_by_key(|(r, _)| (r.dir.len() + usize::from(!r.dir.is_empty()), !r.dev))?;
    let mut parts: Vec<&str> = Vec::new();
    if !root.prefix.is_empty() {
        parts.push(&root.prefix);
    }
    for seg in rest.split('/').filter(|s| !s.is_empty()) {
        if !is_identifier(seg) {
            return None;
        }
        parts.push(seg);
    }
    Some(parts.join("\\"))
}

/// The folder for classes of `namespace`: from the rule with the longest matching prefix
/// (non-dev rules win ties). `None` without a matching rule.
pub fn dir_for_namespace(roots: &[Psr4Root], namespace: &str) -> Option<String> {
    let ns = namespace.trim().trim_matches('\\');
    let segments: Vec<&str> = ns.split('\\').filter(|s| !s.is_empty()).collect();
    let (root, used) = roots
        .iter()
        .filter_map(|r| {
            let prefix: Vec<&str> = r.prefix.split('\\').filter(|s| !s.is_empty()).collect();
            let matches = prefix.len() <= segments.len()
                && prefix
                    .iter()
                    .zip(&segments)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b));
            matches.then_some((r, prefix.len()))
        })
        .max_by_key(|(r, used)| (*used, !r.dev))?;
    let mut dir = root.dir.clone();
    for seg in &segments[used..] {
        if !dir.is_empty() {
            dir.push('/');
        }
        dir.push_str(seg);
    }
    Some(dir)
}

/// The folder of the composer.json at the project-relative `path`.
pub fn composer_dir(path: &str) -> String {
    let path = normalize_dir(path);
    path.rsplit_once('/')
        .map_or(String::new(), |(dir, _)| dir.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LARAVEL: &str = r#"{
        "autoload": {"psr-4": {"App\\": "app/", "Database\\Factories\\": "database/factories/", "Database\\Seeders\\": "database/seeders/"}},
        "autoload-dev": {"psr-4": {"Tests\\": "tests/"}}
    }"#;

    #[test]
    fn reads_rules() {
        let r = roots(LARAVEL, "");
        assert_eq!(r.len(), 4);
        assert_eq!(
            r[3],
            Psr4Root {
                prefix: "Tests".into(),
                dir: "tests".into(),
                dev: true
            }
        );
        let r = roots(
            r#"{"autoload":{"psr-4":{"Acme\\":["src/", "./lib"]," ":""}}}"#,
            "backend",
        );
        let dirs: Vec<&str> = r.iter().map(|r| r.dir.as_str()).collect();
        assert_eq!(dirs, vec!["backend", "backend/src", "backend/lib"]);
        assert!(roots("not json", "").is_empty());
    }

    #[test]
    fn namespace_of_a_folder() {
        let r = roots(LARAVEL, "");
        assert_eq!(namespace_for_dir(&r, "app").as_deref(), Some("App"));
        assert_eq!(
            namespace_for_dir(&r, "app/Http/Controllers/").as_deref(),
            Some("App\\Http\\Controllers")
        );
        assert_eq!(
            namespace_for_dir(&r, "database/seeders").as_deref(),
            Some("Database\\Seeders")
        );
        assert_eq!(
            namespace_for_dir(&r, "tests/Feature").as_deref(),
            Some("Tests\\Feature")
        );
        assert_eq!(namespace_for_dir(&r, "application"), None);
        assert_eq!(namespace_for_dir(&r, "resources/views"), None);
        assert_eq!(namespace_for_dir(&r, "app/2024-backup"), None);
        // The global prefix maps the whole folder.
        let g = roots(r#"{"autoload":{"psr-4":{"":"src/"}}}"#, "");
        assert_eq!(namespace_for_dir(&g, "src").as_deref(), Some(""));
        assert_eq!(namespace_for_dir(&g, "src/Model").as_deref(), Some("Model"));
        // A composer.json in the project root with a rule for the root folder.
        let root = roots(r#"{"autoload":{"psr-4":{"Acme\\":""}}}"#, "");
        assert_eq!(
            namespace_for_dir(&root, "Domain").as_deref(),
            Some("Acme\\Domain")
        );
        assert_eq!(namespace_for_dir(&root, "").as_deref(), Some("Acme"));
    }

    #[test]
    fn deepest_folder_wins() {
        let r = roots(
            r#"{"autoload":{"psr-4":{"Acme\\":"src/","Acme\\Legacy\\":"src/legacy/"}},"autoload-dev":{"psr-4":{"Acme\\Tests\\":"src/"}}}"#,
            "",
        );
        assert_eq!(
            namespace_for_dir(&r, "src/legacy/Old").as_deref(),
            Some("Acme\\Legacy\\Old")
        );
        assert_eq!(
            namespace_for_dir(&r, "src/Domain").as_deref(),
            Some("Acme\\Domain")
        );
    }

    #[test]
    fn folder_of_a_namespace() {
        let r = roots(LARAVEL, "");
        assert_eq!(
            dir_for_namespace(&r, "App\\Models").as_deref(),
            Some("app/Models")
        );
        assert_eq!(
            dir_for_namespace(&r, "\\app\\Http\\").as_deref(),
            Some("app/Http")
        );
        assert_eq!(dir_for_namespace(&r, "App").as_deref(), Some("app"));
        assert_eq!(
            dir_for_namespace(&r, "Database\\Seeders\\Demo").as_deref(),
            Some("database/seeders/Demo")
        );
        assert_eq!(dir_for_namespace(&r, "Vendor\\Thing"), None);
        assert_eq!(dir_for_namespace(&r, "Application"), None);
        let g = roots(r#"{"autoload":{"psr-4":{"":"src/"}}}"#, "api");
        assert_eq!(dir_for_namespace(&g, "Foo").as_deref(), Some("api/src/Foo"));
    }

    #[test]
    fn composer_folder() {
        assert_eq!(composer_dir("composer.json"), "");
        assert_eq!(composer_dir("backend/composer.json"), "backend");
        assert_eq!(composer_dir("./a/b/composer.json"), "a/b");
    }
}
