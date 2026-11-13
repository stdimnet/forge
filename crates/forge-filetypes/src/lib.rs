//! File type recognition (PHPStorm's Settings | Editor | File Types): which language a file
//! is, what the status bar calls it, its indentation and, for template languages, the data
//! language between the template tags.
//!
//! The bundled types ([`builtin_file_types`]) are extended or replaced by the file types of
//! plugin manifests (`[[plugin.contributions.fileTypes]]`). Detection prefers, in order:
//! a manual override (File | File Properties | Override File Type, [`overrides`]), file names
//! and path patterns, the longest matching extension (so `x.blade.php` beats `php`), the
//! interpreter of a `#!` line (`#!/usr/bin/env php`) and a Vim or Emacs modeline on the first
//! line (`# vim: set ft=php:`, `-*- mode: python -*-`). Later registrations win ties.
//!
//! The user's changes from Settings | Editor | File Types (patterns added to or removed from
//! a type, custom file types with their own highlighting) are applied on top ([`user`]).
//!
//! Template files also get the delimiters of their (template language, data language) pair
//! from the `templates.delimiters` setting ([`delimiters`]).
//!
//! The crate is pure so the desktop app (IPC) and the browser-mode UI (WebAssembly) share it.
//! The UI only maps a file type id to its CodeMirror syntax mode.

use serde::{Deserialize, Serialize};

mod builtin;
pub mod data;
pub mod delimiters;
pub mod glob;
mod modeline;
pub mod overrides;
pub mod user;

pub use builtin::builtin_file_types;
pub use data::{data_language_for, DataLanguage, DATA_LANGUAGES};
pub use delimiters::{delimiter_catalog, resolve_delimiters, DelimiterCatalog, Delimiters};
pub use glob::{matches_glob_list, wildcard_match};
pub use overrides::{
    association_pattern, edit_overrides, override_for, parse_overrides, EditOverridesRequest,
    FileTypeOverride, OverrideChoice, OverrideChoices, OverrideEdit, PLAIN_TEXT_ID,
};
pub use user::{
    custom_id, Association, CustomFileType, CustomSyntax, FileTypeRow, UserFileTypes,
    DEFAULT_IGNORED_FILES,
};

/// Indentation used when a file type does not set one.
pub const DEFAULT_INDENT_SIZE: u8 = 4;

/// Status bar label of files without a file type.
pub const PLAIN_TEXT: &str = "Plain text";

/// A file type: how files are recognised and what the status bar calls them. Syntax
/// highlighting is registered by the UI under the same `id`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeSpec {
    /// Stable id, also the LSP language id when a server handles the type, e.g. `yaml`.
    pub id: String,
    pub name: String,
    /// Extensions without the dot; compound ones such as `blade.php` are allowed.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Exact file names (`Dockerfile`), name patterns with `*` wildcards (`.env.*`) or path
    /// patterns (`config/services*.yaml`).
    #[serde(default)]
    pub file_names: Vec<String>,
    /// Interpreters that select the type from a `#!` line (`python` also matches
    /// `python3.12`), for files without a recognised name. A plugin's type without
    /// interpreters keeps those of the bundled type with the same id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interpreters: Vec<String>,
    /// Indentation in spaces; [`DEFAULT_INDENT_SIZE`] when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent_size: Option<u8>,
    /// A template language: its text between tags is highlighted in a data language.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub template: bool,
}

#[derive(Debug, Clone)]
struct Entry {
    spec: FileTypeSpec,
    /// Contributing plugin; `None` for bundled types.
    plugin: Option<String>,
}

/// All known file types in registration order.
#[derive(Debug, Clone, Default)]
pub struct FileTypeRegistry {
    entries: Vec<Entry>,
    /// Syntax of the user's custom file types by id.
    custom: std::collections::BTreeMap<String, CustomSyntax>,
    /// Descriptions of custom file types by id.
    descriptions: std::collections::BTreeMap<String, String>,
}

