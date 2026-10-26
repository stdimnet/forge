//! cargo's JSON messages (`--message-format=json`, one object per line):
//!
//! - `compiler-artifact`: the executables a build produced ([`artifacts`]); the debugger
//!   picks the one of the configuration's target ([`pick_executable`]),
//! - `compiler-message`: rustc and Clippy diagnostics as problems ([`problems`]), with the
//!   suggestions of their `help` children as quick fixes (the replacements rustc marks
//!   with an applicability),
//! - `build-finished`: whether the build succeeded.
//!
//! File names in messages are relative to the workspace root, where cargo runs rustc;
//! [`problems`] makes them project relative. Positions are 0-based lines and character
//! columns (rustc counts characters).

use serde::{Deserialize, Serialize};

use crate::metadata::TargetKind;

#[derive(Deserialize)]
struct RawMessage {
    reason: String,
    #[serde(default)]
    package_id: String,
    #[serde(default)]
    target: Option<RawTarget>,
    #[serde(default)]
    profile: Option<RawProfile>,
    #[serde(default)]
    executable: Option<String>,
    #[serde(default)]
    message: Option<Diagnostic>,
    #[serde(default)]
    success: Option<bool>,
}

#[derive(Deserialize)]
struct RawTarget {
    name: String,
    kind: Vec<String>,
}

#[derive(Deserialize)]
struct RawProfile {
    #[serde(default)]
    test: bool,
}

#[derive(Deserialize)]
struct Diagnostic {
    message: String,
    #[serde(default)]
    code: Option<DiagnosticCode>,
    level: String,
    #[serde(default)]
    spans: Vec<Span>,
    #[serde(default)]
    children: Vec<Diagnostic>,
    #[serde(default)]
    rendered: Option<String>,
}

#[derive(Deserialize)]
struct DiagnosticCode {
    code: String,
}

#[derive(Deserialize)]
struct Span {
    file_name: String,
    line_start: u32,
    line_end: u32,
    column_start: u32,
    column_end: u32,
    #[serde(default)]
    is_primary: bool,
    #[serde(default)]
    suggested_replacement: Option<String>,
    #[serde(default)]
    suggestion_applicability: Option<String>,
}

/// An executable cargo built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    /// `path+file:///w/shop/crates/shop-core#0.3.0`.
    pub package_id: String,
    pub target: String,
    pub kind: TargetKind,
    /// Built as a test harness (`cargo test --no-run`).
    pub test: bool,
    pub executable: String,
}

/// The executables among cargo's messages.
pub fn artifacts(json_lines: &str) -> Vec<Artifact> {
    json_lines
        .lines()
        .filter(|l| l.starts_with('{'))
        .filter_map(|l| serde_json::from_str::<RawMessage>(l).ok())
        .filter(|m| m.reason == "compiler-artifact")
        .filter_map(|m| {
            let t = m.target?;
            Some(Artifact {
                package_id: m.package_id,
                kind: TargetKind::from_kinds(&t.kind),
                target: t.name,
                test: m.profile.map(|p| p.test).unwrap_or(false),
                executable: m.executable?,
            })
        })
        .collect()
}

/// The package name of a package id (`path+file:///w/shop-core#0.3.0`,
/// `shop-core 0.3.0 (path+file:///…)`, `registry+…#serde@1.0.0`).
pub fn package_name(id: &str) -> String {
    if let Some((name, _)) = id.split_once(' ') {
        return name.to_string();
    }
    let frag = id.rsplit_once('#');
    match frag {
        Some((_, f)) if f.contains('@') => f.split('@').next().unwrap_or(f).to_string(),
        Some((url, _)) => url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(url)
            .to_string(),
        None => id.to_string(),
    }
}

