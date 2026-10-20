//! Quick scan of a project: which catalog languages occur (by file name, honouring
//! `.gitignore`, capped), which root files exist and which npm packages it depends on.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::catalog::Catalog;

/// Directories never scanned: dependencies and build output rarely say anything about
/// the languages the project itself is written in.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "vendor",
    "target",
    ".forge",
    ".idea",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
    ".next",
    ".nuxt",
];

/// Default maximum number of files looked at.
pub const DEFAULT_SCAN_LIMIT: usize = 20_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScan {
    /// Catalog language ids found in the project.
    pub languages: BTreeSet<String>,
    /// File names directly in the project root.
    pub root_files: BTreeSet<String>,
    /// Dependencies from the root `package.json` (all sections).
    pub packages: BTreeSet<String>,
    pub files_scanned: usize,
    /// The limit was hit; rarer languages may be missing.
    pub truncated: bool,
}

impl ProjectScan {
    /// Whether one of `patterns` exists in the root (`name` or `prefix*`).
    pub fn has_root_file(&self, patterns: &[String]) -> bool {
        patterns.iter().any(|p| match p.strip_suffix('*') {
            Some(prefix) => self.root_files.iter().any(|f| f.starts_with(prefix)),
            None => self.root_files.contains(p),
        })
    }

    pub fn has_package(&self, names: &[String]) -> bool {
        names.iter().any(|n| self.packages.contains(n))
    }
}

/// Dependencies of every section of a `package.json`.
pub fn package_dependencies(package_json: &str) -> BTreeSet<String> {
    let Ok(v) = serde_json::from_str::<Value>(package_json) else {
        return BTreeSet::new();
    };
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .filter_map(|k| v.get(k).and_then(Value::as_object))
    .flat_map(|o| o.keys().cloned())
    .collect()
}

/// Scan `root` for languages, looking at no more than `limit` files.
pub fn scan_project(root: &Path, catalog: &Catalog, limit: usize) -> ProjectScan {
    let mut scan = ProjectScan::default();
    if let Ok(entries) = std::fs::read_dir(root) {
        scan.root_files = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
    }
    if let Ok(src) = std::fs::read_to_string(root.join("package.json")) {
        scan.packages = package_dependencies(&src);
    }
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|e| {
            !(e.file_type().is_some_and(|t| t.is_dir())
                && e.depth() > 0
                && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
        })
        .build();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if scan.files_scanned >= limit {
            scan.truncated = true;
            break;
        }
        scan.files_scanned += 1;
        let name = entry.file_name().to_string_lossy();
        if let Some(lang) = catalog.language_of(&name) {
            if !scan.languages.contains(&lang.id) {
                scan.languages.insert(lang.id.clone());
            }
        }
    }
    scan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn finds_languages_root_files_and_packages() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/main.ts", "");
        write(root, "src/App.vue", "");
        write(root, "Dockerfile", "");
        write(root, ".github/workflows/ci.yml", "");
        write(root, "tailwind.config.js", "");
        write(
            root,
            "package.json",
            r#"{"dependencies": {"vue": "^3"}, "devDependencies": {"eslint": "^9"}}"#,
        );
        // Ignored or skipped content does not count.
        write(root, ".gitignore", "generated/\n");
        write(root, "generated/x.py", "");
        write(root, "node_modules/lib/index.go", "");
        write(root, "notes.txt", "");

        let scan = scan_project(root, Catalog::builtin(), 1000);
        let langs: Vec<&str> = scan.languages.iter().map(String::as_str).collect();
        assert_eq!(
            langs,
            vec![
                "dockerfile",
                "javascript",
                "json",
                "typescript",
                "vue",
                "yaml"
            ]
        );
        assert!(scan.has_root_file(&["tailwind.config.*".into()]));
        assert!(!scan.has_root_file(&["eslint.config.*".into()]));
        assert!(scan.has_package(&["eslint".into()]));
        assert!(!scan.truncated);
    }

    #[test]
    fn stops_at_the_limit() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..20 {
            write(tmp.path(), &format!("f{i}.rs"), "");
        }
        let scan = scan_project(tmp.path(), Catalog::builtin(), 5);
        assert_eq!(scan.files_scanned, 5);
        assert!(scan.truncated);
        assert!(scan.languages.contains("rust"));
    }

    #[test]
    fn package_json_sections() {
        let deps = package_dependencies(
            r#"{"peerDependencies": {"@angular/core": "*"}, "optionalDependencies": {"x": "1"}}"#,
        );
        assert!(deps.contains("@angular/core") && deps.contains("x"));
        assert!(package_dependencies("nope").is_empty());
    }
}
