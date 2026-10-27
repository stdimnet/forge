//! The tools' command lines. The editor text always goes through stdin, so unsaved text is
//! checked, fixed and formatted without writing the file; only Actions on Save fix the
//! saved file in place.
//!
//! | Tool | Check | Fix (text) | Format | On save |
//! | --- | --- | --- | --- | --- |
//! | ESLint | `--format json --stdin --stdin-filename <file>` | `--fix-dry-run …` (JSON `output`) | | `--fix <file>` |
//! | Prettier | | | `--stdin-filepath <file>` | `--write <file>` |
//! | Stylelint | `--formatter json --stdin-filename <file>` | `--fix --stdin-filename <file>` (stdout) | | `--fix <file>` |
//! | Biome | `lint --reporter=json --stdin-file-path=<file>` | `lint --write --stdin-file-path=<file>` | `format --stdin-file-path=<file>` | `check --write <file>` |
//!
//! Package scripts run with the Node.js interpreter (`node <script> …`); a manually set
//! executable runs directly.

use serde::{Deserialize, Serialize};

use crate::detect::{Launcher, Located};
use crate::model::{ToolConfig, WebTool};

/// One run: the program, its arguments, stdin, extra environment and working directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invocation {
    /// Run `program` with the Node.js interpreter (`node <program> <args>`).
    pub node: bool,
    /// Absolute script or executable.
    pub program: String,
    pub args: Vec<String>,
    pub stdin: String,
    pub env: Vec<(String, String)>,
    /// Absolute working directory.
    pub cwd: String,
}

