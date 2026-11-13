//! Map a requested file to a project: the open one, a recent one that has to be opened
//! first, or none (the user has to confirm opening it).

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::request::OpenRequest;

/// Files that mark a project root when guessing one for a file outside known projects.
const ROOT_MARKERS: &[&str] = &[
    ".forge",
    ".idea",
    ".git",
    "composer.json",
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("relative path {0} needs an open project")]
    NoProject(String),
    #[error("{0} is not a known project")]
    UnknownProject(String),
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("not a file: {0}")]
    NotAFile(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnownProject {
    pub name: String,
    pub root: PathBuf,
}

impl KnownProject {
    pub fn new(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
        }
    }
}

/// The projects Forge knows about when a request comes in.
#[derive(Debug, Clone, Default)]
pub struct ProjectContext {
    pub open: Option<KnownProject>,
    pub recent: Vec<KnownProject>,
}

/// Where a requested file lives. Paths are absolute (root) or root-relative with `/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    /// Inside the open project: open it right away.
    #[serde(rename_all = "camelCase")]
    Open { root: String, path: String },
    /// Inside a recent project that is not open: ask before switching projects.
    #[serde(rename_all = "camelCase")]
    Recent {
        root: String,
        name: String,
        path: String,
    },
    /// Outside every known project: only opened after the user confirms, with
    /// `suggested_root` as the project to open.
    #[serde(rename_all = "camelCase")]
    Outside {
        suggested_root: String,
        path: String,
    },
}

impl Target {
    pub fn root(&self) -> &str {
        match self {
            Target::Open { root, .. } | Target::Recent { root, .. } => root,
            Target::Outside { suggested_root, .. } => suggested_root,
        }
    }

    /// Path relative to [`Target::root`].
    pub fn path(&self) -> &str {
        match self {
            Target::Open { path, .. }
            | Target::Recent { path, .. }
            | Target::Outside { path, .. } => path,
        }
    }
}

/// Remove `.` and `..` without touching the file system (so `..` cannot climb above `/`).
pub fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            // `..` at the root stays at the root; a relative path climbing out of its base
            // keeps the `..` so containment checks fail.
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn canonical(p: &Path) -> Option<PathBuf> {
    p.canonicalize().ok()
}

