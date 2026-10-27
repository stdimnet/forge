//! The tools' command lines, as if they ran locally in the project root: the app runs them
//! directly (where the project lives, so remote projects work) or wraps them with the
//! tool's PHP interpreter (`phpforge-interpreters`, which maps the absolute paths in the
//! arguments).
//!
//! | Tool | Check | Fix |
//! | --- | --- | --- |
//! | PHPStan | `analyse --error-format=json [--tmp-file … --instead-of …]` | |
//! | Psalm | `--output-format=json` | |
//! | PHP_CodeSniffer | `phpcs --report=json [--stdin-path=<file> -]` | `phpcbf --stdin-path=<file> -` (stdout) |
//! | PHP Mess Detector | `phpmd <files> json <rulesets>` | |
//! | PHP CS Fixer | `fix --dry-run --diff --format=json [-]` | the diff, applied in Rust |
//! | Laravel Pint | `--test --format=json` | `pint <file>` (in place) |
//! | Rector | `process --dry-run --output-format=json` | the diff, applied in Rust |

use serde::{Deserialize, Serialize};

use crate::model::{QualityTool, ToolConfig};

/// PHPMD's rulesets when neither a configuration file nor rulesets are set.
pub const DEFAULT_PHPMD_RULESETS: &str =
    "cleancode,codesize,controversial,design,naming,unusedcode";

/// What the tool checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// The saved files.
    Files(Vec<String>),
    /// The editor text of one file, on stdin.
    Stdin { file: String, text: String },
    /// The editor text of one file in a temporary copy (PHPStan's editor mode).
    TempCopy { file: String, copy: String },
}

impl Input {
    /// The project files the run is about.
    pub fn files(&self) -> Vec<String> {
        match self {
            Input::Files(f) => f.clone(),
            Input::Stdin { file, .. } | Input::TempCopy { file, .. } => vec![file.clone()],
        }
    }

    /// Names the report may use instead of the project file (with the file they stand for).
    pub fn aliases(&self) -> Vec<(String, String)> {
        match self {
            Input::Files(_) => Vec::new(),
            Input::Stdin { file, .. } => ["-", "STDIN", "php://stdin"]
                .iter()
                .map(|a| (a.to_string(), file.clone()))
                .collect(),
            Input::TempCopy { file, copy } => vec![(copy.clone(), file.clone())],
        }
    }
}

/// Arguments and input of one run (the program is the tool's executable).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolArgs {
    pub args: Vec<String>,
    pub stdin: String,
}

/// How a fixer produces the fixed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FixMode {
    /// Reads the text on stdin and prints the fixed text (phpcbf).
    Stdout,
    /// Prints a diff (dry run) that Forge applies (PHP CS Fixer on stdin, Rector on the file).
    Diff,
    /// Rewrites the saved file (Pint).
    InPlace,
}

pub fn fix_mode(tool: QualityTool) -> Option<FixMode> {
    match tool {
        QualityTool::Phpcs => Some(FixMode::Stdout),
        QualityTool::PhpCsFixer | QualityTool::Rector => Some(FixMode::Diff),
        QualityTool::Pint => Some(FixMode::InPlace),
        _ => None,
    }
}

/// `config` (the configured or found configuration file, absolute) as the tool's option.
fn config_arg(tool: QualityTool, config: Option<&str>) -> Option<String> {
    let c = config?;
    Some(match tool {
        QualityTool::Phpstan => format!("--configuration={c}"),
        QualityTool::Phpcs => format!("--standard={c}"),
        // PHPMD takes it as its rulesets argument.
        QualityTool::Phpmd => return None,
        _ => format!("--config={c}"),
    })
}

/// The level / ruleset option, when the configuration sets one.
fn level_args(cfg: &ToolConfig, config: Option<&str>) -> Vec<String> {
    let level = cfg.level.as_str();
    if level.is_empty() {
        return Vec::new();
    }
    match cfg.tool {
        QualityTool::Phpstan => vec![format!("--level={level}")],
        // A configuration file is the standard itself.
        QualityTool::Phpcs if config.is_none() => vec![format!("--standard={level}")],
        QualityTool::PhpCsFixer => vec![format!("--rules={level}")],
        QualityTool::Pint => vec![format!("--preset={level}")],
        _ => Vec::new(),
    }
}

fn base(cfg: &ToolConfig, config: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = config_arg(cfg.tool, config).into_iter().collect();
    args.extend(level_args(cfg, config));
    args
}

fn phpmd_rulesets(cfg: &ToolConfig, config: Option<&str>) -> String {
    config
        .map(str::to_string)
        .or_else(|| Some(cfg.level.clone()).filter(|l| !l.is_empty()))
        .unwrap_or_else(|| DEFAULT_PHPMD_RULESETS.to_string())
}

