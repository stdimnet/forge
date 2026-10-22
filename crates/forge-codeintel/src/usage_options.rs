//! Find Usages settings (Ctrl+Shift+Alt+F7, the "Settings" button of the Find tool window
//! and the Show Usages popup): which usages to look for per kind of element, where to look
//! (the scope) and the text occurrences that come on top. The choices are settings
//! (`findUsages.options.*`, `findUsages.scope.*`), so they apply to every later Find Usages
//! (Alt+F7) and Show Usages (Ctrl+Alt+F7) run.

use serde::{Deserialize, Serialize};

use crate::edit::{Position, Range};
use crate::usages::{classify_usage, UsageKind, UsageLocation};

/// What kind of element Find Usages searches, as the Find Usages dialog names it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ElementKind {
    Class,
    Interface,
    Trait,
    Enum,
    Method,
    Property,
    Constant,
    EnumCase,
    Function,
    Variable,
    /// A language server's symbol whose kind Forge does not know.
    #[default]
    Symbol,
}

impl ElementKind {
    /// "class", "interface", "method", ... for "Find Usages of class User".
    pub fn label(self) -> &'static str {
        match self {
            ElementKind::Class => "class",
            ElementKind::Interface => "interface",
            ElementKind::Trait => "trait",
            ElementKind::Enum => "enum",
            ElementKind::Method => "method",
            ElementKind::Property => "property",
            ElementKind::Constant => "constant",
            ElementKind::EnumCase => "enum case",
            ElementKind::Function => "function",
            ElementKind::Variable => "variable",
            ElementKind::Symbol => "symbol",
        }
    }
}

/// The options of the Find Usages dialog for every kind of element, as stored in the
/// settings (`findUsages.options.<field>`). [`UsageOptions::effective`] picks the ones of
/// one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageOptions {
    pub class_usages: bool,
    pub class_method_usages: bool,
    pub class_field_usages: bool,
    pub class_derived_classes: bool,
    pub interface_usages: bool,
    pub interface_method_usages: bool,
    pub interface_implementing_classes: bool,
    pub interface_derived_interfaces: bool,
    pub method_usages: bool,
    pub method_overriding_methods: bool,
    /// Properties and variables: reads and writes.
    pub read_access: bool,
    pub write_access: bool,
    /// Functions, constants and symbols of language servers.
    pub usages: bool,
    pub search_in_comments_and_strings: bool,
    pub search_for_text_occurrences: bool,
}

impl Default for UsageOptions {
    fn default() -> Self {
        UsageOptions {
            class_usages: true,
            class_method_usages: false,
            class_field_usages: false,
            class_derived_classes: false,
            interface_usages: true,
            interface_method_usages: false,
            interface_implementing_classes: false,
            interface_derived_interfaces: false,
            method_usages: true,
            method_overriding_methods: false,
            read_access: true,
            write_access: true,
            usages: true,
            search_in_comments_and_strings: false,
            search_for_text_occurrences: false,
        }
    }
}

/// The options that apply to one element.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveOptions {
    pub usages: bool,
    pub method_usages: bool,
    pub field_usages: bool,
    pub derived_classes: bool,
    pub implementing_classes: bool,
    pub derived_interfaces: bool,
    pub overriding_methods: bool,
    pub read_access: bool,
    pub write_access: bool,
    pub comments_and_strings: bool,
    pub text_occurrences: bool,
}

/// One checkbox of the Find Usages dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionField {
    /// The setting that stores it (`findUsages.options.classMethodUsages`).
    pub key: String,
    pub label: String,
    /// Whether the scope applies (variables live in their file).
    pub section: OptionSection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OptionSection {
    /// "Find" (what to look for).
    Find,
    /// "Options" (comments, strings, text occurrences).
    Options,
}

/// Prefix of the option settings.
pub const OPTIONS_PREFIX: &str = "findUsages.options.";

