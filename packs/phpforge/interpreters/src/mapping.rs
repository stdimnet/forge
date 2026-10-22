//! Path mappings between the project (local paths) and where PHP runs (container, SSH
//! host, WSL distribution).
//!
//! Some mappings follow from the interpreter itself and are derived automatically: Docker
//! mounts the project at `/opt/project` (like PHPStorm), Docker Compose services map the
//! host paths of their `volumes`, and WSL sees `C:\work\shop` as `/mnt/c/work/shop`. The
//! user's own "Path mappings" are added to (and win over) those. Run configurations map
//! paths in arguments and the working directory to the remote side, the Run console maps
//! paths in the output back, and the debugger maps breakpoints with the same pairs.

use serde::{Deserialize, Serialize};

use crate::model::{InterpreterKind, PhpInterpreter};

/// The default mount point of the project in a Docker container.
pub const DOCKER_PROJECT_ROOT: &str = "/opt/project";

/// One mapping row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathMapping {
    /// Local path: absolute, or relative to the project root (empty = the root).
    pub local: String,
    /// Absolute path where PHP runs.
    pub remote: String,
}

/// Forward slashes, `.` and `..` resolved, no trailing slash (except for `/`).
pub fn normalize(path: &str) -> String {
    let p = path.trim().replace('\\', "/");
    // Keep the leading `//` of UNC paths (`//wsl$/Ubuntu`).
    let (prefix, rest) = if let Some(r) = p.strip_prefix("//") {
        ("//", r)
    } else if let Some(r) = p.strip_prefix('/') {
        ("/", r)
    } else {
        ("", p.as_str())
    };
    let mut out: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if prefix.is_empty() {
                    out.push("..");
                }
            }
            p => out.push(p),
        }
    }
    let joined = out.join("/");
    if joined.is_empty() && !prefix.is_empty() {
        "/".into()
    } else {
        format!("{prefix}{joined}")
    }
}

/// Whether `path` is absolute on any platform (`/x`, `C:/x`, `\\server\share`).
pub fn is_absolute(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let b = p.as_bytes();
    p.starts_with('/') || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// `rel` against `base` unless it is absolute.
pub fn join(base: &str, rel: &str) -> String {
    if is_absolute(rel) {
        normalize(rel)
    } else {
        normalize(&format!("{}/{}", base.trim_end_matches(['/', '\\']), rel))
    }
}

/// `path` relative to `base` when it is inside it (`""` for the base itself).
fn strip_dir<'a>(path: &'a str, base: &str) -> Option<&'a str> {
    if base == "/" {
        return path.strip_prefix('/');
    }
    let rest = path.strip_prefix(base)?;
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix('/')
    }
}

/// Translates between local and remote paths with a list of mappings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mapper {
    /// (absolute local, absolute remote), normalized.
    pairs: Vec<(String, String)>,
}

impl Mapper {
    /// `mappings` with relative local paths resolved against `root`; the first mapping of a
    /// local path wins.
    pub fn new(root: &str, mappings: &[PathMapping]) -> Self {
        let root = normalize(root);
        let mut pairs: Vec<(String, String)> = Vec::new();
        for m in mappings {
            if m.remote.trim().is_empty() {
                continue;
            }
            let local = join(&root, m.local.trim());
            if pairs.iter().any(|(l, _)| *l == local) {
                continue;
            }
            pairs.push((local, normalize(m.remote.trim())));
        }
        Self { pairs }
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// The (absolute local, remote) pairs.
    pub fn pairs(&self) -> &[(String, String)] {
        &self.pairs
    }

    /// The most specific mapping for a path on one side.
    fn map(&self, path: &str, to_remote: bool) -> Option<String> {
        let path = normalize(path);
        self.pairs
            .iter()
            .filter_map(|(l, r)| {
                let (from, to) = if to_remote { (l, r) } else { (r, l) };
                let rest = strip_dir(&path, from)?;
                Some((from.len(), to, rest))
            })
            .max_by_key(|(len, _, _)| *len)
            .map(|(_, to, rest)| {
                if rest.is_empty() {
                    to.clone()
                } else if to == "/" {
                    format!("/{rest}")
                } else {
                    format!("{to}/{rest}")
                }
            })
    }

    /// A local absolute path where PHP sees it.
    pub fn to_remote(&self, local: &str) -> Option<String> {
        self.map(local, true)
    }

    /// A path PHP reported, on this machine.
    pub fn to_local(&self, remote: &str) -> Option<String> {
        self.map(remote, false)
    }

    /// An argument with a local path at its start or after `=` (`--configuration=/p/x.xml`,
    /// `-dauto_prepend_file=/p/vendor/autoload.php`) mapped to the remote side.
    pub fn map_arg(&self, arg: &str) -> String {
        if self.pairs.is_empty() {
            return arg.to_string();
        }
        let (head, value) = match arg.find('=') {
            Some(i) if !is_absolute(arg) => (&arg[..=i], &arg[i + 1..]),
            _ => ("", arg),
        };
        if is_absolute(value) {
            if let Some(mapped) = self.to_remote(value) {
                return format!("{head}{mapped}");
            }
        }
        arg.to_string()
    }
}

/// One `volumes` entry of a compose service: `(host, container)` for bind mounts (named
/// volumes and anonymous ones are skipped). `${PWD}` counts as the compose directory.
pub fn parse_volume(volume: &str) -> Option<(String, String)> {
    let v = volume.trim().trim_matches('"').trim_matches('\'');
    let v = v
        .replace("${PWD:-.}", ".")
        .replace("${PWD}", ".")
        .replace("$PWD", ".");
    let b = v.as_bytes();
    // A Windows drive (`C:\src:/app`) has a colon of its own.
    let skip = if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        2
    } else {
        0
    };
    let colon = v[skip..].find(':')? + skip;
    let host = &v[..colon];
    let rest = &v[colon + 1..];
    let container = rest.split(':').next().unwrap_or(rest);
    let path_like = host.starts_with('.') || host.starts_with('~') || is_absolute(host);
    if !path_like || !container.starts_with('/') {
        return None;
    }
    Some((host.to_string(), container.to_string()))
}

