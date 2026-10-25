//! The project's hook index: every hook reference and function declaration of its PHP
//! files (WordPress core included when the project contains it), plus the functions of the
//! WordPress stubs when they are installed.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::core_hook;
use crate::hooks::{self, pattern_matches, FunctionDecl, HookKind, HookRef, HookRole};
use crate::lexer;
use crate::Location;

/// Folders never indexed.
const SKIP_DIRS: &[&str] = &[
    "vendor",
    "node_modules",
    ".git",
    ".svn",
    ".hg",
    ".idea",
    "uploads",
    "cache",
];
/// Upper bounds of one scan.
const MAX_FILES: usize = 40_000;
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct HookIndex {
    pub refs: Vec<HookRef>,
    pub functions: Vec<FunctionDecl>,
    /// Functions declared by the WordPress stubs (`php-stubs/wordpress-stubs`).
    pub stub_functions: Vec<FunctionDecl>,
    /// PHP files indexed.
    pub files: usize,
}

/// One hook with its counts, for the tool window and completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSummary {
    pub name: String,
    pub kind: HookKind,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dynamic: bool,
    pub registrations: usize,
    pub invocations: usize,
    /// Where it fires (else where it is first used).
    pub location: Location,
    /// A WordPress core hook from Forge's list (with its summary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl HookIndex {
    /// Index one file (paths relative to the project root, `/` separators).
    pub fn add_file(&mut self, path: &str, src: &str) {
        let tokens = lexer::tokenize(src);
        self.refs.extend(hooks::scan_tokens(path, src, &tokens));
        self.functions.extend(hooks::functions(path, src, &tokens));
        self.files += 1;
    }

    pub fn from_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut index = Self::default();
        for (path, src) in files {
            if is_php_path(path) {
                index.add_file(path, src);
            }
        }
        index
    }

    /// Index the PHP files below `root`.
    pub fn scan(root: &Path) -> Self {
        let mut index = Self::default();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let name = e.file_name().to_string_lossy().to_string();
                let Ok(kind) = e.file_type() else { continue };
                if kind.is_dir() {
                    if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                        stack.push(e.path());
                    }
                    continue;
                }
                if !kind.is_file() || !is_php_path(&name) || index.files >= MAX_FILES {
                    continue;
                }
                if e.metadata()
                    .map(|m| m.len() > MAX_FILE_SIZE)
                    .unwrap_or(true)
                {
                    continue;
                }
                let Ok(src) = std::fs::read_to_string(e.path()) else {
                    continue;
                };
                let rel = relative(root, &e.path());
                index.add_file(&rel, &src);
            }
        }
        index
    }

    /// Add the declarations of the WordPress stubs file (`rel` is its project path).
    pub fn add_stubs(&mut self, rel: &str, src: &str) {
        let tokens = lexer::tokenize(src);
        self.stub_functions = hooks::functions(rel, src, &tokens)
            .into_iter()
            .filter(|f| f.class.is_none())
            .collect();
    }

    /// The references of the index, with `path`'s replaced by `current` (an unsaved buffer).
    pub fn refs_with<'a>(
        &'a self,
        path: &'a str,
        current: &'a [HookRef],
    ) -> impl Iterator<Item = &'a HookRef> + 'a {
        self.refs
            .iter()
            .filter(move |r| r.path != path)
            .chain(current.iter())
    }

    /// The other side of a reference: the invocations of a registered (or removed / checked)
    /// hook, the registrations of an invoked one. Kinds must agree.
    pub fn related<'a>(
        &'a self,
        r: &HookRef,
        path: &'a str,
        current: &'a [HookRef],
    ) -> Vec<&'a HookRef> {
        let want = if r.role == HookRole::Invocation {
            HookRole::Registration
        } else {
            HookRole::Invocation
        };
        self.refs_with(path, current)
            .filter(|o| o.role == want && o.kind == r.kind && names_match(r, o))
            .collect()
    }

    /// All references to the hook of `r` (registrations, invocations, removals, checks).
    pub fn usages<'a>(
        &'a self,
        r: &HookRef,
        path: &'a str,
        current: &'a [HookRef],
    ) -> Vec<&'a HookRef> {
        let mut out: Vec<&HookRef> = self
            .refs_with(path, current)
            .filter(|o| o.kind == r.kind && names_match(r, o))
            .collect();
        out.sort_by(|a, b| (&a.path, a.line, a.column).cmp(&(&b.path, b.line, b.column)));
        out
    }

    /// Every hook, sorted by name.
    pub fn hooks(&self) -> Vec<HookSummary> {
        let mut by: BTreeMap<(String, HookKind), HookSummary> = BTreeMap::new();
        for r in &self.refs {
            let e = by
                .entry((r.name.clone(), r.kind))
                .or_insert_with(|| HookSummary {
                    name: r.name.clone(),
                    kind: r.kind,
                    dynamic: r.dynamic,
                    registrations: 0,
                    invocations: 0,
                    location: Location::new(&r.path, r.line, r.column),
                    summary: core_hook(&r.name).map(|(_, s)| s.to_string()),
                });
            match r.role {
                HookRole::Registration => e.registrations += 1,
                HookRole::Invocation => {
                    if e.invocations == 0 {
                        e.location = Location::new(&r.path, r.line, r.column);
                    }
                    e.invocations += 1;
                }
                _ => {}
            }
        }
        by.into_values().collect()
    }

    /// Declarations of the function or method `name` a callback names: project functions
    /// first (the current file's first), then the stubs.
    pub fn function_decls(&self, name: &str, prefer: &str) -> Vec<&FunctionDecl> {
        let mut found: Vec<&FunctionDecl> = self
            .functions
            .iter()
            .filter(|f| f.name.eq_ignore_ascii_case(name))
            .collect();
        found.sort_by_key(|f| f.path != prefer);
        if found.is_empty() {
            found = self
                .stub_functions
                .iter()
                .filter(|f| f.name.eq_ignore_ascii_case(name))
                .collect();
        }
        found
    }
}

