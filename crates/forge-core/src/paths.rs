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

/// `abs` relative to `root` with `/` separators (`app/Models/User.php`), or `None` when it is
/// not inside. Compares the spellings Windows mixes up: verbatim prefixes, `\` and `/`,
/// and (for drive and UNC paths, which are case-insensitive) letter case, so a path from a
/// language server (`c:\work\shop\a.php`) matches the project root (`C:\work\shop`).
/// Works on any platform.
pub fn relative_to(root: &str, abs: &str) -> Option<String> {
    let (root, abs) = (comparable(root), comparable(abs));
    let windows = is_windows_path(&root);
    let root = root.trim_end_matches('/');
    let rest = if windows {
        let head = abs.get(..root.len())?;
        head.eq_ignore_ascii_case(root)
            .then(|| &abs[root.len()..])?
    } else {
        abs.strip_prefix(root)?
    };
    if rest.is_empty() {
        return Some(String::new());
    }
    let rest = rest.strip_prefix('/')?;
    Some(rest.trim_end_matches('/').to_string())
}

/// `\\?\C:\a\b` → `C:/a/b`, `\\server\share` → `//server/share`.
fn comparable(path: &str) -> String {
    let path = simplify_verbatim(path).unwrap_or_else(|| path.to_string());
    if path.starts_with('/') && !path.starts_with("//") {
        // POSIX path: backslashes are ordinary characters there.
        return path;
    }
    let mut path = path.replace('\\', "/");
    let b = path.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        let drive = path[..1].to_ascii_uppercase();
        path.replace_range(..1, &drive);
    }
    path
}

