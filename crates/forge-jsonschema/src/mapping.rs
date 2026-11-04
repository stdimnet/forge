//! Which schema a file uses, in this order (like PHPStorm):
//!
//! 1. user mappings (Settings | Languages & Frameworks | Schemas and DTDs | JSON Schema
//!    Mappings, and the status bar widget's choice) — a mapping with no schema turns
//!    schemas off for its files,
//! 2. the file's own `"$schema"` (JSON), `$schema:` key or
//!    `# yaml-language-server: $schema=…` modeline (YAML),
//! 3. Forge's built-in mappings (composer.json, package.json, tsconfig, ESLint, Prettier,
//!    Compose, GitHub Actions, GitLab CI, Minecraft plugin descriptors in
//!    `src/main/resources`, Unity assembly definitions) with bundled schemas,
//! 4. the SchemaStore catalog, when remote schemas are allowed.

use serde::{Deserialize, Serialize};

use crate::bundled;
use crate::schema::Draft;
use crate::uri;

/// A user mapping (`jsonSchema.mappings` entry; all values are strings).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserMapping {
    pub name: String,
    /// A URL, or a path relative to the project root (absolute paths are files on this
    /// machine). Empty: no schema for these files.
    pub schema: String,
    /// `4`, `6`, `7`, `2019-09`, `2020-12`, or empty/`auto`.
    pub version: String,
    /// One entry per line: a file path, a directory (ending in `/`) or a pattern with
    /// `*`, `**`, `?`, `{a,b}` (without `/` it matches file names in any directory).
    pub files: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    User,
    Inline,
    Builtin,
    Catalog,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub name: String,
    /// Empty when a user mapping turned schemas off for the file.
    pub uri: String,
    pub source: Source,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Draft>,
}

pub struct Builtin {
    pub name: &'static str,
    pub uri: &'static str,
    pub files: &'static [&'static str],
}

pub static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "composer.json",
        uri: bundled::COMPOSER,
        files: &["composer.json"],
    },
    Builtin {
        name: "package.json",
        uri: bundled::PACKAGE,
        files: &["package.json"],
    },
    Builtin {
        name: "tsconfig.json",
        uri: bundled::TSCONFIG,
        files: &["tsconfig.json", "tsconfig.*.json", "tsconfig-*.json"],
    },
    Builtin {
        name: "jsconfig.json",
        uri: bundled::JSCONFIG,
        files: &["jsconfig.json", "jsconfig.*.json"],
    },
    Builtin {
        name: ".eslintrc",
        uri: bundled::ESLINTRC,
        files: &[
            ".eslintrc",
            ".eslintrc.json",
            ".eslintrc.yml",
            ".eslintrc.yaml",
        ],
    },
    Builtin {
        name: ".prettierrc",
        uri: bundled::PRETTIERRC,
        files: &[
            ".prettierrc",
            ".prettierrc.json",
            ".prettierrc.json5",
            ".prettierrc.yml",
            ".prettierrc.yaml",
        ],
    },
    Builtin {
        name: "Docker Compose",
        uri: bundled::COMPOSE,
        files: &[
            "docker-compose.yml",
            "docker-compose.yaml",
            "docker-compose.*.yml",
            "docker-compose.*.yaml",
            "compose.yml",
            "compose.yaml",
            "compose.*.yml",
            "compose.*.yaml",
        ],
    },
    Builtin {
        name: "GitHub Workflow",
        uri: bundled::GITHUB_WORKFLOW,
        files: &["**/.github/workflows/*.yml", "**/.github/workflows/*.yaml"],
    },
    Builtin {
        name: "GitHub Action",
        uri: bundled::GITHUB_ACTION,
        files: &["action.yml", "action.yaml"],
    },
    Builtin {
        name: "GitLab CI",
        uri: bundled::GITLAB_CI,
        files: &[
            ".gitlab-ci.yml",
            ".gitlab-ci.yaml",
            "*.gitlab-ci.yml",
            "*.gitlab-ci.yaml",
        ],
    },
    Builtin {
        name: "plugin.yml (Minecraft)",
        uri: bundled::MINECRAFT_BUKKIT,
        files: &["**/src/main/resources/plugin.yml"],
    },
    Builtin {
        name: "paper-plugin.yml",
        uri: bundled::MINECRAFT_PAPER,
        files: &["**/src/main/resources/paper-plugin.yml"],
    },
    Builtin {
        name: "velocity-plugin.json",
        uri: bundled::MINECRAFT_VELOCITY,
        files: &["**/src/main/resources/velocity-plugin.json"],
    },
    Builtin {
        name: "bungee.yml",
        uri: bundled::MINECRAFT_BUNGEE,
        files: &["**/src/main/resources/bungee.yml"],
    },
    Builtin {
        name: "Unity assembly definition",
        uri: bundled::UNITY_ASMDEF,
        files: &["*.asmdef"],
    },
    Builtin {
        name: "Unity assembly definition reference",
        uri: bundled::UNITY_ASMREF,
        files: &["*.asmref"],
    },
];

