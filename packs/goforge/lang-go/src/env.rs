//! The Go environment of a project: the SDK (`GOROOT`), `GOPATH`, `GOFLAGS`, build tags,
//! extra variables and the project's modules ([`GoContext`], built by the backend from
//! settings and `go.mod` / `go.work` files), and what `go env -json` reports ([`GoEnv`]).

use std::collections::BTreeMap;

use forge_output::LocationRule;
use serde::{Deserialize, Serialize};

/// A module of the project: its path (`github.com/acme/ledger`) and folder (project
/// relative, `""` for the root).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModuleRoot {
    pub path: String,
    pub dir: String,
}

/// Everything a Go command line of the project needs besides its configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GoContext {
    /// The SDK's folder; empty: `go` from the PATH.
    pub goroot: String,
    /// The project's GOPATH; empty: Go's default (`~/go`).
    pub gopath: String,
    pub goflags: String,
    /// Build tags every build, run and test gets (configurations may add their own).
    pub build_tags: Vec<String>,
    /// Extra environment variables (`GOPRIVATE`, `CGO_ENABLED`, …).
    pub env: BTreeMap<String, String>,
    pub modules: Vec<ModuleRoot>,
    /// Build for a Windows host (`go.exe`).
    pub windows: bool,
}

/// The key under which the backend hands the context to the Go Test provider
/// (`forge_testing::CommandContext::settings`).
pub const CONTEXT_KEY: &str = "go.context";

/// Join two project-relative paths (`a` may be empty, `b` may start with `./`).
pub fn join(a: &str, b: &str) -> String {
    match (a.trim_end_matches('/'), b.trim_start_matches("./")) {
        ("", b) => b.to_string(),
        (a, "") => a.to_string(),
        (a, b) => format!("{a}/{b}"),
    }
}

/// The folder of a project-relative file path (`""` for the root).
pub fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// `path` relative to `base` (both project relative); `None` when it is not inside.
pub fn strip_dir<'a>(path: &'a str, base: &str) -> Option<&'a str> {
    let base = base.trim_matches('/');
    if base.is_empty() {
        return Some(path.trim_start_matches('/'));
    }
    let rest = path.strip_prefix(base)?;
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix('/')
    }
}

/// Escape a literal for a regular expression (Go's `regexp.QuoteMeta`).
pub fn quote_meta(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl GoContext {
    /// The `go` executable.
    pub fn go(&self) -> String {
        let exe = if self.windows { "go.exe" } else { "go" };
        let root = self.goroot.trim().trim_end_matches(['/', '\\']);
        if root.is_empty() {
            exe.to_string()
        } else {
            let sep = if self.windows && root.contains('\\') {
                "\\"
            } else {
                "/"
            };
            format!("{root}{sep}bin{sep}{exe}")
        }
    }

    /// Environment variables of Go command lines.
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        if !self.gopath.trim().is_empty() {
            env.insert("GOPATH".into(), self.gopath.trim().to_string());
        }
        if !self.goflags.trim().is_empty() {
            env.insert("GOFLAGS".into(), self.goflags.trim().to_string());
        }
        for (k, v) in &self.env {
            if !k.trim().is_empty() {
                env.insert(k.trim().to_string(), v.clone());
            }
        }
        env
    }

    /// The module that contains the project folder `dir` (the deepest one).
    pub fn module_of(&self, dir: &str) -> Option<&ModuleRoot> {
        self.modules
            .iter()
            .filter(|m| strip_dir(dir, &m.dir).is_some())
            .max_by_key(|m| m.dir.len())
    }

    /// Build tags of a configuration (`tags` option, comma or space separated) plus the
    /// project's.
    pub fn tags(&self, own: &str) -> Vec<String> {
        let mut tags: Vec<String> = Vec::new();
        for t in self
            .build_tags
            .iter()
            .map(String::as_str)
            .chain(own.split([',', ' ']))
        {
            let t = t.trim();
            if !t.is_empty() && !tags.iter().any(|x| x == t) {
                tags.push(t.to_string());
            }
        }
        tags
    }

    /// Rules that turn import paths (test packages, coverage profiles) into project paths.
    pub fn location_rules(&self) -> Vec<LocationRule> {
        let mut modules: Vec<&ModuleRoot> =
            self.modules.iter().filter(|m| !m.path.is_empty()).collect();
        // Longer module paths first: `a.com/x/sub` before `a.com/x`.
        modules.sort_by_key(|m| std::cmp::Reverse(m.path.len()));
        modules
            .into_iter()
            .map(|m| {
                let dir = m.dir.trim_matches('/');
                LocationRule {
                    pattern: format!("^{}(?:/|$)", quote_meta(&m.path)),
                    replacement: if dir.is_empty() {
                        String::new()
                    } else {
                        format!("{dir}/")
                    },
                }
            })
            .collect()
    }

    /// Where a Go command for the project folder `dir` runs and how it names the package:
    /// (`-C` folder relative to `working_dir`, if the module is elsewhere; the package
    /// argument relative to the module).
    pub fn package_arg(&self, dir: &str, working_dir: &str) -> (Option<String>, String) {
        let module_dir = self
            .module_of(dir)
            .map(|m| m.dir.trim_matches('/').to_string())
            .unwrap_or_else(|| working_dir.trim_matches('/').to_string());
        let rel = strip_dir(dir, &module_dir).unwrap_or(dir);
        let pkg = if rel.is_empty() {
            ".".to_string()
        } else {
            format!("./{rel}")
        };
        let wd = working_dir.trim_matches('/');
        let change = (module_dir != wd).then(|| match strip_dir(&module_dir, wd) {
            Some(r) if !r.is_empty() => r.to_string(),
            _ => module_dir.clone(),
        });
        (change, pkg)
    }
}

