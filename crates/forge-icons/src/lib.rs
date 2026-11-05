//! Icon themes.
//!
//! An icon theme maps file and folder names, code symbols and UI places to SVG icons.
//! Plugins contribute one with an `icon-theme.json` file (see [`IconThemeDef`]) next to
//! their SVGs; the user picks the active theme in Settings | Appearance & Behavior | File
//! Icons and can add their own associations there. Every place that shows an icon asks the
//! [`IconResolver`] of the active theme first and falls back to Forge's built-in (lucide)
//! icons when it has no answer.
//!
//! ```json
//! {
//!   "version": 2,
//!   "icons": { "file": "icons/file.svg", "php": "icons/php.svg", "folder": "icons/folder.svg" },
//!   "iconsLight": { "php": "icons/light/php.svg" },
//!   "file": "file",
//!   "folder": "folder",
//!   "rootFolder": "folder-root",
//!   "fileExtensions": { "php": "php", "blade.php": "blade" },
//!   "fileNames": { "composer.json": "composer" },
//!   "fileTypes": { "shell": "shell" },
//!   "pathRules": [{ "pattern": "Controllers/*.php", "icon": "controller" }],
//!   "filePatterns": [{ "pattern": "*.test.ts", "icon": "test" }],
//!   "fileRegex": [{ "pattern": "^\\.env(\\..+)?$", "icon": "env" }],
//!   "folderNames": { "src": "folder-src" },
//!   "folderNamesExpanded": { "src": "folder-src-open" },
//!   "symbols": { "class": "sym-class", "method": "sym-method" },
//!   "ui": { "toolwindow.project": "ui-project", "action.run": "ui-run" },
//!   "options": { "treeArrows": "plusMinus", "accentColor": "#f0a050", "hollowFolders": true }
//! }
//! ```
//!
//! Matching order for files: the user's associations, path rules (relative to the project
//! root), file names (exact, then ignoring case), patterns, regular expressions, extensions
//! (the longest compound extension first: `blade.php` before `php`), the detected file type
//! and finally the theme's default file icon. Folders: the user's folder associations, the
//! root folder icons (for the project root), the expanded or closed folder names, then the
//! default folder icons. The light variant (`iconsLight` or `<id>_light`) replaces an icon
//! while a light UI theme is active. The resolver caches the answer per path.

use std::collections::BTreeMap;

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

pub mod folders;
pub mod ids;
mod resolver;
pub mod tint;

pub use folders::{folders_with_open_files, FolderColor, FolderColors};
pub use ids::{SYMBOL_KINDS, UI_ICONS, UI_PREFIXES};
pub use resolver::{IconResolver, Lookup, SymbolIcon, SymbolModifiers};
pub use tint::{has_tint_layer, tint_svg};

/// Most icons a theme may declare.
pub const MAX_ICONS: usize = 5000;
/// Largest SVG file a theme may ship.
pub const MAX_SVG_BYTES: usize = 128 * 1024;
/// The newest `icon-theme.json` format this Forge reads. Format 1 (no `version`) is the
/// format of Forge 0.x's first icon themes; format 2 adds `iconsLight`, `rootFolder`,
/// `rootFolderExpanded`, `folderHollow`, `folderHollowExpanded`, `fileTypes`, `pathRules`,
/// `symbols`, `ui`, the `plusMinus` and `arrow` tree arrows and `options.hollowFolders`.
/// Format 1 files keep working unchanged.
pub const FORMAT_VERSION: u32 = 2;
/// Suffix of an icon id's light variant (`php_light`).
pub const LIGHT_SUFFIX: &str = "_light";
/// Suffix of a folder icon's hollow variant (`folder-src_hollow`).
pub const HOLLOW_SUFFIX: &str = "_hollow";

