//! Path mappings: which deployment path a project path has on a server (the longest
//! matching local path of the Mappings tab wins, like in PHPStorm), the way back, the web
//! URL of a file, excluded paths and "Exclude items by name" patterns.
//!
//! Project paths are relative to the project root, deployment paths relative to the
//! server's root path; both use `/` and have no leading or trailing slash (`""` is the
//! root).

use crate::config::{DeployServer, ExcludedSide, Mapping};

/// `\` → `/`, no empty or `.` segments: `"/public/./css/"` → `"public/css"`.
pub fn normalize(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Join two normalized paths.
pub fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a}/{b}"),
    }
}

/// The parent of a normalized path (`""` for top-level entries).
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

/// The last segment of a normalized path.
pub fn name(path: &str) -> &str {
    path.rsplit_once('/').map(|(_, n)| n).unwrap_or(path)
}

/// `path` with `prefix` removed when `prefix` is `path` itself or one of its folders.
fn strip(path: &str, prefix: &str) -> Option<String> {
    if prefix.is_empty() {
        return Some(path.to_string());
    }
    if path == prefix {
        return Some(String::new());
    }
    path.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_string)
}

/// Whether `path` is `folder` or below it (`""` contains everything).
pub fn is_within(path: &str, folder: &str) -> bool {
    strip(path, folder).is_some()
}

/// Rejects `..` so a mapping can never point outside the root path or the project.
fn has_parent_refs(path: &str) -> bool {
    path.split('/').any(|p| p == "..")
}

/// How a project path maps to a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The deployment path (relative to the root path).
    Mapped(String),
    /// No mapping covers the path.
    NotMapped,
    /// Excluded on the Excluded Paths tab.
    Excluded,
}

impl DeployServer {
    /// The mappings in effect: the configured ones, or the project root deployed to the
    /// root path when none is configured.
    pub fn effective_mappings(&self) -> Vec<Mapping> {
        if self.mappings.is_empty() {
            vec![Mapping::default()]
        } else {
            self.mappings.clone()
        }
    }

    /// The deployment path of a project path.
    pub fn resolve(&self, local: &str) -> Resolution {
        let local = normalize(local);
        if has_parent_refs(&local) {
            return Resolution::NotMapped;
        }
        if self.is_excluded(ExcludedSide::Local, &local) {
            return Resolution::Excluded;
        }
        let best = self
            .effective_mappings()
            .into_iter()
            .filter_map(|m| {
                let from = normalize(&m.local);
                strip(&local, &from).map(|rest| (from.len(), m, rest))
            })
            .max_by_key(|(len, _, _)| *len);
        let Some((_, mapping, rest)) = best else {
            return Resolution::NotMapped;
        };
        let to = normalize(&mapping.deployment);
        if has_parent_refs(&to) {
            return Resolution::NotMapped;
        }
        let deployment = join(&to, &rest);
        if self.is_excluded(ExcludedSide::Deployment, &deployment) {
            return Resolution::Excluded;
        }
        Resolution::Mapped(deployment)
    }

    /// The deployment path of a project path, `None` when it is not mapped or excluded.
    pub fn deployment_path(&self, local: &str) -> Option<String> {
        match self.resolve(local) {
            Resolution::Mapped(p) => Some(p),
            _ => None,
        }
    }

    /// The project path of a deployment path (the longest matching deployment path wins).
    pub fn local_path(&self, deployment: &str) -> Option<String> {
        let deployment = normalize(deployment);
        if has_parent_refs(&deployment) || self.is_excluded(ExcludedSide::Deployment, &deployment) {
            return None;
        }
        let (_, local) = self
            .effective_mappings()
            .into_iter()
            .filter_map(|m| {
                let to = normalize(&m.deployment);
                strip(&deployment, &to).map(|rest| (to.len(), join(&normalize(&m.local), &rest)))
            })
            .max_by_key(|(len, _)| *len)?;
        (!self.is_excluded(ExcludedSide::Local, &local)).then_some(local)
    }

