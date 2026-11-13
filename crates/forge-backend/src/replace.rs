//! Replace in Files over any backend: the files come from `find_in_files`, are read and written
//! through the backend, so remote projects work the same. The text logic is `forge_find::Finder`.

use forge_core::search::FindRequest;
use forge_core::{ForgeError, Result};
use forge_find::{Finder, ReplacePreview};
use serde::Serialize;

use crate::Backend;

/// Matches a replace can touch in one run; past this the search result is cut.
const MAX_MATCHES: usize = 5_000_000;

/// The changed lines of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub lines: Vec<ReplacePreview>,
    /// Matches in the file, counting those on lines beyond the preview limit.
    pub matches: usize,
}

/// What a replace changed, and what is needed to undo it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceDone {
    pub files: usize,
    pub replacements: usize,
    /// Files that could not be read as text or written, with the reason.
    pub skipped: Vec<String>,
    #[serde(skip)]
    pub undo: Vec<Restore>,
}

/// One file to put back: its text before and after the replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restore {
    pub path: String,
    pub before: String,
    pub after: String,
}

fn finder_and_files(backend: &dyn Backend, req: &FindRequest) -> Result<(Finder, Vec<String>)> {
    let finder = Finder::new(req).map_err(ForgeError::Other)?;
    let mut search = req.clone();
    search.limit = MAX_MATCHES;
    search.context = 0;
    let found = backend.find_in_files(&search)?;
    if let Some(error) = found.error {
        return Err(ForgeError::Other(error));
    }
    Ok((finder, found.files.into_iter().map(|f| f.path).collect()))
}

/// The lines a replace would change, at most `lines_per_file` per file and `max_files` files.
pub fn preview(
    backend: &dyn Backend,
    req: &FindRequest,
    with: &str,
    max_files: usize,
    lines_per_file: usize,
) -> Result<Vec<FilePreview>> {
    let (finder, files) = finder_and_files(backend, req)?;
    let mut out = Vec::new();
    for path in files.into_iter().take(max_files) {
        let Ok(text) = backend.read_file(&path) else {
            continue;
        };
        let (_, matches) = finder.replace_text(&text, with);
        let lines = finder.replace_preview(&text, with, lines_per_file);
        if !lines.is_empty() {
            out.push(FilePreview {
                path,
                lines,
                matches,
            });
        }
    }
    Ok(out)
}

/// Replaces every match in the files `req` finds.
pub fn replace_all(backend: &dyn Backend, req: &FindRequest, with: &str) -> Result<ReplaceDone> {
    let (finder, files) = finder_and_files(backend, req)?;
    let mut done = ReplaceDone::default();
    for path in files {
        let text = match backend.read_file(&path) {
            Ok(text) => text,
            Err(e) => {
                done.skipped.push(format!("{path}: {e}"));
                continue;
            }
        };
        let (after, count) = finder.replace_text(&text, with);
        if count == 0 {
            continue;
        }
        if let Err(e) = backend.write_file(&path, &after) {
            done.skipped.push(format!("{path}: {e}"));
            continue;
        }
        done.files += 1;
        done.replacements += count;
        done.undo.push(Restore {
            path,
            before: text,
            after,
        });
    }
    Ok(done)
}

/// Puts the files of `undo` back, leaving out any that changed since the replace.
/// Returns how many were restored and the paths that were left alone.
pub fn undo(backend: &dyn Backend, undo: &[Restore]) -> Result<(usize, Vec<String>)> {
    let (mut restored, mut left) = (0, Vec::new());
    for r in undo {
        match backend.read_file(&r.path) {
            Ok(now) if now == r.after => {
                backend.write_file(&r.path, &r.before)?;
                restored += 1;
            }
            _ => left.push(r.path.clone()),
        }
    }
    Ok((restored, left))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalBackend;

    fn project() -> (tempfile::TempDir, LocalBackend) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.php"), "old one\nkeep\nold two\r\n").unwrap();
        std::fs::write(dir.path().join("src/b.php"), "nothing here\n").unwrap();
        std::fs::write(dir.path().join("c.txt"), "an old file\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        (dir, backend)
    }

    fn req(query: &str) -> FindRequest {
        FindRequest {
            query: query.into(),
            ..FindRequest::default()
        }
    }

    #[test]
    fn previews_replaces_and_undoes_through_the_backend() {
        let (dir, backend) = project();
        let seen = preview(&backend, &req("old"), "new", 10, 10).unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].path, "c.txt");
        assert_eq!(seen[1].lines[1].after, "new two");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/a.php")).unwrap(),
            "old one\nkeep\nold two\r\n"
        );

        let mut scoped = req("old");
        scoped.directory = "src".into();
        let done = replace_all(&backend, &scoped, "new").unwrap();
        assert_eq!((done.files, done.replacements), (1, 2));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/a.php")).unwrap(),
            "new one\nkeep\nnew two\r\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("c.txt")).unwrap(),
            "an old file\n"
        );

        let (restored, left) = undo(&backend, &done.undo).unwrap();
        assert_eq!((restored, left.len()), (1, 0));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/a.php")).unwrap(),
            "old one\nkeep\nold two\r\n"
        );
    }

    #[test]
    fn undo_leaves_a_file_alone_that_changed_since() {
        let (dir, backend) = project();
        let done = replace_all(&backend, &req("old"), "new").unwrap();
        std::fs::write(dir.path().join("c.txt"), "edited since\n").unwrap();
        let (restored, left) = undo(&backend, &done.undo).unwrap();
        assert_eq!(restored, 1);
        assert_eq!(left, vec!["c.txt".to_string()]);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("c.txt")).unwrap(),
            "edited since\n"
        );
    }

    #[test]
    fn a_bad_pattern_is_an_error_and_changes_nothing() {
        let (dir, backend) = project();
        let mut bad = req("(");
        bad.regex = true;
        assert!(replace_all(&backend, &bad, "x").is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("c.txt")).unwrap(),
            "an old file\n"
        );
    }
}
