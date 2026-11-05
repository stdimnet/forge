//! The resolver of the active theme: path → icon id (files and folders), symbol kind →
//! icon id, UI id → icon id, with the user's associations, the light variants and a cache
//! per path.

use std::collections::{BTreeMap, HashMap};

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

use crate::folders::path_glob;
use crate::ids::{symbol_fallback, ui_fallbacks};
use crate::{
    glob_match, relative_path, Association, AssociationKind, IconKind, IconThemeDef, Problem,
    HOLLOW_SUFFIX, LIGHT_SUFFIX,
};

enum Matcher {
    Name(String),
    Extension(String),
    Glob(String),
    Regex(Regex),
    Folder(String),
}

/// What an icon is asked for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lookup<'a> {
    /// A path (relative to the project root or absolute) or just a file name.
    pub path: &'a str,
    pub kind: IconKind,
    /// The project root folder (Project view).
    pub root: bool,
    /// A folder that holds open files, with hollow folders on.
    pub hollow: bool,
    /// The detected file type id (`forge-filetypes`), when the caller knows it.
    pub file_type: Option<&'a str>,
}

impl<'a> Lookup<'a> {
    pub fn new(path: &'a str, kind: IconKind) -> Self {
        Lookup {
            path,
            kind,
            ..Default::default()
        }
    }

    /// Bit 2 (4): the project root, bit 3 (8): hollow, the low bits: [`IconKind::from_code`]
    /// (the WebAssembly calling convention).
    pub fn from_code(path: &'a str, code: u8, file_type: Option<&'a str>) -> Self {
        Lookup {
            path,
            kind: IconKind::from_code(code),
            root: code & 4 != 0,
            hollow: code & 8 != 0,
            file_type: file_type.filter(|t| !t.is_empty()),
        }
    }
}

/// Modifiers of a code symbol that pick a combined icon (`abstractClass`) or a marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolModifiers {
    #[serde(default)]
    pub is_abstract: bool,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default)]
    pub is_static: bool,
}

/// The icon of a code symbol: its own icon and the marker overlays drawn on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolIcon {
    pub icon: String,
    pub markers: Vec<String>,
}

type CacheKey = (String, u8, Option<String>);

/// Resolves paths, symbols and UI ids to icon ids of one theme plus the user's
/// associations, caching per path.
pub struct IconResolver {
    theme: IconThemeDef,
    associations: Vec<(Matcher, String)>,
    regexes: Vec<(Regex, String)>,
    names_lower: HashMap<String, String>,
    extensions_lower: HashMap<String, String>,
    folders_lower: HashMap<String, String>,
    folders_open_lower: HashMap<String, String>,
    light: bool,
    project_root: Option<String>,
    cache: HashMap<CacheKey, Option<String>>,
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
            light: false,
            project_root: None,
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

    /// Answer with light variants (a light UI theme). Clears the cache when it changes.
    pub fn set_light(&mut self, light: bool) {
        if self.light != light {
            self.light = light;
            self.cache.clear();
        }
    }

    pub fn is_light(&self) -> bool {
        self.light
    }

    /// The project root that `pathRules` are relative to. Clears the cache when it changes.
    pub fn set_project_root(&mut self, root: Option<&str>) {
        let root = root.filter(|r| !r.is_empty()).map(str::to_string);
        if self.project_root != root {
            self.project_root = root;
            self.cache.clear();
        }
    }

    /// The icon id for `path` (a path or just a file name), or `None` for Forge's own icon.
    pub fn resolve(&mut self, path: &str, kind: IconKind) -> Option<String> {
        self.lookup(&Lookup::new(path, kind))
    }

    /// The icon id (the key of its SVG, `<id>_light` for a light variant) for `q`, or `None`
    /// for Forge's own icon.
    pub fn lookup(&mut self, q: &Lookup) -> Option<String> {
        let code = match q.kind {
            IconKind::File => 0,
            IconKind::Folder => 1,
            IconKind::FolderOpen => 2,
        } | if q.root { 4 } else { 0 }
            | if q.hollow { 8 } else { 0 };
        let key = (q.path.to_string(), code, q.file_type.map(str::to_string));
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let icon = if q.kind.is_folder() {
            self.folder(q)
        } else {
            self.file(q.path, q.file_type).0
        }
        .filter(|id| self.theme.icons.contains_key(id))
        .map(|id| self.variant(id));
        self.cache.insert(key, icon.clone());
        icon
    }

    /// Whether the icon of `path` could depend on its detected file type: the theme maps
    /// file types and no rule more specific than the default icon matched. Callers ask for
    /// the type (which may mean reading the file's first line) only then.
    pub fn needs_file_type(&self, path: &str) -> bool {
        !self.theme.file_types.is_empty() && !self.file(path, None).1
    }

    /// The light variant of `id` when a light UI theme is active and the theme has one.
    fn variant(&self, id: String) -> String {
        if self.light {
            let light = format!("{id}{LIGHT_SUFFIX}");
            if self.theme.icons.contains_key(&light) || self.theme.icons_light.contains_key(&id) {
                return light;
            }
        }
        id
    }