/// `rel` (project-relative or absolute) as an absolute path below `root`.
pub fn absolute(root: &str, rel: &str) -> String {
    let root = root.trim_end_matches('/');
    if rel.is_empty() {
        root.to_string()
    } else if rel.starts_with('/') || rel.get(1..3).is_some_and(|s| s == ":/" || s == ":\\") {
        rel.to_string()
    } else {
        format!("{root}/{}", rel.trim_start_matches("./"))
    }
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn base(l: &Located, root: &str) -> Invocation {
    let (node, program) = match &l.launcher {
        Launcher::Node { script } => (true, absolute(root, script)),
        Launcher::Executable { path } => (false, absolute(root, path)),
    };
    let mut env = Vec::new();
    if let Some(flat) = l.flat_config {
        // ESLint 8 needs it for eslint.config.*, ESLint 9 for .eslintrc*.
        env.push(("ESLINT_USE_FLAT_CONFIG".into(), flat.to_string()));
    }
    Invocation {
        node,
        program,
        cwd: absolute(root, &l.working_dir),
        env,
        ..Invocation::default()
    }
}

/// The configuration file option, when it was set manually (the tools find theirs).
fn config_args(l: &Located, root: &str) -> Vec<String> {
    let Some(config) = l.config.as_deref().filter(|_| l.explicit_config) else {
        return Vec::new();
    };
    let abs = absolute(root, config);
    match l.tool {
        WebTool::Biome => {
            // Biome takes the folder of biome.json.
            let dir = if abs.ends_with(".json") || abs.ends_with(".jsonc") {
                parent(&abs).to_string()
            } else {
                abs
            };
            vec![format!("--config-path={dir}")]
        }
        _ => vec!["--config".into(), abs],
    }
}

fn with(mut inv: Invocation, args: Vec<String>, stdin: &str) -> Invocation {
    inv.args = args;
    inv.stdin = stdin.to_string();
    inv
}

/// Check the editor `text` of `file` (project-relative) and print a JSON report.
pub fn lint(l: &Located, cfg: &ToolConfig, root: &str, file: &str, text: &str) -> Invocation {
    let abs = absolute(root, file);
    let mut args: Vec<String> = match l.tool {
        WebTool::Eslint => vec!["--format".into(), "json".into()],
        WebTool::Stylelint => vec!["--formatter".into(), "json".into()],
        WebTool::Biome => vec![
            "lint".into(),
            "--reporter=json".into(),
            "--colors=off".into(),
        ],
        WebTool::Prettier => vec!["--check".into()],
    };
    args.extend(config_args(l, root));
    args.extend(cfg.extra_args());
    match l.tool {
        WebTool::Eslint => {
            args.extend(["--stdin".into(), "--stdin-filename".into(), abs]);
        }
        WebTool::Stylelint => args.extend(["--stdin-filename".into(), abs]),
        WebTool::Biome => args.push(format!("--stdin-file-path={abs}")),
        WebTool::Prettier => args.extend(["--stdin-filepath".into(), abs]),
    }
    with(base(l, root), args, text)
}

/// Check files or folders (absolute) for Inspect Code, printing a JSON report.
pub fn lint_paths(l: &Located, cfg: &ToolConfig, root: &str, targets: &[String]) -> Invocation {
    let mut args: Vec<String> = match l.tool {
        WebTool::Eslint => vec![
            "--format".into(),
            "json".into(),
            "--no-error-on-unmatched-pattern".into(),
        ],
        WebTool::Stylelint => vec![
            "--formatter".into(),
            "json".into(),
            "--allow-empty-input".into(),
        ],
        WebTool::Biome => vec![
            "lint".into(),
            "--reporter=json".into(),
            "--colors=off".into(),
        ],
        WebTool::Prettier => vec!["--check".into()],
    };
    args.extend(config_args(l, root));
    args.extend(cfg.extra_args());
    let targets: Vec<String> = match (l.tool, targets.is_empty()) {
        (WebTool::Stylelint, true) => vec!["**/*.{css,scss,less,vue}".into()],
        (_, true) => vec![".".into()],
        (WebTool::Stylelint, false) => targets
            .iter()
            .map(|t| {
                // Stylelint takes globs: a folder means the style sheets below it.
                if t.rsplit('/').next().is_some_and(|n| n.contains('.')) {
                    t.clone()
                } else {
                    format!("{}/**/*.{{css,scss,less,vue}}", t.trim_end_matches('/'))
                }
            })
            .collect(),
        (_, false) => targets.to_vec(),
    };
    args.extend(targets);
    with(base(l, root), args, "")
}

/// Fix the editor `text` of `file`: ESLint prints JSON with the fixed `output`, Stylelint
/// and Biome print the fixed text.
pub fn fix(l: &Located, cfg: &ToolConfig, root: &str, file: &str, text: &str) -> Invocation {
    let abs = absolute(root, file);
    let mut args: Vec<String> = match l.tool {
        WebTool::Eslint => vec!["--fix-dry-run".into(), "--format".into(), "json".into()],
        WebTool::Stylelint => vec!["--fix".into()],
        WebTool::Biome => vec!["lint".into(), "--write".into(), "--colors=off".into()],
        WebTool::Prettier => return format(l, cfg, root, file, text),
    };
    args.extend(config_args(l, root));
    args.extend(cfg.extra_args());
    match l.tool {
        WebTool::Eslint => args.extend(["--stdin".into(), "--stdin-filename".into(), abs]),
        WebTool::Stylelint => args.extend(["--stdin-filename".into(), abs]),
        _ => args.push(format!("--stdin-file-path={abs}")),
    }
    with(base(l, root), args, text)
}

/// Format the editor `text` of `file` (Prettier, Biome); the formatted text is printed.
pub fn format(l: &Located, cfg: &ToolConfig, root: &str, file: &str, text: &str) -> Invocation {
    let abs = absolute(root, file);
    let mut args: Vec<String> = match l.tool {
        WebTool::Biome => vec!["format".into(), "--colors=off".into()],
        _ => Vec::new(),
    };
    args.extend(config_args(l, root));
    args.extend(cfg.extra_args());
    match l.tool {
        WebTool::Biome => args.push(format!("--stdin-file-path={abs}")),
        _ => args.extend(["--stdin-filepath".into(), abs]),
    }
    with(base(l, root), args, text)
}

/// Fix the saved `file` in place (Actions on Save).
pub fn fix_in_place(l: &Located, cfg: &ToolConfig, root: &str, file: &str) -> Invocation {
    let mut args: Vec<String> = match l.tool {
        WebTool::Eslint | WebTool::Stylelint => vec!["--fix".into()],
        WebTool::Prettier => vec!["--write".into()],
        WebTool::Biome => vec!["check".into(), "--write".into(), "--colors=off".into()],
    };
    args.extend(config_args(l, root));
    args.extend(cfg.extra_args());
    args.push(absolute(root, file));
    with(base(l, root), args, "")
}

/// Print the tool's version ("Validate" on the settings page).
pub fn version(l: &Located, root: &str) -> Invocation {
    with(base(l, root), vec!["--version".into()], "")
}

/// The version line of `--version` ("v9.25.1", "Version: 1.9.4", "16.19.0").
pub fn parse_version(out: &str) -> Option<String> {
    out.lines()
        .map(str::trim)
        .find(|l| l.chars().any(|c| c.is_ascii_digit()))
        .map(|l| l.trim_start_matches("Version:").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Mode;

    fn located(tool: WebTool) -> Located {
        Located {
            tool,
            launcher: Launcher::Node {
                script: format!("node_modules/{}/{}", tool.package(), tool.default_entry()),
            },
            package: None,
            version: None,
            config: None,
            explicit_config: false,
            working_dir: "packages/web".into(),
            flat_config: (tool == WebTool::Eslint).then_some(true),
        }
    }

    #[test]
    fn eslint_reads_the_editor_text_from_stdin() {
        let l = located(WebTool::Eslint);
        let cfg = ToolConfig::new(WebTool::Eslint);
        let inv = lint(&l, &cfg, "/p", "packages/web/src/a.ts", "let a\n");
        assert!(inv.node);
        assert_eq!(inv.program, "/p/node_modules/eslint/bin/eslint.js");
        assert_eq!(
            inv.args,
            vec![
                "--format",
                "json",
                "--stdin",
                "--stdin-filename",
                "/p/packages/web/src/a.ts"
            ]
        );
        assert_eq!(inv.stdin, "let a\n");
        assert_eq!(inv.cwd, "/p/packages/web");
        assert_eq!(
            inv.env,
            vec![("ESLINT_USE_FLAT_CONFIG".to_string(), "true".to_string())]
        );
        let f = fix(&l, &cfg, "/p", "packages/web/src/a.ts", "let a\n");
        assert_eq!(f.args[..3], ["--fix-dry-run", "--format", "json"]);
        let s = fix_in_place(&l, &cfg, "/p", "packages/web/src/a.ts");
        assert_eq!(s.args, vec!["--fix", "/p/packages/web/src/a.ts"]);
    }

    #[test]
    fn manual_configuration_files_and_options_are_passed() {
        let mut l = located(WebTool::Stylelint);
        l.config = Some("config/stylelint.json".into());
        l.explicit_config = true;
        let cfg = ToolConfig {
            mode: Mode::Manual,
            options: "--quiet".into(),
            ..ToolConfig::new(WebTool::Stylelint)
        };
        let inv = lint(&l, &cfg, "/p", "a.scss", "a{}");
        assert_eq!(
            inv.args,
            vec![
                "--formatter",
                "json",
                "--config",
                "/p/config/stylelint.json",
                "--quiet",
                "--stdin-filename",
                "/p/a.scss"
            ]
        );
        let mut b = located(WebTool::Biome);
        b.config = Some("tools/biome.jsonc".into());
        b.explicit_config = true;
        let inv = lint(&b, &ToolConfig::new(WebTool::Biome), "/p", "src/a.ts", "");
        assert_eq!(
            inv.args,
            vec![
                "lint",
                "--reporter=json",
                "--colors=off",
                "--config-path=/p/tools",
                "--stdin-file-path=/p/src/a.ts"
            ]
        );
    }

    #[test]
    fn formatters_and_fixers() {
        let p = located(WebTool::Prettier);
        let cfg = ToolConfig::new(WebTool::Prettier);
        assert_eq!(
            format(&p, &cfg, "/p", "src/a.ts", "x").args,
            vec!["--stdin-filepath", "/p/src/a.ts"]
        );
        assert_eq!(
            fix_in_place(&p, &cfg, "/p", "src/a.ts").args,
            vec!["--write", "/p/src/a.ts"]
        );
        let b = located(WebTool::Biome);
        let bc = ToolConfig::new(WebTool::Biome);
        assert_eq!(
            format(&b, &bc, "/p", "src/a.ts", "x").args,
            vec!["format", "--colors=off", "--stdin-file-path=/p/src/a.ts"]
        );
        assert_eq!(
            fix_in_place(&b, &bc, "/p", "src/a.ts").args,
            vec!["check", "--write", "--colors=off", "/p/src/a.ts"]
        );
        assert_eq!(
            lint_paths(
                &located(WebTool::Stylelint),
                &ToolConfig::new(WebTool::Stylelint),
                "/p",
                &["/p/src".into(), "/p/a.css".into()]
            )
            .args[3..],
            ["/p/src/**/*.{css,scss,less,vue}", "/p/a.css"]
        );
        let exe = Located {
            launcher: Launcher::Executable {
                path: "/usr/bin/eslint".into(),
            },
            ..located(WebTool::Eslint)
        };
        let v = version(&exe, "/p");
        assert!(!v.node);
        assert_eq!(v.program, "/usr/bin/eslint");
    }

    #[test]
    fn reads_versions() {
        assert_eq!(parse_version("v9.25.1\n").as_deref(), Some("v9.25.1"));
        assert_eq!(parse_version("Version: 1.9.4\n").as_deref(), Some("1.9.4"));
        assert_eq!(parse_version("no version"), None);
    }
}
