//! The result of a run: the Inspection Results tree (grouped by severity, inspection group,
//! inspection, directory or file) and its export as HTML, XML (one file, or one file per
//! inspection like PHPStorm's command-line inspector) and JSON.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::duplicates::DuplicateGroup;
use crate::problem::{Fix, Location};
use crate::severity::{self, EditorSeverity};

/// A reported problem with everything the results view and the exports need.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportProblem {
    pub inspection: String,
    pub inspection_name: String,
    /// The inspection's group path (`PHP/Data flow`).
    pub group: String,
    pub path: String,
    /// 0-based line, UTF-16 columns.
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    /// Bytes (for fixes and suppressions).
    pub start: usize,
    pub end: usize,
    pub message: String,
    /// The level (`WEAK WARNING`).
    pub severity: String,
    pub severity_name: String,
    pub editor: EditorSeverity,
    /// The line's text.
    pub snippet: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<Fix>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<Location>,
    /// The file's fingerprint when it was inspected.
    pub stamp: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unnecessary: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Report {
    pub problems: Vec<ReportProblem>,
    /// Files inspected.
    pub files: usize,
    pub profile: String,
    /// The scope's description (`Project Files`, `Directory 'src'`).
    pub scope: String,
    pub cancelled: bool,
    /// Providers that failed (an external tool that is not installed).
    pub errors: Vec<String>,
    /// Duplicate groups, for the Duplicates view.
    pub duplicates: Vec<DuplicateGroup>,
}

/// A level of the Inspection Results tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Grouping {
    Severity,
    /// The inspection's language / group (`PHP`, `PHP/Data flow`).
    Group,
    Inspection,
    Directory,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Severity,
    Group,
    Inspection,
    Directory,
    File,
    Problem,
}

/// A row of the Inspection Results tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultNode {
    /// Unique within the tree (for expansion state).
    pub key: String,
    pub kind: NodeKind,
    pub label: String,
    /// Problems below.
    pub count: usize,
    /// The most severe problem below.
    pub editor: EditorSeverity,
    /// Index into `Report::problems` for problem rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<usize>,
    /// A file or directory path, an inspection id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub children: Vec<ResultNode>,
}

fn editor_rank(e: EditorSeverity) -> u8 {
    match e {
        EditorSeverity::Error => 0,
        EditorSeverity::Warning => 1,
        EditorSeverity::Weak => 2,
        EditorSeverity::Typo => 3,
        EditorSeverity::Info => 4,
    }
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn level_key(p: &ReportProblem, g: Grouping) -> (String, String, String) {
    // (sort key, label, detail)
    match g {
        Grouping::Severity => (
            format!("{}{}", severity::rank(&p.severity), p.severity),
            p.severity_name.clone(),
            p.severity.clone(),
        ),
        Grouping::Group => {
            let label = p.group.replace('/', " / ");
            (
                format!(
                    "{}{}",
                    u8::from(p.group != "General"),
                    p.group.to_lowercase()
                ),
                label,
                p.group.clone(),
            )
        }
        Grouping::Inspection => (
            p.inspection_name.to_lowercase(),
            p.inspection_name.clone(),
            p.inspection.clone(),
        ),
        Grouping::Directory => {
            let d = dir_of(&p.path);
            (
                d.to_string(),
                if d.is_empty() {
                    "<project root>".into()
                } else {
                    d.to_string()
                },
                d.to_string(),
            )
        }
        Grouping::File => {
            let name = p.path.rsplit('/').next().unwrap_or(&p.path).to_string();
            (p.path.clone(), name, p.path.clone())
        }
    }
}

/// The Inspection Results tree of `report`, grouped by `levels` (problems are the leaves).
pub fn tree(report: &Report, levels: &[Grouping]) -> Vec<ResultNode> {
    fn build(report: &Report, idx: &[usize], levels: &[Grouping], prefix: &str) -> Vec<ResultNode> {
        let Some((&g, rest)) = levels.split_first() else {
            let mut idx = idx.to_vec();
            idx.sort_by_key(|&i| {
                let p = &report.problems[i];
                (p.path.clone(), p.line, p.column)
            });
            return idx
                .into_iter()
                .map(|i| {
                    let p = &report.problems[i];
                    ResultNode {
                        key: format!("{prefix}/p{i}"),
                        kind: NodeKind::Problem,
                        label: p.message.clone(),
                        count: 1,
                        editor: p.editor,
                        problem: Some(i),
                        detail: format!("{}:{}", p.path, p.line + 1),
                        children: Vec::new(),
                    }
                })
                .collect();
        };
        let mut buckets: BTreeMap<String, (String, String, Vec<usize>)> = BTreeMap::new();
        for &i in idx {
            let (sort, label, detail) = level_key(&report.problems[i], g);
            buckets
                .entry(sort)
                .or_insert_with(|| (label, detail, Vec::new()))
                .2
                .push(i);
        }
        let kind = match g {
            Grouping::Severity => NodeKind::Severity,
            Grouping::Group => NodeKind::Group,
            Grouping::Inspection => NodeKind::Inspection,
            Grouping::Directory => NodeKind::Directory,
            Grouping::File => NodeKind::File,
        };
        buckets
            .into_iter()
            .map(|(sort, (label, detail, list))| {
                let key = format!("{prefix}/{sort}");
                let editor = list
                    .iter()
                    .map(|&i| report.problems[i].editor)
                    .min_by_key(|e| editor_rank(*e))
                    .unwrap_or(EditorSeverity::Info);
                ResultNode {
                    children: build(report, &list, rest, &key),
                    key,
                    kind,
                    label,
                    count: list.len(),
                    editor,
                    problem: None,
                    detail,
                }
            })
            .collect()
    }
    let all: Vec<usize> = (0..report.problems.len()).collect();
    build(report, &all, levels, "")
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

fn attribute_key(level: &str) -> &'static str {
    match severity::normalize(level).as_str() {
        severity::ERROR => "ERRORS_ATTRIBUTES",
        severity::WEAK_WARNING => "INFO_ATTRIBUTES",
        severity::TYPO => "TYPO",
        severity::INFO => "INFORMATION_ATTRIBUTES",
        _ => "WARNING_ATTRIBUTES",
    }
}

fn xml_problem(p: &ReportProblem) -> String {
    format!(
        "  <problem>\n    <file>file://$PROJECT_DIR$/{path}</file>\n    <line>{line}</line>\n    <column>{col}</column>\n    <entry_point TYPE=\"file\" FQNAME=\"file://$PROJECT_DIR$/{path}\" />\n    <problem_class id=\"{id}\" severity=\"{sev}\" attribute_key=\"{attr}\">{name}</problem_class>\n    <description>{desc}</description>\n    <highlighted_element>{snippet}</highlighted_element>\n  </problem>\n",
        path = esc(&p.path),
        line = p.line + 1,
        col = p.column + 1,
        id = esc(&p.inspection),
        sev = esc(&p.severity),
        attr = attribute_key(&p.severity),
        name = esc(&p.inspection_name),
        desc = esc(&p.message),
        snippet = esc(&p.snippet),
    )
}

/// All problems in one PHPStorm-style XML document.
pub fn to_xml(report: &Report) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<problems is_local_tool=\"true\" profile=\"{}\" files=\"{}\">\n",
        esc(&report.profile),
        report.files
    );
    for p in &report.problems {
        out.push_str(&xml_problem(p));
    }
    out.push_str("</problems>\n");
    out
}

