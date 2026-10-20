//! The project files the framework indexes read, and module resolution between them
//! (relative specifiers, `tsconfig.json`/`jsconfig.json` path aliases, index files and
//! workspace packages imported by their package name).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::json;
use crate::text::{join_path, parent_dir};

/// A project file with its content; paths are workspace relative with `/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub content: String,
}

/// Extensions tried for an import without one, in this order.
pub const MODULE_EXTENSIONS: &[&str] = &["tsx", "ts", "jsx", "js", "vue", "mts", "mjs", "cjs"];

/// The loaded project: file contents (scripts, templates, manifests) and the paths of
/// every other file (stylesheets, images), so references to them can be checked.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    files: BTreeMap<String, String>,
    paths: BTreeSet<String>,
    /// `(prefix, targets)` from `compilerOptions.paths`: `("@/", ["src/"])`.
    aliases: Vec<(String, Vec<String>)>,
    /// `(package name, package.json path)` of the project's own packages (monorepo
    /// workspaces), longest name first.
    packages: Vec<(String, String)>,
}

impl Sources {
    pub fn new(files: Vec<SourceFile>, other_paths: impl IntoIterator<Item = String>) -> Self {
        let mut s = Sources {
            files: files.into_iter().map(|f| (f.path, f.content)).collect(),
            paths: other_paths.into_iter().collect(),
            aliases: Vec::new(),
            packages: Vec::new(),
        };
        s.paths.extend(s.files.keys().cloned());
        s.aliases = s.read_aliases();
        s.packages = s.read_packages();
        s
    }

    pub fn get(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    pub fn exists(&self, path: &str) -> bool {
        self.paths.contains(path)
    }

    /// Files with content, in path order.
    pub fn files(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files.iter().map(|(p, c)| (p.as_str(), c.as_str()))
    }

    /// Every known path, also those without loaded content.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.paths.iter().map(String::as_str)
    }

    /// Replace a file's content (the editor's unsaved text).
    pub fn set(&mut self, path: &str, content: &str) {
        self.paths.insert(path.to_string());
        self.files.insert(path.to_string(), content.to_string());
    }

    /// The file an import specifier in `from` refers to, if it is a project file.
    pub fn resolve_module(&self, from: &str, spec: &str) -> Option<String> {
        let bases: Vec<String> = if spec.starts_with('.') || spec.starts_with('/') {
            vec![join_path(parent_dir(from), spec)]
        } else {
            self.aliases
                .iter()
                .filter_map(|(prefix, targets)| {
                    spec.strip_prefix(prefix.as_str())
                        .map(|rest| targets.iter().map(move |t| join_path(t, rest)))
                })
                .flatten()
                .collect()
        };
        bases
            .iter()
            .find_map(|base| self.resolve_file(base))
            .or_else(|| self.resolve_package(spec))
    }

