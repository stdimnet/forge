//! TAP, the Test Anything Protocol (versions 12–14): `busted -o TAP`, `prove`, node-tap,
//! `node --test --test-reporter=tap`, Bats and many more.
//!
//! - `ok N - description` passes, `not ok N - description` fails; `# SKIP` and `# TODO`
//!   directives ignore a test (a passing TODO counts as passed).
//! - A YAML diagnostics block (`  ---` … `  ...`) after a test adds the failure message,
//!   the expected and actual values (`expected`/`wanted`/`expect`, `actual`/`found`/`got`),
//!   the location (`at: file:line`, `file` + `line`) and `duration_ms`.
//! - `# ` comment lines right after a failure are its details; busted's comments give the
//!   location (`# spec/cart_spec.lua @ 12`), the message (`# Failure message: …`) and the
//!   values (`# Passed in:` / `# Expected:`).
//! - `# Subtest: name` and indented test lines nest suites (TAP 14, node-tap); a subtest's
//!   closing `ok` line is its suite and not a test of its own.
//! - `1..N` announces the count; `Bail out!` fails the run (console text).

use super::{dedent, strip_ansi};
use crate::teamcity::TestLocation;
use crate::testevents::{location, LineParser, ParsedLine, TestEvent};

#[derive(Debug, Default)]
struct Pending {
    name: String,
    failed: bool,
    yaml: Option<Vec<String>>,
    yaml_done: bool,
    comments: Vec<String>,
    indent: usize,
    /// The closing line of a subtest: its diagnostics are swallowed, nothing is reported.
    suite_close: bool,
}

#[derive(Debug, Default)]
pub struct TapParser {
    pending: Option<Pending>,
    /// Open subtests: (name, indentation of their lines).
    suites: Vec<(String, usize)>,
    /// A `# Subtest:` line announced a suite at this indentation.
    announced: Option<(String, usize)>,
    /// Names already used at this level (TAP descriptions need not be unique).
    seen: Vec<String>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// `ok 3 - adds numbers # SKIP no db` → (ok, "adds numbers", directive, reason).
fn test_line(text: &str) -> Option<(bool, String, Option<String>, String)> {
    let (ok, rest) = if let Some(r) = text.strip_prefix("not ok") {
        (false, r)
    } else if let Some(r) = text.strip_prefix("ok") {
        (true, r)
    } else {
        return None;
    };
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    let rest = rest.trim_start();
    let rest = rest
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .trim_start();
    let rest = rest.strip_prefix('-').unwrap_or(rest).trim();
    // An unescaped `#` starts the directive.
    let mut desc = String::new();
    let mut directive = None;
    let mut reason = String::new();
    let mut chars = rest.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, n)) = chars.next() {
                desc.push(n);
            }
            continue;
        }
        if c == '#' {
            let d = rest[i + 1..].trim();
            let word: String = d.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            let upper = word.to_ascii_uppercase();
            if upper.starts_with("SKIP") || upper == "TODO" {
                directive = Some(if upper == "TODO" { "TODO" } else { "SKIP" }.to_string());
                reason = d[word.len()..].trim().to_string();
            }
            break;
        }
        desc.push(c);
    }
    Some((ok, desc.trim().to_string(), directive, reason))
}

/// A YAML scalar of the diagnostics: `key: value` (quotes removed) or a `|` block.
fn yaml_value(lines: &[String], key: &str) -> Option<String> {
    let base = lines.iter().map(|l| indent_of(l)).min().unwrap_or(0);
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        if indent_of(l) == base {
            if let Some(v) = l.trim_start().strip_prefix(&format!("{key}:")) {
                let v = v.trim();
                if v.is_empty() || v.starts_with('|') || v.starts_with('>') {
                    let mut block = Vec::new();
                    for b in &lines[i + 1..] {
                        if indent_of(b) <= base && !b.trim().is_empty() {
                            break;
                        }
                        block.push(b.as_str());
                    }
                    let text = dedent(&block.join("\n"));
                    return Some(text.trim_end().to_string()).filter(|t| !t.is_empty());
                }
                let unquoted = v
                    .strip_prefix('\'')
                    .and_then(|x| x.strip_suffix('\''))
                    .map(|x| x.replace("''", "'"))
                    .or_else(|| {
                        v.strip_prefix('"')
                            .and_then(|x| x.strip_suffix('"'))
                            .map(|x| x.replace("\\\"", "\"").replace("\\n", "\n"))
                    })
                    .unwrap_or_else(|| v.to_string());
                return Some(unquoted);
            }
        }
        i += 1;
    }
    None
}