/// The executable of `package`'s target (`kind` / `name`, a library's test harness for
/// `Lib`); with only a package, its single executable.
pub fn pick_executable<'a>(
    found: &'a [Artifact],
    package: &str,
    kind: Option<TargetKind>,
    name: &str,
) -> Result<&'a Artifact, String> {
    let of_package: Vec<&Artifact> = found
        .iter()
        .filter(|a| package.is_empty() || package_name(&a.package_id) == package)
        .collect();
    let matching: Vec<&Artifact> = match kind {
        Some(TargetKind::Lib | TargetKind::ProcMacro) => of_package
            .into_iter()
            .filter(|a| matches!(a.kind, TargetKind::Lib | TargetKind::ProcMacro))
            .collect(),
        Some(k) => of_package
            .into_iter()
            .filter(|a| a.kind == k && (name.is_empty() || a.target == name))
            .collect(),
        None => of_package,
    };
    match matching.as_slice() {
        [one] => Ok(one),
        [] => Err("cargo built no executable for the configuration".into()),
        many => Err(format!(
            "Choose the target to debug: {}",
            many.iter()
                .map(|a| format!("{} ({})", a.target, a.kind.id()))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Errors of a failed build (the rendered compiler errors), or `None` when it succeeded.
pub fn build_failure(json_lines: &str) -> Option<String> {
    let mut errors = Vec::new();
    let mut success = None;
    for m in json_lines
        .lines()
        .filter(|l| l.starts_with('{'))
        .filter_map(|l| serde_json::from_str::<RawMessage>(l).ok())
    {
        match m.reason.as_str() {
            "build-finished" => success = m.success,
            "compiler-message" => {
                if let Some(d) = m.message.filter(|d| d.level == "error") {
                    errors.push(d.rendered.unwrap_or(d.message));
                }
            }
            _ => {}
        }
    }
    match success {
        Some(true) => None,
        _ if errors.is_empty() && success.is_none() => None,
        _ => Some(if errors.is_empty() {
            "The build failed".into()
        } else {
            errors.join("\n")
        }),
    }
}

/// One replacement of a quick fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustEdit {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustFix {
    pub title: String,
    /// `MachineApplicable`, `MaybeIncorrect`, `HasPlaceholders`, `Unspecified`.
    pub applicability: String,
    pub edits: Vec<RustEdit>,
}

/// A diagnostic of rustc or Clippy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustProblem {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    /// `error`, `warning`, `information` or `hint`.
    pub severity: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// `clippy` or `rustc`.
    pub source: String,
    /// rustc's text with the code excerpt, for the Problems tool window's details.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rendered: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<RustFix>,
}

fn severity(level: &str) -> Option<&'static str> {
    Some(match level {
        "error" | "error: internal compiler error" => "error",
        "warning" => "warning",
        "note" => "information",
        "help" => "hint",
        _ => return None,
    })
}

/// Problems of `cargo check` / `cargo clippy --message-format=json`. `workspace` is the
/// workspace root relative to the project (`""`: the project root); files outside the
/// project (dependencies, the standard library) are skipped.
pub fn problems(json_lines: &str, workspace: &str) -> Vec<RustProblem> {
    let to_project = |file: &str| -> Option<String> {
        let f = file.replace('\\', "/");
        if f.starts_with('/') || f.contains(":/") || f.starts_with("..") {
            return None;
        }
        Some(if workspace.is_empty() {
            f
        } else {
            format!("{}/{}", workspace.trim_end_matches('/'), f)
        })
    };
    let mut out: Vec<RustProblem> = Vec::new();
    for m in json_lines
        .lines()
        .filter(|l| l.starts_with('{'))
        .filter_map(|l| serde_json::from_str::<RawMessage>(l).ok())
        .filter(|m| m.reason == "compiler-message")
    {
        let Some(d) = m.message else { continue };
        let Some(sev) = severity(&d.level) else {
            continue;
        };
        let Some(span) = d.spans.iter().find(|s| s.is_primary).or(d.spans.first()) else {
            continue;
        };
        let Some(path) = to_project(&span.file_name) else {
            continue;
        };
        let code = d.code.as_ref().map(|c| c.code.clone());
        let fixes: Vec<RustFix> = d
            .children
            .iter()
            .filter_map(|c| {
                let edits: Vec<RustEdit> = c
                    .spans
                    .iter()
                    .filter_map(|s| {
                        let text = s.suggested_replacement.clone()?;
                        Some(RustEdit {
                            path: to_project(&s.file_name)?,
                            line: s.line_start.saturating_sub(1),
                            column: s.column_start.saturating_sub(1),
                            end_line: s.line_end.saturating_sub(1),
                            end_column: s.column_end.saturating_sub(1),
                            text,
                        })
                    })
                    .collect();
                if edits.is_empty() {
                    return None;
                }
                let applicability = c
                    .spans
                    .iter()
                    .find_map(|s| s.suggestion_applicability.clone())
                    .unwrap_or_else(|| "Unspecified".into());
                let mut title = c.message.clone();
                if let [one] = edits.as_slice() {
                    if !one.text.is_empty() && one.text.len() <= 40 && !one.text.contains('\n') {
                        title = format!("{title}: `{}`", one.text);
                    }
                }
                let mut chars = title.chars();
                let title = match chars.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                    None => title,
                };
                Some(RustFix {
                    title,
                    applicability,
                    edits,
                })
            })
            .collect();
        let problem = RustProblem {
            path,
            line: span.line_start.saturating_sub(1),
            column: span.column_start.saturating_sub(1),
            end_line: span.line_end.saturating_sub(1),
            end_column: span.column_end.saturating_sub(1),
            severity: sev.into(),
            message: d.message.clone(),
            source: if code.as_deref().is_some_and(|c| c.starts_with("clippy::")) {
                "clippy".into()
            } else {
                "rustc".into()
            },
            code,
            rendered: d
                .rendered
                .clone()
                .unwrap_or_default()
                .trim_end()
                .to_string(),
            fixes,
        };
        // The same diagnostic arrives once per target that compiles the file.
        if !out.iter().any(|p| {
            p.path == problem.path
                && p.line == problem.line
                && p.column == problem.column
                && p.message == problem.message
        }) {
            out.push(problem);
        }
    }
    out
}

