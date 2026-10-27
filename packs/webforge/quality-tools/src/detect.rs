//! Which tools apply to a file, like PHPStorm's automatic configuration: the package from
//! the nearest `node_modules` (walking up from the file to the project root) and the
//! nearest configuration file (or package.json key). Manual mode takes the configured
//! package, configuration file and working directories instead.
//!
//! Everything works on project-relative paths through [`ProjectFs`], so the app can answer
//! from any backend (local or remote).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Mode, ToolConfig, WebTool, WebToolsSettings};

/// Read access to the project: project-relative paths (`""` is the root); absolute paths
/// only for a manually configured package outside the project.
pub trait ProjectFs {
    /// A file's text, `None` when it does not exist.
    fn read(&self, path: &str) -> Option<String>;
    /// The names in a folder (empty when it does not exist).
    fn list(&self, dir: &str) -> Vec<String>;
}

/// How the tool is started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Launcher {
    /// `node <script>`: the package's bin script (project-relative or absolute).
    Node { script: String },
    /// An executable run directly (a manually configured `node_modules/.bin/eslint`).
    Executable { path: String },
}

/// A tool that applies to a file, and how it runs there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Located {
    pub tool: WebTool,
    pub launcher: Launcher,
    /// The package folder (project-relative or absolute), when a package was found.
    pub package: Option<String>,
    pub version: Option<String>,
    /// The configuration file (project-relative or absolute); `None` when the tool finds
    /// it itself in package.json or uses its defaults.
    pub config: Option<String>,
    /// The configuration file was set manually (passed on the command line).
    pub explicit_config: bool,
    /// Project-relative working directory (`""`: the root).
    pub working_dir: String,
    /// ESLint: flat config (`eslint.config.*`), `Some(false)` for `.eslintrc*`.
    pub flat_config: Option<bool>,
}