fn first_of(lines: &[String], keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| yaml_value(lines, k))
}

/// `file:line[:column]` (also inside `at: …` or `(file:line:col)`).
fn file_line(text: &str) -> Option<TestLocation> {
    let t = text.trim().trim_start_matches('(').trim_end_matches(')');
    let t = t.rsplit(" (").next().unwrap_or(t).trim_end_matches(')');
    let mut parts = t.rsplitn(3, ':');
    let last = parts.next()?;
    let mid = parts.next()?;
    let (file, line) = match parts.next() {
        Some(f) if mid.bytes().all(|b| b.is_ascii_digit()) => (f, mid),
        _ => (t.rsplit_once(':').map(|(f, _)| f)?, last),
    };
    let line: u32 = line.parse().ok()?;
    let file = file.strip_prefix("file://").unwrap_or(file);
    (!file.is_empty()).then(|| location(file, Some(line), None))
}

impl TapParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn unique(&mut self, name: &str) -> String {
        let mut n = name.to_string();
        let mut k = 2;
        while self.seen.contains(&n) {
            n = format!("{name} ({k})");
            k += 1;
        }
        self.seen.push(n.clone());
        n
    }

    /// Events of the test waiting for its diagnostics.
    fn flush(&mut self) -> Vec<TestEvent> {
        let Some(p) = self.pending.take() else {
            return Vec::new();
        };
        if p.suite_close {
            return Vec::new();
        }
        let mut out = Vec::new();
        let yaml = p.yaml.unwrap_or_default();
        let duration = first_of(&yaml, &["duration_ms"])
            .and_then(|d| d.parse::<f64>().ok())
            .map(|d| d.max(0.0).round() as u64);
        if p.failed {
            let mut message = first_of(&yaml, &["message", "msg", "error"]);
            let mut expected = first_of(&yaml, &["expected", "wanted", "expect"]);
            let mut actual = first_of(&yaml, &["actual", "found", "got"]);
            let mut loc = first_of(&yaml, &["at", "location"]).and_then(|a| file_line(&a));
            if loc.is_none() {
                if let (Some(f), Some(l)) = (yaml_value(&yaml, "file"), yaml_value(&yaml, "line")) {
                    loc = Some(location(&f, l.parse().ok(), None));
                }
            }
            // busted: `# file @ line`, `# Failure message: …`, `# Passed in:` / `# Expected:`.
            let mut section: Option<&str> = None;
            let (mut passed_in, mut wanted) = (Vec::new(), Vec::new());
            // Comments that are none of these stay as details.
            let mut rest: Vec<&str> = Vec::new();
            for c in &p.comments {
                if let Some((f, l)) = c.split_once(" @ ") {
                    if loc.is_none() {
                        if let Ok(l) = l.trim().parse() {
                            loc = Some(location(f.trim(), Some(l), None));
                            continue;
                        }
                    }
                }
                if let Some(m) = c.strip_prefix("Failure message:") {
                    message.get_or_insert_with(|| m.trim().to_string());
                    section = None;
                } else if c.trim() == "Passed in:" {
                    section = Some("actual");
                } else if c.trim() == "Expected:" {
                    section = Some("expected");
                } else if section == Some("actual") {
                    passed_in.push(c.trim().to_string());
                } else if section == Some("expected") {
                    wanted.push(c.trim().to_string());
                } else {
                    rest.push(c);
                }
            }
            if expected.is_none() && !wanted.is_empty() {
                expected = Some(wanted.join("\n"));
            }
            if actual.is_none() && !passed_in.is_empty() {
                actual = Some(passed_in.join("\n"));
            }
            let mut details: Vec<String> = Vec::new();
            if let Some(stack) = first_of(&yaml, &["stack", "stackTrace"]) {
                details.push(stack);
            }
            if !rest.is_empty() {
                details.push(rest.join("\n"));
            }
            if let Some(l) = &loc {
                details.push(format!(
                    "{}{}",
                    l.path,
                    l.line.map(|n| format!(":{n}")).unwrap_or_default()
                ));
            }
            let message = message
                .or_else(|| rest.first().map(|c| c.to_string()))
                .unwrap_or_else(|| "Test failed".to_string());
            out.push(TestEvent::TestFailed {
                name: p.name.clone(),
                message: strip_ansi(&message),
                details: Some(strip_ansi(&details.join("\n"))).filter(|d| !d.is_empty()),
                expected,
                actual,
                location: loc,
            });
        }
        out.push(TestEvent::TestFinished {
            name: p.name,
            duration,
        });
        out
    }

    fn close_suites_above(&mut self, indent: usize, out: &mut Vec<TestEvent>) {
        while self.suites.last().is_some_and(|(_, i)| *i > indent) {
            let (name, _) = self.suites.pop().unwrap_or_default();
            out.push(TestEvent::SuiteFinished { name });
        }
    }
}

