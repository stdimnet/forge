//! File icon themes.
//!
//! An icon theme maps file and folder names to SVG icons. Plugins contribute one with an
//! `icon-theme.json` file (see [`IconThemeDef`]) next to their SVGs; the user picks the
//! active theme in Settings | Appearance & Behavior | File Icons and can add their own
//! associations there. Every place that shows a file icon asks the [`IconResolver`] of the
//! active theme first and falls back to Forge's built-in icons when it has no answer.
//!
//! ```json
//! {
//!   "icons": { "file": "icons/file.svg", "php": "icons/php.svg", "folder": "icons/folder.svg" },
//!   "file": "file",
//!   "folder": "folder",
//!   "fileExtensions": { "php": "php", "blade.php": "blade" },
//!   "fileNames": { "composer.json": "composer" },
//!   "filePatterns": [{ "pattern": "*.test.ts", "icon": "test" }],
//!   "fileRegex": [{ "pattern": "^\\.env(\\..+)?$", "icon": "env" }],
//!   "folderNames": { "src": "folder-src" },
//!   "folderNamesExpanded": { "src": "folder-src-open" },
//!   "options": { "treeArrows": "triangle", "accentColor": "#f0a050", "monochrome": false, "iconSize": 16 }
//! }
//! ```
//!
//! Matching order for files: the user's associations, then file names (exact, then ignoring
//! case), patterns, regular expressions, extensions (the longest compound extension first:
//! `blade.php` before `php`) and finally the theme's default file icon. Folders: the user's
//! folder associations, the expanded or closed folder names, then the default folder icons.
//! The resolver caches the answer per path.

use std::collections::{BTreeMap, HashMap};

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// Most icons a theme may declare.
pub const MAX_ICONS: usize = 5000;
/// Largest SVG file a theme may ship.
pub const MAX_SVG_BYTES: usize = 128 * 1024;

/// The `icon-theme.json` of an icon theme.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IconThemeDef {
    /// Icon id → SVG file, relative to the theme file.
    #[serde(default)]
    pub icons: BTreeMap<String, String>,
    /// Default icon of files nothing else matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Default icon of closed folders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Default icon of expanded folders (else `folder`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_expanded: Option<String>,
    /// Exact file names (`composer.json`).
    #[serde(default)]
    pub file_names: BTreeMap<String, String>,
    /// Extensions without the leading dot, compound ones too (`blade.php`).
    #[serde(default)]
    pub file_extensions: BTreeMap<String, String>,
    /// Glob patterns (`*.test.ts`, `docker-compose*.yml`), tried in order. A pattern with a
    /// `/` is matched against the whole path (`**/tests/*.php`).
    #[serde(default)]
    pub file_patterns: Vec<Rule>,
    /// Regular expressions on the file name, tried in order.
    #[serde(default)]
    pub file_regex: Vec<Rule>,
    /// Closed folders by name.
    #[serde(default)]
    pub folder_names: BTreeMap<String, String>,
    /// Expanded folders by name (else the closed icon).
    #[serde(default)]
    pub folder_names_expanded: BTreeMap<String, String>,
    #[serde(default)]
    pub options: IconThemeOptions,
}

/// A pattern or regular expression and its icon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub pattern: String,
    pub icon: String,
}

/// How a theme wants the trees around its icons to look. The user's settings override
/// each of them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IconThemeOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree_arrows: Option<TreeArrows>,
    /// `#rgb` or `#rrggbb`: folders are drawn in this color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
    /// Draw every icon in one color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monochrome: Option<bool>,
    /// Icon size in pixels in trees and tabs (8 to 32).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<u32>,
}

/// The expand arrows of trees (Project view, Structure, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TreeArrows {
    Chevron,
    Triangle,
    None,
}

/// A user association (Settings | File Icons), stored in `ide.iconTheme.associations`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Association {
    pub kind: AssociationKind,
    pub value: String,
    /// An icon id of the active theme.
    pub icon: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssociationKind {
    FileName,
    Extension,
    Pattern,
    Regex,
    FolderName,
}

/// What is shown: a file, a closed or an expanded folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IconKind {
    File,
    Folder,
    FolderOpen,
}

