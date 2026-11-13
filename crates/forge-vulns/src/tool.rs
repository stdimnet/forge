//! The audit tools and what a project gives them to check: one [`Target`] per ecosystem
//! found in the project's root folder.

use serde::{Deserialize, Serialize};
use webforge_lang_web::{PackageJson, PackageManager};

use crate::model::Ecosystem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    Composer,
    Npm,
    Pnpm,
    /// Yarn 1: `yarn audit --json`.
    YarnClassic,
    /// Yarn 2+: `yarn npm audit --json`.
    YarnBerry,
    PipAudit,
    CargoAudit,
    Govulncheck,
    /// OWASP dependency-check: its JSON report is read, nothing is run.
    DependencyCheck,
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Tool::Composer,
        Tool::Npm,
        Tool::Pnpm,
        Tool::YarnClassic,
        Tool::YarnBerry,
        Tool::PipAudit,
        Tool::CargoAudit,
        Tool::Govulncheck,
        Tool::DependencyCheck,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Composer => "composer",
            Tool::Npm => "npm",
            Tool::Pnpm => "pnpm",
            Tool::YarnClassic => "yarn-classic",
            Tool::YarnBerry => "yarn-berry",
            Tool::PipAudit => "pip-audit",
            Tool::CargoAudit => "cargo-audit",
            Tool::Govulncheck => "govulncheck",
            Tool::DependencyCheck => "dependency-check",
        }
    }

    pub fn parse(id: &str) -> Option<Tool> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    /// The command as the user would type it.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Composer => "composer audit",
            Tool::Npm => "npm audit",
            Tool::Pnpm => "pnpm audit",
            Tool::YarnClassic => "yarn audit",
            Tool::YarnBerry => "yarn npm audit",
            Tool::PipAudit => "pip-audit",
            Tool::CargoAudit => "cargo audit",
            Tool::Govulncheck => "govulncheck",
            Tool::DependencyCheck => "OWASP dependency-check",
        }
    }

    pub fn ecosystem(self) -> Ecosystem {
        match self {
            Tool::Composer => Ecosystem::Composer,
            Tool::Npm | Tool::Pnpm | Tool::YarnClassic | Tool::YarnBerry => Ecosystem::Npm,
            Tool::PipAudit => Ecosystem::Pypi,
            Tool::CargoAudit => Ecosystem::Cargo,
            Tool::Govulncheck => Ecosystem::Go,
            Tool::DependencyCheck => Ecosystem::Maven,
        }
    }

    /// The executable that is looked up (`None`: Composer and pip-audit run through the
    /// project's PHP / Python interpreter, dependency-check is not run).
    pub fn executable(self) -> Option<&'static str> {
        match self {
            Tool::Npm => Some("npm"),
            Tool::Pnpm => Some("pnpm"),
            Tool::YarnClassic | Tool::YarnBerry => Some("yarn"),
            Tool::CargoAudit => Some("cargo-audit"),
            Tool::Govulncheck => Some("govulncheck"),
            Tool::Composer | Tool::PipAudit | Tool::DependencyCheck => None,
        }
    }

    /// Arguments after the program (`composer`, `npm`, `python`, `cargo`, ...).
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Tool::Composer => &["audit", "--format=json", "--no-interaction"],
            Tool::Npm | Tool::Pnpm | Tool::YarnClassic => &["audit", "--json"],
            Tool::YarnBerry => &["npm", "audit", "--json", "--recursive"],
            Tool::PipAudit => &["-m", "pip_audit", "-f", "json", "--progress-spinner", "off"],
            Tool::CargoAudit => &["audit", "--json"],
            Tool::Govulncheck => &["-json", "./..."],
            Tool::DependencyCheck => &[],
        }
    }

    /// How to get the tool when it is missing.
    pub fn install_hint(self) -> &'static str {
        match self {
            Tool::Composer => "Configure Composer in Settings | PHP | Composer.",
            Tool::Npm | Tool::Pnpm | Tool::YarnClassic | Tool::YarnBerry => {
                "Install Node.js and the package manager of the project."
            }
            Tool::PipAudit => "Install pip-audit into the project interpreter.",
            Tool::CargoAudit => "Install it with `cargo install cargo-audit --locked`.",
            Tool::Govulncheck => {
                "Install it with `go install golang.org/x/vuln/cmd/govulncheck@latest`."
            }
            Tool::DependencyCheck => {
                "Run dependency-check with the JSON format and set the report in Settings."
            }
        }
    }
}