impl UsageOptions {
    pub fn effective(&self, kind: ElementKind) -> EffectiveOptions {
        let mut e = EffectiveOptions {
            comments_and_strings: self.search_in_comments_and_strings,
            text_occurrences: self.search_for_text_occurrences,
            read_access: true,
            write_access: true,
            ..Default::default()
        };
        match kind {
            ElementKind::Class | ElementKind::Trait | ElementKind::Enum => {
                e.usages = self.class_usages;
                e.method_usages = self.class_method_usages;
                e.field_usages = self.class_field_usages;
                e.derived_classes = self.class_derived_classes && kind == ElementKind::Class;
            }
            ElementKind::Interface => {
                e.usages = self.interface_usages;
                e.method_usages = self.interface_method_usages;
                e.implementing_classes = self.interface_implementing_classes;
                e.derived_interfaces = self.interface_derived_interfaces;
            }
            ElementKind::Method => {
                e.usages = self.method_usages;
                e.overriding_methods = self.method_overriding_methods;
            }
            ElementKind::Property | ElementKind::Variable => {
                e.usages = self.read_access || self.write_access;
                e.read_access = self.read_access;
                e.write_access = self.write_access;
                if kind == ElementKind::Variable {
                    e.comments_and_strings = false;
                    e.text_occurrences = false;
                }
            }
            ElementKind::Constant
            | ElementKind::EnumCase
            | ElementKind::Function
            | ElementKind::Symbol => e.usages = self.usages,
        }
        e
    }

    /// The checkboxes of the Find Usages dialog for `kind`, in PHPStorm's order.
    pub fn fields(kind: ElementKind) -> Vec<OptionField> {
        let find = |key: &str, label: &str| OptionField {
            key: format!("{OPTIONS_PREFIX}{key}"),
            label: label.into(),
            section: OptionSection::Find,
        };
        let mut out = match kind {
            ElementKind::Class | ElementKind::Trait | ElementKind::Enum => {
                let mut v = vec![
                    find("classUsages", "Usages"),
                    find("classMethodUsages", "Usages of methods"),
                    find("classFieldUsages", "Usages of fields"),
                ];
                if kind == ElementKind::Class {
                    v.push(find("classDerivedClasses", "Derived classes"));
                }
                v
            }
            ElementKind::Interface => vec![
                find("interfaceUsages", "Usages"),
                find("interfaceMethodUsages", "Usages of methods"),
                find("interfaceImplementingClasses", "Implementing classes"),
                find("interfaceDerivedInterfaces", "Derived interfaces"),
            ],
            ElementKind::Method => vec![
                find("methodUsages", "Usages"),
                find("methodOverridingMethods", "Overriding methods"),
            ],
            ElementKind::Property | ElementKind::Variable => vec![
                find("readAccess", "Read access"),
                find("writeAccess", "Write access"),
            ],
            _ => vec![find("usages", "Usages")],
        };
        if kind != ElementKind::Variable {
            for (key, label) in [
                (
                    "searchInCommentsAndStrings",
                    "Search in comments and strings",
                ),
                ("searchForTextOccurrences", "Search for text occurrences"),
            ] {
                out.push(OptionField {
                    key: format!("{OPTIONS_PREFIX}{key}"),
                    label: label.into(),
                    section: OptionSection::Options,
                });
            }
        }
        out
    }
}

impl EffectiveOptions {
    /// Whether a usage of this kind is kept: read / write access of properties and
    /// variables, comments and strings and text occurrences only on request. Whether to
    /// look for usages at all (`usages`) is the search's own decision.
    pub fn keeps(&self, kind: UsageKind) -> bool {
        match kind {
            UsageKind::Write => self.write_access,
            UsageKind::Read => self.read_access,
            UsageKind::Comment | UsageKind::StringLiteral => self.comments_and_strings,
            UsageKind::NonCode => self.text_occurrences,
            _ => true,
        }
    }
}

/// PHPStorm's search scopes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchScope {
    #[default]
    ProjectFiles,
    ProjectAndLibraries,
    OpenFiles,
    CurrentFile,
    RecentlyChangedFiles,
    Directory,
    Custom,
}

/// Where Find Usages looks: the scope settings (`findUsages.scope.*`) and what the UI
/// knows about the workbench (the current, open and recently changed files).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageScope {
    pub kind: SearchScope,
    /// Project-relative directory of the Directory scope.
    pub directory: String,
    pub include_subdirectories: bool,
    /// Globs of a custom scope, separated by `;` or `,`; `!` excludes (`app/**; !app/Legacy/**`).
    pub pattern: String,
    pub current_file: String,
    pub open_files: Vec<String>,
    pub changed_files: Vec<String>,
}

