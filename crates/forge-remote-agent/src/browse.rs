//! Listing absolute directories for the IDE's remote folder chooser.

use std::path::{Component, Path, PathBuf};

use forge_backend::protocol::{BrowseEntry, BrowseResult};
use forge_core::{ForgeError, Result};

/// Files and folders that mark a project folder (shown with a project icon).
const PROJECT_MARKERS: &[&str] = &[
    "composer.json",
    "package.json",
    ".git",
    "artisan",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
];

/// At most this many entries are checked for project markers.
const MARKER_CHECKS: usize = 400;

/// `~` and `~/x` relative to the home directory; other paths unchanged.
pub fn expand_home(path: &str) -> PathBuf {
    let home = || dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    match path.trim() {
        "" | "~" => home(),
        p if p.starts_with("~/") => home().join(&p[2..]),
        p => PathBuf::from(p),
    }
}

/// `path` without `.` / `..` components (lexically, like a shell's `cd`).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push("/");
    }
    out
}

/// One level of an absolute directory: folders first, then files, alphabetical.
pub fn browse(path: &str) -> Result<BrowseResult> {
    let dir = normalize(&expand_home(path));
    if !dir.is_absolute() {
        return Err(ForgeError::Other(format!("not an absolute path: {path}")));
    }
    let read = std::fs::read_dir(&dir).map_err(|e| ForgeError::io(&dir, e))?;
    let mut entries: Vec<BrowseEntry> = read
        .filter_map(|e| e.ok())
        .map(|e| {
            let p = e.path();
            // Follow symlinks: a link to a folder is browsable.
            let is_dir = p.is_dir();
            BrowseEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                path: p.to_string_lossy().into_owned(),
                dir: is_dir,
                project: false,
            }
        })
        .collect();
    entries.sort_by_key(|a| (!a.dir, a.name.to_lowercase()));
    for e in entries.iter_mut().filter(|e| e.dir).take(MARKER_CHECKS) {
        let p = Path::new(&e.path);
        e.project = PROJECT_MARKERS.iter().any(|m| p.join(m).exists());
    }
    Ok(BrowseResult {
        path: dir.to_string_lossy().into_owned(),
        parent: dir.parent().map(|p| p.to_string_lossy().into_owned()),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_first_and_marks_projects() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("shop")).unwrap();
        std::fs::write(dir.path().join("shop/composer.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.path().join("Backups")).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        let root = dir.path().canonicalize().unwrap();
        let listing = browse(&root.join("shop/..").to_string_lossy()).unwrap();
        assert_eq!(listing.path, root.to_string_lossy());
        let names: Vec<(&str, bool, bool)> = listing
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.dir, e.project))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Backups", true, false),
                ("shop", true, true),
                ("notes.txt", false, false)
            ]
        );
    }

    #[test]
    fn relative_paths_are_refused_and_home_is_expanded() {
        assert!(browse("relative/dir").is_err());
        assert_eq!(expand_home("~"), dirs::home_dir().unwrap());
        assert_eq!(
            normalize(Path::new("/var/www/../tmp/.")),
            PathBuf::from("/var/tmp")
        );
        assert_eq!(normalize(Path::new("/..")), PathBuf::from("/"));
    }
}