/// The `icon-theme.json` of an icon theme.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IconThemeDef {
    /// The format version ([`FORMAT_VERSION`]); absent means 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Icon id → SVG file, relative to the theme file.
    #[serde(default)]
    pub icons: BTreeMap<String, String>,
    /// Icon id → SVG file of its light variant (used with light UI themes). An icon
    /// `<id>_light` in `icons` does the same.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub icons_light: BTreeMap<String, String>,
    /// Default icon of files nothing else matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Default icon of closed folders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Default icon of expanded folders (else `folder`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_expanded: Option<String>,
    /// The project root in the Project view, closed (else the folder icons).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_folder: Option<String>,
    /// The expanded project root (else `rootFolder`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_folder_expanded: Option<String>,
    /// Hollow folders (folders that hold open files, with the hollow option on) whose icon
    /// has no `<id>_hollow` variant and is the default folder icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_hollow: Option<String>,
    /// The expanded hollow folder (else `folderHollow`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_hollow_expanded: Option<String>,
    /// Exact file names (`composer.json`).
    #[serde(default)]
    pub file_names: BTreeMap<String, String>,
    /// Extensions without the leading dot, compound ones too (`blade.php`).
    #[serde(default)]
    pub file_extensions: BTreeMap<String, String>,
    /// File type ids (`forge-filetypes`: `php`, `shell`, `python`, …) → icon, for files no
    /// name or extension rule matched: extensionless scripts detected by their `#!` line.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub file_types: BTreeMap<String, String>,
    /// Path globs relative to the project root (`Controllers/*.php`; a leading `/` anchors
    /// a rule at the root, else it matches at any folder depth), tried in order before the
    /// file names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_rules: Vec<Rule>,
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
    /// Stable symbol kinds ([`ids::SYMBOL_KINDS`]) → icon.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub symbols: BTreeMap<String, String>,
    /// Stable UI icon ids ([`ids::UI_ICONS`]) → icon.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ui: BTreeMap<String, String>,
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
    /// `#rgb` or `#rrggbb`: folders are drawn in this color (their tint layer, or the whole
    /// icon without one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
    /// Draw every icon in one color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monochrome: Option<bool>,
    /// Icon size in pixels in trees and tabs (8 to 32).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<u32>,
    /// Folders that hold open files get their hollow variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hollow_folders: Option<bool>,
}

/// The expand arrows of trees (Project view, Structure, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TreeArrows {
    Chevron,
    Triangle,
    /// Boxed plus and minus signs.
    PlusMinus,
    /// Straight arrows (→ closed, ↓ open).
    Arrow,
    None,
}

/// A user association (Settings | File Icons), stored in `fileIcons.associations`.
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IconKind {
    #[default]
    File,
    Folder,
    FolderOpen,
}

impl IconKind {
    /// `0` file, `1` folder, `2` expanded folder (the WebAssembly calling convention; the
    /// higher bits are [`Lookup`] flags).
    pub fn from_code(code: u8) -> Self {
        match code & 3 {
            1 => IconKind::Folder,
            2 => IconKind::FolderOpen,
            _ => IconKind::File,
        }
    }

    pub fn is_folder(self) -> bool {
        self != IconKind::File
    }
}

/// One problem of a theme definition or SVG.
pub type Problem = String;