impl Default for UsageScope {
    fn default() -> Self {
        UsageScope {
            kind: SearchScope::ProjectFiles,
            directory: String::new(),
            include_subdirectories: true,
            pattern: String::new(),
            current_file: String::new(),
            open_files: Vec::new(),
            changed_files: Vec::new(),
        }
    }
}

/// Libraries inside the project folder (Composer and npm packages).
pub fn is_library_path(path: &str) -> bool {
    ["vendor/", "node_modules/"]
        .iter()
        .any(|d| path.starts_with(d) || path.contains(&format!("/{d}")))
}

/// Glob match with `*` (within a segment), `**` (across segments) and `?`.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match p.first() {
            None => s.is_empty(),
            // `**/`: zero or more directories; a trailing `**`: anything.
            Some(b'*') if p.get(1) == Some(&b'*') && p.get(2) == Some(&b'/') => (0..=s.len())
                .filter(|&i| i == 0 || s[i - 1] == b'/')
                .any(|i| go(&p[3..], &s[i..])),
            Some(b'*') if p.get(1) == Some(&b'*') => (0..=s.len()).any(|i| go(&p[2..], &s[i..])),
            Some(b'*') => (0..=s.len())
                .take_while(|&i| i == 0 || s[i - 1] != b'/')
                .any(|i| go(&p[1..], &s[i..])),
            Some(b'?') => !s.is_empty() && s[0] != b'/' && go(&p[1..], &s[1..]),
            Some(c) => s.first() == Some(c) && go(&p[1..], &s[1..]),
        }
    }
    go(pattern.as_bytes(), path.as_bytes())
}

impl UsageScope {
    fn pattern_matches(&self, path: &str) -> bool {
        let mut includes = 0;
        let mut included = false;
        for raw in self.pattern.split([';', ',']) {
            let p = raw.trim().trim_start_matches("./");
            if p.is_empty() {
                continue;
            }
            let (exclude, p) = match p.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, p),
            };
            // A pattern without a slash matches file names anywhere.
            let hit = if p.contains('/') {
                glob_match(p, path)
            } else {
                glob_match(p, path.rsplit('/').next().unwrap_or(path))
            };
            if exclude && hit {
                return false;
            }
            if !exclude {
                includes += 1;
                included |= hit;
            }
        }
        includes == 0 || included
    }

    /// Whether a project-relative `path` is in the scope.
    pub fn contains(&self, path: &str) -> bool {
        match self.kind {
            SearchScope::ProjectFiles => !is_library_path(path),
            SearchScope::ProjectAndLibraries => true,
            SearchScope::OpenFiles => self.open_files.iter().any(|p| p == path),
            SearchScope::CurrentFile => path == self.current_file,
            SearchScope::RecentlyChangedFiles => self.changed_files.iter().any(|p| p == path),
            SearchScope::Directory => {
                let dir = self.directory.trim_matches('/');
                if dir.is_empty() {
                    return self.include_subdirectories || !path.contains('/');
                }
                match path.strip_prefix(dir).and_then(|r| r.strip_prefix('/')) {
                    Some(rest) => self.include_subdirectories || !rest.contains('/'),
                    None => false,
                }
            }
            SearchScope::Custom => self.pattern_matches(path),
        }
    }

    /// "Project Files", "Directory 'app/Models'" for "Usages of User in Project Files".
    pub fn label(&self) -> String {
        match self.kind {
            SearchScope::ProjectFiles => "Project Files".into(),
            SearchScope::ProjectAndLibraries => "Project and Libraries".into(),
            SearchScope::OpenFiles => "Open Files".into(),
            SearchScope::CurrentFile => match self.current_file.rsplit('/').next() {
                Some(name) if !name.is_empty() => format!("File '{name}'"),
                _ => "Current File".into(),
            },
            SearchScope::RecentlyChangedFiles => "Recently Changed Files".into(),
            SearchScope::Directory => {
                let dir = self.directory.trim_matches('/');
                format!("Directory '{}'", if dir.is_empty() { "/" } else { dir })
            }
            SearchScope::Custom => format!("Scope '{}'", self.pattern.trim()),
        }
    }

    /// The usages in the scope.
    pub fn filter(&self, usages: Vec<UsageLocation>) -> Vec<UsageLocation> {
        usages
            .into_iter()
            .filter(|u| self.contains(&u.path))
            .collect()
    }
}