/// Built-in mappings chosen by content instead of the file name: OpenAPI 3.0 / 3.1
/// documents (a top-level `openapi: 3.x` key in any JSON or YAML file).
pub static CONTENT_BUILTINS: &[(&str, &str)] = &[
    ("OpenAPI 3.0", bundled::OPENAPI_30),
    ("OpenAPI 3.1", bundled::OPENAPI_31),
];

/// The OpenAPI version of a JSON / YAML document from its top-level `openapi` key
/// (`3.0.3`, `3.1.0`), read from the first lines without parsing the whole file.
pub fn openapi_version(text: &str) -> Option<String> {
    let head = &text[..text.len().min(text.floor_char_boundary_compat(16 * 1024))];
    // YAML: `openapi: 3.0.3` at column 0.
    let yaml = head
        .lines()
        .find_map(|l| l.strip_prefix("openapi:"))
        .map(|v| v.trim().trim_matches(['"', '\'']).to_string());
    // JSON: `"openapi": "3.1.0"` anywhere in the head (first key of the root object).
    let json = || {
        let i = head.find("\"openapi\"")?;
        let rest = head[i + 9..].trim_start().strip_prefix(':')?.trim_start();
        let rest = rest.strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_string())
    };
    yaml.or_else(json).filter(|v| v.starts_with('3'))
}

trait FloorBoundary {
    fn floor_char_boundary_compat(&self, i: usize) -> usize;
}

impl FloorBoundary for str {
    fn floor_char_boundary_compat(&self, mut i: usize) -> usize {
        if i >= self.len() {
            return self.len();
        }
        while !self.is_char_boundary(i) {
            i -= 1;
        }
        i
    }
}

fn content_builtin(text: &str) -> Option<(&'static str, &'static str)> {
    let v = openapi_version(text)?;
    if v.starts_with("3.0") {
        Some(CONTENT_BUILTINS[0])
    } else if v.starts_with("3.1") {
        Some(CONTENT_BUILTINS[1])
    } else {
        None
    }
}

/// The name the status bar shows for a schema URI.
pub fn name_of(uri: &str) -> String {
    if let Some((name, _)) = CONTENT_BUILTINS
        .iter()
        .find(|(_, u)| bundled::find(uri).is_some_and(|b| b.uri() == *u))
    {
        return name.to_string();
    }
    if let Some(b) = bundled::find(uri) {
        if let Some(m) = BUILTINS.iter().find(|m| m.uri == b.uri()) {
            return m.name.to_string();
        }
    }
    let d = uri::document(uri).trim_end_matches('/');
    d.rsplit(['/', '\\']).next().unwrap_or(d).to_string()
}

// ---- Glob patterns ------------------------------------------------------------------

fn expand_braces(p: &str) -> Vec<String> {
    let Some(open) = p.find('{') else {
        return vec![p.to_string()];
    };
    let Some(close) = p[open..].find('}').map(|i| open + i) else {
        return vec![p.to_string()];
    };
    let (head, tail) = (&p[..open], &p[close + 1..]);
    p[open + 1..close]
        .split(',')
        .flat_map(|alt| expand_braces(&format!("{head}{alt}{tail}")))
        .collect()
}

fn match_here(p: &[u8], s: &[u8]) -> bool {
    if p.is_empty() {
        return s.is_empty();
    }
    match p[0] {
        b'*' if p.get(1) == Some(&b'*') => {
            // `**/` also matches no directory.
            let rest = if p.get(2) == Some(&b'/') {
                &p[3..]
            } else {
                &p[2..]
            };
            (0..=s.len()).any(|i| (i == 0 || s[i - 1] == b'/') && match_here(rest, &s[i..]))
                || (0..=s.len()).any(|i| match_here(&p[2..], &s[i..]))
        }
        b'*' => (0..=s.len())
            .take_while(|&i| i == 0 || s[i - 1] != b'/')
            .any(|i| match_here(&p[1..], &s[i..])),
        b'?' => !s.is_empty() && s[0] != b'/' && match_here(&p[1..], &s[1..]),
        b'[' => {
            let Some(end) = p.iter().position(|&c| c == b']').filter(|&e| e > 1) else {
                return !s.is_empty() && s[0] == b'[' && match_here(&p[1..], &s[1..]);
            };
            let Some(&c) = s.first() else { return false };
            let (neg, set) = match p[1] {
                b'!' | b'^' => (true, &p[2..end]),
                _ => (false, &p[1..end]),
            };
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == b'-' {
                    hit |= set[i] <= c && c <= set[i + 2];
                    i += 3;
                } else {
                    hit |= set[i] == c;
                    i += 1;
                }
            }
            hit != neg && match_here(&p[end + 1..], &s[1..])
        }
        c => !s.is_empty() && s[0] == c && match_here(&p[1..], &s[1..]),
    }
}

