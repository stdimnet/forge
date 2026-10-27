//! Problems the tools report, with the files mapped back into the project.

use serde::{Deserialize, Serialize};

use crate::model::WebTool;

/// The severities of the editor and the Problems tool window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    Weak,
    Info,
}

/// One problem of a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub tool: WebTool,
    /// Relative to the project root (absolute when the file is outside of it).
    pub file: String,
    /// 0-based.
    pub line: u32,
    /// 0-based, in characters.
    pub column: u32,
    /// Where the problem ends (0-based, exclusive column); `None`: one character.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
    pub severity: Severity,
    pub message: String,
    /// The rule (`no-unused-vars`, `color-no-invalid-hex`, `lint/style/useConst`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// The tool can fix it ("Fix ESLint problems").
    pub fixable: bool,
    /// Documentation of the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// Where the tool ran, to turn the paths it reports into project paths.
#[derive(Debug, Clone, Default)]
pub struct PathContext {
    /// The project root (absolute).
    pub root: String,
    /// The tool's working directory (absolute); relative paths in reports start there.
    pub cwd: String,
    /// The project file whose text went through stdin (names like `<input>` stand for it).
    pub stdin_file: Option<String>,
}

fn normalize(p: &str) -> String {
    let p = p.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    let absolute = p.starts_with('/');
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

impl PathContext {
    pub fn new(root: &str, cwd: &str) -> Self {
        Self {
            root: normalize(root),
            cwd: normalize(cwd),
            stdin_file: None,
        }
    }

    pub fn with_stdin(mut self, file: &str) -> Self {
        self.stdin_file = Some(file.to_string());
        self
    }

    /// A reported file as a project path.
    pub fn localize(&self, reported: &str) -> String {
        let reported = reported.trim();
        if let Some(stdin) = &self.stdin_file {
            if matches!(reported, "" | "-" | "<input>" | "<text>" | "<stdin>" | "stdin") {
                return stdin.clone();
            }
        }
        let is_abs = reported.starts_with('/')
            || reported.replace('\\', "/").get(1..3).is_some_and(|s| s == ":/");
        let abs = if is_abs {
            normalize(reported)
        } else {
            normalize(&format!("{}/{reported}", self.cwd))
        };
        let root = self.root.trim_end_matches('/');
        match abs.strip_prefix(root) {
            Some(rest) if rest.starts_with('/') => rest[1..].to_string(),
            _ => abs,
        }
    }
}

/// 0-based line and character column of a byte `offset` in `text`.
pub fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() as u32;
    let start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = before[start..].chars().count() as u32;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_reported_paths_into_the_project() {
        let ctx = PathContext::new("/home/me/shop/", "/home/me/shop/packages/web")
            .with_stdin("packages/web/src/a.ts");
        assert_eq!(ctx.localize("/home/me/shop/src/App.tsx"), "src/App.tsx");
        assert_eq!(ctx.localize("src/App.tsx"), "packages/web/src/App.tsx");
        assert_eq!(ctx.localize("../api/x.ts"), "packages/api/x.ts");
        assert_eq!(ctx.localize("<input>"), "packages/web/src/a.ts");
        assert_eq!(ctx.localize("/usr/lib/x.js"), "/usr/lib/x.js");
    }

    #[test]
    fn offsets_become_lines_and_columns() {
        let t = "a\nbé c\n";
        assert_eq!(line_col(t, 0), (0, 0));
        assert_eq!(line_col(t, 2), (1, 0));
        // `é` is two bytes, one character.
        assert_eq!(line_col(t, 6), (1, 3));
        assert_eq!(line_col(t, 100), (2, 0));
    }
}