/// Families of file extensions that count as "code" of the same language: text
/// occurrences elsewhere are non-code usages.
const LANGUAGES: &[&[&str]] = &[
    &["php", "phtml", "inc"],
    &["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "vue"],
    &["lua"],
    &["py"],
    &["rs"],
    &["go"],
    &["java", "kt"],
    &["css", "scss", "sass", "less"],
];

fn extension(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if name.ends_with(".blade.php") {
        return "blade".into();
    }
    name.rsplit_once('.')
        .map(|(_, e)| e.to_string())
        .unwrap_or_default()
}

/// Whether two files are written in the same language.
pub fn same_language(a: &str, b: &str) -> bool {
    let (ea, eb) = (extension(a), extension(b));
    ea == eb
        || LANGUAGES
            .iter()
            .any(|l| l.contains(&ea.as_str()) && l.contains(&eb.as_str()))
}

/// Text occurrences of `word` (whole words, case-sensitive) that come on top of the
/// usages: in comments and strings of files in the same language as `source` (with
/// `comments_and_strings`) and anywhere in files of other languages (with
/// `text_occurrences`, as non-code usages). Places already in `existing` are skipped.
pub fn text_occurrences<'a>(
    word: &str,
    source: &str,
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
    existing: &[UsageLocation],
    options: &EffectiveOptions,
) -> Vec<UsageLocation> {
    let word = word.trim_start_matches('$');
    if word.is_empty() || !(options.comments_and_strings || options.text_occurrences) {
        return Vec::new();
    }
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    for (path, text) in files {
        let code = same_language(path, source);
        if code && !options.comments_and_strings || !code && !options.text_occurrences {
            continue;
        }
        for (n, line) in text.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            for (i, _) in line.match_indices(word) {
                let before = line[..i].chars().next_back();
                let after = line[i + word.len()..].chars().next();
                if before.is_some_and(|c| ident(c) || c == '$') || after.is_some_and(ident) {
                    continue;
                }
                let start = line[..i].encode_utf16().count() as u32;
                let end = start + word.encode_utf16().count() as u32;
                let pos = Position::new(n as u32, start);
                if existing.iter().any(|u| {
                    u.path == path && u.range.start.line == pos.line && u.range.contains(pos)
                }) {
                    continue;
                }
                let kind = if code {
                    match classify_usage(line, start, end) {
                        k @ (UsageKind::Comment | UsageKind::StringLiteral) => k,
                        UsageKind::Doc => UsageKind::Comment,
                        _ => continue,
                    }
                } else {
                    UsageKind::NonCode
                };
                out.push(UsageLocation {
                    path: path.to_string(),
                    range: Range::on_line(n as u32, start, end),
                    kind: Some(kind),
                });
            }
        }
    }
    out
}

