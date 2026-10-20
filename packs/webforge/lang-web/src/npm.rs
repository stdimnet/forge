//! The npm tool window and `package.json` gutter: every `package.json` of the project with
//! its scripts and package manager.

use serde::{Deserialize, Serialize};

use crate::component::{GutterKind, GutterMark};
use crate::package_json::{PackageJson, Script};
use crate::pm::{self, Detection};
use crate::source::{is_vendored, Sources};
use crate::text::{parent_dir, Location};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NpmPackage {
    /// Workspace-relative path of the `package.json`.
    pub path: String,
    /// Its directory (`""` for the root).
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub package_manager: Detection,
    pub scripts: Vec<Script>,
}

/// Every `package.json` outside `node_modules` and build output: the root first, then by
/// path. `setting` is `web.packageManager`.
pub fn packages(sources: &Sources, setting: &str) -> Vec<NpmPackage> {
    let mut out: Vec<NpmPackage> = sources
        .files()
        .filter(|(path, _)| is_package_json(path) && !is_vendored(path))
        .map(|(path, content)| {
            let pkg = PackageJson::parse(content).unwrap_or_default();
            NpmPackage {
                path: path.to_string(),
                dir: parent_dir(path).to_string(),
                name: pkg.name,
                version: pkg.version,
                package_manager: pm::detect(sources, path, setting),
                scripts: pkg.scripts,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        (
            a.dir.matches('/').count() + usize::from(!a.dir.is_empty()),
            &a.path,
        )
            .cmp(&(
                b.dir.matches('/').count() + usize::from(!b.dir.is_empty()),
                &b.path,
            ))
    });
    out
}

pub fn is_package_json(path: &str) -> bool {
    path == "package.json" || path.ends_with("/package.json")
}

/// Run icons next to the scripts of a `package.json` (its current, maybe unsaved, text).
pub fn gutter(path: &str, content: &str) -> Vec<GutterMark> {
    if !is_package_json(path) || is_vendored(path) {
        return Vec::new();
    }
    let Some(pkg) = PackageJson::parse(content) else {
        return Vec::new();
    };
    pkg.scripts
        .iter()
        .map(|s| GutterMark {
            line: s.line,
            kind: GutterKind::Run,
            title: format!("Run '{}'", s.name),
            targets: vec![Location {
                path: path.to_string(),
                line: s.line,
                column: s.column,
            }],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pm::PackageManager;
    use crate::source::SourceFile;

    fn f(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.into(),
            content: content.into(),
        }
    }

    #[test]
    fn lists_packages_root_first() {
        let s = Sources::new(
            vec![
                f(
                    "packages/ui/package.json",
                    r#"{"name": "@shop/ui", "scripts": {"storybook": "storybook dev"}}"#,
                ),
                f(
                    "package.json",
                    r#"{"name": "shop", "scripts": {"dev": "turbo dev", "build": "turbo build"}}"#,
                ),
                f("apps/web/package.json", r#"{"name": "web"}"#),
                f("node_modules/react/package.json", r#"{"name": "react"}"#),
            ],
            ["pnpm-lock.yaml".to_string()],
        );
        let pkgs = packages(&s, "auto");
        let paths: Vec<_> = pkgs.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "package.json",
                "apps/web/package.json",
                "packages/ui/package.json"
            ]
        );
        assert_eq!(pkgs[0].scripts.len(), 2);
        assert!(pkgs
            .iter()
            .all(|p| p.package_manager.manager == PackageManager::Pnpm));
        assert_eq!(pkgs[2].name.as_deref(), Some("@shop/ui"));
    }

    #[test]
    fn gutter_marks_scripts() {
        let src =
            "{\n  \"scripts\": {\n    \"dev\": \"vite\",\n    \"build\": \"vite build\"\n  }\n}\n";
        let marks = gutter("package.json", src);
        let lines: Vec<_> = marks.iter().map(|m| (m.line, m.title.as_str())).collect();
        assert_eq!(lines, [(2, "Run 'dev'"), (3, "Run 'build'")]);
        assert!(gutter("src/package.ts", src).is_empty());
        assert!(gutter("node_modules/x/package.json", src).is_empty());
    }
}
