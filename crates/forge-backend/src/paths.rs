//! Path helpers that do not touch the file system (the project may live on another machine).

use std::path::{Component, Path, PathBuf};

use forge_core::{ForgeError, Result};

/// `root` joined with a workspace-relative path, refusing `..` escapes and absolute paths.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    let mut out = root.to_path_buf();
    for c in rel_path.components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return Err(ForgeError::OutsideWorkspace(rel_path.to_path_buf())),
        }
    }
    Ok(out)
}

/// Workspace-relative, `/`-separated form of `abs`, or `None` when it is outside `root`.
pub fn relative(root: &Path, abs: &Path) -> Option<String> {
    abs.strip_prefix(root).ok().map(|rel| {
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_inside_the_root_only() {
        let root = Path::new("/var/www/shop");
        assert_eq!(
            resolve(root, "app/Models/User.php").unwrap(),
            PathBuf::from("/var/www/shop/app/Models/User.php")
        );
        assert_eq!(
            resolve(root, "./a").unwrap(),
            PathBuf::from("/var/www/shop/a")
        );
        assert!(resolve(root, "../etc/passwd").is_err());
        assert!(resolve(root, "/etc/passwd").is_err());
    }

    #[test]
    fn relative_paths_use_slashes() {
        let root = Path::new("/var/www/shop");
        assert_eq!(
            relative(root, Path::new("/var/www/shop/app/x.php")).as_deref(),
            Some("app/x.php")
        );
        assert_eq!(relative(root, Path::new("/tmp/x")), None);
    }
}