impl IconKind {
    /// `0` file, `1` folder, `2` expanded folder (the WebAssembly calling convention).
    pub fn from_code(code: u8) -> Self {
        match code {
            1 => IconKind::Folder,
            2 => IconKind::FolderOpen,
            _ => IconKind::File,
        }
    }
}

/// One problem of a theme definition or SVG.
pub type Problem = String;

impl IconThemeDef {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("invalid icon theme: {e}"))
    }

    /// Check paths, icon references, patterns and options.
    pub fn validate(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        if self.icons.is_empty() {
            out.push("icons: declares no icons".into());
        }
        if self.icons.len() > MAX_ICONS {
            out.push(format!("icons: more than {MAX_ICONS} icons"));
        }
        for (id, path) in &self.icons {
            if id.is_empty() {
                out.push("icons: an icon id is empty".into());
            }
            if !is_svg_path(path) {
                out.push(format!(
                    "icons.{id}: `{path}` must be a relative `.svg` path inside the plugin"
                ));
            }
        }
        let mut known = |field: String, icon: &str| {
            if !self.icons.contains_key(icon) {
                out.push(format!("{field}: unknown icon `{icon}`"));
            }
        };
        for (field, icon) in [
            ("file", &self.file),
            ("folder", &self.folder),
            ("folderExpanded", &self.folder_expanded),
        ] {
            if let Some(icon) = icon {
                known(field.into(), icon);
            }
        }
        for (field, map) in [
            ("fileNames", &self.file_names),
            ("fileExtensions", &self.file_extensions),
            ("folderNames", &self.folder_names),
            ("folderNamesExpanded", &self.folder_names_expanded),
        ] {
            for (key, icon) in map {
                known(format!("{field}.{key}"), icon);
            }
        }
        for (field, rules) in [
            ("filePatterns", &self.file_patterns),
            ("fileRegex", &self.file_regex),
        ] {
            for (i, r) in rules.iter().enumerate() {
                known(format!("{field}[{i}].icon"), &r.icon);
            }
        }
        for (i, r) in self.file_patterns.iter().enumerate() {
            if r.pattern.trim().is_empty() {
                out.push(format!("filePatterns[{i}].pattern: must not be empty"));
            }
        }
        for (i, r) in self.file_regex.iter().enumerate() {
            if let Err(e) = Regex::new(&r.pattern) {
                out.push(format!("fileRegex[{i}].pattern: {e}"));
            }
        }
        let o = &self.options;
        if let Some(c) = &o.accent_color {
            if !is_hex_color(c) {
                out.push("options.accentColor: must be `#rgb` or `#rrggbb`".into());
            }
        }
        if let Some(size) = o.icon_size {
            if !(8..=32).contains(&size) {
                out.push("options.iconSize: must be 8 to 32".into());
            }
        }
        out
    }
}