    /// `@scope/ui` or `@scope/ui/button` when `@scope/ui` is a package of the project: its
    /// `exports` (`"."` or the subpath; conditions `source`, `types`, `import`, `module`,
    /// `default`, `require`), then `module`, `main`, `types`, `source` and `src/index`.
    /// The first entry that exists in the project wins, so a `main` pointing at unbuilt
    /// `dist/` output falls back to the sources.
    fn resolve_package(&self, spec: &str) -> Option<String> {
        let (name, manifest) = self.packages.iter().find(|(name, _)| {
            spec == name
                || spec
                    .strip_prefix(name.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })?;
        let dir = parent_dir(manifest);
        let doc = self.get(manifest).and_then(json::parse)?;
        let subpath = &spec[name.len()..];
        let export_key = format!(".{subpath}");
        let mut candidates: Vec<String> = Vec::new();
        if let Some(exports) = doc.get("exports") {
            let entry = match exports.as_str() {
                Some(_) if subpath.is_empty() => Some(exports),
                Some(_) => None,
                None if exports.members().iter().any(|m| m.key.starts_with('.')) => {
                    exports.get(&export_key)
                }
                // Conditions directly under `exports` describe `"."`.
                None if subpath.is_empty() => Some(exports),
                None => None,
            };
            if let Some(entry) = entry {
                collect_conditions(entry, &mut candidates, 0);
            }
        }
        if subpath.is_empty() {
            for field in ["module", "main", "types", "typings", "source"] {
                if let Some(p) = doc.get(field).and_then(|v| v.as_str()) {
                    candidates.push(p.to_string());
                }
            }
            candidates.extend(["src/index".to_string(), "index".to_string()]);
        } else {
            let rest = subpath.trim_start_matches('/');
            candidates.extend([rest.to_string(), format!("src/{rest}")]);
        }
        candidates.iter().find_map(|c| {
            let base = join_path(dir, c);
            self.resolve_file(&base)
                .or_else(|| self.resolve_file(base.trim_end_matches(".js")))
        })
    }

    /// The workspace packages: every `package.json` with a name outside `node_modules`,
    /// except the root one.
    fn read_packages(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .files
            .iter()
            .filter(|(path, _)| path.ends_with("/package.json") && !is_vendored(path))
            .filter_map(|(path, content)| {
                let name = json::parse(content)?.get("name")?.as_str()?.to_string();
                (!name.is_empty()).then(|| (name, path.clone()))
            })
            .collect();
        out.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
        out
    }

    /// `base` as written, with an extension, or as a directory with an index file.
    fn resolve_file(&self, base: &str) -> Option<String> {
        if self.exists(base) && base.rsplit('/').next().is_some_and(|n| n.contains('.')) {
            return Some(base.to_string());
        }
        MODULE_EXTENSIONS
            .iter()
            .map(|ext| format!("{base}.{ext}"))
            .chain(
                MODULE_EXTENSIONS
                    .iter()
                    .map(|ext| format!("{base}/index.{ext}")),
            )
            .find(|p| self.exists(p))
    }

    /// Path aliases of the root `tsconfig.json` / `jsconfig.json` (following one `extends`
    /// to a project file); without any, the `@/` and `~/` -> `src/` convention of Vite and Nuxt.
    fn read_aliases(&self) -> Vec<(String, Vec<String>)> {
        let mut out = Vec::new();
        for config in ["tsconfig.json", "jsconfig.json", "tsconfig.app.json"] {
            let mut path = config.to_string();
            for _ in 0..3 {
                let Some(doc) = self.get(&path).and_then(json::parse) else {
                    break;
                };
                let dir = parent_dir(&path).to_string();
                let options = doc.get("compilerOptions");
                let base = options
                    .and_then(|o| o.get("baseUrl"))
                    .and_then(|b| b.as_str())
                    .map_or(dir.clone(), |b| join_path(&dir, b));
                if let Some(paths) = options.and_then(|o| o.get("paths")) {
                    for m in paths.members() {
                        let prefix = m.key.trim_end_matches('*').to_string();
                        let targets = m
                            .value
                            .items()
                            .iter()
                            .filter_map(|t| t.as_str())
                            .map(|t| {
                                let t = join_path(&base, t.trim_end_matches('*'));
                                if t.is_empty() {
                                    t
                                } else {
                                    format!("{t}/")
                                }
                            })
                            .collect();
                        out.push((prefix, targets));
                    }
                }
                match doc.get("extends").and_then(|e| e.as_str()) {
                    Some(e) if e.starts_with('.') => path = join_path(&dir, e),
                    _ => break,
                }
            }
        }
        if out.is_empty() {
            out.push(("@/".into(), vec!["src/".into()]));
            out.push(("~/".into(), vec!["src/".into(), String::new()]));
        }
        // Longest prefix first, like TypeScript.
        out.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
        out
    }
}

/// Entry points of an `exports` value in condition order (nested conditions included).
fn collect_conditions(node: &json::Node, out: &mut Vec<String>, depth: u32) {
    const ORDER: &[&str] = &["source", "types", "import", "module", "default", "require"];
    if let Some(p) = node.as_str() {
        out.push(p.to_string());
        return;
    }
    if depth > 3 {
        return;
    }
    for key in ORDER {
        if let Some(v) = node.get(key) {
            collect_conditions(v, out, depth + 1);
        }
    }
    for m in node.members() {
        if !ORDER.contains(&m.key.as_str()) {
            collect_conditions(&m.value, out, depth + 1);
        }
    }
    for item in node.items() {
        collect_conditions(item, out, depth + 1);
    }
}

/// Whether a path belongs to installed packages or build output.
pub fn is_vendored(path: &str) -> bool {
    path.split('/').any(|seg| {
        matches!(
            seg,
            "node_modules" | "dist" | ".nuxt" | ".next" | ".angular" | ".output" | "coverage"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.into(),
            content: content.into(),
        }
    }

    #[test]
    fn resolves_relative_alias_and_index_imports() {
        let s = Sources::new(
            vec![
                file(
                    "tsconfig.json",
                    r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@ui/*": ["src/components/ui/*"] } } }"#,
                ),
                file("src/App.tsx", ""),
                file("src/components/Header.tsx", ""),
                file("src/components/ui/index.ts", ""),
                file("src/components/ui/Card.vue", ""),
            ],
            ["src/styles/app.css".to_string()],
        );
        assert_eq!(
            s.resolve_module("src/App.tsx", "./components/Header")
                .as_deref(),
            Some("src/components/Header.tsx")
        );
        assert_eq!(
            s.resolve_module("src/App.tsx", "@ui/Card.vue").as_deref(),
            Some("src/components/ui/Card.vue")
        );
        assert_eq!(
            s.resolve_module("src/components/Header.tsx", "./ui")
                .as_deref(),
            Some("src/components/ui/index.ts")
        );
        assert_eq!(s.resolve_module("src/App.tsx", "react"), None);
        assert!(s.exists("src/styles/app.css"));
    }

    #[test]
    fn default_aliases_without_tsconfig() {
        let s = Sources::new(vec![file("src/components/Nav.vue", "")], []);
        assert_eq!(
            s.resolve_module("src/App.vue", "@/components/Nav.vue")
                .as_deref(),
            Some("src/components/Nav.vue")
        );
        assert_eq!(
            s.resolve_module("src/App.vue", "~/components/Nav")
                .as_deref(),
            Some("src/components/Nav.vue")
        );
    }

    #[test]
    fn resolves_workspace_packages_by_name() {
        let s = Sources::new(
            vec![
                file(
                    "package.json",
                    r#"{ "name": "storefront", "workspaces": ["packages/*"] }"#,
                ),
                file(
                    "packages/ui/package.json",
                    r#"{ "name": "@storefront/ui", "main": "./dist/index.js",
                        "exports": { ".": { "types": "./src/index.ts", "import": "./dist/index.js" },
                                     "./button": "./src/Button.tsx" } }"#,
                ),
                file("packages/ui/src/index.ts", ""),
                file("packages/ui/src/Button.tsx", ""),
                file("packages/ui/src/Rating.tsx", ""),
                file(
                    "packages/icons/package.json",
                    r#"{ "name": "@storefront/icons", "module": "lib/index.js" }"#,
                ),
                file("packages/icons/lib/index.ts", ""),
                file("packages/plain/package.json", r#"{ "name": "plain" }"#),
                file("packages/plain/src/index.tsx", ""),
                file(
                    "node_modules/react/package.json",
                    r#"{ "name": "react", "main": "index.js" }"#,
                ),
                file("node_modules/react/index.js", ""),
                file("src/App.tsx", ""),
            ],
            [],
        );
        let r = |spec: &str| s.resolve_module("src/App.tsx", spec);
        assert_eq!(
            r("@storefront/ui").as_deref(),
            Some("packages/ui/src/index.ts")
        );
        assert_eq!(
            r("@storefront/ui/button").as_deref(),
            Some("packages/ui/src/Button.tsx")
        );
        // Subpaths that `exports` does not list fall back to the package's files.
        assert_eq!(
            r("@storefront/ui/Rating").as_deref(),
            Some("packages/ui/src/Rating.tsx")
        );
        // `module: lib/index.js` with only the TypeScript source present.
        assert_eq!(
            r("@storefront/icons").as_deref(),
            Some("packages/icons/lib/index.ts")
        );
        assert_eq!(r("plain").as_deref(), Some("packages/plain/src/index.tsx"));
        // Installed packages and the root package are not project packages.
        assert_eq!(r("react"), None);
        assert_eq!(r("storefront"), None);
        assert_eq!(r("@storefront/uikit"), None);
    }

    #[test]
    fn vendored_paths() {
        assert!(is_vendored("node_modules/react/index.js"));
        assert!(is_vendored("apps/web/dist/app.js"));
        assert!(!is_vendored("src/distance.ts"));
    }
}