    fn folder(&self, q: &Lookup) -> Option<String> {
        let t = &self.theme;
        let open = q.kind == IconKind::FolderOpen;
        let name = file_name(q.path);
        let lower = name.to_lowercase();
        let named = || -> Option<String> {
            for (m, icon) in &self.associations {
                if matches!(m, Matcher::Folder(n) if *n == lower) {
                    return Some(icon.clone());
                }
            }
            if q.root {
                let root = if open {
                    t.root_folder_expanded.as_ref().or(t.root_folder.as_ref())
                } else {
                    t.root_folder.as_ref()
                };
                if let Some(icon) = root {
                    return Some(icon.clone());
                }
            }
            if open {
                if let Some(icon) = t
                    .folder_names_expanded
                    .get(name)
                    .or_else(|| self.folders_open_lower.get(&lower))
                {
                    return Some(icon.clone());
                }
            }
            t.folder_names
                .get(name)
                .or_else(|| self.folders_lower.get(&lower))
                .cloned()
        };
        let default = || {
            if open {
                t.folder_expanded.clone().or_else(|| t.folder.clone())
            } else {
                t.folder.clone()
            }
        };
        let (icon, is_default) = match named() {
            Some(icon) => (icon, false),
            None => (default()?, true),
        };
        if !q.hollow {
            return Some(icon);
        }
        let hollow = format!("{icon}{HOLLOW_SUFFIX}");
        if t.icons.contains_key(&hollow) {
            return Some(hollow);
        }
        let fallback = if open {
            t.folder_hollow_expanded
                .as_ref()
                .or(t.folder_hollow.as_ref())
        } else {
            t.folder_hollow.as_ref()
        };
        Some(match fallback {
            Some(h) if is_default => h.clone(),
            _ => icon,
        })
    }

    /// The icon of a file and whether a rule more specific than the default matched.
    fn file(&self, path: &str, file_type: Option<&str>) -> (Option<String>, bool) {
        let name = file_name(path);
        let lower = name.to_lowercase();
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
                return (Some(icon.clone()), true);
            }
        }
        let t = &self.theme;
        if !t.path_rules.is_empty() {
            let rel = relative_path(path, self.project_root.as_deref());
            if let Some(r) = t.path_rules.iter().find(|r| path_glob(&r.pattern, &rel)) {
                return (Some(r.icon.clone()), true);
            }
        }
        if let Some(icon) = t
            .file_names
            .get(name)
            .or_else(|| self.names_lower.get(&lower))
        {
            return (Some(icon.clone()), true);
        }
        if let Some(r) = t
            .file_patterns
            .iter()
            .find(|r| glob_on(&r.pattern, path, name))
        {
            return (Some(r.icon.clone()), true);
        }
        if let Some((_, icon)) = self.regexes.iter().find(|(r, _)| r.is_match(name)) {
            return (Some(icon.clone()), true);
        }
        for ext in &extensions {
            if let Some(icon) = t
                .file_extensions
                .get(ext.as_str())
                .or_else(|| self.extensions_lower.get(ext.as_str()))
            {
                return (Some(icon.clone()), true);
            }
        }
        if let Some(icon) = file_type.and_then(|ft| t.file_types.get(ft)) {
            return (Some(icon.clone()), true);
        }
        (t.file.clone(), false)
    }

    /// The icon of a code symbol of `kind` ([`crate::ids::SYMBOL_KINDS`]), or `None` for
    /// Forge's own badge. A combined kind (`abstractClass`, `staticMethod`, …) wins over the
    /// kind plus a marker; otherwise the kind falls back along
    /// [`crate::ids::symbol_fallback`] and each modifier adds its `marker.*` icon.
    pub fn symbol(&self, kind: &str, modifiers: SymbolModifiers) -> Option<SymbolIcon> {
        let s = &self.theme.symbols;
        let known = |id: &String| self.theme.icons.contains_key(id);
        let on = [
            ("abstract", modifiers.is_abstract),
            ("final", modifiers.is_final),
            ("static", modifiers.is_static),
        ];
        let mut cap = kind.to_string();
        if let Some(first) = cap.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        let mut used = None;
        let mut icon = None;
        for (m, set) in on {
            if set {
                if let Some(i) = s.get(&format!("{m}{cap}")).filter(|i| known(i)) {
                    icon = Some(i.clone());
                    used = Some(m);
                    break;
                }
            }
        }
        if icon.is_none() {
            let mut k = kind;
            loop {
                if let Some(i) = s.get(k).filter(|i| known(i)) {
                    icon = Some(i.clone());
                    break;
                }
                k = symbol_fallback(k)?;
            }
        }
        let markers = on
            .iter()
            .filter(|(m, set)| *set && used != Some(*m))
            .filter_map(|(m, _)| s.get(&format!("marker.{m}")).filter(|i| known(i)))
            .map(|i| self.variant(i.clone()))
            .collect();
        Some(SymbolIcon {
            icon: self.variant(icon?),
            markers,
        })
    }

    /// The icon of the UI id `id` ([`crate::ids::UI_ICONS`]), trying shorter ids along
    /// [`crate::ids::ui_fallbacks`], or `None` for Forge's own icon.
    pub fn ui(&self, id: &str) -> Option<String> {
        ui_fallbacks(id)
            .into_iter()
            .find_map(|k| self.theme.ui.get(k))
            .filter(|i| self.theme.icons.contains_key(*i))
            .map(|i| self.variant(i.clone()))
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