/// The bind mounts of a compose service that cover the project: the project root (or the
/// part of it that is mounted) and where the container sees it.
pub fn compose_mappings(root: &str, compose_dir: &str, volumes: &[String]) -> Vec<PathMapping> {
    let root = normalize(root);
    let mut out = Vec::new();
    for volume in volumes {
        let Some((host, container)) = parse_volume(volume) else {
            continue;
        };
        if host.starts_with('~') {
            continue;
        }
        let host = join(compose_dir, &host);
        let container = normalize(&container);
        let mapping = if let Some(rest) = strip_dir(&root, &host) {
            // The mount contains the project root.
            PathMapping {
                local: root.clone(),
                remote: if rest.is_empty() {
                    container
                } else {
                    normalize(&format!("{container}/{rest}"))
                },
            }
        } else if strip_dir(&host, &root).is_some() {
            // A folder of the project is mounted.
            PathMapping {
                local: host,
                remote: container,
            }
        } else {
            continue;
        };
        if !out.iter().any(|m: &PathMapping| m.local == mapping.local) {
            out.push(mapping);
        }
    }
    out
}

/// `C:\work\shop` → `/mnt/c/work/shop`; `\\wsl$\Ubuntu\home\me\shop` (or
/// `\\wsl.localhost\…`) → `/home/me/shop`. `None` for other paths.
pub fn wsl_path(path: &str) -> Option<String> {
    let p = path.trim().replace('\\', "/");
    for prefix in ["//wsl$/", "//wsl.localhost/"] {
        if p.len() >= prefix.len() && p[..prefix.len()].eq_ignore_ascii_case(prefix) {
            let rest = &p[prefix.len()..];
            let inside = rest.split_once('/').map_or("", |(_, r)| r);
            return Some(normalize(&format!("/{inside}")));
        }
    }
    let b = p.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        let drive = (b[0] as char).to_ascii_lowercase();
        return Some(normalize(&format!("/mnt/{drive}/{}", &p[2..])));
    }
    None
}

/// The mappings that follow from the interpreter: Docker's `/opt/project` mount, the
/// compose service's volumes (`compose`: the compose file's directory and the service's
/// volumes) and WSL's drive paths.
pub fn automatic(
    interp: &PhpInterpreter,
    root: &str,
    compose: Option<(&str, &[String])>,
) -> Vec<PathMapping> {
    match interp.kind {
        InterpreterKind::Docker => vec![PathMapping {
            local: normalize(root),
            remote: DOCKER_PROJECT_ROOT.into(),
        }],
        InterpreterKind::Compose => compose
            .map(|(dir, volumes)| compose_mappings(root, dir, volumes))
            .unwrap_or_default(),
        InterpreterKind::Wsl => wsl_path(root)
            .map(|remote| {
                vec![PathMapping {
                    local: normalize(root),
                    remote,
                }]
            })
            .unwrap_or_default(),
        InterpreterKind::Local | InterpreterKind::Ssh => Vec::new(),
    }
}

