//! What scopes need to know about the project and the workbench: test roots (PHPStorm's
//! test source roots), libraries and the open, changed and current files.

use serde::{Deserialize, Serialize};

/// Facts a scope is evaluated against. Paths are project-relative with `/`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScopeContext {
    /// Folders with tests (Composer's `autoload-dev` folders); conventional test folders
    /// and file names count as tests as well.
    pub test_roots: Vec<String>,
    /// Files open in editor tabs.
    pub open_files: Vec<String>,
    /// Files with uncommitted changes.
    pub changed_files: Vec<String>,
    /// The file of the active editor.
    pub current_file: String,
}

/// Folders whose content counts as tests wherever they are (outside libraries).
const TEST_DIRS: &[&str] = &["tests", "test", "__tests__", "spec", "specs"];

/// Libraries inside the project folder (Composer and npm packages).
pub fn is_library_path(path: &str) -> bool {
    ["vendor/", "node_modules/"]
        .iter()
        .any(|d| path.starts_with(d) || path.contains(&format!("/{d}")))
}

/// Files outside the project: library files of language servers, scratches, absolute paths.
pub fn is_non_project_path(path: &str) -> bool {
    path.contains("://") || path.starts_with('/') || path.as_bytes().get(1) == Some(&b':')
}

/// Conventional test file names: `UserTest.php`, `user.test.ts`, `user.spec.js`,
/// `user_spec.lua`, `user_test.go`.
pub fn is_test_file_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let has_ext = name.contains('.');
    has_ext
        && (stem.len() > 4 && stem.ends_with("Test") && name.ends_with(".php")
            || name.contains(".test.")
            || name.contains(".spec.")
            || stem.ends_with("_spec")
            || stem.ends_with("_test"))
}

impl ScopeContext {
    pub fn is_library(&self, path: &str) -> bool {
        is_library_path(path)
    }

    /// Whether a project file is a test: below a test root or a conventional test folder,
    /// or named like a test.
    pub fn is_test(&self, path: &str) -> bool {
        let path = path.trim_start_matches("./");
        if self.test_roots.iter().any(|r| {
            let r = r.trim_matches('/');
            !r.is_empty()
                && (path == r
                    || path
                        .strip_prefix(r)
                        .is_some_and(|rest| rest.starts_with('/')))
        }) {
            return true;
        }
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                return is_test_file_name(part);
            }
            if TEST_DIRS.contains(&part) {
                return true;
            }
        }
        false
    }
}

/// Test folders declared in a `composer.json` (`autoload-dev` PSR-4 and PSR-0 entries and
/// class maps), project-relative without trailing `/`.
pub fn composer_test_roots(composer_json: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(composer_json) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    let mut add = |p: &str| {
        let p = p.trim().trim_start_matches("./").trim_end_matches('/');
        if !p.is_empty() && !out.iter().any(|x| x == p) {
            out.push(p.to_string());
        }
    };
    let dev = &v["autoload-dev"];
    for key in ["psr-4", "psr-0"] {
        if let Some(map) = dev[key].as_object() {
            for value in map.values() {
                match value {
                    serde_json::Value::String(s) => add(s),
                    serde_json::Value::Array(a) => {
                        a.iter().filter_map(|x| x.as_str()).for_each(&mut add)
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(a) = dev["classmap"].as_array() {
        a.iter().filter_map(|x| x.as_str()).for_each(add);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_and_libraries() {
        let ctx = ScopeContext {
            test_roots: vec!["qa/".into()],
            ..Default::default()
        };
        assert!(ctx.is_test("qa/Feature/Checkout.php"));
        assert!(ctx.is_test("tests/Unit/UserTest.php"));
        assert!(ctx.is_test("resources/js/__tests__/cart.ts"));
        assert!(ctx.is_test("src/Cart/CartTest.php"));
        assert!(ctx.is_test("resources/js/cart.spec.ts"));
        assert!(!ctx.is_test("app/Models/User.php"));
        assert!(!ctx.is_test("app/Test.php"));
        assert!(!ctx.is_test("qaz/x.php"));
        assert!(ctx.is_library("vendor/laravel/framework/src/Model.php"));
        assert!(ctx.is_library("resources/js/node_modules/vue/index.js"));
        assert!(!ctx.is_library("app/vendors.php"));
        assert!(is_non_project_path("library://php-stubs/Core/Core_c.php"));
        assert!(is_non_project_path("/etc/hosts"));
        assert!(!is_non_project_path("app/User.php"));
    }

    #[test]
    fn composer_dev_autoload() {
        let json = r#"{
            "autoload": { "psr-4": { "App\\": "app/" } },
            "autoload-dev": {
                "psr-4": { "Tests\\": "tests/", "Acme\\Qa\\": ["qa/", "./more/"] },
                "classmap": ["database/seeds"]
            }
        }"#;
        assert_eq!(
            composer_test_roots(json),
            // Keys in map order: `Acme\Qa\` before `Tests\`.
            vec!["qa", "more", "tests", "database/seeds"]
        );
        assert!(composer_test_roots("not json").is_empty());
    }
}
