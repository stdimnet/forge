//! Fuzzy file search for "Go to File" / Search Everywhere, and Find in Files.
//!
//! The matchers live in the pure `forge-fuzzy` and `forge-find` crates (also compiled to
//! WebAssembly for the browser-mode UI); this module walks the workspace.

pub use forge_find::todo::{TodoRequest, TodoResult};
pub use forge_find::{FindRequest, FindResult};
pub use forge_fuzzy::{
    fuzzy_match, fuzzy_match_with, match_file, FileMatch, FuzzyMatch, MatchOptions,
};

use crate::workspace::{Workspace, DEFAULT_EXCLUDES};
use crate::Result;

/// Hard cap on files visited per query so huge trees stay responsive.
const MAX_WALKED_FILES: usize = 200_000;

impl Workspace {
    /// Find files whose name (or path, if the query contains `/`) fuzzy-matches `query`.
    /// Respects `.gitignore`/`.ignore` and the default excludes; best matches first.
    pub fn find_files(&self, query: &str, limit: usize) -> Result<Vec<FileMatch>> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(vec![]);
        }
        let walker = ignore::WalkBuilder::new(self.root())
            .hidden(false)
            .require_git(false)
            .filter_entry(|e| !DEFAULT_EXCLUDES.contains(&e.file_name().to_string_lossy().as_ref()))
            .build();
        let paths: Vec<String> = walker
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .take(MAX_WALKED_FILES)
            .map(|e| self.relative(e.path()))
            .collect();
        Ok(forge_fuzzy::rank_files(
            query,
            paths.iter().map(String::as_str),
            limit,
        ))
    }

    /// Find in Files: search the text of every file in scope (`.gitignore`/`.ignore` and the
    /// default excludes respected; binary and huge files skipped). An invalid query is
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
            |rel| finder.wants(rel),
            |rel, text| {
                collector.add(rel, text);
                !collector.full()
            },
        );
        Ok(collector.finish())
    }

    /// TODO tool window: TODO patterns in the comments of every project file (or only of
    /// `req.paths`), `.gitignore` and the default excludes respected. An invalid pattern is
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
                            .is_ok_and(|m| m.is_file() && m.len() <= forge_find::MAX_FILE_SIZE)
                    {
                        continue;
                    }
                    let Ok(bytes) = std::fs::read(&abs) else {
                        continue;
                    };
                    if !forge_find::is_binary(&bytes) {
                        collector.add(rel, &String::from_utf8_lossy(&bytes));
                    }
                }
            }
            None => {
                let root = self.root().to_path_buf();
                self.walk_text_files(
                    &root,
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
    /// default excludes respected, binary and huge files skipped) until `visit` returns false.
    fn walk_text_files(
        &self,
        start: &std::path::Path,
        wants: impl Fn(&str) -> bool,
        mut visit: impl FnMut(&str, &str) -> bool,
    ) {
        let walker = ignore::WalkBuilder::new(start)
            .hidden(false)
            .require_git(false)
            .filter_entry(|e| !DEFAULT_EXCLUDES.contains(&e.file_name().to_string_lossy().as_ref()))
            .build();
        for entry in walker.filter_map(|e| e.ok()).take(MAX_WALKED_FILES) {
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let rel = self.relative(entry.path());
            if !wants(&rel)
                || entry
                    .metadata()
                    .is_ok_and(|m| m.len() > forge_find::MAX_FILE_SIZE)
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            if forge_find::is_binary(&bytes) {
                continue;
            }
            if !visit(&rel, &String::from_utf8_lossy(&bytes)) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