fn is_windows_path(path: &str) -> bool {
    let b = path.as_bytes();
    path.starts_with("//") || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
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

/// Whether `path` is `root` or lies inside it. The one comparison behind "is this a project
/// file or program?" decisions that guard Safe Mode, so a spelling cannot sneak a project
/// file past them:
///
/// - as written: Windows' verbatim prefix (`\\?\C:\…`, `\\?\UNC\…`), `\` and `/`, `.` and
///   `..`, and letter case where the file system ignores it (drive and UNC paths everywhere,
///   every path on Windows and macOS);
/// - as the file system resolves both ([`resolve_existing`]: symlinks, `/proc/self/root/…`).
///
/// Either view counting `path` as inside is enough. Relative paths are never inside.
pub fn is_within(root: &Path, path: &Path) -> bool {
    let fold = cfg!(any(windows, target_os = "macos"));
    let lexical = within(&root.to_string_lossy(), &path.to_string_lossy(), fold);
    lexical
        || match (resolve_existing(root), resolve_existing(path)) {
            (Some(r), Some(p)) => within(&r.to_string_lossy(), &p.to_string_lossy(), fold),
            _ => false,
        }
}

/// `path` as the file system resolves it ([`canonicalize`]), also when it does not exist
/// (yet): its deepest existing folder is resolved and the rest appended. `None` when nothing
/// of it resolves, or a part that does not exist is `..`.
pub fn resolve_existing(path: &Path) -> Option<PathBuf> {
    let mut existing = path;
    let mut rest = Vec::new();
    loop {
        if let Ok(mut out) = canonicalize(existing) {
            for part in rest.iter().rev() {
                out.push(part);
            }
            return Some(out);
        }
        rest.push(existing.file_name()?.to_os_string());
        existing = existing.parent()?;
    }
}

/// [`is_within`] on the spelling alone; `fold_posix` folds the case of POSIX paths too.
/// Works on any platform.
pub fn within(root: &str, path: &str, fold_posix: bool) -> bool {
    let (Some(root), Some(path)) = (lexical(root), lexical(path)) else {
        return false;
    };
    let fold = fold_posix || is_windows_path(&root) || is_windows_path(&path);
    let (root, path) = if fold {
        (root.to_lowercase(), path.to_lowercase())
    } else {
        (root, path)
    };
    if root == path {
        return true;
    }
    let dir = if root.ends_with('/') {
        root
    } else {
        format!("{root}/")
    };
    path.starts_with(&dir)
}

/// An absolute path with `/` separators, without a verbatim prefix and without `.` and `..`
/// (`\\?\C:\a\..\b` → `C:/b`); `None` for a relative path.
fn lexical(path: &str) -> Option<String> {
    let mut path = comparable(path.trim());
    // Verbatim paths `simplify_verbatim` keeps (long ones, with `..`): drop the prefix.
    if let Some(rest) = path.strip_prefix("//?/") {
        path = match rest
            .strip_prefix("UNC/")
            .or_else(|| rest.strip_prefix("unc/"))
        {
            Some(unc) => format!("//{unc}"),
            None => comparable(rest),
        };
    }
    let (prefix, rest) = if let Some(rest) = path.strip_prefix("//") {
        ("//".to_string(), rest)
    } else if is_windows_path(&path) {
        (
            format!("{}/", &path[..2]),
            path[2..].trim_start_matches('/'),
        )
    } else {
        ("/".to_string(), path.strip_prefix('/')?)
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    Some(format!("{prefix}{}", parts.join("/")))
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
    fn relative_paths_survive_windows_spellings() {
        let rel = |root: &str, abs: &str| relative_to(root, abs);
        // php-lsp: lower-case drive; Forge's root: canonicalized, maybe verbatim.
        assert_eq!(
            rel(r"C:\work\shop", r"c:\work\shop\app\Models\User.php").as_deref(),
            Some("app/Models/User.php")
        );
        assert_eq!(
            rel(r"\\?\C:\work\shop", r"C:\work\shop\a.php").as_deref(),
            Some("a.php")
        );
        assert_eq!(
            rel(r"C:\Work\Shop\", "C:/work/shop/src/a.php").as_deref(),
            Some("src/a.php")
        );
        assert_eq!(
            rel(r"\\?\UNC\nas\www\shop", r"\\nas\www\shop\a.php").as_deref(),
            Some("a.php")
        );
        assert_eq!(rel(r"C:\work\shop", r"C:\work\shop").as_deref(), Some(""));
        // Not inside.
        assert_eq!(rel(r"C:\work\shop", r"C:\work\shopping\a.php"), None);
        assert_eq!(rel(r"C:\work\shop", r"D:\work\shop\a.php"), None);
        // POSIX paths stay case-sensitive.
        assert_eq!(
            rel("/p/shop", "/p/shop/app/User.php").as_deref(),
            Some("app/User.php")
        );
        assert_eq!(rel("/p/shop", "/p/Shop/app/User.php"), None);
        assert_eq!(rel("/p/shop/", "/p/shop/a.php").as_deref(), Some("a.php"));
        assert_eq!(rel("/p/shop", "/stubs/a.php"), None);
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

    #[test]
    fn within_compares_spellings() {
        // Windows' verbatim prefix (canonicalize) against the ordinary root.
        assert!(within(r"C:\proj", r"\\?\C:\proj\build.ps1", false));
        assert!(within(r"\\?\C:\proj", r"C:\proj\build.ps1", false));
        assert!(within(r"C:\proj", r"\\?\C:\proj\a\..\build.ps1", false));
        assert!(within(
            r"\\server\share\proj",
            r"\\?\UNC\server\share\proj\x",
            false
        ));
        // Drive and UNC paths ignore letter case everywhere.
        assert!(within(
            "C:/Users/me/proj",
            r"c:\users\ME\PROJ\evil.exe",
            false
        ));
        // POSIX paths only where the file system does.
        assert!(!within("/users/me/proj", "/Users/me/PROJ/evil", false));
        assert!(within("/users/me/proj", "/Users/me/PROJ/evil", true));
        assert!(within("/home/me/proj", "/home/me/proj", false));
        assert!(within("/home/me/proj/", "/home/me/proj/./a/../b", false));
        assert!(!within("/home/me/proj", "/home/me/project/x", false));
        assert!(!within("/home/me/proj", "/home/me/proj/../other/x", false));
        assert!(!within("/home/me/proj", "proj/x", false));
        assert!(!within("C:/proj", "D:/proj/x", false));
        assert!(within("/", "/anything", false));
    }

    #[test]
    fn within_resolves_the_file_system() {
        let dir = tempfile::tempdir().unwrap();
        let root = canonicalize(dir.path()).unwrap();
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("bin")).unwrap();
        // Files that do not exist yet count by their folder.
        assert!(is_within(&proj, &proj.join("bin/new/tool")));
        assert!(!is_within(&proj, &root.join("other/tool")));
        #[cfg(unix)]
        {
            // A link outside the project that points into it.
            std::os::unix::fs::symlink(proj.join("bin"), root.join("link")).unwrap();
            assert!(is_within(&proj, &root.join("link/evil")));
            // A link inside the project that points out of it still is a project path.
            assert!(is_within(&proj, &proj.join("bin/../x")));
        }
        #[cfg(target_os = "linux")]
        {
            let via_proc = format!("/proc/self/root{}", proj.join("evil").display());
            assert!(is_within(&proj, Path::new(&via_proc)));
        }
    }
}