/// The arguments that check `input` and print the tool's JSON report.
pub fn check_args(cfg: &ToolConfig, config: Option<&str>, input: &Input) -> ToolArgs {
    let tool = cfg.tool;
    let mut stdin = String::new();
    let files = input.files();
    let mut args: Vec<String> = match tool {
        QualityTool::Phpstan => {
            let mut a = vec![
                "analyse".into(),
                "--error-format=json".into(),
                "--no-progress".into(),
                "--no-interaction".into(),
            ];
            a.extend(base(cfg, config));
            a
        }
        QualityTool::Psalm => {
            let mut a = vec![
                "--output-format=json".into(),
                "--no-progress".into(),
                "--monochrome".into(),
            ];
            a.extend(base(cfg, config));
            a
        }
        QualityTool::Phpcs => {
            let mut a = vec!["--report=json".into(), "-q".into()];
            a.extend(base(cfg, config));
            a
        }
        QualityTool::Phpmd => vec![files.join(","), "json".into(), phpmd_rulesets(cfg, config)],
        QualityTool::PhpCsFixer => {
            let mut a = vec![
                "fix".into(),
                "--dry-run".into(),
                "--diff".into(),
                "--format=json".into(),
                "--no-interaction".into(),
                "--show-progress=none".into(),
                "--using-cache=no".into(),
            ];
            a.extend(base(cfg, config));
            a
        }
        QualityTool::Pint => {
            let mut a = vec!["--test".into(), "--format=json".into()];
            a.extend(base(cfg, config));
            a
        }
        QualityTool::Rector => {
            let mut a = vec![
                "process".into(),
                "--dry-run".into(),
                "--output-format=json".into(),
                "--no-progress-bar".into(),
            ];
            a.extend(base(cfg, config));
            a
        }
    };
    args.extend(cfg.extra_args());
    match input {
        // PHPMD got its files as the first argument.
        _ if tool == QualityTool::Phpmd => {}
        Input::Stdin { file, text } => {
            stdin = text.clone();
            match tool {
                QualityTool::Phpcs => {
                    args.push(format!("--stdin-path={file}"));
                    args.push("-".into());
                }
                _ => args.push("-".into()),
            }
        }
        Input::TempCopy { file, copy } => {
            if tool == QualityTool::Phpstan {
                args.push(format!("--tmp-file={copy}"));
                args.push(format!("--instead-of={file}"));
            }
            args.push(file.clone());
        }
        Input::Files(files) => {
            if tool == QualityTool::PhpCsFixer {
                args.push("--".into());
            }
            args.extend(files.iter().cloned());
        }
    }
    ToolArgs { args, stdin }
}

/// The arguments that make a fixer fix `file` (absolute) with `text`, for its [`FixMode`].
pub fn fix_args(cfg: &ToolConfig, config: Option<&str>, file: &str, text: &str) -> ToolArgs {
    match cfg.tool {
        QualityTool::Phpcs => {
            let mut args = vec!["-q".to_string()];
            args.extend(base(cfg, config));
            args.extend(cfg.extra_args());
            args.push(format!("--stdin-path={file}"));
            args.push("-".into());
            ToolArgs {
                args,
                stdin: text.to_string(),
            }
        }
        QualityTool::PhpCsFixer => check_args(
            cfg,
            config,
            &Input::Stdin {
                file: file.to_string(),
                text: text.to_string(),
            },
        ),
        QualityTool::Rector => check_args(cfg, config, &Input::Files(vec![file.to_string()])),
        _ => in_place_args(cfg, config, file),
    }
}

/// The arguments that fix the saved `file` in place (Actions on Save).
pub fn in_place_args(cfg: &ToolConfig, config: Option<&str>, file: &str) -> ToolArgs {
    let mut args: Vec<String> = match cfg.tool {
        QualityTool::PhpCsFixer => vec![
            "fix".into(),
            "--no-interaction".into(),
            "--show-progress=none".into(),
        ],
        QualityTool::Rector => vec!["process".into(), "--no-progress-bar".into()],
        QualityTool::Phpcs => vec!["-q".into()],
        _ => Vec::new(),
    };
    args.extend(base(cfg, config));
    args.extend(cfg.extra_args());
    if cfg.tool == QualityTool::PhpCsFixer {
        args.push("--".into());
    }
    args.push(file.to_string());
    ToolArgs {
        args,
        stdin: String::new(),
    }
}

/// The arguments that print the tool's version ("Validate" on the settings page).
pub fn version_args() -> Vec<String> {
    vec!["--version".into()]
}