/// A character column of a line as UTF-16 code units (LSP positions), given the line.
pub fn utf16_column(line_text: &str, column: u32) -> u32 {
    line_text
        .chars()
        .take(column as usize)
        .map(|c| c.len_utf16() as u32)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clippy_problems_with_fixes() {
        let p = problems(include_str!("../tests/fixtures/clippy.jsonl"), "");
        assert_eq!(p.len(), 2, "{p:#?}");
        let ret = &p[0];
        assert_eq!(ret.path, "crates/shop-core/src/cart.rs");
        assert_eq!((ret.line, ret.column, ret.end_column), (49, 8, 36));
        assert_eq!(ret.code.as_deref(), Some("clippy::needless_return"));
        assert_eq!(
            (ret.severity.as_str(), ret.source.as_str()),
            ("warning", "clippy")
        );
        assert!(ret
            .rendered
            .contains("--> crates/shop-core/src/cart.rs:50:9"));
        assert_eq!(ret.fixes.len(), 1);
        assert_eq!(ret.fixes[0].title, "Remove `return`");
        assert_eq!(ret.fixes[0].applicability, "MachineApplicable");
        assert_eq!(ret.fixes[0].edits.len(), 2);
        assert_eq!(ret.fixes[0].edits[0].text, "self.items.len() == 0");
        let cmp = &p[1];
        assert_eq!(cmp.path, "crates/shop-core/src/money.rs");
        assert_eq!(cmp.fixes[0].title, "Try: `cents == 0`");
        let edit = &cmp.fixes[0].edits[0];
        assert_eq!((edit.line, edit.column), (13, 7));
        // In a workspace below the project root.
        assert_eq!(
            problems(include_str!("../tests/fixtures/clippy.jsonl"), "rust")[0].path,
            "rust/crates/shop-core/src/cart.rs"
        );
    }

    #[test]
    fn errors_and_foreign_files() {
        let line = |file: &str, level: &str| {
            format!(
                r#"{{"reason":"compiler-message","package_id":"x","message":{{"message":"mismatched types","code":{{"code":"E0308","explanation":null}},"level":"{level}","spans":[{{"file_name":"{file}","line_start":3,"line_end":3,"column_start":5,"column_end":9,"is_primary":true}}],"children":[],"rendered":"error[E0308]: mismatched types\n"}}}}"#
            )
        };
        let text = [
            line("src/lib.rs", "error"),
            line("/home/anna/.cargo/registry/src/x/lib.rs", "error"),
            line("src/lib.rs", "failure-note"),
            r#"{"reason":"build-finished","success":false}"#.to_string(),
        ]
        .join("\n");
        let p = problems(&text, "");
        assert_eq!(p.len(), 1);
        assert_eq!(
            (p[0].severity.as_str(), p[0].source.as_str()),
            ("error", "rustc")
        );
        assert_eq!(
            build_failure(&text).as_deref(),
            Some("error[E0308]: mismatched types\n\nerror[E0308]: mismatched types\n")
        );
        assert_eq!(
            build_failure(r#"{"reason":"build-finished","success":true}"#),
            None
        );
        assert_eq!(utf16_column("let ü = '😀';", 10), 11);
    }

    #[test]
    fn executables_of_test_builds() {
        let found = artifacts(include_str!("../tests/fixtures/test-no-run.jsonl"));
        let names: Vec<(&str, TargetKind, bool)> = found
            .iter()
            .map(|a| (a.target.as_str(), a.kind, a.test))
            .collect();
        assert!(
            names.contains(&("shop_core", TargetKind::Lib, true)),
            "{names:?}"
        );
        assert!(names.contains(&("checkout", TargetKind::Test, true)));
        let lib = pick_executable(&found, "shop-core", Some(TargetKind::Lib), "").unwrap();
        assert!(lib
            .executable
            .ends_with("target/debug/deps/shop_core-49a7891832f2bb85"));
        let it = pick_executable(&found, "shop-core", Some(TargetKind::Test), "checkout").unwrap();
        assert!(it.executable.contains("checkout-"));
        assert!(pick_executable(&found, "shop-core", None, "").is_err());
        let built = artifacts(include_str!("../tests/fixtures/build.jsonl"));
        let shop = pick_executable(&built, "shop-cli", Some(TargetKind::Bin), "shop").unwrap();
        assert_eq!(
            shop.executable,
            "/home/anna/Projects/shop/target/debug/shop"
        );
        assert_eq!(
            package_name("path+file:///w/shop/crates/shop-core#0.3.0"),
            "shop-core"
        );
        assert_eq!(
            package_name("path+file:///w/shop/crates/core#shop-core@0.3.0"),
            "shop-core"
        );
        assert_eq!(
            package_name("shop-core 0.3.0 (path+file:///w)"),
            "shop-core"
        );
    }
}