/// Request of the `filetypes_detect` IPC command (and its WebAssembly twin).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectRequest {
    pub files: Vec<FileRef>,
    /// Plugins that are disabled or not used by the project: their file types are skipped
    /// (a bundled type with the same id, or a more general one, takes over).
    #[serde(default)]
    pub inactive_plugins: Vec<String>,
    /// The `templates.dataLanguages` setting (`pattern=language` entries).
    #[serde(default)]
    pub data_languages: Vec<String>,
    /// The `editor.softWrapFiles` setting (`*.md; *.txt`); `None` disables soft wraps.
    #[serde(default)]
    pub soft_wrap_files: Option<String>,
    /// The `templates.delimiters` setting: delimiters per (template, data language) pair.
    #[serde(default)]
    pub template_delimiters: Vec<serde_json::Value>,
    /// The `fileTypes.associations` setting: patterns added to or removed from file types.
    #[serde(default)]
    pub associations: Vec<serde_json::Value>,
    /// The `fileTypes.custom` setting: the user's own file types.
    #[serde(default)]
    pub custom_types: Vec<serde_json::Value>,
    /// The `fileTypes.overrides` setting: file types chosen by hand (Override File Type).
    #[serde(default)]
    pub overrides: Vec<serde_json::Value>,
}

impl DetectRequest {
    /// The user's file type settings of this request.
    pub fn user_file_types(&self) -> UserFileTypes {
        UserFileTypes::parse(&self.associations, &self.custom_types)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub path: String,
    /// First line of the content, for `#!` and modeline detection.
    #[serde(default)]
    pub first_line: Option<String>,
}

/// Everything the editor needs to know about a file's type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeInfo {
    /// File type id, `None` for plain text.
    pub id: Option<String>,
    /// Status bar label, e.g. `PHP`, `Twig (LaTeX)` or `Plain text`.
    pub label: String,
    /// Indentation unit (spaces).
    pub indent: String,
    /// Data language of template files.
    pub data_language: Option<DataLanguage>,
    /// Lines wrap softly (the soft-wrap file patterns match).
    pub soft_wrap: bool,
    /// Template delimiters by field key (`left`, `variableStart`, …) for template languages
    /// whose delimiters are configurable; `None` for the others.
    pub delimiters: Option<Delimiters>,
    /// Highlighting of a custom file type (Settings | Editor | File Types).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomSyntax>,
    /// The type was chosen by hand (Override File Type), not detected.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub overridden: bool,
}

