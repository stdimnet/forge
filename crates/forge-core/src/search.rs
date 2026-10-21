//! Fuzzy file search for "Go to File" / Search Everywhere, and Find in Files.
//!
//! The matchers live in the pure `forge-fuzzy` and `forge-find` crates (also compiled to
//! WebAssembly for the browser-mode UI); this module walks the workspace.

pub use forge_find::{FileMatches, FindPaged, FindRequest, FindResult};
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
        Ok(forge_fuzzy::rank_files_with(
            query,
            paths.iter().map(String::as_str),
            limit,
            opts,
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
        let walker = ignore::WalkBuilder::new(start)
            .hidden(false)
            .require_git(false)
            .filter_entry(|e| !DEFAULT_EXCLUDES.contains(&e.file_name().to_string_lossy().as_ref()))
            .build();
        let mut collector = forge_find::Collector::new(&finder);
        for entry in walker.filter_map(|e| e.ok()).take(MAX_WALKED_FILES) {
            if collector.full() {
                break;
            }
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let rel = self.relative(entry.path());
            if !finder.wants(&rel)
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
            collector.add(&rel, &String::from_utf8_lossy(&bytes));
        }
        Ok(collector.finish())
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
}