/// One XML file per inspection (`PhpUnusedAssignment.xml`), as PHPStorm's command-line
/// inspector writes them, plus `.descriptions.xml` listing the inspections.
pub fn to_xml_files(report: &Report) -> Vec<(String, String)> {
    let mut by: BTreeMap<&str, Vec<&ReportProblem>> = BTreeMap::new();
    for p in &report.problems {
        by.entry(&p.inspection).or_default().push(p);
    }
    let mut out = Vec::new();
    let mut descriptions = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<inspections profile=\"{}\">\n",
        esc(&report.profile)
    );
    for (id, list) in &by {
        let mut x = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<problems is_local_tool=\"true\">\n",
        );
        for p in list {
            x.push_str(&xml_problem(p));
        }
        x.push_str("</problems>\n");
        out.push((format!("{id}.xml"), x));
        let p = list[0];
        descriptions.push_str(&format!(
            "  <inspection shortName=\"{}\" displayName=\"{}\" group=\"{}\" problems=\"{}\" />\n",
            esc(id),
            esc(&p.inspection_name),
            esc(&p.group),
            list.len()
        ));
    }
    descriptions.push_str("</inspections>\n");
    out.push((".descriptions.xml".into(), descriptions));
    out
}

pub fn to_json(report: &Report) -> String {
    serde_json::to_string_pretty(report).unwrap_or_default()
}