/// Match a project-relative path: patterns without `/` match the file name, others the
/// whole path (a leading `/` or `./` anchors at the root).
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let path = path.trim_start_matches("./").trim_start_matches('/');
    expand_braces(pattern.trim()).iter().any(|p| {
        let p = p.trim_start_matches("./").trim_start_matches('/');
        if p.is_empty() {
            return false;
        }
        if p.contains('/') {
            match_here(p.as_bytes(), path.as_bytes())
        } else {
            let name = path.rsplit('/').next().unwrap_or(path);
            match_here(p.as_bytes(), name.as_bytes())
        }
    })
}

/// Whether a user mapping's `files` entry covers `path`.
pub fn entry_matches(entry: &str, path: &str) -> bool {
    let e = entry.trim().replace('\\', "/");
    let e = e.trim_start_matches("./");
    if e.is_empty() {
        return false;
    }
    if let Some(dir) = e.strip_suffix('/') {
        let dir = dir.trim_start_matches('/');
        return dir.is_empty() || path == dir || path.starts_with(&format!("{dir}/"));
    }
    if e.contains(['*', '?', '{', '[']) {
        return glob_match(e, path);
    }
    path == e.trim_start_matches('/')
}

// ---- The SchemaStore catalog --------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CatalogEntry {
    pub name: String,
    pub description: String,
    pub url: String,
    pub file_match: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub schemas: Vec<CatalogEntry>,
}

impl Catalog {
    pub fn parse(text: &str) -> Result<Catalog, String> {
        serde_json::from_str(text).map_err(|e| format!("not a schema catalog: {e}"))
    }

    pub fn find(&self, path: &str) -> Option<&CatalogEntry> {
        self.schemas.iter().find(|e| {
            let (neg, pos): (Vec<&String>, Vec<&String>) =
                e.file_match.iter().partition(|p| p.starts_with('!'));
            pos.iter().any(|p| glob_match(p, path))
                && !neg.iter().any(|p| glob_match(&p[1..], path))
        })
    }
}

// ---- Selecting the schema of a file -------------------------------------------------

/// The schema the file names itself, resolved against the file's directory.
pub fn inline_schema(path: &str, text: &str, kind: crate::DocKind) -> Option<String> {
    let raw = match kind {
        crate::DocKind::Json => {
            let root = crate::json::parse(text)?;
            root.get("$schema")?.as_str()?.to_string()
        }
        crate::DocKind::Yaml => {
            let modeline = text
                .lines()
                .take_while(|l| {
                    l.trim().is_empty() || l.trim_start().starts_with('#') || l.starts_with("---")
                })
                .find_map(|l| {
                    let l = l.trim_start().strip_prefix('#')?.trim_start();
                    let rest = l.strip_prefix("yaml-language-server:")?.trim_start();
                    Some(rest.strip_prefix("$schema=")?.trim().to_string())
                });
            match modeline {
                Some(m) => m,
                None => crate::yaml::parse(text)
                    .first()?
                    .get("$schema")?
                    .as_str()?
                    .to_string(),
            }
        }
    };
    let raw = raw.trim();
    // A schema file names its meta-schema: that is not a mapping for its content.
    if raw.is_empty() || raw.contains("json-schema.org/") {
        return None;
    }
    if uri::is_absolute(raw) {
        return Some(raw.to_string());
    }
    if std::path::Path::new(raw).is_absolute() {
        return Some(format!("file://{}", raw.replace('\\', "/")));
    }
    Some(uri::resolve(&uri::project(path), raw))
}

/// The URI of a user mapping's schema.
pub fn mapping_uri(schema: &str) -> String {
    let s = schema.trim();
    if s.is_empty() || uri::is_absolute(s) {
        return s.to_string();
    }
    if std::path::Path::new(s).is_absolute() || s.starts_with('/') {
        return format!("file://{}", s.replace('\\', "/"));
    }
    uri::project(&s.replace('\\', "/"))
}

