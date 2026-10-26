//! "Go Build" run configurations (GoLand's): build a package, a directory or files, and run
//! the program afterwards (`go run`) or only build it (`go build -o`). Also the module
//! commands of the Go Modules actions (`go mod tidy`, `go mod download`, `go get`), tool
//! installs (`go install`) and the arguments Delve gets for the same configuration.
//!
//! Options: `kind` (`package`, `directory`, `file`), `package` (import path or `./dir`),
//! `directory`, `files` (one per line), `output`, `runAfterBuild` (`true` default),
//! `tags`, `flags` (Go tool arguments); `arguments` are the program's.

use std::collections::BTreeMap;

use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use serde::{Deserialize, Serialize};

use crate::env::{dir_of, strip_dir, GoContext};

pub const GO_BUILD: &str = "go-build";

/// A command line relative to the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Working directory relative to the project (`""`: the root).
    pub cwd: String,
    pub env: BTreeMap<String, String>,
}

/// What a Go Build configuration builds, relative to its working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTarget {
    /// Package, directory or file arguments for `go build` / `go run`.
    pub targets: Vec<String>,
    /// Working directory (project relative).
    pub cwd: String,
    /// The folder of the package (project relative), for Delve's `program`.
    pub package_dir: String,
}

fn working_dir(cfg: &Configuration, go: &GoContext, near: &str) -> String {
    let own = cfg.working_directory.trim().trim_matches('/');
    if !own.is_empty() {
        return own.to_string();
    }
    go.module_of(near)
        .map(|m| m.dir.trim_matches('/').to_string())
        .unwrap_or_default()
}

fn relative_to(path: &str, cwd: &str) -> String {
    match strip_dir(path, cwd) {
        Some("") => ".".into(),
        Some(r) => format!("./{r}"),
        None => path.to_string(),
    }
}

/// The targets of a Go Build configuration.
pub fn build_target(cfg: &Configuration, go: &GoContext) -> Result<BuildTarget, String> {
    let opt = |k: &str| cfg.option(k).trim().replace('\\', "/");
    match cfg.option("kind").trim() {
        "file" => {
            let files: Vec<String> = opt("files")
                .lines()
                .chain(std::iter::once(opt("file").as_str()))
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            let Some(first) = files.first() else {
                return Err("Choose the Go file to build".into());
            };
            let package_dir = dir_of(first).to_string();
            let cwd = working_dir(cfg, go, &package_dir);
            Ok(BuildTarget {
                targets: files.iter().map(|f| relative_to(f, &cwd)).collect(),
                cwd,
                package_dir,
            })
        }
        "directory" => {
            let dir = opt("directory").trim_matches('/').to_string();
            let cwd = working_dir(cfg, go, &dir);
            Ok(BuildTarget {
                targets: vec![relative_to(&dir, &cwd)],
                cwd,
                package_dir: dir,
            })
        }
        _ => {
            let pkg = opt("package");
            if pkg.is_empty() {
                return Err("Enter the package to build (an import path or ./cmd/app)".into());
            }
            // `./cmd/app` is a folder; an import path of a project module is mapped to its folder.
            let package_dir = if pkg.starts_with("./") || pkg == "." {
                pkg.trim_start_matches("./").trim_matches('/').to_string()
            } else {
                go.modules
                    .iter()
                    .filter_map(|m| {
                        let rest = pkg.strip_prefix(&m.path)?;
                        let rest = if rest.is_empty() {
                            ""
                        } else {
                            rest.strip_prefix('/')?
                        };
                        Some(crate::env::join(&m.dir, rest))
                    })
                    .next()
                    .unwrap_or_default()
            };
            let cwd = working_dir(cfg, go, &package_dir);
            Ok(BuildTarget {
                targets: vec![pkg],
                cwd,
                package_dir,
            })
        }
    }
}

/// The `go run` / `go build` command of a Go Build configuration.
pub fn build_command(cfg: &Configuration, go: &GoContext) -> Result<GoCommand, String> {
    let target = build_target(cfg, go)?;
    let run = cfg.option("runAfterBuild").trim() != "false";
    let mut args = vec![if run { "run" } else { "build" }.to_string()];
    let tags = go.tags(cfg.option("tags"));
    if !tags.is_empty() {
        args.push(format!("-tags={}", tags.join(",")));
    }
    let output = cfg.option("output").trim();
    if !run && !output.is_empty() {
        args.push("-o".into());
        args.push(output.to_string());
    }
    args.extend(split_args(cfg.option("flags")));
    args.extend(target.targets);
    if run {
        args.extend(split_args(&cfg.arguments));
    }
    Ok(GoCommand {
        program: go.go(),
        args,
        cwd: target.cwd,
        env: go.environment(),
    })
}

