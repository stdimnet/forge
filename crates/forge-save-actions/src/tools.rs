//! The external tools of Actions on Save. Each one is looked up in the project (or at the
//! path from its setting) and run on the saved file where the project lives, through the
//! project's [`Backend`]:
//!
//! | Tool | Found at | Command |
//! | --- | --- | --- |
//! | ESLint | `node_modules/.bin/eslint` | `eslint --fix <file>` |
//! | Prettier | `node_modules/.bin/prettier` | `prettier --write <file>` |
//! | PHP CS Fixer | `vendor/bin/php-cs-fixer` | `php-cs-fixer fix <file>` |
//! | Laravel Pint | `vendor/bin/pint` | `pint <file>` |
//!
//! A missing tool is skipped with a message. When a tool fails, the file gets back the text
//! it had before that tool ran, so a failing tool never loses the user's text.

use std::path::PathBuf;
use std::time::Duration;

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::Result;
use forge_process::CommandLine;
use serde::{Deserialize, Serialize};

use crate::Tool;

/// A tool that does not finish within this time is stopped.
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(60);

impl Tool {
    /// Where the tool lives in a project (relative to the root), first match wins.
    pub fn candidates(self) -> &'static [&'static str] {
        match self {
            Tool::Eslint => &["node_modules/.bin/eslint", "node_modules/.bin/eslint.cmd"],
            Tool::Prettier => &[
                "node_modules/.bin/prettier",
                "node_modules/.bin/prettier.cmd",
            ],
            Tool::PhpCsFixer => &["vendor/bin/php-cs-fixer", "vendor/bin/php-cs-fixer.bat"],
            Tool::Pint => &["vendor/bin/pint", "vendor/bin/pint.bat"],
        }
    }

    fn args(self, file: &str) -> Vec<String> {
        let file = file.to_string();
        match self {
            Tool::Eslint => vec!["--fix".into(), file],
            Tool::Prettier => vec!["--write".into(), file],
            Tool::PhpCsFixer => vec![
                "fix".into(),
                "--no-interaction".into(),
                "--show-progress=none".into(),
                file,
            ],
            Tool::Pint => vec![file],
        }
    }

    /// Exit codes after which the file holds the tool's result.
    fn succeeded(self, code: i32) -> bool {
        match self {
            // 1: problems that `--fix` cannot fix remain; the fixes were written.
            Tool::Eslint => code == 0 || code == 1,
            _ => code == 0,
        }
    }
}