impl FileTypeRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// The bundled file types.
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        for spec in builtin_file_types() {
            r.register(spec, None);
        }
        r
    }

    /// Add a file type. A type with the same id *from the same source* is replaced; one from
    /// another source (a bundled type a plugin overrides) stays as the fallback for when the
    /// plugin is inactive. The newest registration wins ties.
    pub fn register(&mut self, spec: FileTypeSpec, plugin: Option<&str>) {
        let plugin = plugin.map(str::to_string);
        self.entries
            .retain(|e| !(e.spec.id == spec.id && e.plugin == plugin));
        self.entries.push(Entry { spec, plugin });
    }

    /// Specs of every registered type, in registration order.
    pub fn specs(&self) -> impl Iterator<Item = &FileTypeSpec> {
        self.entries.iter().map(|e| &e.spec)
    }

    /// The active types: of several types with one id only the newest (a plugin's override)
    /// counts, and types of inactive plugins are skipped.
    fn active(&self, inactive_plugins: &[String]) -> Vec<&FileTypeSpec> {
        let enabled: Vec<&FileTypeSpec> = self
            .entries
            .iter()
            .filter(|e| {
                e.plugin
                    .as_ref()
                    .is_none_or(|p| !inactive_plugins.contains(p))
            })
            .map(|e| &e.spec)
            .collect();
        enabled
            .iter()
            .enumerate()
            .filter(|(i, t)| !enabled[i + 1..].iter().any(|later| later.id == t.id))
            .map(|(_, t)| *t)
            .collect()
    }

    /// The interpreters of `spec`: its own, or those of another registration of its id (a
    /// plugin that overrides a bundled type without listing them keeps the bundled ones).
    fn interpreters_of<'a>(&'a self, spec: &'a FileTypeSpec) -> &'a [String] {
        if !spec.interpreters.is_empty() {
            return &spec.interpreters;
        }
        self.entries
            .iter()
            .rev()
            .find(|e| e.spec.id == spec.id && !e.spec.interpreters.is_empty())
            .map_or(&[], |e| &e.spec.interpreters)
    }

    /// The type registered under `id`: the active one, else one of an inactive plugin (an
    /// override activates the type for that file).
    pub fn spec_by_id(&self, id: &str, inactive_plugins: &[String]) -> Option<&FileTypeSpec> {
        self.active(inactive_plugins)
            .into_iter()
            .find(|t| t.id == id)
            .or_else(|| {
                self.entries
                    .iter()
                    .rev()
                    .map(|e| &e.spec)
                    .find(|t| t.id == id)
            })
    }

    /// The file type of `path`, or `None` for plain text.
    pub fn detect(
        &self,
        path: &str,
        first_line: Option<&str>,
        inactive_plugins: &[String],
    ) -> Option<&FileTypeSpec> {
        let active = self.active(inactive_plugins);
        let lower = path.to_lowercase();
        let name = lower.rsplit('/').next().unwrap_or(&lower);
        let mut best: Option<(&FileTypeSpec, usize)> = None;
        for spec in &active {
            let s = score(spec, &lower, name);
            if s > 0 && best.is_none_or(|(_, b)| s >= b) {
                best = Some((spec, s));
            }
        }
        if let Some((spec, _)) = best {
            return Some(spec);
        }
        let first_line = first_line?;
        if let Some(interpreter) = shebang_interpreter(first_line) {
            let hit = active.iter().rev().find(|t| {
                self.interpreters_of(t)
                    .iter()
                    .any(|i| interpreter_matches(i, &interpreter))
            });
            if let Some(spec) = hit {
                return Some(spec);
            }
        }
        let mode = modeline::mode_of(first_line)?;
        modeline::resolve(&mode, &active, |t, m| {
            self.interpreters_of(t)
                .iter()
                .any(|i| i.eq_ignore_ascii_case(m))
        })
    }

    /// Resolve one file for the editor.
    pub fn info(&self, file: &FileRef, req: &DetectRequest) -> FileTypeInfo {
        self.info_with(file, req, &parse_overrides(&req.overrides))
    }

    fn info_with(
        &self,
        file: &FileRef,
        req: &DetectRequest,
        overrides: &[FileTypeOverride],
    ) -> FileTypeInfo {
        let forced = override_for(overrides, &file.path);
        let overridden = forced.is_some_and(|id| {
            id == PLAIN_TEXT_ID || self.spec_by_id(id, &req.inactive_plugins).is_some()
        });
        let spec = match forced {
            Some(PLAIN_TEXT_ID) => None,
            Some(id) if overridden => self.spec_by_id(id, &req.inactive_plugins),
            _ => self.detect(
                &file.path,
                file.first_line.as_deref(),
                &req.inactive_plugins,
            ),
        };
        let soft_wrap = req
            .soft_wrap_files
            .as_deref()
            .is_some_and(|p| matches_glob_list(&file.path, p));
        let Some(spec) = spec else {
            return FileTypeInfo {
                id: None,
                label: PLAIN_TEXT.to_string(),
                indent: " ".repeat(DEFAULT_INDENT_SIZE as usize),
                data_language: None,
                soft_wrap,
                delimiters: None,
                custom: None,
                overridden,
            };
        };
        let data_language = spec
            .template
            .then(|| data_language_for(&file.path, &req.data_languages));
        let delimiters = data_language
            .and_then(|d| resolve_delimiters(&spec.id, d.id, &req.template_delimiters));
        let label = match data_language {
            Some(d) if d.id != DATA_LANGUAGES[0].id => format!("{} ({})", spec.name, d.label),
            _ => spec.name.clone(),
        };
        FileTypeInfo {
            id: Some(spec.id.clone()),
            label,
            indent: " ".repeat(spec.indent_size.unwrap_or(DEFAULT_INDENT_SIZE) as usize),
            data_language,
            soft_wrap,
            delimiters,
            custom: self.custom.get(&spec.id).cloned(),
            overridden,
        }
    }

    /// Resolve every file of a request, with the user's file type settings applied.
    pub fn resolve(&self, req: &DetectRequest) -> Vec<FileTypeInfo> {
        let user = req.user_file_types();
        let overrides = parse_overrides(&req.overrides);
        if user.is_empty() {
            return req
                .files
                .iter()
                .map(|f| self.info_with(f, req, &overrides))
                .collect();
        }
        let registry = self.customized(&user);
        req.files
            .iter()
            .map(|f| registry.info_with(f, req, &overrides))
            .collect()
    }
}