    /// The URL of a project file on the web server (`web_url` + web path + the rest).
    pub fn web_url_of(&self, local: &str) -> Option<String> {
        let base = self.web_url.trim().trim_end_matches('/');
        if base.is_empty() {
            return None;
        }
        let local = normalize(local);
        let (_, mapping, rest) = self
            .effective_mappings()
            .into_iter()
            .filter_map(|m| {
                let from = normalize(&m.local);
                strip(&local, &from).map(|rest| (from.len(), m, rest))
            })
            .max_by_key(|(len, _, _)| *len)?;
        let path = join(&normalize(&mapping.web), &rest);
        Some(if path.is_empty() {
            format!("{base}/")
        } else {
            format!("{base}/{path}")
        })
    }

    /// Whether an Excluded Paths entry of `side` covers `path`.
    pub fn is_excluded(&self, side: ExcludedSide, path: &str) -> bool {
        self.excluded
            .iter()
            .filter(|e| e.side == side)
            .map(|e| normalize(&e.path))
            .any(|e| !e.is_empty() && is_within(path, &e))
    }

    /// A deployment path as the server shows it: below the root path (or the login folder).
    pub fn display_path(&self, deployment: &str) -> String {
        let root = self.root_path.trim().replace('\\', "/");
        let root = root.trim_end_matches('/');
        match (root.is_empty(), deployment.is_empty()) {
            (true, true) => "~".to_string(),
            (true, false) => format!("~/{deployment}"),
            (false, true) => root.to_string(),
            (false, false) => format!("{root}/{deployment}"),
        }
        .replace("//", "/")
    }
}

/// "Exclude items by name" (Deployment | Options): `;`-separated names with `*` and `?`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExcludeNames(Vec<String>);

impl ExcludeNames {
    pub fn parse(patterns: &str) -> Self {
        Self(
            patterns
                .split(';')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect(),
        )
    }

    pub fn matches(&self, name: &str) -> bool {
        self.0.iter().any(|p| glob(p.as_bytes(), name.as_bytes()))
    }

    /// Whether any segment of a normalized path is excluded by name.
    pub fn matches_path(&self, path: &str) -> bool {
        path.split('/').any(|segment| self.matches(segment))
    }
}