/// One audit of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub tool: Tool,
    /// The folder it runs in (project-relative, `""` for the root).
    pub dir: String,
    /// The main manifest (`composer.json`, `package.json`, `go.mod`, ...).
    pub manifest: String,
    /// The lock file that pins what is installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lockfile: Option<String>,
    /// Files that get the "Vulnerable Dependencies" problems.
    pub annotate: Vec<String>,
    /// Files whose contents decide whether a cached result is still valid.
    pub inputs: Vec<String>,
    /// Why it cannot run (no lock file yet, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DetectOptions {
    /// composer.json (Settings | PHP | Composer), project-relative.
    pub composer_json: String,
    /// The dependency-check JSON report (empty: Maven / Gradle are not audited).
    pub dependency_check_report: String,
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// Whether a root file is a pip requirements file.
fn is_requirements(name: &str) -> bool {
    pyforge_lang_python::deps::is_requirements_file(name)
}

/// The audits of a project: `files` are the paths that exist (at least the root folder's
/// entries), `read` returns a file's text.
pub fn detect(
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
    opts: &DetectOptions,
) -> Vec<Target> {
    let has = |p: &str| files.iter().any(|f| f == p);
    let mut out = Vec::new();

    let composer_json = match opts.composer_json.trim().trim_start_matches("./") {
        "" => "composer.json".to_string(),
        p => p.to_string(),
    };
    if has(&composer_json) || read(&composer_json).is_some() {
        let dir = parent(&composer_json).to_string();
        let lock = phpforge_composer::lock_path(&composer_json);
        let locked = has(&lock) || read(&lock).is_some();
        out.push(Target {
            tool: Tool::Composer,
            dir,
            manifest: composer_json.clone(),
            lockfile: locked.then(|| lock.clone()),
            annotate: if locked {
                vec![composer_json.clone(), lock.clone()]
            } else {
                vec![composer_json.clone()]
            },
            inputs: vec![composer_json.clone(), lock],
            blocked: None,
        });
    }

    if has("package.json") {
        let field = read("package.json")
            .as_deref()
            .and_then(PackageJson::parse)
            .and_then(|p| p.package_manager);
        let (name, version) = field
            .as_deref()
            .map(|f| f.split_once('@').unwrap_or((f, "")))
            .unwrap_or(("", ""));
        let manager = PackageManager::parse(name).or_else(|| {
            [
                PackageManager::Pnpm,
                PackageManager::Yarn,
                PackageManager::Bun,
                PackageManager::Npm,
            ]
            .into_iter()
            .find(|m| m.lock_files().iter().any(|l| has(l)))
        });
        let manager = manager.unwrap_or(PackageManager::Npm);
        let lock = manager
            .lock_files()
            .iter()
            .find(|l| has(l))
            .map(|l| l.to_string());
        let berry = manager == PackageManager::Yarn
            && (version
                .split('.')
                .next()
                .and_then(|m| m.parse::<u32>().ok())
                .is_some_and(|m| m >= 2)
                || has(".yarnrc.yml")
                || lock
                    .as_deref()
                    .and_then(read)
                    .is_some_and(|t| t.contains("__metadata:")));
        let tool = match manager {
            PackageManager::Npm | PackageManager::Bun => Tool::Npm,
            PackageManager::Pnpm => Tool::Pnpm,
            PackageManager::Yarn if berry => Tool::YarnBerry,
            PackageManager::Yarn => Tool::YarnClassic,
        };
        let blocked = match (manager, &lock) {
            (PackageManager::Bun, _) => {
                Some("Bun projects are not audited: Bun writes no npm audit report.".to_string())
            }
            (_, None) => Some(format!(
                "There is no lock file yet: run `{} install` first.",
                manager.id()
            )),
            _ => None,
        };
        let mut inputs = vec!["package.json".to_string()];
        inputs.extend(lock.clone());
        out.push(Target {
            tool,
            dir: String::new(),
            manifest: "package.json".into(),
            lockfile: lock,
            annotate: vec!["package.json".into()],
            inputs,
            blocked,
        });
    }

    let mut requirements: Vec<String> = files
        .iter()
        .filter(|f| !f.contains('/') && is_requirements(f))
        .cloned()
        .collect();
    requirements.sort();
    let pyproject = has("pyproject.toml");
    if pyproject || !requirements.is_empty() || has("setup.py") || has("Pipfile") {
        let mut annotate = requirements.clone();
        if pyproject {
            annotate.push("pyproject.toml".into());
        }
        let mut inputs = annotate.clone();
        for lock in [
            "uv.lock",
            "poetry.lock",
            "Pipfile.lock",
            "Pipfile",
            "setup.py",
        ] {
            if has(lock) {
                inputs.push(lock.into());
            }
        }
        out.push(Target {
            tool: Tool::PipAudit,
            dir: String::new(),
            manifest: annotate
                .first()
                .cloned()
                .unwrap_or_else(|| "setup.py".into()),
            lockfile: ["uv.lock", "poetry.lock", "Pipfile.lock"]
                .into_iter()
                .find(|l| has(l))
                .map(str::to_string),
            annotate,
            inputs,
            blocked: None,
        });
    }

    if has("Cargo.toml") {
        let locked = has("Cargo.lock");
        out.push(Target {
            tool: Tool::CargoAudit,
            dir: String::new(),
            manifest: "Cargo.toml".into(),
            lockfile: locked.then(|| "Cargo.lock".to_string()),
            annotate: vec!["Cargo.toml".into()],
            inputs: vec!["Cargo.toml".into(), "Cargo.lock".into()],
            blocked: (!locked)
                .then(|| "There is no Cargo.lock yet: build the project first.".to_string()),
        });
    }

    if has("go.mod") {
        out.push(Target {
            tool: Tool::Govulncheck,
            dir: String::new(),
            manifest: "go.mod".into(),
            lockfile: has("go.sum").then(|| "go.sum".to_string()),
            annotate: vec!["go.mod".into()],
            inputs: vec!["go.mod".into(), "go.sum".into()],
            blocked: None,
        });
    }

    let report = opts.dependency_check_report.trim().trim_start_matches("./");
    let builds: Vec<String> = ["pom.xml", "build.gradle", "build.gradle.kts"]
        .into_iter()
        .filter(|b| has(b))
        .map(str::to_string)
        .collect();
    if !report.is_empty() && !builds.is_empty() {
        let mut inputs = builds.clone();
        inputs.push(report.to_string());
        out.push(Target {
            tool: Tool::DependencyCheck,
            dir: String::new(),
            manifest: builds[0].clone(),
            lockfile: Some(report.to_string()),
            annotate: builds,
            inputs,
            blocked: None,
        });
    }
    out
}