/// A self-contained HTML page (Export to HTML).
pub fn to_html(report: &Report) -> String {
    let mut out = String::from(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Inspection Results</title>\n<style>\nbody{font:14px/1.5 system-ui,sans-serif;margin:24px;color:#222;background:#fff}\nh1{font-size:20px}h2{font-size:16px;margin-top:24px}h3{font-size:14px;margin:12px 0 4px}\ntable{border-collapse:collapse;width:100%}td{padding:2px 8px;vertical-align:top;border-bottom:1px solid #eee}\n.sev{white-space:nowrap;font-size:12px}.ERROR{color:#c62828}.WARNING{color:#b26a00}.loc{white-space:nowrap;font-family:monospace}\ncode{font-family:monospace;color:#555}\n@media (prefers-color-scheme: dark){body{background:#1e1f22;color:#ddd}td{border-color:#333}code{color:#aaa}}\n</style>\n</head>\n<body>\n",
    );
    out.push_str(&format!(
        "<h1>Inspection Results</h1>\n<p>Profile: {} &middot; Scope: {} &middot; {} problem{} in {} file{}</p>\n",
        esc(&report.profile),
        esc(if report.scope.is_empty() { "Project Files" } else { &report.scope }),
        report.problems.len(),
        if report.problems.len() == 1 { "" } else { "s" },
        report.files,
        if report.files == 1 { "" } else { "s" },
    ));
    for group in tree(report, &[Grouping::Group, Grouping::Inspection]) {
        out.push_str(&format!(
            "<h2>{} ({})</h2>\n",
            esc(&group.label),
            group.count
        ));
        for insp in &group.children {
            out.push_str(&format!(
                "<h3>{} ({})</h3>\n<table>\n",
                esc(&insp.label),
                insp.count
            ));
            for leaf in &insp.children {
                let Some(i) = leaf.problem else { continue };
                let p = &report.problems[i];
                out.push_str(&format!(
                    "<tr><td class=\"sev {cls}\">{sev}</td><td class=\"loc\">{path}:{line}</td><td>{msg}<br><code>{snip}</code></td></tr>\n",
                    cls = esc(&p.severity.replace(' ', "_")),
                    sev = esc(&p.severity_name),
                    path = esc(&p.path),
                    line = p.line + 1,
                    msg = esc(&p.message),
                    snip = esc(&p.snippet),
                ));
            }
            out.push_str("</table>\n");
        }
    }
    out.push_str("</body>\n</html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(id: &str, path: &str, line: u32, level: &str) -> ReportProblem {
        ReportProblem {
            inspection: id.into(),
            inspection_name: format!("{id} name"),
            group: if id.starts_with("Php") {
                "PHP/Data flow".into()
            } else {
                "General".into()
            },
            path: path.into(),
            line,
            column: 0,
            end_line: line,
            end_column: 3,
            start: 0,
            end: 3,
            message: format!("{id} at {line} <&>"),
            severity: level.into(),
            severity_name: severity::display_name(level, &[]),
            editor: severity::editor_of(level, &[]),
            snippet: "$a = 1;".into(),
            fixes: vec![],
            related: vec![],
            stamp: 1,
            unnecessary: false,
            deprecated: false,
        }
    }

    fn report() -> Report {
        Report {
            problems: vec![
                problem("PhpUnusedAssignment", "src/A.php", 3, "WARNING"),
                problem("PhpUnusedAssignment", "src/B.php", 1, "WARNING"),
                problem("DuplicatedCode", "src/A.php", 9, "WEAK WARNING"),
                problem("PhpConstantCondition", "app/C.php", 2, "ERROR"),
            ],
            files: 3,
            profile: "Project Default".into(),
            ..Report::default()
        }
    }

    #[test]
    fn trees_group_by_levels() {
        let r = report();
        let t = tree(&r, &[Grouping::Group, Grouping::Inspection, Grouping::File]);
        assert_eq!(t[0].label, "General");
        assert_eq!(t[1].label, "PHP / Data flow");
        assert_eq!(t[1].count, 3);
        assert_eq!(t[1].editor, EditorSeverity::Error);
        let unused = t[1]
            .children
            .iter()
            .find(|n| n.detail == "PhpUnusedAssignment")
            .unwrap();
        assert_eq!(unused.children.len(), 2);
        assert_eq!(unused.children[0].children[0].kind, NodeKind::Problem);
        let by_sev = tree(&r, &[Grouping::Severity]);
        let labels: Vec<&str> = by_sev.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["Error", "Warning", "Weak Warning"]);
        let by_dir = tree(&r, &[Grouping::Directory]);
        assert_eq!(by_dir[0].label, "app");
        assert_eq!(by_dir[1].children.len(), 3);
    }

    #[test]
    fn exports() {
        let r = report();
        let xml = to_xml(&r);
        assert!(xml.contains("<problem_class id=\"PhpConstantCondition\" severity=\"ERROR\" attribute_key=\"ERRORS_ATTRIBUTES\">"), "{xml}");
        assert!(xml.contains("<line>4</line>"));
        assert!(xml.contains("&lt;&amp;&gt;"));
        let files = to_xml_files(&r);
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "DuplicatedCode.xml",
                "PhpConstantCondition.xml",
                "PhpUnusedAssignment.xml",
                ".descriptions.xml"
            ]
        );
        assert!(files[3].1.contains("problems=\"2\""));
        let html = to_html(&r);
        assert!(html.contains("<h2>PHP / Data flow (3)</h2>"), "{html}");
        assert!(html.contains("src/A.php:4"));
        let back: Report = serde_json::from_str(&to_json(&r)).unwrap();
        assert_eq!(back, r);
    }
}
