//! Fuzzy file search for "Go to File" / Search Everywhere, and Find in Files.
//!
//! The matchers live in the pure `forge-fuzzy` and `forge-find` crates (also compiled to
//! WebAssembly for the browser-mode UI); this module walks the workspace.

pub use forge_find::todo::{TodoRequest, TodoResult};
pub use forge_find::{FileMatches, FindPaged, FindRequest, FindResult};
pub use forge_fuzzy::{
    fuzzy_match, fuzzy_match_with, match_file, FileMatch, FuzzyMatch, MatchOptions,
};

use serde::{Deserialize, Serialize};

use crate::excludes::skips_entry;
use crate::workspace::Workspace;
use crate::Result;

/// Which files [`Workspace::collect_files`] reads: everything a language index needs in one
/// pass (one round trip for a remote project instead of one per file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectRequest {
    /// File name extensions without the dot (`lua`), compared case-insensitively.
    pub extensions: Vec<String>,
    /// At most this many files are read.
    pub max_files: usize,
    /// Larger files (generated code, bundled libraries) are skipped.
    pub max_file_size: u64,
    /// Reading stops once the texts add up to this many bytes.
    pub max_total: u64,
}

/// A file read by [`Workspace::collect_files`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    /// Workspace-relative, `/`-separated.
    pub path: String,
    pub text: String,
}

/// Larger files are not scanned for TODO comments.
const TODO_MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;
/// Hard cap on files visited per query so huge trees stay responsive.
const MAX_WALKED_FILES: usize = 200_000;

impl Workspace {
    /// Find files whose name (or path, if the query contains `/`) fuzzy-matches `query`.
    /// Respects `.gitignore`/`.ignore` and the excluded folders; best matches first.
    pub fn find_files(&self, query: &str, limit: usize) -> Result<Vec<FileMatch>> {
        self.find_files_with(query, limit, MatchOptions::default())
    }