/// A stable 64-bit FNV-1a hash (cache keys; not for security).
pub fn fnv1a(parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.as_bytes().iter().chain(std::iter::once(&0xffu8)) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// The cache key of a target: its tool and the contents of its inputs.
pub fn cache_key(target: &Target, read: &dyn Fn(&str) -> Option<String>) -> String {
    let texts: Vec<String> = target
        .inputs
        .iter()
        .map(|p| format!("{p}\n{}", read(p).unwrap_or_default()))
        .collect();
    let mut parts: Vec<&str> = vec![target.tool.id(), &target.dir];
    parts.extend(texts.iter().map(String::as_str));
    fnv1a(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn run(files: &[(&str, &str)], opts: &DetectOptions) -> Vec<Target> {
        let map: HashMap<String, String> = files
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect();
        let names: Vec<String> = map.keys().cloned().collect();
        detect(&names, &|p| map.get(p).cloned(), opts)
    }

    #[test]
    fn finds_the_audits_of_a_project() {
        let t = run(
            &[
                ("composer.json", "{}"),
                ("composer.lock", "{}"),
                ("package.json", r#"{"packageManager": "pnpm@9.15.4"}"#),
                ("pnpm-lock.yaml", ""),
                ("requirements.txt", "requests==2.25.0"),
                ("requirements-dev.txt", ""),
                ("pyproject.toml", ""),
                ("Cargo.toml", ""),
                ("go.mod", "module x"),
            ],
            &DetectOptions::default(),
        );
        let tools: Vec<Tool> = t.iter().map(|t| t.tool).collect();
        assert_eq!(
            tools,
            [
                Tool::Composer,
                Tool::Pnpm,
                Tool::PipAudit,
                Tool::CargoAudit,
                Tool::Govulncheck
            ]
        );
        assert_eq!(t[0].annotate, ["composer.json", "composer.lock"]);
        assert_eq!(t[1].lockfile.as_deref(), Some("pnpm-lock.yaml"));
        assert_eq!(
            t[2].annotate,
            ["requirements-dev.txt", "requirements.txt", "pyproject.toml"]
        );
        assert!(t[3].blocked.is_some());
        assert!(t[4].blocked.is_none());
    }

    #[test]
    fn tells_yarn_versions_and_missing_locks_apart() {
        let classic = run(
            &[
                ("package.json", "{}"),
                ("yarn.lock", "# yarn lockfile v1\n"),
            ],
            &DetectOptions::default(),
        );
        assert_eq!(classic[0].tool, Tool::YarnClassic);
        let berry = run(
            &[
                ("package.json", "{}"),
                ("yarn.lock", "__metadata:\n  version: 8\n"),
            ],
            &DetectOptions::default(),
        );
        assert_eq!(berry[0].tool, Tool::YarnBerry);
        let field = run(
            &[("package.json", r#"{"packageManager": "yarn@4.5.0"}"#)],
            &DetectOptions::default(),
        );
        assert_eq!(field[0].tool, Tool::YarnBerry);
        assert!(field[0]
            .blocked
            .as_deref()
            .unwrap()
            .contains("yarn install"));
        let npm = run(
            &[("package.json", "{}"), ("package-lock.json", "{}")],
            &DetectOptions::default(),
        );
        assert_eq!(npm[0].tool, Tool::Npm);
        assert!(npm[0].blocked.is_none());
    }

    #[test]
    fn audits_maven_only_with_a_report() {
        let files = [("pom.xml", "<project/>")];
        assert!(run(&files, &DetectOptions::default()).is_empty());
        let t = run(
            &files,
            &DetectOptions {
                dependency_check_report: "target/dependency-check-report.json".into(),
                ..Default::default()
            },
        );
        assert_eq!(t[0].tool, Tool::DependencyCheck);
        assert_eq!(
            t[0].inputs,
            ["pom.xml", "target/dependency-check-report.json"]
        );
    }

    #[test]
    fn composer_json_may_live_in_a_folder() {
        let t = run(
            &[("api/composer.json", "{}")],
            &DetectOptions {
                composer_json: "./api/composer.json".into(),
                ..Default::default()
            },
        );
        assert_eq!(t[0].dir, "api");
        assert_eq!(t[0].manifest, "api/composer.json");
        assert_eq!(t[0].lockfile, None);
        assert_eq!(t[0].inputs, ["api/composer.json", "api/composer.lock"]);
    }

    #[test]
    fn cache_keys_follow_the_lock_file() {
        let target = &run(
            &[("Cargo.toml", ""), ("Cargo.lock", "a")],
            &DetectOptions::default(),
        )[0];
        let a = cache_key(target, &|p| (p == "Cargo.lock").then(|| "a".to_string()));
        let b = cache_key(target, &|p| (p == "Cargo.lock").then(|| "b".to_string()));
        assert_ne!(a, b);
        assert_eq!(
            a,
            cache_key(target, &|p| (p == "Cargo.lock").then(|| "a".to_string()))
        );
    }
}