/// Why a tool does not apply to a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "reason")]
pub enum Skip {
    Disabled,
    /// "Run for files" does not match.
    NotMatched,
    /// Automatic mode: no configuration file above the file.
    NoConfig,
    /// The package is not installed (or the manual path does not exist).
    NoPackage,
    /// Biome applies to the file and takes priority.
    BiomeFirst,
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn is_absolute(p: &str) -> bool {
    p.starts_with('/') || p.get(1..3).is_some_and(|s| s == ":/" || s == ":\\")
}

/// `dir` and the folders above it, up to the root (`""`).
fn ancestors(dir: &str) -> Vec<String> {
    let mut out = vec![dir.to_string()];
    let mut d = dir;
    while !d.is_empty() {
        d = parent(d);
        out.push(d.to_string());
    }
    out
}

/// The script of the package's `bin` entry for `tool`.
pub fn bin_entry(tool: WebTool, package_json: &str) -> String {
    let json: Value = serde_json::from_str(package_json).unwrap_or(Value::Null);
    let entry = match &json["bin"] {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => map
            .get(tool.bin())
            .or_else(|| map.values().next())
            .and_then(|v| v.as_str().map(str::to_string)),
        _ => None,
    };
    entry
        .map(|e| e.trim_start_matches("./").to_string())
        .unwrap_or_else(|| tool.default_entry().to_string())
}

fn version_of(package_json: &str) -> Option<String> {
    let json: Value = serde_json::from_str(package_json).ok()?;
    json["version"].as_str().map(str::to_string)
}

/// The nearest configuration file above `file` (its folder first): the file name, or the
/// folder's package.json when it has the tool's key.
pub fn nearest_config(fs: &dyn ProjectFs, tool: WebTool, file: &str) -> Option<String> {
    for dir in ancestors(parent(file)) {
        let names = fs.list(&dir);
        if let Some(name) = tool
            .config_files()
            .iter()
            .find(|n| names.iter().any(|x| x == *n))
        {
            return Some(join(&dir, name));
        }
        if let Some(key) = tool.package_json_key() {
            if names.iter().any(|n| n == "package.json") {
                let pj = join(&dir, "package.json");
                let has = fs
                    .read(&pj)
                    .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                    .is_some_and(|v| !v[key].is_null());
                if has {
                    return Some(pj);
                }
            }
        }
    }
    None
}

/// The nearest `node_modules/<package>` above `dir` (project-relative package folder).
pub fn nearest_package(fs: &dyn ProjectFs, tool: WebTool, dir: &str) -> Option<(String, String)> {
    ancestors(dir).into_iter().find_map(|d| {
        let pkg = join(&d, &format!("node_modules/{}", tool.package()));
        fs.read(&format!("{pkg}/package.json")).map(|pj| (pkg, pj))
    })
}

/// The manually configured package: a package folder (or its package.json), else an
/// executable.
fn manual_package(fs: &dyn ProjectFs, tool: WebTool, custom: &str) -> Located {
    let custom = custom.trim_end_matches('/');
    let dir = custom.strip_suffix("/package.json").unwrap_or(custom);
    let launcher_for = |pj: &str| Launcher::Node {
        script: join(dir, &bin_entry(tool, pj)),
    };
    let (launcher, package, version) = match fs.read(&format!("{dir}/package.json")) {
        Some(pj) => (launcher_for(&pj), Some(dir.to_string()), version_of(&pj)),
        None => (
            Launcher::Executable {
                path: custom.to_string(),
            },
            None,
            None,
        ),
    };
    Located {
        tool,
        launcher,
        package,
        version,
        config: None,
        explicit_config: false,
        working_dir: String::new(),
        flat_config: None,
    }
}

/// Whether a manual executable exists (`fs.read` answers for files only).
fn exists(fs: &dyn ProjectFs, path: &str) -> bool {
    if is_absolute(path) {
        // Outside the project: trust the setting, running it reports what is wrong.
        return true;
    }
    fs.list(parent(path))
        .iter()
        .any(|n| n == path.rsplit('/').next().unwrap_or(path))
}

/// How `cfg`'s tool runs for the project file `file`, or why it does not apply.
pub fn locate(fs: &dyn ProjectFs, cfg: &ToolConfig, file: &str) -> Result<Located, Skip> {
    let tool = cfg.tool;
    if cfg.mode == Mode::Disabled {
        return Err(Skip::Disabled);
    }
    if !cfg.matches(file) {
        return Err(Skip::NotMatched);
    }
    let found_config = nearest_config(fs, tool, file);
    let (config, explicit) = match cfg.custom_config() {
        Some(c) => (Some(c.to_string()), true),
        None => (found_config.clone(), false),
    };
    if cfg.mode == Mode::Automatic && config.is_none() {
        return Err(Skip::NoConfig);
    }
    // The tool runs in the folder of its configuration (ESLint's flat config lookup and
    // Prettier's .prettierignore start there), or a manual working directory.
    let config_dir = found_config.as_deref().map(parent).unwrap_or("").to_string();
    let working_dir = cfg
        .working_dirs()
        .into_iter()
        .filter(|d| d.is_empty() || file.starts_with(&format!("{d}/")))
        .max_by_key(|d| d.len())
        .unwrap_or(config_dir);
    let mut located = match cfg.custom_package() {
        Some(custom) => {
            let l = manual_package(fs, tool, custom);
            if let Launcher::Executable { path } = &l.launcher {
                if !exists(fs, path) {
                    return Err(Skip::NoPackage);
                }
            }
            l
        }
        None => {
            let (pkg, pj) = nearest_package(fs, tool, parent(file)).ok_or(Skip::NoPackage)?;
            Located {
                tool,
                launcher: Launcher::Node {
                    script: join(&pkg, &bin_entry(tool, &pj)),
                },
                version: version_of(&pj),
                package: Some(pkg),
                config: None,
                explicit_config: false,
                working_dir: String::new(),
                flat_config: None,
            }
        }
    };
    // A manual configuration file: ESLint decides by itself what kind it is.
    located.flat_config = (tool == WebTool::Eslint && !explicit)
        .then(|| {
            config.as_deref().map(|c| {
                let name = c.rsplit('/').next().unwrap_or(c);
                name.starts_with("eslint.config.")
            })
        })
        .flatten();
    located.config = config;
    located.explicit_config = explicit;
    located.working_dir = working_dir;
    Ok(located)
}

/// Every tool for `file`: those that apply and why the others do not. Biome takes over
/// ESLint and Prettier when it applies and has priority.
pub fn locate_all(
    fs: &dyn ProjectFs,
    settings: &WebToolsSettings,
    file: &str,
) -> Vec<(WebTool, Result<Located, Skip>)> {
    let mut out: Vec<(WebTool, Result<Located, Skip>)> = settings
        .tools
        .iter()
        .map(|c| (c.tool, locate(fs, c, file)))
        .collect();
    let biome = out
        .iter()
        .any(|(t, r)| *t == WebTool::Biome && r.is_ok());
    if biome && settings.biome_priority {
        for (t, r) in &mut out {
            if matches!(t, WebTool::Eslint | WebTool::Prettier) && r.is_ok() {
                *r = Err(Skip::BiomeFirst);
            }
        }
    }
    out
}

/// The tools that apply to `file`.
pub fn applicable(fs: &dyn ProjectFs, settings: &WebToolsSettings, file: &str) -> Vec<Located> {
    locate_all(fs, settings, file)
        .into_iter()
        .filter_map(|(_, r)| r.ok())
        .collect()
}

/// The formatter of Reformat Code for `file`, when one is turned on for it: Biome before
/// Prettier.
pub fn formatter(fs: &dyn ProjectFs, settings: &WebToolsSettings, file: &str) -> Option<Located> {
    let on = |t: WebTool| settings.config(t).on_reformat;
    let found = applicable(fs, settings, file);
    [WebTool::Biome, WebTool::Prettier]
        .into_iter()
        .filter(|&t| on(t))
        .find_map(|t| found.iter().find(|l| l.tool == t).cloned())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A project in memory: path -> text.
    pub struct MemFs(pub BTreeMap<String, String>);

    impl MemFs {
        pub fn new(files: &[(&str, &str)]) -> Self {
            Self(
                files
                    .iter()
                    .map(|(p, t)| (p.to_string(), t.to_string()))
                    .collect(),
            )
        }
    }

    impl ProjectFs for MemFs {
        fn read(&self, path: &str) -> Option<String> {
            self.0.get(path).cloned()
        }
        fn list(&self, dir: &str) -> Vec<String> {
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            let mut names: Vec<String> = self
                .0
                .keys()
                .filter_map(|p| p.strip_prefix(&prefix))
                .map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
                .collect();
            names.dedup();
            names
        }
    }

    fn monorepo() -> MemFs {
        MemFs::new(&[
            ("package.json", r#"{"name":"root","prettier":{"semi":false}}"#),
            (
                "node_modules/eslint/package.json",
                r#"{"version":"9.25.1","bin":{"eslint":"./bin/eslint.js"}}"#,
            ),
            (
                "node_modules/prettier/package.json",
                r#"{"version":"3.5.3","bin":"./bin/prettier.cjs"}"#,
            ),
            ("packages/web/eslint.config.js", "export default []"),
            ("packages/web/src/App.tsx", ""),
            ("packages/legacy/.eslintrc.json", "{}"),
            ("packages/legacy/index.js", ""),
            ("packages/api/src/a.ts", ""),
            (
                "packages/styles/node_modules/stylelint/package.json",
                r#"{"version":"16.19.0","bin":{"stylelint":"bin/stylelint.mjs"}}"#,
            ),
            ("packages/styles/.stylelintrc.json", "{}"),
            ("packages/styles/card.scss", ""),
        ])
    }

    #[test]
    fn automatic_mode_needs_a_package_and_a_configuration() {
        let fs = monorepo();
        let eslint = ToolConfig::new(WebTool::Eslint);
        let l = locate(&fs, &eslint, "packages/web/src/App.tsx").unwrap();
        assert_eq!(
            l.launcher,
            Launcher::Node {
                script: "node_modules/eslint/bin/eslint.js".into()
            }
        );
        assert_eq!(l.version.as_deref(), Some("9.25.1"));
        assert_eq!(l.config.as_deref(), Some("packages/web/eslint.config.js"));
        assert_eq!(l.working_dir, "packages/web");
        assert_eq!(l.flat_config, Some(true));
        let legacy = locate(&fs, &eslint, "packages/legacy/index.js").unwrap();
        assert_eq!(legacy.flat_config, Some(false));
        assert_eq!(
            locate(&fs, &eslint, "packages/api/src/a.ts"),
            Err(Skip::NoConfig)
        );
        assert_eq!(
            locate(&fs, &eslint, "packages/styles/card.scss"),
            Err(Skip::NotMatched)
        );
        // Prettier's configuration is the root package.json key.
        let p = locate(&fs, &ToolConfig::new(WebTool::Prettier), "packages/api/src/a.ts").unwrap();
        assert_eq!(p.config.as_deref(), Some("package.json"));
        assert_eq!(
            p.launcher,
            Launcher::Node {
                script: "node_modules/prettier/bin/prettier.cjs".into()
            }
        );
        // A package in a nested node_modules.
        let s = locate(&fs, &ToolConfig::new(WebTool::Stylelint), "packages/styles/card.scss").unwrap();
        assert_eq!(s.package.as_deref(), Some("packages/styles/node_modules/stylelint"));
        let off = ToolConfig {
            mode: Mode::Disabled,
            ..ToolConfig::new(WebTool::Eslint)
        };
        assert_eq!(locate(&fs, &off, "packages/web/src/App.tsx"), Err(Skip::Disabled));
    }

    #[test]
    fn manual_mode_takes_the_configured_package_and_working_directories() {
        let fs = monorepo();
        let cfg = ToolConfig {
            mode: Mode::Manual,
            package: "node_modules/eslint".into(),
            config: "configs/eslint.strict.js".into(),
            working_dirs: "packages/api; packages".into(),
            ..ToolConfig::new(WebTool::Eslint)
        };
        let l = locate(&fs, &cfg, "packages/api/src/a.ts").unwrap();
        assert_eq!(l.config.as_deref(), Some("configs/eslint.strict.js"));
        assert!(l.explicit_config);
        assert_eq!(l.working_dir, "packages/api");
        assert_eq!(l.flat_config, None);
        // An executable instead of a package.
        let fs = MemFs::new(&[("tools/eslint", "#!/bin/sh"), ("a.js", "")]);
        let exe = ToolConfig {
            mode: Mode::Manual,
            package: "tools/eslint".into(),
            ..ToolConfig::new(WebTool::Eslint)
        };
        assert_eq!(
            locate(&fs, &exe, "a.js").unwrap().launcher,
            Launcher::Executable {
                path: "tools/eslint".into()
            }
        );
        let missing = ToolConfig {
            package: "tools/nope".into(),
            ..exe
        };
        assert_eq!(locate(&fs, &missing, "a.js"), Err(Skip::NoPackage));
    }

    #[test]
    fn biome_takes_priority_over_eslint_and_prettier() {
        let fs = MemFs::new(&[
            ("biome.json", "{}"),
            ("eslint.config.mjs", ""),
            (".prettierrc", "{}"),
            ("node_modules/@biomejs/biome/package.json", r#"{"version":"1.9.4","bin":{"biome":"bin/biome"}}"#),
            ("node_modules/eslint/package.json", "{}"),
            ("node_modules/prettier/package.json", "{}"),
            ("src/a.ts", ""),
            ("src/page.html", ""),
        ]);
        let mut settings = WebToolsSettings::from_settings(&|_| None);
        let tools = |s: &WebToolsSettings, f: &str| -> Vec<WebTool> {
            applicable(&fs, s, f).iter().map(|l| l.tool).collect()
        };
        assert_eq!(tools(&settings, "src/a.ts"), vec![WebTool::Biome]);
        // Biome does not handle HTML: ESLint stays.
        assert_eq!(tools(&settings, "src/page.html"), vec![WebTool::Eslint]);
        settings.biome_priority = false;
        assert_eq!(
            tools(&settings, "src/a.ts"),
            vec![WebTool::Eslint, WebTool::Prettier, WebTool::Biome]
        );
        // Reformat Code: Biome before Prettier, only when turned on.
        assert_eq!(formatter(&fs, &settings, "src/a.ts"), None);
        for c in &mut settings.tools {
            c.on_reformat = true;
        }
        assert_eq!(formatter(&fs, &settings, "src/a.ts").unwrap().tool, WebTool::Biome);
        assert_eq!(
            bin_entry(WebTool::Biome, r#"{"bin":{"biome":"bin/biome"}}"#),
            "bin/biome"
        );
        assert_eq!(bin_entry(WebTool::Eslint, "{}"), "bin/eslint.js");
    }
}