/// The user's mappings followed by the automatic ones (the user's win for the same local
/// path, see [`Mapper::new`]).
pub fn effective(interp: &PhpInterpreter, automatic: Vec<PathMapping>) -> Vec<PathMapping> {
    let mut all = interp.mappings.clone();
    all.extend(automatic);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(local: &str, remote: &str) -> PathMapping {
        PathMapping {
            local: local.into(),
            remote: remote.into(),
        }
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize("/a/b/../c/./"), "/a/c");
        assert_eq!(normalize("C:\\work\\shop\\"), "C:/work/shop");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("../x"), "../x");
        assert_eq!(join("/proj", "docker/../src"), "/proj/src");
        assert_eq!(join("/proj", "/abs"), "/abs");
    }

    #[test]
    fn maps_both_ways_with_the_most_specific_pair() {
        let mapper = Mapper::new(
            "/home/me/shop",
            &[m("", "/var/www/html"), m("vendor", "/opt/vendor")],
        );
        assert_eq!(
            mapper.to_remote("/home/me/shop/app/User.php").as_deref(),
            Some("/var/www/html/app/User.php")
        );
        assert_eq!(
            mapper.to_remote("/home/me/shop/vendor/a/b.php").as_deref(),
            Some("/opt/vendor/a/b.php")
        );
        assert_eq!(
            mapper
                .to_local("/var/www/html/tests/UserTest.php")
                .as_deref(),
            Some("/home/me/shop/tests/UserTest.php")
        );
        assert_eq!(
            mapper.to_local("/var/www/html").as_deref(),
            Some("/home/me/shop")
        );
        assert_eq!(mapper.to_local("/var/www/htmlx/a.php"), None);
        assert_eq!(mapper.to_remote("/elsewhere/a.php"), None);
        assert_eq!(
            mapper.map_arg("-dauto_prepend_file=/home/me/shop/vendor/autoload.php"),
            "-dauto_prepend_file=/opt/vendor/autoload.php"
        );
        assert_eq!(mapper.map_arg("--filter=/Foo/"), "--filter=/Foo/");
        assert_eq!(mapper.map_arg("tests/Unit"), "tests/Unit");
        assert_eq!(
            mapper.map_arg("/home/me/shop/bin/run.php"),
            "/var/www/html/bin/run.php"
        );
    }

    #[test]
    fn user_mappings_win_over_automatic_ones() {
        let interp = PhpInterpreter {
            kind: InterpreterKind::Docker,
            mappings: vec![m("", "/app")],
            ..Default::default()
        };
        let all = effective(&interp, automatic(&interp, "/p", None));
        assert_eq!(all.len(), 2);
        let mapper = Mapper::new("/p", &all);
        assert_eq!(mapper.to_remote("/p/x.php").as_deref(), Some("/app/x.php"));
        let plain = PhpInterpreter {
            kind: InterpreterKind::Docker,
            ..Default::default()
        };
        let mapper = Mapper::new("/p", &effective(&plain, automatic(&plain, "/p", None)));
        assert_eq!(
            mapper.to_remote("/p/x.php").as_deref(),
            Some("/opt/project/x.php")
        );
    }

    #[test]
    fn derives_mappings_from_compose_volumes() {
        assert_eq!(
            parse_volume("./:/var/www/html:cached"),
            Some(("./".into(), "/var/www/html".into()))
        );
        assert_eq!(parse_volume("db-data:/var/lib/mysql"), None);
        assert_eq!(parse_volume("/var/lib/data"), None);
        assert_eq!(
            parse_volume("C:\\src:/app"),
            Some(("C:\\src".into(), "/app".into()))
        );
        assert_eq!(
            parse_volume("${PWD}:/app"),
            Some((".".into(), "/app".into()))
        );
        let volumes: Vec<String> = vec![
            ".:/var/www/html".into(),
            "./storage/logs:/var/log/app".into(),
            "db:/var/lib/mysql".into(),
            "../shared:/shared".into(),
        ];
        // compose file in the project root
        assert_eq!(
            compose_mappings("/home/me/shop", "/home/me/shop", &volumes),
            vec![
                m("/home/me/shop", "/var/www/html"),
                m("/home/me/shop/storage/logs", "/var/log/app")
            ]
        );
        // compose file in docker/ mounting the parent
        assert_eq!(
            compose_mappings("/home/me/shop", "/home/me/shop/docker", &["..:/app".into()]),
            vec![m("/home/me/shop", "/app")]
        );
        // a mount of the project's parent folder
        assert_eq!(
            compose_mappings("/home/me/shop", "/home/me", &["./:/src".into()]),
            vec![m("/home/me/shop", "/src/shop")]
        );
    }

    #[test]
    fn maps_windows_paths_into_wsl() {
        assert_eq!(
            wsl_path("C:\\Users\\me\\shop").as_deref(),
            Some("/mnt/c/Users/me/shop")
        );
        assert_eq!(
            wsl_path("\\\\wsl$\\Ubuntu\\home\\me\\shop").as_deref(),
            Some("/home/me/shop")
        );
        assert_eq!(
            wsl_path("//wsl.localhost/Debian/srv").as_deref(),
            Some("/srv")
        );
        assert_eq!(wsl_path("/home/me"), None);
        let wsl = PhpInterpreter {
            kind: InterpreterKind::Wsl,
            ..Default::default()
        };
        assert_eq!(automatic(&wsl, "D:\\p", None), vec![m("D:/p", "/mnt/d/p")]);
    }
}