/// `go mod tidy`, `go mod download`, `go get <module>@<version>`, `go work sync`,
/// `go install <tool>@<version>` in the module folder `dir`.
pub fn module_command(action: &str, arg: &str, dir: &str, go: &GoContext) -> Result<GoCommand, String> {
    let args: Vec<String> = match action {
        "tidy" => vec!["mod".into(), "tidy".into()],
        "download" => vec!["mod".into(), "download".into()],
        "verify" => vec!["mod".into(), "verify".into()],
        "workSync" => vec!["work".into(), "sync".into()],
        "get" | "install" => {
            let module = arg.trim();
            if module.is_empty() || module.starts_with('-') || module.contains(char::is_whitespace)
            {
                return Err(format!("Not a module path: '{module}'"));
            }
            vec![action.into(), module.into()]
        }
        other => return Err(format!("Unknown Go modules action '{other}'")),
    };
    Ok(GoCommand {
        program: go.go(),
        args,
        cwd: dir.trim_matches('/').to_string(),
        env: go.environment(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::ModuleRoot;

    fn go() -> GoContext {
        GoContext {
            build_tags: vec!["prod".into()],
            modules: vec![
                ModuleRoot {
                    path: "github.com/acme/ledger".into(),
                    dir: "".into(),
                },
                ModuleRoot {
                    path: "github.com/acme/api".into(),
                    dir: "services/api".into(),
                },
            ],
            ..GoContext::default()
        }
    }

    #[test]
    fn go_run_and_go_build() {
        let mut cfg = Configuration::new("ledger", GO_BUILD)
            .with_option("kind", "package")
            .with_option("package", "./cmd/ledger")
            .with_option("flags", "-race");
        cfg.arguments = "--port 8080".into();
        let c = build_command(&cfg, &go()).unwrap();
        assert_eq!(c.program, "go");
        assert_eq!(
            c.args,
            ["run", "-tags=prod", "-race", "./cmd/ledger", "--port", "8080"]
        );
        assert_eq!(c.cwd, "");
        let built = build_command(
            &cfg.clone()
                .with_option("runAfterBuild", "false")
                .with_option("output", "bin/ledger"),
            &go(),
        )
        .unwrap();
        assert_eq!(
            built.args,
            ["build", "-tags=prod", "-o", "bin/ledger", "-race", "./cmd/ledger"]
        );
        let t = build_target(
            &Configuration::new("api", GO_BUILD).with_option("package", "github.com/acme/api/cmd/server"),
            &go(),
        )
        .unwrap();
        assert_eq!((t.cwd.as_str(), t.package_dir.as_str()), ("services/api", "services/api/cmd/server"));
        assert!(build_command(&Configuration::new("x", GO_BUILD), &go()).is_err());
    }

    #[test]
    fn files_and_directories_are_relative_to_their_module() {
        let files = Configuration::new("f", GO_BUILD)
            .with_option("kind", "file")
            .with_option("files", "services/api/main.go\nservices/api/flags.go");
        let c = build_command(&files, &go()).unwrap();
        assert_eq!(c.cwd, "services/api");
        assert_eq!(c.args[2..], ["./main.go", "./flags.go"]);
        let dir = Configuration::new("d", GO_BUILD)
            .with_option("kind", "directory")
            .with_option("directory", "tools/gen");
        assert_eq!(build_command(&dir, &go()).unwrap().args[2], "./tools/gen");
    }

    #[test]
    fn module_commands() {
        let g = go();
        assert_eq!(module_command("tidy", "", "", &g).unwrap().args, ["mod", "tidy"]);
        let get = module_command("get", "github.com/google/uuid@v1.6.0", "services/api", &g).unwrap();
        assert_eq!(get.args, ["get", "github.com/google/uuid@v1.6.0"]);
        assert_eq!(get.cwd, "services/api");
        assert!(module_command("get", "-u all", "", &g).is_err());
        assert!(module_command("rm", "", "", &g).is_err());
    }
}
