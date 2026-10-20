//! `package.json`: name, scripts with their positions, dependencies, workspaces and the
//! `packageManager` field.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::json::{self, Node};
use crate::text::line_col;

/// One entry of `"scripts"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub name: String,
    pub command: String,
    /// 0-based line and UTF-16 column of the script's key.
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub scripts: Vec<Script>,
    /// `"packageManager": "pnpm@9.15.0"` as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_manager: Option<String>,
    /// `dependencies`, `devDependencies` and `peerDependencies` merged (first one wins).
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    /// Workspace globs (`"workspaces": [...]` or `{ "packages": [...] }`).
    #[serde(default)]
    pub workspaces: Vec<String>,
}

impl PackageJson {
    /// Parse a `package.json`. A document that is not valid JSON (while it is being edited)
    /// still yields its scripts, found line by line.
    pub fn parse(text: &str) -> Option<PackageJson> {
        match json::parse(text) {
            Some(doc) => Some(Self::from_doc(text, &doc)),
            None => {
                let scripts = scripts_by_lines(text);
                (!scripts.is_empty()).then(|| PackageJson {
                    scripts,
                    ..Default::default()
                })
            }
        }
    }

    fn from_doc(text: &str, doc: &Node) -> PackageJson {
        let string = |key: &str| doc.get(key).and_then(Node::as_str).map(str::to_string);
        let scripts = doc
            .get("scripts")
            .map(|s| {
                s.members()
                    .iter()
                    .filter_map(|m| {
                        let command = m.value.as_str()?.to_string();
                        let (line, column) = line_col(text, m.key_start);
                        Some(Script {
                            name: m.key.clone(),
                            command,
                            line,
                            column,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut dependencies = BTreeMap::new();
        for section in ["dependencies", "devDependencies", "peerDependencies"] {
            for m in doc.get(section).map(Node::members).unwrap_or_default() {
                dependencies
                    .entry(m.key.clone())
                    .or_insert_with(|| m.value.as_str().unwrap_or("").to_string());
            }
        }
        let workspaces = doc
            .get("workspaces")
            .map(|w| match w.get("packages") {
                Some(p) => p.items(),
                None => w.items(),
            })
            .unwrap_or_default()
            .iter()
            .filter_map(|n| n.as_str().map(str::to_string))
            .collect();
        PackageJson {
            name: string("name"),
            version: string("version"),
            scripts,
            package_manager: string("packageManager"),
            dependencies,
            workspaces,
        }
    }

    pub fn depends_on(&self, package: &str) -> bool {
        self.dependencies.contains_key(package)
    }

    pub fn script(&self, name: &str) -> Option<&Script> {
        self.scripts.iter().find(|s| s.name == name)
    }
}

/// Scripts of a document that does not parse: `"name": "command"` lines inside the block
/// that follows `"scripts": {`.
fn scripts_by_lines(text: &str) -> Vec<Script> {
    let mut out = Vec::new();
    let mut inside = false;
    for (line_no, line) in text.lines().enumerate() {
        let t = line.trim();
        if !inside {
            inside = t.starts_with("\"scripts\"") && t.ends_with('{');
            continue;
        }
        if t.starts_with('}') {
            break;
        }
        let Some(rest) = t.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else { continue };
        let name = &rest[..end];
        let value = rest[end + 1..].trim_start().trim_start_matches(':').trim();
        let value = value.trim_end_matches(',').trim();
        let Some(command) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
            continue;
        };
        let indent = line.len() - line.trim_start().len();
        out.push(Script {
            name: name.to_string(),
            command: command.replace("\\\"", "\""),
            line: line_no as u32,
            column: line[..indent].encode_utf16().count() as u32,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PKG: &str = r#"{
  "name": "storefront-ui",
  "version": "1.4.0",
  "packageManager": "pnpm@9.15.0",
  "workspaces": ["packages/*"],
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "test": "vitest run"
  },
  "dependencies": { "react": "^19.1.0" },
  "devDependencies": { "typescript": "~5.8.3", "react": "ignored" }
}
"#;

    #[test]
    fn parses_scripts_with_positions() {
        let p = PackageJson::parse(PKG).unwrap();
        assert_eq!(p.name.as_deref(), Some("storefront-ui"));
        assert_eq!(p.package_manager.as_deref(), Some("pnpm@9.15.0"));
        let names: Vec<_> = p
            .scripts
            .iter()
            .map(|s| (s.name.as_str(), s.line, s.column))
            .collect();
        assert_eq!(names, [("dev", 6, 4), ("build", 7, 4), ("test", 8, 4)]);
        assert_eq!(p.script("build").unwrap().command, "tsc -b && vite build");
        assert_eq!(p.dependencies["react"], "^19.1.0");
        assert!(p.depends_on("typescript"));
        assert_eq!(p.workspaces, ["packages/*"]);
    }

    #[test]
    fn yarn_workspaces_object() {
        let p =
            PackageJson::parse(r#"{"workspaces": {"packages": ["apps/*", "libs/*"]}}"#).unwrap();
        assert_eq!(p.workspaces, ["apps/*", "libs/*"]);
        assert!(p.scripts.is_empty());
    }

    #[test]
    fn broken_document_keeps_scripts() {
        let src = "{\n  \"scripts\": {\n    \"dev\": \"vite --host\",\n    \"lint\": \"eslint .\"\n  },\n  \"dependencies\": {\n";
        let p = PackageJson::parse(src).unwrap();
        let names: Vec<_> = p
            .scripts
            .iter()
            .map(|s| (s.name.as_str(), s.command.as_str(), s.line))
            .collect();
        assert_eq!(names, [("dev", "vite --host", 2), ("lint", "eslint .", 3)]);
        assert!(PackageJson::parse("{ nope").is_none());
    }
}