/// Drop the usages the options do not want (read / write access, comments and strings);
/// `kind_of` classifies usages without a known kind.
pub fn apply_options(
    usages: Vec<UsageLocation>,
    options: &EffectiveOptions,
    mut kind_of: impl FnMut(&UsageLocation) -> UsageKind,
) -> Vec<UsageLocation> {
    usages
        .into_iter()
        .filter(|u| {
            let kind = u.kind.unwrap_or_else(|| kind_of(u));
            options.keeps(kind)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(path: &str, line: u32, start: u32, end: u32) -> UsageLocation {
        UsageLocation {
            path: path.into(),
            range: Range::on_line(line, start, end),
            kind: None,
        }
    }

    #[test]
    fn options_come_from_settings_by_name() {
        let o: UsageOptions = serde_json::from_value(serde_json::json!({
            "classMethodUsages": true,
            "searchForTextOccurrences": true,
        }))
        .unwrap();
        let class = o.effective(ElementKind::Class);
        assert!(class.usages && class.method_usages && class.text_occurrences);
        assert!(!class.field_usages && !class.derived_classes);
        let var = o.effective(ElementKind::Variable);
        assert!(!var.text_occurrences, "variables live in their file");
        let keys: Vec<String> = UsageOptions::fields(ElementKind::Interface)
            .into_iter()
            .map(|f| f.key)
            .collect();
        assert_eq!(keys[2], "findUsages.options.interfaceImplementingClasses");
        assert_eq!(keys.len(), 6);
        assert_eq!(UsageOptions::fields(ElementKind::Variable).len(), 2);
    }

    #[test]
    fn read_and_write_access_filter_property_usages() {
        let o = UsageOptions {
            write_access: false,
            ..Default::default()
        };
        let e = o.effective(ElementKind::Property);
        let usages = vec![
            UsageLocation {
                kind: Some(UsageKind::Write),
                ..loc("a.php", 1, 0, 3)
            },
            UsageLocation {
                kind: Some(UsageKind::Read),
                ..loc("a.php", 2, 0, 3)
            },
            loc("a.php", 3, 0, 3),
        ];
        let kept = apply_options(usages, &e, |_| UsageKind::Comment);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].range.start.line, 2);
    }

    #[test]
    fn scopes_filter_paths() {
        let mut s = UsageScope {
            current_file: "app/Models/User.php".into(),
            open_files: vec!["app/Models/User.php".into(), "routes/web.php".into()],
            changed_files: vec!["routes/web.php".into()],
            directory: "app".into(),
            pattern: "app/**/*.php; !app/Legacy/**".into(),
            ..Default::default()
        };
        assert!(s.contains("app/Models/User.php"));
        assert!(!s.contains("vendor/laravel/framework/src/Model.php"));
        assert_eq!(s.label(), "Project Files");
        s.kind = SearchScope::ProjectAndLibraries;
        assert!(s.contains("vendor/laravel/framework/src/Model.php"));
        s.kind = SearchScope::OpenFiles;
        assert!(s.contains("routes/web.php") && !s.contains("app/Http/Kernel.php"));
        s.kind = SearchScope::CurrentFile;
        assert!(!s.contains("routes/web.php"));
        assert_eq!(s.label(), "File 'User.php'");
        s.kind = SearchScope::RecentlyChangedFiles;
        assert!(s.contains("routes/web.php") && !s.contains("app/Models/User.php"));
        s.kind = SearchScope::Directory;
        assert!(s.contains("app/Models/User.php") && !s.contains("routes/web.php"));
        assert!(!s.contains("application.php"));
        s.include_subdirectories = false;
        assert!(!s.contains("app/Models/User.php") && s.contains("app/helpers.php"));
        assert_eq!(s.label(), "Directory 'app'");
        s.kind = SearchScope::Custom;
        assert!(s.contains("app/Models/User.php"));
        assert!(!s.contains("app/Legacy/Old.php"));
        assert!(!s.contains("routes/web.php"));
        s.pattern = "*.blade.php".into();
        assert!(s.contains("resources/views/welcome.blade.php"));
        assert!(!s.contains("app/Models/User.php"));
    }

    #[test]
    fn globs() {
        assert!(glob_match("app/**/*.php", "app/User.php"));
        assert!(glob_match("app/**/*.php", "app/Models/User.php"));
        assert!(!glob_match("app/*.php", "app/Models/User.php"));
        assert!(glob_match("**/User.php", "app/Models/User.php"));
        assert!(glob_match("Us?r.php", "User.php"));
    }

    #[test]
    fn text_occurrences_add_comments_strings_and_non_code_files() {
        let php = "<?php\n// the Order model\n$x = 'Order';\nnew Order();\n";
        let yaml = "model: Order\nOrders: 2\n";
        let existing = vec![loc("app/A.php", 3, 4, 9)];
        let mut options = EffectiveOptions {
            usages: true,
            comments_and_strings: true,
            ..Default::default()
        };
        let files = [("app/A.php", php), ("config/x.yaml", yaml)];
        let found = text_occurrences("Order", "app/Models/Order.php", files, &existing, &options);
        let kinds: Vec<_> = found.iter().map(|u| (u.range.start.line, u.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                (1, Some(UsageKind::Comment)),
                (2, Some(UsageKind::StringLiteral))
            ]
        );
        options.comments_and_strings = false;
        options.text_occurrences = true;
        let found = text_occurrences("Order", "app/Models/Order.php", files, &existing, &options);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "config/x.yaml");
        assert_eq!(found[0].kind, Some(UsageKind::NonCode));
        assert!(same_language("a.ts", "b.vue") && !same_language("a.php", "b.blade.php"));
    }
}