impl LineParser for TapParser {
    fn line(&mut self, raw: &str) -> ParsedLine {
        let line = raw.trim_end_matches('\r');
        let indent = indent_of(line);
        let text = line.trim();

        // Inside a YAML block of the pending test.
        if let Some(p) = self.pending.as_mut() {
            if let Some(y) = p.yaml.as_mut().filter(|_| !p.yaml_done) {
                if text == "..." {
                    p.yaml_done = true;
                } else {
                    y.push(line.to_string());
                }
                return ParsedLine::events(Vec::new());
            }
            if text == "---" && p.yaml.is_none() && indent > p.indent {
                p.yaml = Some(Vec::new());
                return ParsedLine::events(Vec::new());
            }
            if let Some(c) = text.strip_prefix('#') {
                if !c.trim_start().starts_with("Subtest:") && p.failed {
                    p.comments
                        .push(c.strip_prefix(' ').unwrap_or(c).to_string());
                    return ParsedLine::events(Vec::new());
                }
            }
        }

        let mut out = Vec::new();
        if let Some(name) = text
            .strip_prefix('#')
            .map(str::trim)
            .and_then(|t| t.strip_prefix("Subtest:"))
        {
            out.extend(self.flush());
            self.close_suites_above(indent, &mut out);
            // A deeper announcement makes the previous one a suite (`node --test`).
            if let Some((parent, at)) = self.announced.take() {
                if at < indent {
                    out.push(TestEvent::SuiteStarted {
                        name: parent.clone(),
                        location: None,
                    });
                    self.suites.push((parent, indent));
                }
            }
            self.announced = Some((name.trim().to_string(), indent));
            return ParsedLine::events(out);
        }
        if let Some((ok, desc, directive, reason)) = test_line(text) {
            out.extend(self.flush());
            // The closing line of a subtest: its suite ends.
            if let Some(pos) = self
                .suites
                .iter()
                .rposition(|(n, i)| *i > indent && (desc.is_empty() || *n == desc))
            {
                while self.suites.len() > pos {
                    let (name, _) = self.suites.pop().unwrap_or_default();
                    out.push(TestEvent::SuiteFinished { name });
                }
                self.announced = None;
                self.pending = Some(Pending {
                    suite_close: true,
                    indent,
                    ..Pending::default()
                });
                return ParsedLine::events(out);
            }
            self.close_suites_above(indent, &mut out);
            if let Some((suite, at)) = self.announced.take() {
                // node-tap prints the subtest's lines indented below the announcement.
                if indent > at {
                    out.push(TestEvent::SuiteStarted {
                        name: suite.clone(),
                        location: None,
                    });
                    self.suites.push((suite, indent));
                }
            }
            let base = match (desc.is_empty(), directive.is_some() && !reason.is_empty()) {
                (false, _) => desc,
                // busted: `ok 3 - # SKIP <name>`.
                (true, true) => reason.clone(),
                (true, false) => "(unnamed test)".to_string(),
            };
            let name = self.unique(&base);
            match directive.as_deref() {
                Some("SKIP") | Some("TODO") if !(ok && directive.as_deref() == Some("TODO")) => {
                    out.push(TestEvent::TestIgnored {
                        message: Some(reason).filter(|r| !r.is_empty() && *r != name),
                        name,
                        location: None,
                    });
                }
                _ => {
                    out.push(TestEvent::TestStarted {
                        name: name.clone(),
                        location: None,
                    });
                    self.pending = Some(Pending {
                        name,
                        failed: !ok,
                        indent,
                        ..Pending::default()
                    });
                }
            }
            return ParsedLine::events(out);
        }
        if let Some(plan) = text.strip_prefix("1..") {
            out.extend(self.flush());
            let count = plan
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse::<u32>().ok());
            if let Some(n) = count.filter(|_| self.suites.is_empty() && indent == 0) {
                out.push(TestEvent::Count { count: n });
            }
            return ParsedLine::events(out);
        }
        if text.starts_with("TAP version") || text == "pragma +strict" {
            return ParsedLine::events(Vec::new());
        }
        let mut parsed = ParsedLine::console(line);
        parsed.events = self.flush();
        parsed
    }

    fn finish(&mut self) -> Vec<TestEvent> {
        let mut out = self.flush();
        while let Some((name, _)) = self.suites.pop() {
            out.push(TestEvent::SuiteFinished { name });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, run_lines, tree_of};
    use crate::teamcity::TestStatus;

    #[test]
    fn parses_test_lines_and_directives() {
        assert_eq!(
            test_line("ok 1 - adds numbers"),
            Some((true, "adds numbers".into(), None, String::new()))
        );
        assert_eq!(
            test_line("not ok 2 subtracts \\# twice # TODO later"),
            Some((
                false,
                "subtracts # twice".into(),
                Some("TODO".into()),
                "later".into()
            ))
        );
        assert_eq!(
            test_line("ok 3 # skip no database"),
            Some((
                true,
                String::new(),
                Some("SKIP".into()),
                "no database".into()
            ))
        );
        assert_eq!(test_line("okay"), None);
    }

    #[test]
    fn busted_output_builds_a_tree_with_failure_values() {
        let events = run_lines(&mut TapParser::new(), &fixture("busted.tap"));
        let tree = tree_of(&events);
        let nodes = tree.snapshot();
        let names: Vec<_> = nodes
            .iter()
            .skip(1)
            .map(|n| (n.name.as_str(), n.status))
            .collect();
        assert_eq!(
            names,
            [
                ("Cart adds items", TestStatus::Passed),
                ("Cart totals rounds to cents", TestStatus::Failed),
                ("Cart applies vouchers", TestStatus::Ignored),
                ("Inventory reserves stock", TestStatus::Passed),
            ]
        );
        let failed = &nodes[2];
        assert_eq!(
            failed.message.as_deref(),
            Some("spec/cart_spec.lua:14: Expected objects to be the same.")
        );
        assert_eq!(failed.expected.as_deref(), Some("(number) 1.18"));
        assert_eq!(failed.actual.as_deref(), Some("(number) 1.1781"));
        assert_eq!(tree.summary().expected_count, Some(4));
        let loc = failed.location.as_ref().unwrap();
        assert_eq!(loc.workspace_path.as_deref(), Some("spec/cart_spec.lua"));
        assert_eq!(loc.line, Some(12));
        assert_eq!(nodes[3].message, None);
    }

    #[test]
    fn yaml_diagnostics_and_subtests() {
        let events = run_lines(&mut TapParser::new(), &fixture("node-test.tap"));
        let tree = tree_of(&events);
        let nodes = tree.snapshot();
        let rows: Vec<_> = nodes
            .iter()
            .map(|n| (n.name.as_str(), n.parent_id, n.status))
            .collect();
        assert_eq!(
            rows,
            [
                ("Test Results", None, TestStatus::Failed),
                ("cart", Some(0), TestStatus::Failed),
                ("adds items", Some(1), TestStatus::Passed),
                ("rounds to cents", Some(1), TestStatus::Failed),
                ("is empty at first", Some(0), TestStatus::Passed),
            ]
        );
        let failed = &nodes[3];
        assert_eq!(
            failed.message.as_deref(),
            Some("Expected values to be strictly equal:\n\n1.1781 !== 1.18")
        );
        assert_eq!(failed.expected.as_deref(), Some("1.18"));
        assert_eq!(failed.actual.as_deref(), Some("1.1781"));
        assert_eq!(failed.duration, Some(3));
        let loc = failed.location.as_ref().unwrap();
        assert_eq!(
            (loc.path.as_str(), loc.line),
            ("/p/test/cart.test.mjs", Some(10))
        );
        assert_eq!(loc.workspace_path.as_deref(), Some("test/cart.test.mjs"));
    }
}