pub fn select(
    path: &str,
    text: &str,
    kind: crate::DocKind,
    user: &[UserMapping],
    catalog: Option<&Catalog>,
) -> Option<Selection> {
    let path = path.trim_start_matches("./");
    for m in user {
        if m.files.lines().any(|f| entry_matches(f, path)) {
            let uri = mapping_uri(&m.schema);
            return Some(Selection {
                name: if m.name.trim().is_empty() {
                    if uri.is_empty() {
                        "No JSON schema".into()
                    } else {
                        name_of(&uri)
                    }
                } else {
                    m.name.trim().to_string()
                },
                uri,
                source: Source::User,
                version: Draft::from_setting(&m.version),
            });
        }
    }
    if let Some(uri) = inline_schema(path, text, kind) {
        return Some(Selection {
            name: name_of(&uri),
            uri,
            source: Source::Inline,
            version: None,
        });
    }
    if let Some(b) = BUILTINS
        .iter()
        .find(|b| b.files.iter().any(|f| glob_match(f, path)))
    {
        return Some(Selection {
            name: b.name.to_string(),
            uri: b.uri.to_string(),
            source: Source::Builtin,
            version: None,
        });
    }
    if let Some((name, uri)) = content_builtin(text) {
        return Some(Selection {
            name: name.to_string(),
            uri: uri.to_string(),
            source: Source::Builtin,
            version: None,
        });
    }
    let entry = catalog?.find(path)?;
    Some(Selection {
        name: entry.name.clone(),
        uri: entry.url.clone(),
        source: Source::Catalog,
        version: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocKind;

    #[test]
    fn globs() {
        assert!(glob_match("composer.json", "packages/a/composer.json"));
        assert!(glob_match(
            "**/.github/workflows/*.yml",
            ".github/workflows/ci.yml"
        ));
        assert!(glob_match(
            "**/.github/workflows/*.yml",
            "sub/.github/workflows/ci.yml"
        ));
        assert!(!glob_match(
            "**/.github/workflows/*.yml",
            ".github/workflows/x/ci.yml"
        ));
        assert!(glob_match("tsconfig.*.json", "tsconfig.app.json"));
        assert!(glob_match("*.{yml,yaml}", "a/b.yaml"));
        assert!(glob_match("config/*.json", "config/a.json"));
        assert!(!glob_match("config/*.json", "x/config/a.json"));
        assert!(glob_match("conf[0-9].json", "conf1.json"));
        assert!(entry_matches("config/", "config/a/b.json"));
        assert!(entry_matches("src/app.json", "src/app.json"));
        assert!(!entry_matches("app.json", "src/app.json"));
    }

    #[test]
    fn selection_order() {
        let user = vec![UserMapping {
            name: "Mine".into(),
            schema: "schemas/mine.json".into(),
            version: "7".into(),
            files: "config/\n*.mine.json".into(),
        }];
        let s = select("config/package.json", "{}", DocKind::Json, &user, None).unwrap();
        assert_eq!(
            (s.name.as_str(), s.uri.as_str(), s.source),
            ("Mine", "project:/schemas/mine.json", Source::User)
        );
        assert_eq!(s.version, Some(Draft::D7));
        let s = select("package.json", "{}", DocKind::Json, &user, None).unwrap();
        assert_eq!(
            (s.name.as_str(), s.source),
            ("package.json", Source::Builtin)
        );
        let s = select(
            "app/package.json",
            r#"{"$schema": "../s.json"}"#,
            DocKind::Json,
            &user,
            None,
        )
        .unwrap();
        assert_eq!(
            (s.uri.as_str(), s.source),
            ("project:/s.json", Source::Inline)
        );
        let s = select("x.yml", "# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json\non: push\n", DocKind::Yaml, &[], None).unwrap();
        assert_eq!(s.name, "GitHub Workflow");
        assert_eq!(
            select(".gitlab-ci.yml", "", DocKind::Yaml, &[], None)
                .unwrap()
                .name,
            "GitLab CI"
        );
        assert!(select("random.json", "{}", DocKind::Json, &[], None).is_none());
        let off = vec![UserMapping {
            files: "package.json".into(),
            ..Default::default()
        }];
        let s = select("package.json", "{}", DocKind::Json, &off, None).unwrap();
        assert_eq!((s.uri.as_str(), s.name.as_str()), ("", "No JSON schema"));
        // A schema file's meta-schema is not a mapping.
        assert!(select(
            "s.json",
            r#"{"$schema": "http://json-schema.org/draft-07/schema#"}"#,
            DocKind::Json,
            &[],
            None
        )
        .is_none());
    }

    #[test]
    fn catalog_lookup() {
        let c = Catalog::parse(
            r#"{"schemas": [
                {"name": "Renovate", "url": "https://docs.renovatebot.com/renovate-schema.json", "fileMatch": ["renovate.json", "!**/x/renovate.json"]},
                {"name": "Other", "url": "https://x/o.json"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(c.find("renovate.json").unwrap().name, "Renovate");
        assert!(c.find("x/renovate.json").is_none());
        let s = select("renovate.json", "{}", DocKind::Json, &[], Some(&c)).unwrap();
        assert_eq!(s.source, Source::Catalog);
    }
}
