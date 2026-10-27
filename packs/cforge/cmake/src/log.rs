//! CMake's configure output, as the CMake tool window shows it: `CMake Error at
//! CMakeLists.txt:6 (find_package):` blocks become messages linked to the file and line,
//! with their indented explanation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    /// `CMake Deprecation Warning` and `CMake Warning (dev)`.
    Deprecation,
}

/// One message of a configure run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureMessage {
    pub severity: Severity,
    /// The CMake file, as CMake printed it (relative to the project or absolute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// `find_package`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// The explanation, unindented, paragraphs kept.
    pub message: String,
    /// 0-based line of the header in the log.
    pub log_line: u32,
}

impl ConfigureMessage {
    /// The first line of the message (the Problems view's text).
    pub fn summary(&self) -> &str {
        self.message
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
    }
}

/// `Error at CMakeLists.txt:6 (find_package)`, `Warning (dev) in CMakeLists.txt`, `Error`.
fn header(line: &str) -> Option<(Severity, Option<String>, Option<u32>, Option<String>)> {
    let rest = line.strip_prefix("CMake ")?;
    let (kind, rest) = [
        ("Deprecation Warning", Severity::Deprecation),
        ("Deprecation Error", Severity::Error),
        ("Warning (dev)", Severity::Deprecation),
        ("Warning", Severity::Warning),
        ("Error", Severity::Error),
    ]
    .iter()
    .find_map(|(k, sev)| rest.strip_prefix(k).map(|r| (*sev, r)))?;
    let rest = rest.trim_end();
    let rest = rest.strip_suffix(':')?;
    if rest.is_empty() {
        return Some((kind, None, None, None));
    }
    if let Some(r) = rest.strip_prefix(" at ") {
        // `file:line (command)`.
        let (place, command) = match r.rsplit_once(" (") {
            Some((p, c)) => (p, c.strip_suffix(')').map(str::to_string)),
            None => (r, None),
        };
        let (file, line) = match place.rsplit_once(':') {
            Some((f, l)) if l.chars().all(|c| c.is_ascii_digit()) && !l.is_empty() => {
                (f.to_string(), l.parse().ok())
            }
            _ => (place.to_string(), None),
        };
        return Some((kind, Some(file), line, command));
    }
    if let Some(r) = rest.strip_prefix(" in ") {
        return Some((kind, Some(r.to_string()), None, None));
    }
    // `CMake Error: The source directory "…" does not exist.` has no colon at the end and
    // is handled by `inline`.
    None
}

/// `CMake Error: <text>` on one line.
fn inline(line: &str) -> Option<(Severity, String)> {
    let rest = line.strip_prefix("CMake ")?;
    for (k, sev) in [
        ("Error: ", Severity::Error),
        ("Warning: ", Severity::Warning),
    ] {
        if let Some(text) = rest.strip_prefix(k) {
            return Some((sev, text.trim().to_string()));
        }
    }
    None
}

/// The messages of a configure log.
pub fn parse_log(text: &str) -> Vec<ConfigureMessage> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some((severity, file, line_no, command)) = header(line) {
            let start = i;
            i += 1;
            let mut body: Vec<&str> = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if l.trim().is_empty() || l.starts_with(' ') || l.starts_with('\t') {
                    body.push(l);
                    i += 1;
                } else {
                    break;
                }
            }
            while body.last().is_some_and(|l| l.trim().is_empty()) {
                body.pop();
            }
            let message = body
                .iter()
                .map(|l| l.strip_prefix("  ").unwrap_or(l.trim_start()))
                .collect::<Vec<_>>()
                .join("\n");
            out.push(ConfigureMessage {
                severity,
                file,
                line: line_no,
                command,
                message,
                log_line: start as u32,
            });
            continue;
        }
        if let Some((severity, message)) = inline(line) {
            out.push(ConfigureMessage {
                severity,
                file: None,
                line: None,
                command: None,
                message,
                log_line: i as u32,
            });
        }
        i += 1;
    }
    out
}

/// Whether the configure run failed according to its last lines.
pub fn failed(text: &str) -> bool {
    text.contains("-- Configuring incomplete, errors occurred!")
        || text.contains("CMake Generate step failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn find_package_error() {
        let log = fixture("cmake-configure-error.txt");
        let m = parse_log(&log);
        assert_eq!(m.len(), 1);
        let e = &m[0];
        assert_eq!(e.severity, Severity::Error);
        assert_eq!(e.file.as_deref(), Some("CMakeLists.txt"));
        assert_eq!(e.line, Some(6));
        assert_eq!(e.command.as_deref(), Some("find_package"));
        assert_eq!(
            e.summary(),
            "By not providing \"FindNotThere.cmake\" in CMAKE_MODULE_PATH this project has"
        );
        assert!(e.message.contains("\n  NotThereConfig.cmake\n"));
        assert_eq!(e.log_line, 6);
        assert!(failed(&log));
    }

    #[test]
    fn warnings_and_generate_errors() {
        let log = fixture("cmake-configure-warning.txt");
        let m = parse_log(&log);
        let summary: Vec<(Severity, Option<u32>, &str)> = m
            .iter()
            .map(|m| (m.severity, m.line, m.summary()))
            .collect();
        assert_eq!(
            summary,
            [
                (Severity::Warning, Some(3), "Old option used"),
                (Severity::Deprecation, Some(5), "x"),
                (Severity::Error, Some(4), "Cannot find source file:"),
                (Severity::Error, Some(4), "No SOURCES given to target: app"),
            ]
        );
        assert_eq!(m[2].message, "Cannot find source file:\n\n  missing.cpp");
        assert!(failed(&log));
    }

    #[test]
    fn other_headers() {
        let m = parse_log(
            "CMake Error in src/CMakeLists.txt:\n  Target \"app\" links to a missing library.\n\nCMake Error: The source directory \"/x\" does not exist.\nCMake Warning (dev) at cmake/deps.cmake:12 (cmake_policy):\n  Policy CMP0135 is not set.\n",
        );
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].file.as_deref(), Some("src/CMakeLists.txt"));
        assert_eq!(m[0].line, None);
        assert_eq!(m[1].message, "The source directory \"/x\" does not exist.");
        assert_eq!(m[2].severity, Severity::Deprecation);
        assert_eq!(m[2].file.as_deref(), Some("cmake/deps.cmake"));
        assert!(!failed("-- Configuring done (0.3s)\n-- Generating done"));
    }
}
