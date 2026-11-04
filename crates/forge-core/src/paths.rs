//! Path spelling that works the same on every platform.
//!
//! On Windows, [`std::fs::canonicalize`] returns verbatim paths (`\\?\C:\work\shop`,
//! `\\?\UNC\server\share\shop`). Many programs do not understand them (language servers,
//! Git, PHP, `cmd.exe`), they never compare equal to the ordinary spelling of the same file
//! (`C:\work\shop`), and URIs built from them can end up as `file:////?/C:/…`. Forge keeps
//! paths in the ordinary spelling: [`canonicalize`] resolves like the standard library and
//! then drops the verbatim prefix whenever that does not change the meaning of the path.

use std::path::{Path, PathBuf};

/// [`std::fs::canonicalize`] without Windows' verbatim `\\?\` prefix (when the path means the
/// same without it). Identical to the standard function on other platforms.
pub fn canonicalize(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(|p| simplified(&p))
}

/// `path` without Windows' verbatim `\\?\` prefix, when that keeps its meaning (see
/// [`simplify_verbatim`]). Other paths, and every path on other platforms, are returned
/// unchanged.
pub fn simplified(path: &Path) -> PathBuf {
    if cfg!(windows) {
        if let Some(simple) = path.to_str().and_then(simplify_verbatim) {
            return PathBuf::from(simple);
        }
    }
    path.to_path_buf()
}

/// The ordinary spelling of a Windows verbatim path: `\\?\C:\a\b` → `C:\a\b`,
/// `\\?\UNC\server\share\a` → `\\server\share\a`. `None` when `path` is not a verbatim disk
/// or UNC path, or when dropping the prefix would change its meaning: paths that are too
/// long for the ordinary form, with `/` or `.`/`..` components, components ending in a dot
/// or space, or reserved device names (`NUL`, `COM1`, …). Works on any platform, so the
/// rules are tested everywhere.
pub fn simplify_verbatim(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    let (prefix, tail) = if let Some(unc) = rest
        .strip_prefix("UNC\\")
        .or_else(|| rest.strip_prefix("unc\\"))
    {
        (r"\\".to_string(), unc)
    } else {
        let b = rest.as_bytes();
        if b.len() < 2 || !b[0].is_ascii_alphabetic() || b[1] != b':' {
            return None;
        }
        if b.len() > 2 && b[2] != b'\\' {
            return None;
        }
        (String::new(), rest)
    };
    let simple = format!("{prefix}{tail}");
    // MAX_PATH including the terminating NUL.
    if simple.len() >= 260 || tail.contains('/') {
        return None;
    }
    let components = if prefix.is_empty() {
        // Skip the drive (`C:`).
        tail.split('\\').skip(1).collect::<Vec<_>>()
    } else {
        tail.split('\\').collect()
    };
    let last = components.len().saturating_sub(1);
    for (i, c) in components.iter().enumerate() {
        // A trailing separator leaves one empty component at the end; others are not
        // ordinary (`\\?\C:\a\\b`).
        if c.is_empty() {
            if i == last {
                continue;
            }
            return None;
        }
        if *c == "." || *c == ".." || c.ends_with('.') || c.ends_with(' ') || is_reserved(c) {
            return None;
        }
    }
    Some(simple)
}

/// Windows' reserved device names, also with an extension (`nul.txt`).
fn is_reserved(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let stem = stem.trim_end_matches(' ').to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.len() == 4
        && stem.as_bytes()[3].is_ascii_digit()
        && stem.as_bytes()[3] != b'0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_disk_paths_lose_the_prefix() {
        assert_eq!(
            simplify_verbatim(r"\\?\C:\work\shop").as_deref(),
            Some(r"C:\work\shop")
        );
        assert_eq!(simplify_verbatim(r"\\?\d:\").as_deref(), Some(r"d:\"));
        assert_eq!(simplify_verbatim(r"\\?\C:").as_deref(), Some("C:"));
        assert_eq!(
            simplify_verbatim(r"\\?\C:\Program Files (x86)\Forge\forge.exe").as_deref(),
            Some(r"C:\Program Files (x86)\Forge\forge.exe")
        );
    }

    #[test]
    fn verbatim_unc_paths_become_unc_paths() {
        assert_eq!(
            simplify_verbatim(r"\\?\UNC\server\share\shop").as_deref(),
            Some(r"\\server\share\shop")
        );
    }

    #[test]
    fn paths_that_need_the_prefix_keep_it() {
        // Not verbatim at all.
        assert_eq!(simplify_verbatim(r"C:\work"), None);
        assert_eq!(simplify_verbatim("/home/me/shop"), None);
        // Other verbatim forms (volume GUIDs, devices).
        assert_eq!(simplify_verbatim(r"\\?\Volume{1234}\x"), None);
        assert_eq!(simplify_verbatim(r"\\?\C:x"), None);
        // Components the ordinary form would change.
        assert_eq!(simplify_verbatim(r"\\?\C:\work\nul"), None);
        assert_eq!(simplify_verbatim(r"\\?\C:\work\COM1.txt"), None);
        assert_eq!(simplify_verbatim(r"\\?\C:\work\dots."), None);
        assert_eq!(simplify_verbatim(r"\\?\C:\work\space "), None);
        assert_eq!(simplify_verbatim(r"\\?\C:\work\..\x"), None);
        assert_eq!(simplify_verbatim(r"\\?\C:\a/b"), None);
        assert_eq!(simplify_verbatim(r"\\?\C:\a\\b"), None);
        // Too long for MAX_PATH.
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(simplify_verbatim(&long), None);
        // But a COM port name needs a digit 1-9 after it.
        assert_eq!(
            simplify_verbatim(r"\\?\C:\work\COMPOSER").as_deref(),
            Some(r"C:\work\COMPOSER")
        );
        assert_eq!(
            simplify_verbatim(r"\\?\C:\work\com0").as_deref(),
            Some(r"C:\work\com0")
        );
    }

    #[test]
    fn canonicalize_matches_the_standard_library_off_windows() {
        let dir = tempfile::tempdir().unwrap();
        let expected = std::fs::canonicalize(dir.path()).unwrap();
        let got = canonicalize(dir.path()).unwrap();
        if cfg!(windows) {
            assert!(!got.to_string_lossy().starts_with(r"\\?\"), "{got:?}");
            assert_eq!(simplified(&expected), got);
        } else {
            assert_eq!(got, expected);
        }
    }
}