/// What `go env -json` reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoEnv {
    pub goroot: String,
    pub gopath: String,
    pub goversion: String,
    pub gomodcache: String,
    pub gocache: String,
    pub goflags: String,
    pub goos: String,
    pub goarch: String,
    pub goproxy: String,
    pub gomod: String,
    pub gowork: String,
    pub gotoolchain: String,
}

/// Parse `go env -json` output.
pub fn parse_go_env(json: &str) -> Result<GoEnv, String> {
    let v: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|e| format!("unexpected go env output: {e}"))?;
    let s = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    Ok(GoEnv {
        goroot: s("GOROOT"),
        gopath: s("GOPATH"),
        goversion: s("GOVERSION"),
        gomodcache: s("GOMODCACHE"),
        gocache: s("GOCACHE"),
        goflags: s("GOFLAGS"),
        goos: s("GOOS"),
        goarch: s("GOARCH"),
        goproxy: s("GOPROXY"),
        gomod: s("GOMOD"),
        gowork: s("GOWORK"),
        gotoolchain: s("GOTOOLCHAIN"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> GoContext {
        GoContext {
            goroot: "/usr/local/go".into(),
            gopath: "/home/u/go".into(),
            goflags: "-mod=mod".into(),
            build_tags: vec!["integration".into()],
            env: BTreeMap::from([("CGO_ENABLED".into(), "0".into())]),
            modules: vec![
                ModuleRoot {
                    path: "github.com/acme/ledger".into(),
                    dir: "".into(),
                },
                ModuleRoot {
                    path: "github.com/acme/ledger/tools".into(),
                    dir: "tools".into(),
                },
            ],
            windows: false,
        }
    }

    #[test]
    fn executable_environment_and_tags() {
        let c = ctx();
        assert_eq!(c.go(), "/usr/local/go/bin/go");
        assert_eq!(GoContext::default().go(), "go");
        let w = GoContext {
            goroot: "C:\\Go\\".into(),
            windows: true,
            ..GoContext::default()
        };
        assert_eq!(w.go(), "C:\\Go\\bin\\go.exe");
        let env = c.environment();
        assert_eq!(env["GOPATH"], "/home/u/go");
        assert_eq!(env["GOFLAGS"], "-mod=mod");
        assert_eq!(env["CGO_ENABLED"], "0");
        assert_eq!(c.tags("e2e, integration"), ["integration", "e2e"]);
    }

    #[test]
    fn modules_packages_and_locations() {
        let c = ctx();
        assert_eq!(c.module_of("tools/gen").unwrap().dir, "tools");
        assert_eq!(c.module_of("toolsx").unwrap().dir, "");
        assert_eq!(
            c.package_arg("internal/money", ""),
            (None, "./internal/money".into())
        );
        assert_eq!(c.package_arg("", ""), (None, ".".into()));
        assert_eq!(
            c.package_arg("tools/gen", ""),
            (Some("tools".into()), "./gen".into())
        );
        assert_eq!(c.package_arg("tools", "tools"), (None, ".".into()));
        let rules = c.location_rules();
        assert_eq!(rules[0].pattern, "^github\\.com/acme/ledger/tools(?:/|$)");
        assert_eq!(rules[0].replacement, "tools/");
        assert_eq!(rules[1].replacement, "");
        assert_eq!(dir_of("a/b/c_test.go"), "a/b");
        assert_eq!(dir_of("c_test.go"), "");
        assert_eq!(join("a", "./b"), "a/b");
    }

    #[test]
    fn reads_go_env_json() {
        let env = parse_go_env(
            r#"{"GOROOT":"/usr/local/go","GOPATH":"/root/go","GOVERSION":"go1.24.7","GOMODCACHE":"/root/go/pkg/mod","GOOS":"linux","GOARCH":"amd64","GOFLAGS":""}"#,
        )
        .unwrap();
        assert_eq!(env.goversion, "go1.24.7");
        assert_eq!(env.gomodcache, "/root/go/pkg/mod");
        assert!(parse_go_env("go: not json").is_err());
    }
}
