//! Generate Coverage Report: a suite as one standalone HTML file (no scripts, no external
//! resources), like IntelliJ's HTML coverage report: the totals, the directories and files
//! with their line, branch and method coverage, and every file's source with its covered,
//! partly covered and uncovered lines, hits and the tests that ran them.

use super::{summary, Counter, CoverageNodeKind, CoverageReport, LineHits, LineStatus};

/// Escape text for HTML.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The anchor of a file's section.
fn anchor(path: &str) -> String {
    let mut s = String::from("f-");
    for c in path.chars() {
        s.push(if c.is_ascii_alphanumeric() { c } else { '-' });
    }
    s
}

fn percent_cell(c: Counter) -> String {
    match c.percent() {
        Some(p) => format!(
            "<td class=\"pct\"><span class=\"bar\"><span style=\"width:{p}%\"></span></span>{}</td>",
            esc(&c.text())
        ),
        None => "<td class=\"pct na\">n/a</td>".into(),
    }
}

const STYLE: &str = r#"
:root { color-scheme: light dark; --bg: #fbfaf8; --fg: #2b2622; --muted: #7a716a; --line: #e4ded8;
  --cov: #d9efd9; --part: #f6ebc4; --unc: #f6d6d2; --bar: #4f9a5a; --bar-bg: #e7e1db; --link: #a8551e; }
@media (prefers-color-scheme: dark) { :root { --bg: #1f1b18; --fg: #e8e1da; --muted: #9c928a; --line: #3a332e;
  --cov: #22402a; --part: #4a4020; --unc: #4d2622; --bar: #6dbb78; --bar-bg: #3a332e; --link: #f0a060; } }
body { margin: 0; padding: 24px; background: var(--bg); color: var(--fg); font: 14px/1.45 system-ui, sans-serif; }
h1 { font-size: 20px; margin: 0 0 4px; } h2 { font-size: 15px; margin: 32px 0 8px; }
.muted { color: var(--muted); } a { color: var(--link); text-decoration: none; } a:hover { text-decoration: underline; }
table { border-collapse: collapse; } .summary { width: 100%; max-width: 960px; margin-top: 16px; }
.summary th, .summary td { text-align: left; padding: 4px 10px; border-bottom: 1px solid var(--line); white-space: nowrap; }
.summary th { font-weight: 600; color: var(--muted); }
.pct { font-variant-numeric: tabular-nums; } .na { color: var(--muted); }
.bar { display: inline-block; width: 64px; height: 8px; margin-right: 8px; background: var(--bar-bg); border-radius: 2px; overflow: hidden; vertical-align: middle; }
.bar span { display: block; height: 100%; background: var(--bar); }
.src { font: 12.5px/1.5 ui-monospace, Menlo, Consolas, monospace; width: 100%; }
.src td { padding: 0 8px; vertical-align: top; } .src .n, .src .h { text-align: right; color: var(--muted); user-select: none; width: 1%; }
.src pre { margin: 0; white-space: pre-wrap; } .covered { background: var(--cov); } .partial { background: var(--part); } .uncovered { background: var(--unc); }
.file { border: 1px solid var(--line); border-radius: 4px; overflow: hidden; }
"#;

fn row(l: Option<&LineHits>) -> (&'static str, String, String) {
    let Some(l) = l else {
        return ("", String::new(), String::new());
    };
    let class = match l.status() {
        LineStatus::Covered => "covered",
        LineStatus::Partial => "partial",
        LineStatus::Uncovered => "uncovered",
    };
    let mut title = super::hint(l);
    if !l.tests.is_empty() {
        title.push('\n');
        title.push_str(&l.tests.join("\n"));
    }
    (class, l.hits.to_string(), title)
}

/// The HTML report of a suite. `title` names it (the suite), `generated` says when;
/// `source` gives a file's text by its project path (files without text show their lines
/// only).
pub fn render(
    report: &CoverageReport,
    project: &str,
    title: &str,
    generated: &str,
    source: impl Fn(&str) -> Option<String>,
) -> String {
    let nodes = summary(report, project);
    let mut h = String::new();
    h.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    h.push_str(&format!(
        "<title>Coverage: {}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n",
        esc(title)
    ));
    h.push_str(&format!(
        "<h1>Coverage: {}</h1>\n<div class=\"muted\">{} · {} files · generated {}</div>\n",
        esc(title),
        esc(project),
        report.files.len(),
        esc(generated)
    ));
    h.push_str("<table class=\"summary\">\n<tr><th>Element</th><th>Method, %</th><th>Line, %</th><th>Branch, %</th></tr>\n");
    for n in &nodes {
        let indent = n.depth * 16;
        let name = match n.kind {
            CoverageNodeKind::File => {
                format!("<a href=\"#{}\">{}</a>", anchor(&n.path), esc(&n.name))
            }
            CoverageNodeKind::Directory => format!("{}/", esc(&n.name)),
            CoverageNodeKind::Project => {
                format!(
                    "<b>{}</b>",
                    esc(if n.name.is_empty() {
                        "All files"
                    } else {
                        &n.name
                    })
                )
            }
        };
        h.push_str(&format!(
            "<tr><td style=\"padding-left:{}px\">{name}</td>{}{}{}</tr>\n",
            10 + indent,
            percent_cell(n.methods),
            percent_cell(n.lines),
            percent_cell(n.branches)
        ));
    }
    h.push_str("</table>\n");
    let mut files: Vec<_> = report.files.iter().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    for f in files {
        let t = f.totals();
        h.push_str(&format!(
            "<h2 id=\"{}\">{}</h2>\n<div class=\"muted\">Lines {} · Branches {} · Methods {}</div>\n",
            anchor(&f.path),
            esc(&f.path),
            esc(&t.lines.text()),
            esc(&t.branches.text()),
            esc(&t.methods.text())
        ));
        h.push_str("<div class=\"file\"><table class=\"src\">\n");
        let by_line = |n: u32| {
            f.lines
                .binary_search_by_key(&n, |l| l.line)
                .ok()
                .map(|i| &f.lines[i])
        };
        match source(&f.path) {
            Some(text) => {
                for (i, code) in text.lines().enumerate() {
                    let (class, hits, tip) = row(by_line(i as u32 + 1));
                    h.push_str(&format!(
                        "<tr class=\"{class}\" title=\"{}\"><td class=\"n\">{}</td><td class=\"h\">{hits}</td><td><pre>{}</pre></td></tr>\n",
                        esc(&tip),
                        i + 1,
                        esc(code)
                    ));
                }
            }
            None => {
                for l in &f.lines {
                    let (class, hits, tip) = row(Some(l));
                    h.push_str(&format!(
                        "<tr class=\"{class}\" title=\"{}\"><td class=\"n\">{}</td><td class=\"h\">{hits}</td><td><pre class=\"muted\">source not available</pre></td></tr>\n",
                        esc(&tip),
                        l.line
                    ));
                }
            }
        }
        h.push_str("</table></div>\n");
    }
    h.push_str("</body>\n</html>\n");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::tests::fixture;

    #[test]
    fn renders_totals_files_and_sources() {
        let (r, _) = CoverageReport::parse_any(&fixture("lcov.info")).unwrap();
        let r = r.resolve(|p| p.strip_prefix("/p/").map(str::to_string));
        let html = render(&r, "shop", "All <tests>", "2026-10-28 10:41", |p| {
            (p == "src/cart.js").then(|| {
                "function addItem(a) {\n  a.push(1);\n\n  if (a.length > 2) {\n    return a;\n  }\n  x();\n\n  y();\n"
                    .to_string()
            })
        });
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>Coverage: All &lt;tests&gt;</title>"));
        assert!(html.contains("<a href=\"#f-src-cart-js\">cart.js</a>"));
        assert!(html.contains("62% (5/8)"), "project line coverage");
        assert!(html.contains("100% (1/1)"), "methods of cart.js");
        assert!(html.contains("<tr class=\"partial\" title=\"Hits: 2, branches: 1 of 2 covered\"><td class=\"n\">4</td><td class=\"h\">2</td><td><pre>  if (a.length &gt; 2) {</pre>"));
        assert!(html.contains("<tr class=\"\" title=\"\"><td class=\"n\">3</td>"));
        assert!(
            html.contains("source not available"),
            "format.js has no text"
        );
        assert!(!html.contains("<script"));
    }
}