fn glob(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.first() {
        None => text.is_empty(),
        Some(b'*') => (0..=text.len()).any(|i| glob(&pattern[1..], &text[i..])),
        Some(b'?') => !text.is_empty() && glob(&pattern[1..], &text[1..]),
        Some(c) => text.first() == Some(c) && glob(&pattern[1..], &text[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ExcludedPath;

    fn server() -> DeployServer {
        DeployServer {
            name: "Shop".into(),
            root_path: "/var/www/shop/".into(),
            web_url: "https://shop.example.com/".into(),
            mappings: vec![
                Mapping {
                    local: "".into(),
                    deployment: "/".into(),
                    web: "".into(),
                },
                Mapping {
                    local: "public".into(),
                    deployment: "/htdocs".into(),
                    web: "/".into(),
                },
                Mapping {
                    local: "public/assets".into(),
                    deployment: "cdn/assets".into(),
                    web: "static".into(),
                },
            ],
            excluded: vec![
                ExcludedPath {
                    side: ExcludedSide::Local,
                    path: "storage/".into(),
                },
                ExcludedPath {
                    side: ExcludedSide::Deployment,
                    path: "/htdocs/uploads".into(),
                },
            ],
            ..DeployServer::default()
        }
    }

    #[test]
    fn normalizes_and_joins() {
        assert_eq!(normalize("/public/./css/"), "public/css");
        assert_eq!(normalize("a\\b"), "a/b");
        assert_eq!(join("", "a"), "a");
        assert_eq!(join("a", ""), "a");
        assert_eq!(join("a", "b"), "a/b");
        assert_eq!(parent("a/b/c"), "a/b");
        assert_eq!(parent("a"), "");
        assert_eq!(name("a/b/c"), "c");
        assert!(is_within("public/x", "public"));
        assert!(!is_within("publicity", "public"));
        assert!(is_within("anything", ""));
    }

    #[test]
    fn the_longest_mapping_wins() {
        let s = server();
        assert_eq!(
            s.deployment_path("composer.json").as_deref(),
            Some("composer.json")
        );
        assert_eq!(
            s.deployment_path("public/index.php").as_deref(),
            Some("htdocs/index.php")
        );
        assert_eq!(s.deployment_path("public").as_deref(), Some("htdocs"));
        assert_eq!(
            s.deployment_path("public/assets/app.css").as_deref(),
            Some("cdn/assets/app.css")
        );
        assert_eq!(
            s.deployment_path("/app/Models/User.php").as_deref(),
            Some("app/Models/User.php")
        );
        assert_eq!(
            s.display_path("htdocs/index.php"),
            "/var/www/shop/htdocs/index.php"
        );
        assert_eq!(s.display_path(""), "/var/www/shop");
    }

    #[test]
    fn excluded_and_unmapped_paths() {
        let s = server();
        assert_eq!(s.resolve("storage/logs/laravel.log"), Resolution::Excluded);
        assert_eq!(s.resolve("storage"), Resolution::Excluded);
        assert_eq!(s.resolve("public/uploads/a.png"), Resolution::Excluded);
        assert_eq!(s.resolve("../etc/passwd"), Resolution::NotMapped);
        let only_public = DeployServer {
            mappings: vec![Mapping {
                local: "public".into(),
                deployment: "".into(),
                web: "".into(),
            }],
            ..DeployServer::default()
        };
        assert_eq!(only_public.resolve("app/User.php"), Resolution::NotMapped);
        assert_eq!(
            only_public.deployment_path("public/index.php").as_deref(),
            Some("index.php")
        );
        let evil = DeployServer {
            mappings: vec![Mapping {
                local: "".into(),
                deployment: "../..".into(),
                web: "".into(),
            }],
            ..DeployServer::default()
        };
        assert_eq!(evil.resolve("a"), Resolution::NotMapped);
        // No mappings: the project root goes to the root path.
        let plain = DeployServer::default();
        assert_eq!(plain.deployment_path("a/b").as_deref(), Some("a/b"));
        assert_eq!(plain.display_path("a/b"), "~/a/b");
    }

    #[test]
    fn maps_back_and_to_urls() {
        let s = server();
        assert_eq!(
            s.local_path("htdocs/index.php").as_deref(),
            Some("public/index.php")
        );
        assert_eq!(
            s.local_path("cdn/assets/app.css").as_deref(),
            Some("public/assets/app.css")
        );
        assert_eq!(s.local_path("artisan").as_deref(), Some("artisan"));
        assert_eq!(s.local_path("htdocs/uploads/x").as_deref(), None);
        assert_eq!(s.local_path("storage/x").as_deref(), None);
        assert_eq!(
            s.web_url_of("public/index.php").as_deref(),
            Some("https://shop.example.com/index.php")
        );
        assert_eq!(
            s.web_url_of("public/assets/app.css").as_deref(),
            Some("https://shop.example.com/static/app.css")
        );
        assert_eq!(
            s.web_url_of("public").as_deref(),
            Some("https://shop.example.com/")
        );
        let no_url = DeployServer::default();
        assert_eq!(no_url.web_url_of("x"), None);
    }

    #[test]
    fn excludes_names_with_wildcards() {
        let names = ExcludeNames::parse(".svn;.git; .DS_Store ;*.tmp;cache?;");
        assert!(names.matches(".git"));
        assert!(names.matches(".DS_Store"));
        assert!(names.matches("x.tmp"));
        assert!(names.matches("cache1"));
        assert!(!names.matches("cache"));
        assert!(!names.matches(".gitignore"));
        assert!(names.matches_path("app/.git/config"));
        assert!(!names.matches_path("app/git/config"));
        assert!(!ExcludeNames::parse("").matches("x"));
    }
}
