//! File type recognition (PHPStorm's Settings | Editor | File Types): which language a file
//! is, what the status bar calls it, its indentation and, for template languages, the data
//! language between the template tags.
//!
//! The bundled types ([`builtin_file_types`]) are extended or replaced by the file types of
//! plugin manifests (`[[plugin.contributions.fileTypes]]`). Detection prefers, in order:
//! file names and path patterns, the longest matching extension (so `x.blade.php` beats
//! `php`), and the interpreter of a `#!` line. Later registrations win ties.
//!
//! The crate is pure so the desktop app (IPC) and the browser-mode UI (WebAssembly) share it.
//! The UI only maps a file type id to its CodeMirror syntax mode.

use serde::{Deserialize, Serialize};

mod builtin;
pub mod data;
pub mod glob;

pub use builtin::builtin_file_types;
pub use data::{data_language_for, DataLanguage, DATA_LANGUAGES};
pub use glob::{matches_glob_list, wildcard_match};

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
    /// `python3.12`), for files without a recognised name.
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
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub path: String,
    /// First line of the content, for `#!` detection.
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

    /// The file type of `path`, or `None` for plain text.
    pub fn detect(
        &self,
        path: &str,
        first_line: Option<&str>,
        inactive_plugins: &[String],
    ) -> Option<&FileTypeSpec> {
        let enabled: Vec<&FileTypeSpec> = self
            .entries
            .iter()
            .filter(|e| {
                e.plugin
                    .as_ref()
                    .map_or(true, |p| !inactive_plugins.contains(p))
            })
            .map(|e| &e.spec)
            .collect();
        // Of several types with one id, only the newest (the plugin's override) counts.
        let active: Vec<&FileTypeSpec> = enabled
            .iter()
            .enumerate()
            .filter(|(i, t)| !enabled[i + 1..].iter().any(|later| later.id == t.id))
            .map(|(_, t)| *t)
            .collect();
        let lower = path.to_lowercase();
        let name = lower.rsplit('/').next().unwrap_or(&lower);
        let mut best: Option<(&FileTypeSpec, usize)> = None;
        for spec in &active {
            let s = score(spec, &lower, name);
            if s > 0 && best.map_or(true, |(_, b)| s >= b) {
                best = Some((spec, s));
            }
        }
        if let Some((spec, _)) = best {
            return Some(spec);
        }
        let interpreter = first_line.and_then(shebang_interpreter)?;
        active
            .iter()
            .rev()
            .find(|t| {
                t.interpreters
                    .iter()
                    .any(|i| interpreter_matches(i, &interpreter))
            })
            .copied()
    }

    /// Resolve one file for the editor.
    pub fn info(&self, file: &FileRef, req: &DetectRequest) -> FileTypeInfo {
        let spec = self.detect(
            &file.path,
            file.first_line.as_deref(),
            &req.inactive_plugins,
        );
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
            };
        };
        let data_language = spec
            .template
            .then(|| data_language_for(&file.path, &req.data_languages));
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
        }
    }

    /// Resolve every file of a request.
    pub fn resolve(&self, req: &DetectRequest) -> Vec<FileTypeInfo> {
        req.files.iter().map(|f| self.info(f, req)).collect()
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
        if name == ext || name.strip_suffix(ext.as_str()).is_some_and(|s| s.ends_with('.')) {
            best = best.max(ext.len() + 1);
        }
    }
    best
}

/// The interpreter of a `#!` line: `#!/usr/bin/env -S python3 -u` → `python3`.
fn shebang_interpreter(line: &str) -> Option<String> {
    let rest = line.strip_prefix("#!")?;
    let mut words = rest.split_whitespace();
    let program = words.next()?.rsplit('/').next()?;
    let program = if program == "env" {
        words.find(|w| !w.starts_with('-') && !w.contains('='))?
    } else {
        program
    };
    Some(program.to_lowercase())
}

/// `python` matches `python`, `python3` and `python3.12`, not `pythonista`.
fn interpreter_matches(name: &str, interpreter: &str) -> bool {
    interpreter
        .strip_prefix(&name.to_lowercase())
        .is_some_and(|v| v.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

#[cfg(test)]
mod tests;