/// A relative path inside a package that ends in `.svg` (no `..`, no absolute paths).
pub fn is_svg_path(path: &str) -> bool {
    !path.is_empty()
        && path.to_ascii_lowercase().ends_with(".svg")
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

pub fn is_hex_color(c: &str) -> bool {
    let hex = c.strip_prefix('#').unwrap_or("");
    matches!(hex.len(), 3 | 6) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Check an icon's SVG: a reasonable size, an `<svg>` root, and nothing that runs or loads
/// anything (scripts, event handlers, foreign objects, external references). Forge draws
/// theme icons as images, which never run scripts; this keeps packages honest anyway.
pub fn check_svg(svg: &str) -> Result<(), Problem> {
    if svg.len() > MAX_SVG_BYTES {
        return Err(format!("larger than {} KiB", MAX_SVG_BYTES / 1024));
    }
    let lower = svg.to_ascii_lowercase();
    if !lower.contains("<svg") {
        return Err("is not an SVG image".into());
    }
    for (needle, what) in [
        ("<script", "a script"),
        ("<foreignobject", "a foreignObject"),
        ("javascript:", "a javascript: URL"),
        ("<iframe", "an iframe"),
    ] {
        if lower.contains(needle) {
            return Err(format!("contains {what}"));
        }
    }
    if Regex::new(r#"\son[a-z]+\s*="#)
        .expect("valid regex")
        .is_match(&lower)
    {
        return Err("contains an event handler attribute".into());
    }
    if Regex::new(r#"href\s*=\s*["']\s*(?:https?:|//|data:)"#)
        .expect("valid regex")
        .is_match(&lower)
    {
        return Err("references an external resource".into());
    }
    Ok(())
}

/// Match `name` against a glob: `*` (anything but `/`), `**` (anything), `?` (one
/// character), `[abc]` / `[a-z]` (one of them). Everything else is literal.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    fn rec(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') if p.get(1) == Some(&'*') => {
                let after = &p[2..];
                // `**/` also matches no folder at all.
                (after.first() == Some(&'/') && rec(&after[1..], s))
                    || (0..=s.len()).any(|i| rec(after, &s[i..]))
            }
            Some('*') => {
                let mut i = 0;
                loop {
                    if rec(&p[1..], &s[i..]) {
                        return true;
                    }
                    if i == s.len() || s[i] == '/' {
                        return false;
                    }
                    i += 1;
                }
            }
            Some('?') => !s.is_empty() && s[0] != '/' && rec(&p[1..], &s[1..]),
            Some('[') => {
                let Some(end) = p.iter().skip(1).position(|&c| c == ']').map(|i| i + 1) else {
                    return !s.is_empty() && s[0] == '[' && rec(&p[1..], &s[1..]);
                };
                let Some(&c) = s.first() else { return false };
                let set = &p[1..end];
                let (negate, set) = match set.first() {
                    Some('!') | Some('^') => (true, &set[1..]),
                    _ => (false, set),
                };
                let mut hit = false;
                let mut i = 0;
                while i < set.len() {
                    if i + 2 < set.len() && set[i + 1] == '-' {
                        hit |= set[i] <= c && c <= set[i + 2];
                        i += 3;
                    } else {
                        hit |= set[i] == c;
                        i += 1;
                    }
                }
                hit != negate && rec(&p[end + 1..], &s[1..])
            }
            Some(&c) => s.first() == Some(&c) && rec(&p[1..], &s[1..]),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = name.chars().collect();
    rec(&p, &s)
}

enum Matcher {
    Name(String),
    Extension(String),
    Glob(String),
    Regex(Regex),
    Folder(String),
}

/// Resolves paths to icon ids of one theme plus the user's associations, caching per path.
pub struct IconResolver {
    theme: IconThemeDef,
    associations: Vec<(Matcher, String)>,
    regexes: Vec<(Regex, String)>,
    names_lower: HashMap<String, String>,
    extensions_lower: HashMap<String, String>,
    folders_lower: HashMap<String, String>,
    folders_open_lower: HashMap<String, String>,
    cache: HashMap<(String, IconKind), Option<String>>,
}

fn lower_map(map: &BTreeMap<String, String>) -> HashMap<String, String> {
    map.iter()
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect()
}

/// The last segment of a `/` or `\` separated path.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

impl IconResolver {
    /// A resolver for `theme`. Associations whose regex does not compile or whose icon the
    /// theme lacks are skipped and reported.
    pub fn new(theme: IconThemeDef, associations: &[Association]) -> (Self, Vec<Problem>) {
        let mut problems = Vec::new();
        let mut assoc = Vec::new();
        for (i, a) in associations.iter().enumerate() {
            if !theme.icons.contains_key(&a.icon) {
                problems.push(format!(
                    "associations[{i}]: the theme has no icon `{}`",
                    a.icon
                ));
                continue;
            }
            let value = a.value.trim();
            if value.is_empty() {
                problems.push(format!("associations[{i}]: the value is empty"));
                continue;
            }
            let m = match a.kind {
                AssociationKind::FileName => Matcher::Name(value.to_lowercase()),
                AssociationKind::Extension => {
                    Matcher::Extension(value.trim_start_matches('.').to_lowercase())
                }
                AssociationKind::Pattern => Matcher::Glob(value.to_string()),
                AssociationKind::FolderName => Matcher::Folder(value.to_lowercase()),
                AssociationKind::Regex => match Regex::new(value) {
                    Ok(r) => Matcher::Regex(r),
                    Err(e) => {
                        problems.push(format!("associations[{i}]: {e}"));
                        continue;
                    }
                },
            };
            assoc.push((m, a.icon.clone()));
        }
        let regexes = theme
            .file_regex
            .iter()
            .filter_map(|r| Some((Regex::new(&r.pattern).ok()?, r.icon.clone())))
            .collect();
        let resolver = IconResolver {
            names_lower: lower_map(&theme.file_names),
            extensions_lower: lower_map(&theme.file_extensions),
            folders_lower: lower_map(&theme.folder_names),
            folders_open_lower: lower_map(&theme.folder_names_expanded),
            associations: assoc,
            regexes,
            theme,
            cache: HashMap::new(),
        };
        (resolver, problems)
    }

    pub fn theme(&self) -> &IconThemeDef {
        &self.theme
    }

    /// Number of cached paths.
    pub fn cached(&self) -> usize {
        self.cache.len()
    }

    /// The icon id for `path` (a path or just a file name), or `None` for Forge's own icon.
    pub fn resolve(&mut self, path: &str, kind: IconKind) -> Option<String> {
        let key = (path.to_string(), kind);
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let icon = self
            .compute(path, kind)
            .filter(|id| self.theme.icons.contains_key(id));
        self.cache.insert(key, icon.clone());
        icon
    }

    fn compute(&self, path: &str, kind: IconKind) -> Option<String> {
        let name = file_name(path);
        let lower = name.to_lowercase();
        if kind != IconKind::File {
            for (m, icon) in &self.associations {
                if matches!(m, Matcher::Folder(n) if *n == lower) {
                    return Some(icon.clone());
                }
            }
            let t = &self.theme;
            if kind == IconKind::FolderOpen {
                if let Some(icon) = t
                    .folder_names_expanded
                    .get(name)
                    .or_else(|| self.folders_open_lower.get(&lower))
                {
                    return Some(icon.clone());
                }
            }
            if let Some(icon) = t
                .folder_names
                .get(name)
                .or_else(|| self.folders_lower.get(&lower))
            {
                return Some(icon.clone());
            }
            return if kind == IconKind::FolderOpen {
                t.folder_expanded.clone().or_else(|| t.folder.clone())
            } else {
                t.folder.clone()
            };
        }
        let extensions = extensions_of(&lower);
        for (m, icon) in &self.associations {
            let hit = match m {
                Matcher::Name(n) => *n == lower,
                Matcher::Extension(e) => extensions.iter().any(|x| x == e),
                Matcher::Glob(g) => glob_on(g, path, name),
                Matcher::Regex(r) => r.is_match(name),
                Matcher::Folder(_) => false,
            };
            if hit {
                return Some(icon.clone());
            }
        }
        let t = &self.theme;
        if let Some(icon) = t
            .file_names
            .get(name)
            .or_else(|| self.names_lower.get(&lower))
        {
            return Some(icon.clone());
        }
        if let Some(r) = t
            .file_patterns
            .iter()
            .find(|r| glob_on(&r.pattern, path, name))
        {
            return Some(r.icon.clone());
        }
        if let Some((_, icon)) = self.regexes.iter().find(|(r, _)| r.is_match(name)) {
            return Some(icon.clone());
        }
        for ext in &extensions {
            if let Some(icon) = t
                .file_extensions
                .get(ext.as_str())
                .or_else(|| self.extensions_lower.get(ext.as_str()))
            {
                return Some(icon.clone());
            }
        }
        t.file.clone()
    }
}

/// Patterns with a `/` match the whole path (separators normalized), others the file name.
fn glob_on(pattern: &str, path: &str, name: &str) -> bool {
    if pattern.contains('/') {
        glob_match(pattern, &path.replace('\\', "/"))
    } else {
        glob_match(pattern, name)
    }
}

/// The extensions of a file name, longest first: `a.blade.php` → `blade.php`, `php`;
/// `.env.local` → `env.local`, `local`.
fn extensions_of(name: &str) -> Vec<String> {
    name.char_indices()
        .filter(|&(_, c)| c == '.')
        .map(|(i, _)| name[i + 1..].to_string())
        .filter(|e| !e.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> IconThemeDef {
        IconThemeDef::parse(
            r##"{
              "icons": {
                "file": "icons/file.svg", "php": "icons/php.svg", "blade": "icons/blade.svg",
                "composer": "icons/composer.svg", "test": "icons/test.svg", "env": "icons/env.svg",
                "folder": "icons/folder.svg", "folder-open": "icons/folder-open.svg",
                "src": "icons/src.svg", "src-open": "icons/src-open.svg", "docker": "icons/docker.svg",
                "tests-php": "icons/tests-php.svg"
              },
              "file": "file",
              "folder": "folder",
              "folderExpanded": "folder-open",
              "fileNames": { "composer.json": "composer", "Dockerfile": "docker" },
              "fileExtensions": { "php": "php", "blade.php": "blade" },
              "filePatterns": [
                { "pattern": "*.test.ts", "icon": "test" },
                { "pattern": "**/tests/*.php", "icon": "tests-php" }
              ],
              "fileRegex": [{ "pattern": "^\\.env(\\..+)?$", "icon": "env" }],
              "folderNames": { "src": "src" },
              "folderNamesExpanded": { "src": "src-open" },
              "options": { "treeArrows": "triangle", "accentColor": "#f0a050", "monochrome": false, "iconSize": 16 }
            }"##,
        )
        .unwrap()
    }

    fn resolver(assoc: &[Association]) -> IconResolver {
        IconResolver::new(theme(), assoc).0
    }

    #[test]
    fn a_valid_theme_has_no_problems() {
        assert_eq!(theme().validate(), Vec::<String>::new());
        assert_eq!(theme().options.tree_arrows, Some(TreeArrows::Triangle));
    }

    #[test]
    fn validation_reports_paths_references_patterns_and_options() {
        let mut t = theme();
        t.icons.insert("evil".into(), "../outside.svg".into());
        t.icons.insert("png".into(), "icons/x.png".into());
        t.file_names.insert("x".into(), "missing".into());
        t.file_regex.push(Rule {
            pattern: "(".into(),
            icon: "php".into(),
        });
        t.options.accent_color = Some("orange".into());
        t.options.icon_size = Some(64);
        let problems = t.validate().join("\n");
        assert!(problems.contains("icons.evil"), "{problems}");
        assert!(problems.contains("icons.png"), "{problems}");
        assert!(problems.contains("fileNames.x: unknown icon `missing`"));
        assert!(problems.contains("fileRegex[1].pattern"));
        assert!(problems.contains("options.accentColor"));
        assert!(problems.contains("options.iconSize"));
        assert!(IconThemeDef::parse(r#"{"icons":{},"bogus":1}"#).is_err());
    }

    #[test]
    fn files_match_names_patterns_regex_and_extensions_in_order() {
        let mut r = resolver(&[]);
        let f = |r: &mut IconResolver, p: &str| r.resolve(p, IconKind::File);
        assert_eq!(f(&mut r, "composer.json").as_deref(), Some("composer"));
        assert_eq!(f(&mut r, "COMPOSER.JSON").as_deref(), Some("composer"));
        assert_eq!(f(&mut r, "app/Dockerfile").as_deref(), Some("docker"));
        assert_eq!(f(&mut r, "src/app.test.ts").as_deref(), Some("test"));
        assert_eq!(
            f(&mut r, "pkg/tests/UserTest.php").as_deref(),
            Some("tests-php")
        );
        assert_eq!(f(&mut r, ".env.local").as_deref(), Some("env"));
        assert_eq!(f(&mut r, "views/home.blade.php").as_deref(), Some("blade"));
        assert_eq!(f(&mut r, "src/User.PHP").as_deref(), Some("php"));
        assert_eq!(f(&mut r, r"C:\work\index.php").as_deref(), Some("php"));
        assert_eq!(f(&mut r, "README").as_deref(), Some("file"));
    }

    #[test]
    fn folders_use_expanded_names_and_defaults() {
        let mut r = resolver(&[]);
        assert_eq!(r.resolve("src", IconKind::Folder).as_deref(), Some("src"));
        assert_eq!(
            r.resolve("a/SRC", IconKind::FolderOpen).as_deref(),
            Some("src-open")
        );
        assert_eq!(
            r.resolve("vendor", IconKind::Folder).as_deref(),
            Some("folder")
        );
        assert_eq!(
            r.resolve("vendor", IconKind::FolderOpen).as_deref(),
            Some("folder-open")
        );
    }

    #[test]
    fn user_associations_come_first() {
        let assoc = [
            Association {
                kind: AssociationKind::Extension,
                value: ".json".into(),
                icon: "env".into(),
            },
            Association {
                kind: AssociationKind::FileName,
                value: "Makefile".into(),
                icon: "docker".into(),
            },
            Association {
                kind: AssociationKind::Pattern,
                value: "*.spec.php".into(),
                icon: "test".into(),
            },
            Association {
                kind: AssociationKind::Regex,
                value: "^draft-".into(),
                icon: "blade".into(),
            },
            Association {
                kind: AssociationKind::FolderName,
                value: "app".into(),
                icon: "src".into(),
            },
        ];
        let mut r = resolver(&assoc);
        assert_eq!(
            r.resolve("composer.json", IconKind::File).as_deref(),
            Some("env")
        );
        assert_eq!(
            r.resolve("makefile", IconKind::File).as_deref(),
            Some("docker")
        );
        assert_eq!(
            r.resolve("UserTest.spec.php", IconKind::File).as_deref(),
            Some("test")
        );
        assert_eq!(
            r.resolve("draft-notes.md", IconKind::File).as_deref(),
            Some("blade")
        );
        assert_eq!(
            r.resolve("app", IconKind::FolderOpen).as_deref(),
            Some("src")
        );
    }

    #[test]
    fn bad_associations_are_skipped_and_reported() {
        let assoc = [
            Association {
                kind: AssociationKind::Regex,
                value: "(".into(),
                icon: "php".into(),
            },
            Association {
                kind: AssociationKind::FileName,
                value: "x".into(),
                icon: "nope".into(),
            },
            Association {
                kind: AssociationKind::FileName,
                value: " ".into(),
                icon: "php".into(),
            },
        ];
        let (mut r, problems) = IconResolver::new(theme(), &assoc);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert_eq!(r.resolve("x", IconKind::File).as_deref(), Some("file"));
    }

    #[test]
    fn results_are_cached_per_path_and_unknown_ids_fall_back() {
        let mut t = theme();
        t.file_names.insert("ghost.txt".into(), "ghost".into());
        let (mut r, _) = IconResolver::new(t, &[]);
        assert_eq!(r.resolve("ghost.txt", IconKind::File), None);
        r.resolve("a.php", IconKind::File);
        r.resolve("a.php", IconKind::File);
        r.resolve("a.php", IconKind::Folder);
        assert_eq!(r.cached(), 3);

        let mut bare = IconThemeDef::default();
        bare.icons.insert("php".into(), "php.svg".into());
        bare.file_extensions.insert("php".into(), "php".into());
        let (mut r, _) = IconResolver::new(bare, &[]);
        assert_eq!(
            r.resolve("x.txt", IconKind::File),
            None,
            "no default: Forge's icon"
        );
        assert_eq!(r.resolve("src", IconKind::Folder), None);
        assert_eq!(r.resolve("x.php", IconKind::File).as_deref(), Some("php"));
    }

    #[test]
    fn globs() {
        assert!(glob_match("*.ts", "a.ts"));
        assert!(!glob_match("*.ts", "dir/a.ts"));
        assert!(glob_match("**/*.ts", "dir/sub/a.ts"));
        assert!(glob_match("**/*.ts", "a.ts"));
        assert!(glob_match("docker-compose*.yml", "docker-compose.prod.yml"));
        assert!(glob_match("file?.txt", "file1.txt"));
        assert!(glob_match("[Mm]akefile", "makefile"));
        assert!(glob_match("*.[ch]", "x.h"));
        assert!(!glob_match("*.[!ch]", "x.h"));
        assert!(glob_match("v[0-9].md", "v7.md"));
        assert!(!glob_match("*.ts", "a.tsx"));
    }

    #[test]
    fn svgs_are_checked() {
        assert!(check_svg(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16"/></svg>"#).is_ok());
        assert!(check_svg("<svg><script>alert(1)</script></svg>").is_err());
        assert!(check_svg(r#"<svg onload="x()"></svg>"#).is_err());
        assert!(check_svg(r#"<svg><image href="https://evil.example/x.png"/></svg>"#).is_err());
        assert!(check_svg(r#"<svg><foreignObject/></svg>"#).is_err());
        assert!(check_svg("not an image").is_err());
        assert!(check_svg(&format!("<svg>{}</svg>", " ".repeat(MAX_SVG_BYTES))).is_err());
        assert!(is_svg_path("icons/a.svg") && !is_svg_path("/a.svg") && !is_svg_path("a/../b.svg"));
    }
}
