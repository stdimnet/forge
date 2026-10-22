//! Copy actions of Compare Directories: a file or folder of one side replaces the other.
//! They go through the [`Backend`] file operations, so they work in remote projects too.

use forge_core::{FileKind, ForgeError, Result};

use crate::Backend;

/// Folders the folder diff never shows, so copies leave them out as well.
const SKIPPED_DIRS: &[&str] = &[".git", ".hg", ".svn"];

/// Copy `from` over `to` (both workspace-relative). A file replaces the target file; a
/// folder is merged into the target folder (files replaced, nothing deleted).
pub fn copy_entry(backend: &dyn Backend, from: &str, to: &str, dir: bool) -> Result<()> {
    let (from, to) = (from.trim_matches('/'), to.trim_matches('/'));
    if from == to {
        return Ok(());
    }
    if !dir {
        let bytes = backend.read_bytes(from)?;
        return backend.write_bytes(to, &bytes);
    }
    if from.is_empty() || to.is_empty() || to.starts_with(&format!("{from}/")) {
        return Err(ForgeError::Other(format!("cannot copy {from} into {to}")));
    }
    // The folder may exist already: the copy merges into it.
    let _ = backend.create_dir(to);
    for entry in backend.list_dir(from)? {
        if entry.kind == FileKind::Dir && SKIPPED_DIRS.contains(&entry.name.as_str()) {
            continue;
        }
        let target = format!("{to}/{}", entry.name);
        copy_entry(backend, &entry.path, &target, entry.kind == FileKind::Dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalBackend;

    #[test]
    fn copies_files_and_merges_folders() {
        let dir = tempfile::tempdir().unwrap();
        let write = |p: &str, c: &str| {
            let path = dir.path().join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, c).unwrap();
        };
        write("left/a.txt", "left a\n");
        write("left/sub/b.txt", "left b\n");
        write("left/.git/HEAD", "x\n");
        write("right/a.txt", "right a\n");
        write("right/keep.txt", "keep\n");
        let backend = LocalBackend::open(dir.path()).unwrap();
        let read = |p: &str| std::fs::read_to_string(dir.path().join(p)).ok();

        copy_entry(&backend, "left/a.txt", "right/a.txt", false).unwrap();
        assert_eq!(read("right/a.txt").as_deref(), Some("left a\n"));

        copy_entry(&backend, "left", "right", true).unwrap();
        assert_eq!(read("right/sub/b.txt").as_deref(), Some("left b\n"));
        assert_eq!(read("right/keep.txt").as_deref(), Some("keep\n"));
        assert_eq!(read("right/.git/HEAD"), None);

        copy_entry(&backend, "left/sub", "fresh", true).unwrap();
        assert_eq!(read("fresh/b.txt").as_deref(), Some("left b\n"));

        assert!(copy_entry(&backend, "left", "left/inner", true).is_err());
        assert!(copy_entry(&backend, "left/a.txt", "../out.txt", false).is_err());
    }
}