/// Find `tool`: the custom path (absolute or relative to the project root) when set,
/// otherwise the project's own installation.
pub fn locate(backend: &dyn Backend, tool: Tool, custom: Option<&str>) -> Result<Option<PathBuf>> {
    let root = backend.info().root.clone();
    if let Some(custom) = custom.map(str::trim).filter(|c| !c.is_empty()) {
        let path = if custom.starts_with('/') || custom.contains(":\\") || custom.contains(":/") {
            PathBuf::from(custom)
        } else {
            root.join(custom)
        };
        return backend.find_executable(&[], Some(&path.to_string_lossy()));
    }
    for rel in tool.candidates() {
        let path = root.join(rel);
        if let Some(found) = backend.find_executable(&[], Some(&path.to_string_lossy()))? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// How a tool run went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolStatus {
    Ok,
    /// Not installed in the project: skipped.
    Missing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutcome {
    pub tool: Tool,
    pub label: String,
    pub status: ToolStatus,
    /// What the tool reported (errors, remaining problems) or why it was skipped.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsReport {
    /// The file on disk differs from before the tools ran; the editor reloads it.
    pub changed: bool,
    pub outcomes: Vec<ToolOutcome>,
}

/// The first lines of a tool's output, for a balloon.
fn summary(stdout: &str, stderr: &str) -> String {
    let text = if stderr.trim().is_empty() { stdout } else { stderr };
    let lines: Vec<&str> = text.trim().lines().take(12).collect();
    lines.join("\n")
}

/// Run `tools` one after the other on the project file `path` (written already).
pub fn run_tools(
    backend: &dyn Backend,
    path: &str,
    tools: &[Tool],
    custom: &dyn Fn(Tool) -> Option<String>,
    timeout: Duration,
) -> Result<ToolsReport> {
    let initial = backend.read_bytes(path)?;
    let file = backend.resolve(path)?.to_string_lossy().into_owned();
    let mut outcomes = Vec::new();
    for &tool in tools {
        let outcome = |status, message: String| ToolOutcome {
            tool,
            label: tool.label().to_string(),
            status,
            message,
        };
        let Some(program) = locate(backend, tool, custom(tool).as_deref())? else {
            outcomes.push(outcome(
                ToolStatus::Missing,
                format!(
                    "{} is not installed in the project ({}); skipped.",
                    tool.label(),
                    tool.candidates()[0]
                ),
            ));
            continue;
        };
        let before = backend.read_bytes(path)?;
        let mut cmd = CommandLine::new(program.to_string_lossy());
        for a in tool.args(&file) {
            cmd = cmd.arg(a);
        }
        cmd.cwd = backend.info().root.clone();
        let result = capture(backend, &cmd, "", timeout);
        let failure = match &result {
            Ok(out) => match out.code {
                Some(code) if tool.succeeded(code) => None,
                Some(code) => Some(match summary(&out.stdout, &out.stderr) {
                    s if s.is_empty() => format!("{} exited with code {code}", tool.label()),
                    s => s,
                }),
                None => Some(format!("{} did not finish", tool.label())),
            },
            Err(e) => Some(e.to_string()),
        };
        match failure {
            Some(message) => {
                // Never leave a half-written file behind.
                if backend.read_bytes(path).ok().as_deref() != Some(before.as_slice()) {
                    backend.write_bytes(path, &before)?;
                }
                outcomes.push(outcome(ToolStatus::Failed, message));
            }
            None => {
                let out = result.expect("checked above");
                let message = if out.code == Some(1) {
                    summary(&out.stdout, &out.stderr)
                } else {
                    String::new()
                };
                outcomes.push(outcome(ToolStatus::Ok, message));
            }
        }
    }
    let changed = backend.read_bytes(path)? != initial;
    Ok(ToolsReport { changed, outcomes })
}

/// Where each tool was found, for the Actions on Save page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLocation {
    pub tool: Tool,
    pub label: String,
    /// Path of the tool, `None` when it is not installed.
    pub path: Option<String>,
}

pub fn detect_tools(
    backend: &dyn Backend,
    custom: &dyn Fn(Tool) -> Option<String>,
) -> Vec<ToolLocation> {
    let root = backend.info().root.clone();
    Tool::ALL
        .iter()
        .map(|&tool| {
            let path = locate(backend, tool, custom(tool).as_deref())
                .ok()
                .flatten()
                .map(|p| match p.strip_prefix(&root) {
                    Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
                    Err(_) => p.to_string_lossy().into_owned(),
                });
            ToolLocation {
                tool,
                label: tool.label().to_string(),
                path,
            }
        })
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use forge_backend::LocalBackend;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &std::path::Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn none(_: Tool) -> Option<String> {
        None
    }

    #[test]
    fn runs_project_tools_in_order() {
        let dir = tempfile::tempdir().unwrap();
        // php-cs-fixer fix --no-interaction --show-progress=none <file>
        script(
            dir.path(),
            "vendor/bin/php-cs-fixer",
            "[ \"$1\" = fix ] || exit 9\nsed -i 's/array()/[]/' \"$4\"",
        );
        script(dir.path(), "vendor/bin/pint", "sed -i 's/  / /g' \"$1\"");
        std::fs::write(dir.path().join("a.php"), "<?php $a  = array();\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let report = run_tools(
            &backend,
            "a.php",
            &[Tool::PhpCsFixer, Tool::Pint],
            &none,
            TOOL_TIMEOUT,
        )
        .unwrap();
        assert!(report.changed);
        assert!(report.outcomes.iter().all(|o| o.status == ToolStatus::Ok));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.php")).unwrap(),
            "<?php $a = [];\n"
        );
    }

    #[test]
    fn missing_tools_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.js"), "x\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let report = run_tools(&backend, "a.js", &[Tool::Prettier], &none, TOOL_TIMEOUT).unwrap();
        assert!(!report.changed);
        assert_eq!(report.outcomes[0].status, ToolStatus::Missing);
        assert!(report.outcomes[0].message.contains("node_modules/.bin/prettier"));
    }

    #[test]
    fn a_failing_tool_restores_the_file() {
        let dir = tempfile::tempdir().unwrap();
        script(
            dir.path(),
            "node_modules/.bin/prettier",
            "echo broken > \"$2\"\necho 'SyntaxError: Unexpected token' >&2\nexit 2",
        );
        std::fs::write(dir.path().join("a.js"), "let a = (\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let report = run_tools(&backend, "a.js", &[Tool::Prettier], &none, TOOL_TIMEOUT).unwrap();
        assert!(!report.changed);
        assert_eq!(report.outcomes[0].status, ToolStatus::Failed);
        assert_eq!(report.outcomes[0].message, "SyntaxError: Unexpected token");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.js")).unwrap(),
            "let a = (\n"
        );
    }

    #[test]
    fn eslint_problems_left_after_fixing_are_reported_but_kept() {
        let dir = tempfile::tempdir().unwrap();
        script(
            dir.path(),
            "node_modules/.bin/eslint",
            "[ \"$1\" = --fix ] || exit 9\nsed -i 's/var /let /' \"$2\"\necho '1:5 error no-undef'\nexit 1",
        );
        std::fs::write(dir.path().join("a.js"), "var a = b;\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let report = run_tools(&backend, "a.js", &[Tool::Eslint], &none, TOOL_TIMEOUT).unwrap();
        assert!(report.changed);
        assert_eq!(report.outcomes[0].status, ToolStatus::Ok);
        assert_eq!(report.outcomes[0].message, "1:5 error no-undef");
    }

    #[test]
    fn custom_paths_win_and_detection_reports_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        script(dir.path(), "tools/pint", "exit 0");
        script(dir.path(), "vendor/bin/php-cs-fixer", "exit 0");
        let backend = LocalBackend::open(dir.path()).unwrap();
        let custom = |t: Tool| (t == Tool::Pint).then(|| "tools/pint".to_string());
        let found = detect_tools(&backend, &custom);
        let path = |t: Tool| found.iter().find(|l| l.tool == t).unwrap().path.clone();
        assert_eq!(path(Tool::Pint).as_deref(), Some("tools/pint"));
        assert_eq!(path(Tool::PhpCsFixer).as_deref(), Some("vendor/bin/php-cs-fixer"));
        assert_eq!(path(Tool::Eslint), None);
    }
}