/// The first line of `--version` output with a version number in it.
pub fn parse_version(out: &str) -> Option<String> {
    out.lines()
        .map(str::trim)
        .find(|l| l.chars().any(|c| c.is_ascii_digit()) && !l.starts_with("PHP Warning"))
        .map(|l| {
            // Strip ANSI colors.
            let mut s = String::new();
            let mut esc = false;
            for c in l.chars() {
                match (esc, c) {
                    (false, '\u{1b}') => esc = true,
                    (true, c) if c.is_ascii_alphabetic() => esc = false,
                    (true, _) => {}
                    (false, c) => s.push(c),
                }
            }
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(tool: QualityTool) -> ToolConfig {
        ToolConfig::new(tool)
    }

    #[test]
    fn phpstan_editor_mode_uses_a_temporary_copy() {
        let mut c = cfg(QualityTool::Phpstan);
        c.level = "6".into();
        c.options = "--memory-limit=1G".into();
        let a = check_args(
            &c,
            Some("/p/phpstan.neon"),
            &Input::TempCopy {
                file: "/p/app/A.php".into(),
                copy: "/tmp/forge-qt/A.php".into(),
            },
        );
        assert_eq!(
            a.args,
            vec![
                "analyse",
                "--error-format=json",
                "--no-progress",
                "--no-interaction",
                "--configuration=/p/phpstan.neon",
                "--level=6",
                "--memory-limit=1G",
                "--tmp-file=/tmp/forge-qt/A.php",
                "--instead-of=/p/app/A.php",
                "/p/app/A.php",
            ]
        );
    }

    #[test]
    fn phpcs_reads_the_editor_text_from_stdin() {
        let mut c = cfg(QualityTool::Phpcs);
        c.level = "PSR12".into();
        let input = Input::Stdin {
            file: "/p/a.php".into(),
            text: "<?php\n".into(),
        };
        let a = check_args(&c, None, &input);
        assert_eq!(
            a.args,
            vec![
                "--report=json",
                "-q",
                "--standard=PSR12",
                "--stdin-path=/p/a.php",
                "-"
            ]
        );
        assert_eq!(a.stdin, "<?php\n");
        // A configuration file is the standard.
        let a = check_args(&c, Some("/p/phpcs.xml"), &input);
        assert!(a.args.contains(&"--standard=/p/phpcs.xml".to_string()));
        assert!(!a.args.contains(&"--standard=PSR12".to_string()));
        let f = fix_args(&c, None, "/p/a.php", "<?php\n");
        assert_eq!(
            f.args,
            vec!["-q", "--standard=PSR12", "--stdin-path=/p/a.php", "-"]
        );
        assert!(input
            .aliases()
            .iter()
            .any(|(a, f)| a == "STDIN" && f == "/p/a.php"));
    }

    #[test]
    fn phpmd_takes_files_and_rulesets_as_arguments() {
        let c = cfg(QualityTool::Phpmd);
        let a = check_args(
            &c,
            None,
            &Input::Files(vec!["/p/a.php".into(), "/p/b.php".into()]),
        );
        assert_eq!(
            a.args,
            vec!["/p/a.php,/p/b.php", "json", DEFAULT_PHPMD_RULESETS]
        );
        let a = check_args(
            &c,
            Some("/p/phpmd.xml"),
            &Input::Files(vec!["/p/a.php".into()]),
        );
        assert_eq!(a.args[2], "/p/phpmd.xml");
    }

    #[test]
    fn fixers_check_with_a_dry_run_and_fix_in_place() {
        let mut c = cfg(QualityTool::PhpCsFixer);
        c.level = "@PSR12".into();
        let a = check_args(&c, None, &Input::Files(vec!["/p/a.php".into()]));
        assert_eq!(a.args[..3], ["fix", "--dry-run", "--diff"]);
        assert!(a.args.contains(&"--rules=@PSR12".to_string()));
        assert_eq!(a.args[a.args.len() - 2..], ["--", "/p/a.php"]);
        let p = in_place_args(&c, Some("/p/.php-cs-fixer.php"), "/p/a.php");
        assert_eq!(
            p.args,
            vec![
                "fix",
                "--no-interaction",
                "--show-progress=none",
                "--config=/p/.php-cs-fixer.php",
                "--rules=@PSR12",
                "--",
                "/p/a.php"
            ]
        );
        let pint = check_args(
            &cfg(QualityTool::Pint),
            None,
            &Input::Files(vec!["/p/a.php".into()]),
        );
        assert_eq!(pint.args, vec!["--test", "--format=json", "/p/a.php"]);
        let rector = fix_args(
            &cfg(QualityTool::Rector),
            Some("/p/rector.php"),
            "/p/a.php",
            "",
        );
        assert_eq!(
            rector.args,
            vec![
                "process",
                "--dry-run",
                "--output-format=json",
                "--no-progress-bar",
                "--config=/p/rector.php",
                "/p/a.php"
            ]
        );
        assert_eq!(fix_mode(QualityTool::Pint), Some(FixMode::InPlace));
        assert_eq!(fix_mode(QualityTool::Phpstan), None);
    }

    #[test]
    fn reads_versions() {
        assert_eq!(
            parse_version("PHPStan - PHP Static Analysis Tool 1.11.8\n").as_deref(),
            Some("PHPStan - PHP Static Analysis Tool 1.11.8")
        );
        assert_eq!(
            parse_version("\n\u{1b}[32mPint\u{1b}[39m 1.17.2\n").as_deref(),
            Some("Pint 1.17.2")
        );
        assert_eq!(parse_version("no digits here"), None);
    }
}