impl IconThemeDef {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("invalid icon theme: {e}"))
    }

    /// The format version, 1 when the theme does not say.
    pub fn format_version(&self) -> u32 {
        self.version.unwrap_or(1)
    }

    /// Check the version, paths, icon references, patterns and options.
    pub fn validate(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        match self.version {
            Some(0) => out.push("version: must be 1 or higher".into()),
            Some(v) if v > FORMAT_VERSION => out.push(format!(
                "version: format {v} needs a newer Forge (this one reads up to {FORMAT_VERSION})"
            )),
            _ => {}
        }
        if self.icons.is_empty() {
            out.push("icons: declares no icons".into());
        }
        if self.icons.len() + self.icons_light.len() > MAX_ICONS {
            out.push(format!("icons: more than {MAX_ICONS} icons"));
        }
        for (field, map) in [("icons", &self.icons), ("iconsLight", &self.icons_light)] {
            for (id, path) in map {
                if id.is_empty() {
                    out.push(format!("{field}: an icon id is empty"));
                }
                if !is_svg_path(path) {
                    out.push(format!(
                        "{field}.{id}: `{path}` must be a relative `.svg` path inside the plugin"
                    ));
                }
            }
        }
        for id in self.icons_light.keys() {
            if !self.icons.contains_key(id) {
                out.push(format!("iconsLight.{id}: `icons` has no icon `{id}`"));
            }
            if self.icons.contains_key(&format!("{id}{LIGHT_SUFFIX}")) {
                out.push(format!(
                    "iconsLight.{id}: `icons` declares `{id}{LIGHT_SUFFIX}` too"
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
            ("rootFolder", &self.root_folder),
            ("rootFolderExpanded", &self.root_folder_expanded),
            ("folderHollow", &self.folder_hollow),
            ("folderHollowExpanded", &self.folder_hollow_expanded),
        ] {
            if let Some(icon) = icon {
                known(field.into(), icon);
            }
        }
        for (field, map) in [
            ("fileNames", &self.file_names),
            ("fileExtensions", &self.file_extensions),
            ("fileTypes", &self.file_types),
            ("folderNames", &self.folder_names),
            ("folderNamesExpanded", &self.folder_names_expanded),
            ("symbols", &self.symbols),
            ("ui", &self.ui),
        ] {
            for (key, icon) in map {
                known(format!("{field}.{key}"), icon);
            }
        }
        for (field, rules) in [
            ("pathRules", &self.path_rules),
            ("filePatterns", &self.file_patterns),
            ("fileRegex", &self.file_regex),
        ] {
            for (i, r) in rules.iter().enumerate() {
                known(format!("{field}[{i}].icon"), &r.icon);
            }
        }
        for (field, rules) in [
            ("pathRules", &self.path_rules),
            ("filePatterns", &self.file_patterns),
        ] {
            for (i, r) in rules.iter().enumerate() {
                if r.pattern.trim().is_empty() {
                    out.push(format!("{field}[{i}].pattern: must not be empty"));
                }
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

    /// Things that are allowed but probably mistakes: symbol kinds and UI ids Forge does not
    /// know (it never asks for them). They do not stop a theme from loading.
    pub fn warnings(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        for kind in self.symbols.keys() {
            if !ids::is_symbol_kind(kind) {
                out.push(format!("symbols.{kind}: not a symbol kind Forge knows"));
            }
        }
        for id in self.ui.keys() {
            if !ids::is_ui_id(id) {
                out.push(format!(
                    "ui.{id}: UI icon ids start with {}",
                    UI_PREFIXES.join(", ")
                ));
            }
        }
        out
    }

    /// The SVG files of the theme as (key, path) pairs: every `icons` entry under its id and
    /// every `iconsLight` entry under `<id>_light`, the key the resolver answers with.
    pub fn svg_files(&self) -> Vec<(String, &str)> {
        self.icons
            .iter()
            .map(|(id, p)| (id.clone(), p.as_str()))
            .chain(
                self.icons_light
                    .iter()
                    .map(|(id, p)| (format!("{id}{LIGHT_SUFFIX}"), p.as_str())),
            )
            .collect()
    }
}

/// Whether light variants are drawn: `choice` is the `fileIcons.variant` setting (`light`,
/// `dark`, or `auto`: follow `ui_theme`, the UI theme id). Light, Solarized Light and High
/// Contrast Light (every id that is `light` or ends in `-light`) use the light variants; the
/// dark themes, High Contrast included, the dark ones.
pub fn uses_light_variant(ui_theme: &str, choice: &str) -> bool {
    match choice {
        "light" => true,
        "dark" => false,
        _ => ui_theme == "light" || ui_theme.ends_with("-light"),
    }
}

/// `path` relative to `root` with `/` separators (unchanged when it is not inside `root`).
pub fn relative_path(path: &str, root: Option<&str>) -> String {
    let path = path.replace('\\', "/");
    let Some(root) = root.map(|r| r.replace('\\', "/")) else {
        return path;
    };
    let root = root.trim_end_matches('/');
    if root.is_empty() {
        return path;
    }
    match path.strip_prefix(root) {
        Some(rest) if rest.starts_with('/') => rest[1..].to_string(),
        Some("") => String::new(),
        _ => path,
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

    fn v2() -> IconThemeDef {
        IconThemeDef::parse(
            r##"{
              "version": 2,
              "icons": {
                "file": "f.svg", "php": "php.svg", "php_light": "php-l.svg", "shell": "sh.svg",
                "controller": "ctl.svg", "folder": "d.svg", "folder-open": "do.svg",
                "folder-src": "src.svg", "folder-src_hollow": "src-h.svg", "hollow": "h.svg",
                "hollow-open": "ho.svg", "root": "r.svg", "root-open": "ro.svg",
                "class": "c.svg", "abstract-class": "ac.svg", "method": "m.svg",
                "static": "st.svg", "final": "fi.svg", "constant": "k.svg",
                "tw-project": "twp.svg", "run": "run.svg", "rc-php": "rcphp.svg", "modified": "mod.svg"
              },
              "iconsLight": { "folder": "d-light.svg", "class": "c-light.svg" },
              "file": "file",
              "folder": "folder",
              "folderExpanded": "folder-open",
              "rootFolder": "root",
              "rootFolderExpanded": "root-open",
              "folderHollow": "hollow",
              "folderHollowExpanded": "hollow-open",
              "fileNames": { "Kernel.php": "php" },
              "fileExtensions": { "php": "php" },
              "fileTypes": { "shell": "shell", "php": "php" },
              "pathRules": [
                { "pattern": "Controllers/*.php", "icon": "controller" },
                { "pattern": "/bin/*", "icon": "shell" }
              ],
              "folderNames": { "src": "folder-src" },
              "symbols": {
                "class": "class", "abstractClass": "abstract-class", "method": "method",
                "constant": "constant", "marker.static": "static", "marker.final": "final"
              },
              "ui": {
                "toolwindow.project": "tw-project", "action.run": "run",
                "runconfig.php": "rc-php", "vcs.modified": "modified"
              },
              "options": { "treeArrows": "plusMinus", "hollowFolders": true }
            }"##,
        )
        .unwrap()
    }

    #[test]
    fn format_2_validates_and_format_1_still_parses() {
        assert_eq!(v2().validate(), Vec::<String>::new());
        assert_eq!(v2().warnings(), Vec::<String>::new());
        assert_eq!(v2().options.tree_arrows, Some(TreeArrows::PlusMinus));
        assert_eq!(theme().format_version(), 1);
        assert_eq!(v2().format_version(), 2);
        let mut t = v2();
        t.version = Some(FORMAT_VERSION + 1);
        t.icons_light.insert("nope".into(), "n.svg".into());
        t.icons_light.insert("php".into(), "p.svg".into());
        t.symbols.insert("klass".into(), "class".into());
        t.symbols.insert("function".into(), "missing".into());
        t.ui.insert("other.thing".into(), "run".into());
        let problems = t.validate().join("\n");
        assert!(problems.contains("needs a newer Forge"), "{problems}");
        assert!(problems.contains("iconsLight.nope"), "{problems}");
        assert!(problems.contains("declares `php_light` too"), "{problems}");
        assert!(
            problems.contains("symbols.function: unknown icon"),
            "{problems}"
        );
        let warnings = t.warnings().join("\n");
        assert!(warnings.contains("symbols.klass") && warnings.contains("ui.other.thing"));
        let keys: Vec<String> = v2().svg_files().into_iter().map(|(k, _)| k).collect();
        assert!(keys.contains(&"folder_light".to_string()) && keys.contains(&"php".to_string()));
    }

    #[test]
    fn path_rules_come_before_names_and_file_types_after_extensions() {
        let (mut r, _) = IconResolver::new(v2(), &[]);
        r.set_project_root(Some("/work/shop"));
        let f = |r: &mut IconResolver, p: &str, ft: Option<&str>| {
            r.lookup(&Lookup {
                file_type: ft,
                ..Lookup::new(p, IconKind::File)
            })
        };
        assert_eq!(
            f(&mut r, "app/Http/Controllers/UserController.php", None).as_deref(),
            Some("controller")
        );
        assert_eq!(
            f(&mut r, "/work/shop/app/Http/Controllers/Kernel.php", None).as_deref(),
            Some("controller"),
            "absolute paths are made relative to the project root"
        );
        assert_eq!(
            f(&mut r, "app/Http/Kernel.php", None).as_deref(),
            Some("php")
        );
        assert_eq!(f(&mut r, "bin/console", None).as_deref(), Some("shell"));
        assert_eq!(
            f(&mut r, "tools/bin/console", None).as_deref(),
            Some("file"),
            "anchored"
        );
        assert!(r.needs_file_type("deploy"));
        assert!(!r.needs_file_type("x.php"));
        assert_eq!(f(&mut r, "deploy", Some("shell")).as_deref(), Some("shell"));
        assert_eq!(f(&mut r, "deploy", None).as_deref(), Some("file"));
        assert_eq!(
            f(&mut r, "x.php", Some("shell")).as_deref(),
            Some("php"),
            "extensions first"
        );
        assert!(
            !resolver(&[]).needs_file_type("deploy"),
            "the theme maps no file types"
        );
    }

    #[test]
    fn root_hollow_and_light_folders() {
        let (mut r, _) = IconResolver::new(v2(), &[]);
        let q = |path, kind, root, hollow| Lookup {
            root,
            hollow,
            ..Lookup::new(path, kind)
        };
        use IconKind::{Folder, FolderOpen};
        assert_eq!(
            r.lookup(&q("shop", FolderOpen, true, false)).as_deref(),
            Some("root-open")
        );
        assert_eq!(
            r.lookup(&q("shop", Folder, true, false)).as_deref(),
            Some("root")
        );
        assert_eq!(
            r.lookup(&q("app", Folder, false, true)).as_deref(),
            Some("hollow")
        );
        assert_eq!(
            r.lookup(&q("app", FolderOpen, false, true)).as_deref(),
            Some("hollow-open")
        );
        assert_eq!(
            r.lookup(&q("src", Folder, false, true)).as_deref(),
            Some("folder-src_hollow")
        );
        assert_eq!(
            r.lookup(&q("src", Folder, false, false)).as_deref(),
            Some("folder-src")
        );

        r.set_light(true);
        assert_eq!(r.cached(), 0, "switching the variant clears the cache");
        assert_eq!(r.resolve("app", Folder).as_deref(), Some("folder_light"));
        assert_eq!(
            r.resolve("a.php", IconKind::File).as_deref(),
            Some("php_light")
        );
        assert_eq!(
            r.resolve("README", IconKind::File).as_deref(),
            Some("file"),
            "no light variant"
        );
        r.set_light(false);
        assert_eq!(r.resolve("a.php", IconKind::File).as_deref(), Some("php"));

        // Format 1 themes have no root icon: the root is a folder like the others.
        let mut old = resolver(&[]);
        assert_eq!(
            old.lookup(&q("shop", FolderOpen, true, true)).as_deref(),
            Some("folder-open")
        );
    }

    #[test]
    fn symbols_use_combined_kinds_fallbacks_and_markers() {
        let (mut r, _) = IconResolver::new(v2(), &[]);
        let m = |a, f, s| SymbolModifiers {
            is_abstract: a,
            is_final: f,
            is_static: s,
        };
        let sym = |r: &IconResolver, k: &str, mods| r.symbol(k, mods).map(|s| (s.icon, s.markers));
        assert_eq!(
            sym(&r, "class", m(false, false, false)),
            Some(("class".into(), vec![]))
        );
        assert_eq!(
            sym(&r, "class", m(true, false, false)),
            Some(("abstract-class".into(), vec![]))
        );
        assert_eq!(
            sym(&r, "class", m(false, true, false)),
            Some(("class".into(), vec!["final".into()]))
        );
        assert_eq!(
            sym(&r, "method", m(false, false, true)),
            Some(("method".into(), vec!["static".into()]))
        );
        assert_eq!(
            sym(&r, "constructor", m(false, false, false)),
            Some(("method".into(), vec![]))
        );
        assert_eq!(
            sym(&r, "enumCase", m(false, false, false)),
            Some(("constant".into(), vec![]))
        );
        assert_eq!(
            sym(&r, "function", m(false, false, false)),
            None,
            "Forge's badge"
        );
        r.set_light(true);
        assert_eq!(
            sym(&r, "class", m(false, false, false)),
            Some(("class_light".into(), vec![]))
        );
    }

    #[test]
    fn light_variants_follow_the_ui_theme() {
        assert!(uses_light_variant("light", "auto"));
        assert!(uses_light_variant("high-contrast-light", "auto"));
        assert!(uses_light_variant("solarized-light", "auto"));
        assert!(!uses_light_variant("high-contrast", "auto"));
        assert!(!uses_light_variant("dark", "auto"));
        assert!(uses_light_variant("dark", "light"));
        assert!(!uses_light_variant("light", "dark"));
    }

    #[test]
    fn ui_ids_ask_the_theme_with_fallbacks() {
        let (r, _) = IconResolver::new(v2(), &[]);
        assert_eq!(r.ui("toolwindow.project").as_deref(), Some("tw-project"));
        assert_eq!(r.ui("action.run").as_deref(), Some("run"));
        assert_eq!(
            r.ui("runconfig.php-builtin-server").as_deref(),
            Some("rc-php")
        );
        assert_eq!(r.ui("vcs.modified").as_deref(), Some("modified"));
        assert_eq!(r.ui("toolwindow.git"), None);
        assert_eq!(
            resolver(&[]).ui("action.run"),
            None,
            "format 1 themes have no UI icons"
        );
    }
}
