//! Open requests for a project on a remote host. The browser knows the paths its dev server
//! reports: the host's absolute paths (`/var/www/shop/src/App.tsx`), paths relative to the
//! project, or paths of another machine or a container (`/app/src/App.tsx`). They are
//! mapped to the remote project's files; whether a file exists is asked through the
//! project's backend.

use std::path::{Component, Path};

use crate::request::OpenRequest;
use crate::resolve::{normalize_lexically, ResolveError, Target};

/// The open remote project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteProject {
    /// The root as the UI shows it and identifies the project: `user@host:/var/www/shop`.
    pub display_root: String,
    /// Absolute root on the host: `/var/www/shop`.
    pub root: String,
}

impl RemoteProject {
    /// How a project file is shown: `user@host:/var/www/shop/src/App.tsx`.
    pub fn display_path(&self, rel: &str) -> String {
        format!("{}/{rel}", self.display_root.trim_end_matches('/'))
    }
}

fn to_slash(p: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for c in p.components() {
        match c {
            Component::Normal(n) => parts.push(n.to_str()?.to_string()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Map `request` to a file of `project`: `(display path, Target::Open)`. `exists` tells
/// whether a project-relative path is a file on the host. Absolute paths below the root
/// map directly; other absolute paths (a container's `/app/…`, another checkout) map to the
/// longest trailing part that exists in the project. Returns
/// [`ResolveError::NotFound`] when the file is not part of the project, so the caller can
/// fall back to local projects.
pub fn resolve_remote(
    request: &OpenRequest,
    project: &RemoteProject,
    exists: &dyn Fn(&str) -> bool,
) -> Result<(String, Target), ResolveError> {
    let not_found = || ResolveError::NotFound(request.file.clone());
    let file = request.file.replace('\\', "/");
    // `user@host:/path` as shown in Forge.
    let file = match file.strip_prefix(&format!("{}/", project.display_root.trim_end_matches('/')))
    {
        Some(rel) => rel.to_string(),
        None => file,
    };
    // A Windows path (`C:/Users/…`) is absolute too.
    let file = match file.as_bytes() {
        [d, b':', b'/', ..] if d.is_ascii_alphabetic() => file[2..].to_string(),
        _ => file,
    };
    let root = normalize_lexically(Path::new(&project.root));
    let requested = normalize_lexically(Path::new(&file));
    let candidates: Vec<String> = if requested.is_absolute() {
        match requested.strip_prefix(&root) {
            Ok(rel) => to_slash(rel).into_iter().collect(),
            // Every trailing part, longest first: `/app/src/App.tsx` → `app/src/App.tsx`,
            // `src/App.tsx`, `App.tsx`.
            Err(_) => {
                let parts: Vec<String> = requested
                    .components()
                    .filter_map(|c| match c {
                        Component::Normal(n) => n.to_str().map(str::to_string),
                        _ => None,
                    })
                    .collect();
                (0..parts.len()).map(|i| parts[i..].join("/")).collect()
            }
        }
    } else {
        // Relative to the project (`project` may name it by either root).
        if let Some(p) = &request.project {
            let p = p.trim_end_matches('/');
            if p != project.display_root.trim_end_matches('/')
                && p != project.root.trim_end_matches('/')
            {
                return Err(ResolveError::UnknownProject(p.to_string()));
            }
        }
        to_slash(&requested).into_iter().collect()
    };
    let rel = candidates
        .into_iter()
        .find(|c| exists(c))
        .ok_or_else(not_found)?;
    Ok((
        project.display_path(&rel),
        Target::Open {
            root: project.display_root.clone(),
            path: rel,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> RemoteProject {
        RemoteProject {
            display_root: "deploy@shop.example.com:/var/www/shop".into(),
            root: "/var/www/shop".into(),
        }
    }

    fn exists(p: &str) -> bool {
        matches!(p, "src/App.tsx" | "resources/js/app.ts")
    }

    fn open(file: &str) -> Result<(String, Target), ResolveError> {
        resolve_remote(&OpenRequest::new(file), &project(), &exists)
    }

    #[test]
    fn host_paths_below_the_root_open_directly() {
        let (file, target) = open("/var/www/shop/src/App.tsx").unwrap();
        assert_eq!(file, "deploy@shop.example.com:/var/www/shop/src/App.tsx");
        assert_eq!(
            target,
            Target::Open {
                root: "deploy@shop.example.com:/var/www/shop".into(),
                path: "src/App.tsx".into()
            }
        );
        // Outside the root after `..`: matched by its tail like other machines' paths.
        assert!(open("/var/www/shop/../shop-old/src/App.tsx")
            .is_ok_and(|(_, t)| t.path() == "src/App.tsx"));
        assert!(matches!(
            open("/var/www/shop/src/Missing.tsx"),
            Err(ResolveError::NotFound(_))
        ));
    }

    #[test]
    fn container_and_other_checkout_paths_map_by_their_tail() {
        assert_eq!(open("/app/src/App.tsx").unwrap().1.path(), "src/App.tsx");
        assert_eq!(
            open("C:\\Users\\me\\shop\\resources\\js\\app.ts")
                .unwrap()
                .1
                .path(),
            "resources/js/app.ts"
        );
        assert!(open("/etc/passwd").is_err());
    }

    #[test]
    fn relative_paths_and_displayed_paths() {
        assert_eq!(open("src/App.tsx").unwrap().1.path(), "src/App.tsx");
        assert_eq!(
            open("deploy@shop.example.com:/var/www/shop/src/App.tsx")
                .unwrap()
                .1
                .path(),
            "src/App.tsx"
        );
        let other = OpenRequest {
            project: Some("/srv/other".into()),
            ..OpenRequest::new("src/App.tsx")
        };
        assert!(matches!(
            resolve_remote(&other, &project(), &exists),
            Err(ResolveError::UnknownProject(_))
        ));
        let same = OpenRequest {
            project: Some("/var/www/shop".into()),
            ..OpenRequest::new("src/App.tsx")
        };
        assert!(resolve_remote(&same, &project(), &exists).is_ok());
    }
}