    /// [`Workspace::find_files`] with matching options (e.g. substring instead of fuzzy).
    pub fn find_files_with(
        &self,
        query: &str,
        limit: usize,
        opts: MatchOptions,
    ) -> Result<Vec<FileMatch>> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(vec![]);
        }
        let paths = self.list_files();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        Ok(forge_fuzzy::rank_files_tolerant(query, &paths, limit, opts))
    }

    /// Every project file (workspace-relative; `.gitignore`/`.ignore` and the default
    /// excludes respected), at most 200,000.
    pub fn list_files(&self) -> Vec<String> {
        let (excludes, root) = (self.excludes().get(), self.root().to_path_buf());
        let walker = ignore::WalkBuilder::new(self.root())
            .hidden(false)
            .require_git(false)
            .filter_entry(move |e| !skips_entry(&excludes, &root, e))
            .build();
        walker
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .take(MAX_WALKED_FILES)
            .map(|e| self.relative(e.path()))
            .collect()
    }

    /// Find in Files: search the text of every file in scope (`.gitignore`/`.ignore` and the
    /// excluded folders respected; binary files and files over [`forge_find::MAX_FILE_SIZE`]
    /// skipped, UTF-16 with a byte order mark decoded). An invalid query is
    /// reported in [`FindResult::error`].
    pub fn find_in_files(&self, req: &FindRequest) -> Result<FindResult> {
        let finder = match forge_find::Finder::new(req) {
            Ok(f) => f,
            Err(e) => {
                return Ok(FindResult {
                    error: Some(e),
                    ..FindResult::default()
                })
            }
        };
        let start = if finder.directory().is_empty() {
            self.root().to_path_buf()
        } else {
            self.resolve(finder.directory())?
        };
        let mut collector = forge_find::Collector::new(&finder);
        self.walk_text_files(
            &start,
            forge_find::MAX_FILE_SIZE,
            |rel| finder.wants(rel),
            |rel, text| {
                collector.add(rel, text);
                !collector.full()
            },
        );
        Ok(collector.finish())
    }

    /// TODO tool window: TODO patterns in the comments of every project file (or only of
    /// `req.paths`), `.gitignore` and the excluded folders respected. An invalid pattern is
    /// reported in [`TodoResult::error`].
    pub fn find_todos(&self, req: &TodoRequest) -> Result<TodoResult> {
        let scanner = match forge_find::todo::TodoScanner::new(req) {
            Ok(s) => s,
            Err(e) => {
                return Ok(TodoResult {
                    error: Some(e),
                    ..TodoResult::default()
                })
            }
        };
        let mut collector = forge_find::todo::TodoCollector::new(&scanner);
        match &req.paths {
            Some(paths) => {
                for rel in paths {
                    if let Some(text) = req.buffers.get(rel) {
                        // Unsaved editor contents win over the file on disk.
                        if scanner.wants(rel) && self.resolve(rel).is_ok() {
                            collector.add(rel, text);
                        }
                        continue;
                    }
                    let Ok(abs) = self.resolve(rel) else { continue };
                    if !scanner.wants(rel)
                        || !std::fs::metadata(&abs)
                            .is_ok_and(|m| m.is_file() && m.len() <= TODO_MAX_FILE_SIZE)
                    {
                        continue;
                    }
                    let Ok(bytes) = std::fs::read(&abs) else {
                        continue;
                    };
                    if let Some(bytes) = forge_find::searchable(bytes) {
                        collector.add(rel, &String::from_utf8_lossy(&bytes));
                    }
                }
            }
            None => {
                let root = self.root().to_path_buf();
                self.walk_text_files(
                    &root,
                    TODO_MAX_FILE_SIZE,
                    |rel| scanner.wants(rel),
                    |rel, text| {
                        collector.add(rel, text);
                        !collector.full()
                    },
                );
            }
        }
        Ok(collector.finish())
    }

    /// Visit the text files under `start` that `wants` accepts (`.gitignore`/`.ignore` and the
    /// excluded folders respected, binary files and files over `max_size` skipped) until `visit`
    /// returns false.
    fn walk_text_files(
        &self,
        start: &std::path::Path,
        max_size: u64,
        wants: impl Fn(&str) -> bool,
        mut visit: impl FnMut(&str, &str) -> bool,
    ) {
        let (excludes, root) = (self.excludes().get(), self.root().to_path_buf());
        let walker = ignore::WalkBuilder::new(start)
            .hidden(false)
            .require_git(false)
            .filter_entry(move |e| !skips_entry(&excludes, &root, e))
            .build();
        for entry in walker.filter_map(|e| e.ok()).take(MAX_WALKED_FILES) {
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let rel = self.relative(entry.path());
            if !wants(&rel) || entry.metadata().is_ok_and(|m| m.len() > max_size) {
                continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            let Some(bytes) = forge_find::searchable(bytes) else {
                continue;
            };
            if !visit(&rel, &String::from_utf8_lossy(&bytes)) {
                break;
            }
        }
    }

    /// The text of every file with one of `req.extensions` (`.gitignore`/`.ignore` and the
    /// excluded folders respected; binary and oversized files skipped), in path order.
    pub fn collect_files(&self, req: &CollectRequest) -> Result<Vec<SourceFile>> {
        let exts: Vec<String> = req
            .extensions
            .iter()
            .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
            .collect();
        let (excludes, root) = (self.excludes().get(), self.root().to_path_buf());
        let walker = ignore::WalkBuilder::new(self.root())
            .hidden(false)
            .require_git(false)
            .sort_by_file_path(|a, b| a.cmp(b))
            .filter_entry(move |e| !skips_entry(&excludes, &root, e))
            .build();
        let mut out = Vec::new();
        let mut total = 0u64;
        for entry in walker.filter_map(|e| e.ok()).take(MAX_WALKED_FILES) {
            if out.len() >= req.max_files {
                break;
            }
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let wanted = entry
                .path()
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .is_some_and(|e| exts.contains(&e));
            let size = entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
            if !wanted || size > req.max_file_size {
                continue;
            }
            if total + size > req.max_total {
                break;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            if forge_find::is_binary(&bytes) {
                continue;
            }
            total += bytes.len() as u64;
            out.push(SourceFile {
                path: self.relative(entry.path()),
                text: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collect_files_reads_matching_files_within_limits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src/lib")).unwrap();
        fs::create_dir_all(root.join("ignored")).unwrap();
        fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        fs::write(root.join("main.lua"), "print(1)").unwrap();
        fs::write(root.join("src/lib/util.LUA"), "return {}").unwrap();
        fs::write(root.join("src/big.lua"), "x".repeat(100)).unwrap();
        fs::write(root.join("src/readme.md"), "# no").unwrap();
        fs::write(root.join("ignored/skip.lua"), "skip()").unwrap();
        let ws = Workspace::open(root).unwrap();
        let req = CollectRequest {
            extensions: vec!["lua".into()],
            max_files: 10,
            max_file_size: 50,
            max_total: 1000,
        };
        let files = ws.collect_files(&req).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["main.lua", "src/lib/util.LUA"]);
        assert_eq!(files[0].text, "print(1)");
        let one = ws
            .collect_files(&CollectRequest {
                max_files: 1,
                ..req.clone()
            })
            .unwrap();
        assert_eq!(one.len(), 1);
        let none = ws
            .collect_files(&CollectRequest {
                max_total: 4,
                ..req
            })
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn find_files_respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("app/Http/Controllers")).unwrap();
        fs::create_dir_all(root.join("vendor/x")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "vendor/\n").unwrap();
        fs::write(root.join("app/Http/Controllers/OrderController.php"), "").unwrap();
        fs::write(root.join("app/Http/Controllers/Controller.php"), "").unwrap();
        fs::write(root.join("vendor/x/OrderCollection.php"), "").unwrap();
        fs::write(root.join(".git/ORIG_HEAD"), "").unwrap();
        let ws = Workspace::open(root).unwrap();

        let hits = ws.find_files("OrCo", 10).unwrap();
        let paths: Vec<_> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, vec!["app/Http/Controllers/OrderController.php"]);

        let hits = ws.find_files("contr", 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Controller.php");
        assert!(ws.find_files("ORIG", 10).unwrap().is_empty());
        assert!(ws.find_files("  ", 10).unwrap().is_empty());
    }

    #[test]
    fn find_in_files_respects_gitignore_scope_and_binaries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("app/Models")).unwrap();
        fs::create_dir_all(root.join("vendor/x")).unwrap();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(root.join(".gitignore"), "vendor/\n").unwrap();
        fs::write(root.join("app/Models/User.php"), "<?php\nclass User {}\n").unwrap();
        fs::write(root.join("tests/UserTest.php"), "new User();\n").unwrap();
        fs::write(root.join("vendor/x/User.php"), "class User {}\n").unwrap();
        fs::write(root.join("app/logo.bin"), b"User\0\x01").unwrap();
        let ws = Workspace::open(root).unwrap();

        let req = |query: &str| FindRequest {
            query: query.into(),
            ..FindRequest::default()
        };
        let r = ws.find_in_files(&req("User")).unwrap();
        let paths: Vec<_> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["app/Models/User.php", "tests/UserTest.php"]);
        assert_eq!(r.files[0].matches[0].line, 1);

        let mut scoped = req("User");
        scoped.directory = "tests".into();
        assert_eq!(ws.find_in_files(&scoped).unwrap().files.len(), 1);
        scoped.directory = "../outside".into();
        assert!(ws.find_in_files(&scoped).is_err());

        let mut bad = req("(");
        bad.regex = true;
        assert!(ws.find_in_files(&bad).unwrap().error.is_some());
    }

    #[test]
    fn find_todos_scans_comments_and_respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("app")).unwrap();
        fs::create_dir_all(root.join("vendor/x")).unwrap();
        fs::write(root.join(".gitignore"), "vendor/\n").unwrap();
        fs::write(
            root.join("app/Order.php"),
            "<?php\n// TODO: ship it\n$s = 'todo';\n",
        )
        .unwrap();
        fs::write(root.join("app/b.js"), "/* FIXME */\n").unwrap();
        fs::write(root.join("vendor/x/Lib.php"), "<?php // TODO vendor\n").unwrap();
        let ws = Workspace::open(root).unwrap();

        let r = ws.find_todos(&TodoRequest::default()).unwrap();
        let paths: Vec<_> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["app/Order.php", "app/b.js"]);
        assert_eq!(r.files[0].items[0].text, "TODO: ship it");
        assert_eq!(r.count, 2);

        let only = TodoRequest {
            paths: Some(vec!["app/b.js".into(), "missing.php".into(), "../x".into()]),
            ..TodoRequest::default()
        };
        let r = ws.find_todos(&only).unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].items[0].text, "FIXME");

        let typed = TodoRequest {
            paths: Some(vec!["app/Order.php".into(), "../escape.php".into()]),
            buffers: [
                (
                    "app/Order.php".to_string(),
                    "<?php\n// FIXME: unsaved\n".to_string(),
                ),
                ("../escape.php".to_string(), "<?php // TODO".to_string()),
            ]
            .into(),
            ..TodoRequest::default()
        };
        let r = ws.find_todos(&typed).unwrap();
        assert_eq!(r.count, 1);
        assert_eq!(r.files[0].items[0].text, "FIXME: unsaved");
    }
}