fn to_slash(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// `abs` relative to `root` if it is inside it (both canonical).
fn relative_to(abs: &Path, root: &Path) -> Option<String> {
    abs.strip_prefix(root).ok().map(to_slash)
}

/// The nearest ancestor of `file` that looks like a project root, else its folder.
pub fn guess_project_root(file: &Path) -> PathBuf {
    let dir = file.parent().unwrap_or(file);
    dir.ancestors()
        .find(|d| ROOT_MARKERS.iter().any(|m| d.join(m).exists()))
        .unwrap_or(dir)
        .to_path_buf()
}

/// Resolve a validated request against the known projects.
///
/// Relative paths are joined to `request.project` (which must be a known project) or the
/// open project. Symlinks are resolved before the containment check, so a link inside a
/// project that points elsewhere counts as outside.
pub fn resolve_file(
    request: &OpenRequest,
    ctx: &ProjectContext,
) -> Result<(PathBuf, Target), ResolveError> {
    let known: Vec<(&KnownProject, PathBuf, bool)> = ctx
        .open
        .iter()
        .map(|p| (p, true))
        .chain(ctx.recent.iter().map(|p| (p, false)))
        .filter_map(|(p, open)| canonical(&p.root).map(|c| (p, c, open)))
        .collect();

    let requested = PathBuf::from(&request.file);
    let joined = if requested.is_absolute() {
        requested
    } else {
        let base = match &request.project {
            Some(project) => {
                let wanted = canonical(Path::new(project))
                    .ok_or_else(|| ResolveError::UnknownProject(project.clone()))?;
                known
                    .iter()
                    .find(|(_, c, _)| *c == wanted)
                    .map(|(_, c, _)| c.clone())
                    .ok_or_else(|| ResolveError::UnknownProject(project.clone()))?
            }
            None => known
                .iter()
                .find(|(_, _, open)| *open)
                .map(|(_, c, _)| c.clone())
                .ok_or_else(|| ResolveError::NoProject(request.file.clone()))?,
        };
        base.join(requested)
    };

    let normalized = normalize_lexically(&joined);
    let abs = canonical(&normalized).ok_or_else(|| ResolveError::NotFound(request.file.clone()))?;
    if !abs.is_file() {
        return Err(ResolveError::NotAFile(request.file.clone()));
    }

    // Deepest root wins, so a project nested in another resolves to the inner one; the open
    // project wins ties.
    let best = known
        .iter()
        .filter_map(|(p, c, open)| relative_to(&abs, c).map(|rel| (p, c, *open, rel)))
        .max_by_key(|(_, c, open, _)| (c.components().count(), *open));

    let target = match best {
        Some((_, c, true, path)) => Target::Open {
            root: c.display().to_string(),
            path,
        },
        Some((p, c, false, path)) => Target::Recent {
            root: c.display().to_string(),
            name: p.name.clone(),
            path,
        },
        None => {
            let root = guess_project_root(&abs);
            Target::Outside {
                path: relative_to(&abs, &root).unwrap_or_default(),
                suggested_root: root.display().to_string(),
            }
        }
    };
    Ok((abs, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture {
        _dir: tempfile::TempDir,
        base: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let base = dir.path().canonicalize().unwrap();
            for f in [
                "shop/src/App.tsx",
                "shop/package.json",
                "shop/packages/ui/src/Button.tsx",
                "blog/app/Models/Post.php",
                "elsewhere/lib/composer.json",
                "elsewhere/lib/src/Util.php",
                "secret.txt",
            ] {
                let p = base.join(f);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, "x").unwrap();
            }
            Self { _dir: dir, base }
        }

        fn ctx(&self) -> ProjectContext {
            ProjectContext {
                open: Some(KnownProject::new("shop", self.base.join("shop"))),
                recent: vec![
                    KnownProject::new("blog", self.base.join("blog")),
                    KnownProject::new("ui", self.base.join("shop/packages/ui")),
                    KnownProject::new("gone", self.base.join("deleted")),
                ],
            }
        }

        fn abs(&self, rel: &str) -> String {
            self.base.join(rel).display().to_string()
        }
    }

    fn req(file: &str) -> OpenRequest {
        OpenRequest::new(file)
    }

    #[test]
    fn file_in_open_project() {
        let f = Fixture::new();
        let (abs, target) = resolve_file(&req(&f.abs("shop/src/App.tsx")), &f.ctx()).unwrap();
        assert_eq!(abs, f.base.join("shop/src/App.tsx"));
        assert_eq!(
            target,
            Target::Open {
                root: f.abs("shop"),
                path: "src/App.tsx".into()
            }
        );
    }

    #[test]
    fn relative_paths_use_open_or_given_project() {
        let f = Fixture::new();
        let (_, t) = resolve_file(&req("./src/../src/App.tsx"), &f.ctx()).unwrap();
        assert_eq!(t.path(), "src/App.tsx");

        let with_project = OpenRequest {
            project: Some(f.abs("blog")),
            ..req("app/Models/Post.php")
        };
        let (_, t) = resolve_file(&with_project, &f.ctx()).unwrap();
        assert!(matches!(t, Target::Recent { ref name, .. } if name == "blog"));

        let unknown = OpenRequest {
            project: Some(f.abs("elsewhere/lib")),
            ..req("src/Util.php")
        };
        assert!(matches!(
            resolve_file(&unknown, &f.ctx()),
            Err(ResolveError::UnknownProject(_))
        ));

        let no_open = ProjectContext {
            open: None,
            ..f.ctx()
        };
        assert!(matches!(
            resolve_file(&req("src/App.tsx"), &no_open),
            Err(ResolveError::NoProject(_))
        ));
    }

    #[test]
    fn relative_paths_cannot_escape_the_project() {
        let f = Fixture::new();
        let (_, t) = resolve_file(&req("../secret.txt"), &f.ctx()).unwrap();
        assert!(
            matches!(t, Target::Outside { .. }),
            "escaping with .. must need confirmation, got {t:?}"
        );
    }

    #[test]
    fn recent_and_nested_projects() {
        let f = Fixture::new();
        let (_, t) = resolve_file(&req(&f.abs("blog/app/Models/Post.php")), &f.ctx()).unwrap();
        assert_eq!(
            t,
            Target::Recent {
                root: f.abs("blog"),
                name: "blog".into(),
                path: "app/Models/Post.php".into()
            }
        );
        // The nested recent project is deeper than the open one.
        let (_, t) =
            resolve_file(&req(&f.abs("shop/packages/ui/src/Button.tsx")), &f.ctx()).unwrap();
        assert!(
            matches!(t, Target::Recent { ref name, ref path, .. } if name == "ui" && path == "src/Button.tsx")
        );
    }

    #[test]
    fn outside_files_get_a_suggested_root() {
        let f = Fixture::new();
        let (_, t) = resolve_file(&req(&f.abs("elsewhere/lib/src/Util.php")), &f.ctx()).unwrap();
        assert_eq!(
            t,
            Target::Outside {
                suggested_root: f.abs("elsewhere/lib"),
                path: "src/Util.php".into()
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_out_of_a_project_count_as_outside() {
        let f = Fixture::new();
        std::os::unix::fs::symlink(f.base.join("secret.txt"), f.base.join("shop/src/link.txt"))
            .unwrap();
        let (abs, t) = resolve_file(&req("src/link.txt"), &f.ctx()).unwrap();
        assert_eq!(abs, f.base.join("secret.txt"));
        assert!(matches!(t, Target::Outside { .. }));
    }

    #[test]
    fn missing_files_and_directories_are_errors() {
        let f = Fixture::new();
        assert!(matches!(
            resolve_file(&req(&f.abs("shop/nope.ts")), &f.ctx()),
            Err(ResolveError::NotFound(_))
        ));
        assert!(matches!(
            resolve_file(&req(&f.abs("shop/src")), &f.ctx()),
            Err(ResolveError::NotAFile(_))
        ));
    }

    #[test]
    fn lexical_normalization() {
        assert_eq!(
            normalize_lexically(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(
            normalize_lexically(Path::new("/../../etc")),
            PathBuf::from("/etc")
        );
        assert_eq!(
            normalize_lexically(Path::new("a/../../b/../../c")),
            PathBuf::from("../../c")
        );
    }
}