/// Whether two references name the same hook (a static name matches a pattern).
pub fn names_match(a: &HookRef, b: &HookRef) -> bool {
    match (a.dynamic, b.dynamic) {
        (false, false) | (true, true) => a.name == b.name,
        (true, false) => pattern_matches(&a.name, &b.name),
        (false, true) => pattern_matches(&b.name, &a.name),
    }
}

pub fn is_php_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".php") || lower.ends_with(".inc") || lower.ends_with(".phtml")
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn demo() -> HookIndex {
        HookIndex::from_files([
            (
                "wp-includes/post.php",
                "<?php\nfunction wp_insert_post() {\n\tdo_action( 'save_post', $id, $post );\n\tdo_action( \"save_post_{$post->post_type}\", $id );\n}\n",
            ),
            (
                "wp-content/plugins/acme/acme.php",
                "<?php\nadd_action( 'save_post', 'acme_save' );\nadd_action( 'save_post_book', 'acme_save_book', 5 );\nfunction acme_save() {}\nfunction acme_save_book() {}\n",
            ),
            ("readme.txt", "do_action('ignored')"),
        ])
    }

    #[test]
    fn summarizes_hooks() {
        let index = demo();
        assert_eq!(index.files, 2);
        let hooks = index.hooks();
        let names: Vec<(&str, usize, usize)> = hooks
            .iter()
            .map(|h| (h.name.as_str(), h.registrations, h.invocations))
            .collect();
        assert_eq!(
            names,
            vec![
                ("save_post", 1, 1),
                ("save_post_book", 1, 0),
                ("save_post_{$post->post_type}", 0, 1),
            ]
        );
        assert_eq!(
            hooks[0].location,
            Location::new("wp-includes/post.php", 2, 13)
        );
        assert!(hooks[0].summary.is_some());
    }

    #[test]
    fn relates_registrations_and_invocations() {
        let index = demo();
        let book = index
            .refs
            .iter()
            .find(|r| r.name == "save_post_book")
            .unwrap();
        let fired = index.related(book, "", &[]);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].name, "save_post_{$post->post_type}");
        let pattern = fired[0];
        let callbacks = index.related(pattern, "", &[]);
        assert_eq!(callbacks.len(), 1);
        assert_eq!(
            callbacks[0].callback_name.as_deref(),
            Some("acme_save_book")
        );
        assert_eq!(index.function_decls("acme_save", "x")[0].line, 3);
    }

    #[test]
    fn overlays_the_current_buffer() {
        let index = demo();
        let current = hooks::scan(
            "wp-content/plugins/acme/acme.php",
            "<?php\nadd_action( 'save_post', 'a' );\nadd_action( 'save_post', 'b' );\n",
        );
        let fired = index
            .refs
            .iter()
            .find(|r| r.role == HookRole::Invocation)
            .unwrap();
        assert_eq!(
            index
                .related(fired, "wp-content/plugins/acme/acme.php", &current)
                .len(),
            2
        );
    }

    #[test]
    fn scans_a_folder_without_vendor() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("inc")).unwrap();
        std::fs::create_dir_all(dir.path().join("vendor/x")).unwrap();
        std::fs::write(dir.path().join("inc/a.php"), "<?php do_action('a');").unwrap();
        std::fs::write(dir.path().join("vendor/x/b.php"), "<?php do_action('b');").unwrap();
        let index = HookIndex::scan(dir.path());
        assert_eq!(index.refs.len(), 1);
        assert_eq!(index.refs[0].path, "inc/a.php");
    }
}
