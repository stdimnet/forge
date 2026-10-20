//! Fuzzy file search for "Go to File" / Search Everywhere.
//!
//! The matcher itself lives in the pure `forge-fuzzy` crate (also compiled to WebAssembly
//! for the browser-mode UI); this module walks the workspace and ranks its files.

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
}