/// How specifically `spec` claims a file: higher wins, 0 means no match.
fn score(spec: &FileTypeSpec, path: &str, name: &str) -> usize {
    let mut best = 0;
    for raw in &spec.file_names {
        let pattern = raw.to_lowercase();
        let is_path = pattern.contains('/');
        let hit = if is_path {
            wildcard_match(&format!("*/{pattern}"), &format!("/{path}"))
        } else {
            wildcard_match(&pattern, name)
        };
        if !hit {
            continue;
        }
        // A path pattern beats a file name, an exact name beats a wildcard pattern.
        let literal = pattern.chars().filter(|&c| c != '*').count();
        let exact = if pattern.contains('*') { 0 } else { 500 };
        let by_path = if is_path { 500 } else { 0 };
        best = best.max(1000 + literal + exact + by_path);
    }
    for ext in &spec.extensions {
        let ext = ext.to_lowercase();
        if name == ext
            || name
                .strip_suffix(ext.as_str())
                .is_some_and(|s| s.ends_with('.'))
        {
            best = best.max(ext.len() + 1);
        }
    }
    best
}

/// Options of `env` that take the next word as their argument.
const ENV_ARG_OPTIONS: &[&str] = &["-u", "--unset", "-C", "--chdir", "-P", "-a", "--argv0"];

/// The interpreter of a `#!` line, lower case and without directory or `.exe`:
/// `#!/usr/bin/env -S python3 -u` → `python3`, `#!/usr/bin/env FOO=1 php` → `php`,
/// `#!/opt/homebrew/bin/php -q` → `php`, `#!/usr/bin/make -f` → `make`.
pub fn shebang_interpreter(line: &str) -> Option<String> {
    let rest = line.trim_start_matches('\u{feff}').strip_prefix("#!")?;
    let base = |w: &str| {
        let name = w.rsplit(['/', '\\']).next().unwrap_or(w).to_lowercase();
        name.strip_suffix(".exe")
            .map(str::to_string)
            .unwrap_or(name)
    };
    let mut words = rest.split_whitespace();
    let program = base(words.next()?);
    if program != "env" {
        return Some(program).filter(|p| !p.is_empty());
    }
    while let Some(w) = words.next() {
        if let Some(split) = w.strip_prefix("--split-string=") {
            if !split.is_empty() {
                return Some(base(split));
            }
            continue;
        }
        if w == "--" {
            return words.next().map(base);
        }
        if ENV_ARG_OPTIONS.contains(&w) {
            words.next();
            continue;
        }
        if let Some(opts) = w.strip_prefix('-').filter(|o| !o.starts_with('-')) {
            // `-S'php -q'`, `-Sphp`: the split string glued to the option.
            if let Some(i) = opts.find('S') {
                let glued = opts[i + 1..].trim_matches(['\'', '"']);
                if !glued.is_empty() {
                    return Some(base(glued));
                }
            }
            continue;
        }
        if w.starts_with('-') || w.contains('=') {
            continue;
        }
        return Some(base(w.trim_matches(['\'', '"'])));
    }
    None
}

/// `python` matches `python`, `python3` and `python3.12`, `php` matches `php8.3`, not
/// `pythonista`.
fn interpreter_matches(name: &str, interpreter: &str) -> bool {
    interpreter
        .strip_prefix(&name.to_lowercase())
        .is_some_and(|v| v.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

#[cfg(test)]
mod tests;
