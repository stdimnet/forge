//! Minitest's verbose progress output (`ruby -Itest test/cart_test.rb -v`, `rake test
//! TESTOPTS=-v`):
//!
//! ```text
//! Run options: -v --seed 42
//!
//! # Running:
//!
//! CartTest#test_adds_items = 0.00 s = .
//! CartTest#test_total = computing total
//! 0.00 s = F
//!
//! Finished in 0.010264s, 487.1549 runs/s, 292.2930 assertions/s.
//!
//!   1) Failure:
//! CartTest#test_total [/p/test/cart_test.rb:10]:
//! Expected: 1050
//!   Actual: 1049
//!
//! 5 runs, 3 assertions, 1 failures, 1 errors, 1 skips
//! ```
//!
//! - `<Class>#<method> = <seconds> s = <. F E S>` is a result; output the test prints comes
//!   between ` = ` and the time and is attributed to the test.
//! - Every test class is a suite (its tests run together). A class whose tests all passed is
//!   reported when the next class starts; a class with failures, errors or skips waits for
//!   the summary, whose `Failure:` / `Error:` / `Skipped:` sections give the messages,
//!   `Expected:` / `Actual:` values, the location (`[file:line]` or the first backtrace
//!   frame) and the class's file.
//! - Minitest::Spec methods (`test_0001_adds items`) show as their description (`adds
//!   items`); the location member keeps the method name.

use crate::teamcity::TestLocation;
use crate::testevents::{location, LineParser, ParsedLine, TestEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Pass,
    Fail,
    Error,
    Skip,
}

#[derive(Debug, Clone)]
struct Test {
    method: String,
    seconds: Option<f64>,
    outcome: Option<Outcome>,
    output: String,
    message: Option<String>,
    details: Option<String>,
    location: Option<TestLocation>,
}

impl Test {
    fn name(&self) -> String {
        display_name(&self.method)
    }
}

#[derive(Debug, Default)]
struct Class {
    name: String,
    tests: Vec<Test>,
    file: Option<String>,
}

impl Class {
    fn needs_summary(&self) -> bool {
        self.tests.iter().any(|t| {
            matches!(
                t.outcome,
                Some(Outcome::Fail | Outcome::Error | Outcome::Skip)
            )
        })
    }
}

#[derive(Debug, Default)]
pub struct MinitestParser {
    /// The class whose tests run now.
    current: Option<Class>,
    /// Classes waiting for the summary.
    held: Vec<Class>,
    /// A test that started and printed before its result.
    pending: bool,
    /// The summary section being read: (kind, header seen, test index in `held`, text).
    section: Option<Section>,
}

#[derive(Debug)]
struct Section {
    outcome: Outcome,
    /// (class index, test index) once the `Class#method` line was read.
    target: Option<(usize, usize)>,
    text: String,
}

/// `test_0001_adds items` → `adds items`; other names stay.
pub fn display_name(method: &str) -> String {
    if let Some(rest) = method.strip_prefix("test_") {
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 4 && rest[digits..].starts_with('_') {
            return rest[digits + 1..].to_string();
        }
    }
    method.to_string()
}

/// `Shop::CartTest#test_total = rest` → (class, method, rest).
fn start_line(line: &str) -> Option<(&str, &str, &str)> {
    let hash = line.find("#test_")?;
    let class = &line[..hash];
    if class.is_empty() || class.starts_with(char::is_whitespace) {
        return None;
    }
    let after = &line[hash + 1..];
    let eq = after.find(" = ")?;
    Some((class, &after[..eq], &after[eq + 3..]))
}

/// `… 0.00 s = F` at the end of a line → (text before, seconds, outcome).
fn result_tail(text: &str) -> Option<(&str, f64, Outcome)> {
    let t = text.trim_end();
    let outcome = match t.chars().last()? {
        '.' => Outcome::Pass,
        'F' => Outcome::Fail,
        'E' => Outcome::Error,
        'S' => Outcome::Skip,
        _ => return None,
    };
    let rest = t[..t.len() - 1].strip_suffix(" s = ")?;
    let start = rest
        .rfind(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let seconds: f64 = rest[start..].parse().ok()?;
    Some((&rest[..start], seconds, outcome))
}

/// `[/p/test/cart_test.rb:10]:` at the end of a header line → location.
fn bracket_location(header: &str) -> Option<TestLocation> {
    let inner = header.trim_end().strip_suffix("]:")?;
    let open = inner.rfind(" [")?;
    let spot = &inner[open + 2..];
    let (file, line) = spot.rsplit_once(':')?;
    Some(location(file, line.parse().ok(), None))
}

/// `    /p/test/cart_test.rb:18:in 'test_boom'` → location.
fn frame_location(line: &str) -> Option<TestLocation> {
    let t = line.trim();
    let (file, rest) = t.split_once(".rb:")?;
    let n: u32 = rest
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some(location(&format!("{file}.rb"), Some(n), None))
}

fn value_after(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        l.trim_start()
            .strip_prefix(key)
            .map(|v| v.trim().to_string())
    })
}

impl MinitestParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn class_events(class: &Class) -> Vec<TestEvent> {
        let mut out = vec![TestEvent::SuiteStarted {
            name: class.name.clone(),
            location: class
                .file
                .as_ref()
                .map(|f| location(f, None, Some(&class.name))),
        }];
        for t in &class.tests {
            let name = t.name();
            let loc = t.location.clone().or_else(|| {
                class
                    .file
                    .as_ref()
                    .map(|f| location(f, None, Some(&t.method)))
            });
            let loc = loc.map(|mut l| {
                l.member = Some(t.method.clone());
                l
            });
            if t.outcome == Some(Outcome::Skip) {
                out.push(TestEvent::TestIgnored {
                    name,
                    message: t.message.clone(),
                    location: loc,
                });
                continue;
            }
            out.push(TestEvent::TestStarted {
                name: name.clone(),
                location: loc,
            });
            if !t.output.is_empty() {
                out.push(TestEvent::TestOutput {
                    name: name.clone(),
                    text: t.output.clone(),
                    stderr: false,
                });
            }
            if matches!(t.outcome, Some(Outcome::Fail | Outcome::Error)) {
                let details = t.details.clone().unwrap_or_default();
                let expected = value_after(&details, "Expected:");
                let actual = value_after(&details, "Actual:");
                let both = expected.is_some() && actual.is_some();
                out.push(TestEvent::TestFailed {
                    name: name.clone(),
                    message: t.message.clone().unwrap_or_else(|| {
                        if t.outcome == Some(Outcome::Error) {
                            "Error".into()
                        } else {
                            "Failure".into()
                        }
                    }),
                    details: Some(details).filter(|d| !d.is_empty()),
                    expected: expected.filter(|_| both),
                    actual: actual.filter(|_| both),
                    location: t.location.clone(),
                });
            }
            out.push(TestEvent::TestFinished {
                name,
                duration: t.seconds.map(|s| (s.max(0.0) * 1000.0).round() as u64),
            });
        }
        out.push(TestEvent::SuiteFinished {
            name: class.name.clone(),
        });
        out
    }

    /// The running class is done: report it now, or keep it for the summary.
    fn close_class(&mut self) -> Vec<TestEvent> {
        match self.current.take() {
            Some(c) if c.needs_summary() => {
                self.held.push(c);
                Vec::new()
            }
            Some(c) => Self::class_events(&c),
            None => Vec::new(),
        }
    }

    fn close_section(&mut self) {
        let Some(s) = self.section.take() else {
            return;
        };
        let Some((ci, ti)) = s.target else {
            return;
        };
        let text = s.text.trim_matches('\n').to_string();
        let class = &mut self.held[ci];
        let test = &mut class.tests[ti];
        if s.outcome == Outcome::Skip {
            test.message = Some(text).filter(|t| !t.is_empty());
            return;
        }
        let first = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string();
        if test.location.is_none() {
            test.location = text.lines().find_map(frame_location);
        }
        if class.file.is_none() {
            class.file = test.location.as_ref().map(|l| l.path.clone());
        }
        test.message = Some(first).filter(|m| !m.is_empty());
        test.details = Some(text).filter(|t| !t.is_empty());
    }

    /// The summary is complete: report the held classes.
    fn flush(&mut self) -> Vec<TestEvent> {
        self.close_section();
        let mut out = self.close_class();
        for c in std::mem::take(&mut self.held) {
            out.extend(Self::class_events(&c));
        }
        out
    }

    fn record(&mut self, seconds: f64, outcome: Outcome, output: &str) {
        if let Some(t) = self.current.as_mut().and_then(|c| c.tests.last_mut()) {
            t.output.push_str(output);
            t.seconds = Some(seconds);
            t.outcome = Some(outcome);
        }
        self.pending = false;
    }

    fn find_held(&self, class: &str, method: &str) -> Option<(usize, usize)> {
        self.held.iter().enumerate().find_map(|(ci, c)| {
            (c.name == class)
                .then(|| c.tests.iter().position(|t| t.method == method))
                .flatten()
                .map(|ti| (ci, ti))
        })
    }
}

impl LineParser for MinitestParser {
    fn line(&mut self, line: &str) -> ParsedLine {
        let trimmed = line.trim_end();
        // Summary sections: `  1) Failure:` and their text.
        let header = trimmed.trim_start();
        let numbered = header
            .split_once(") ")
            .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if let Some((_, kind)) = numbered {
            let outcome = match kind {
                "Failure:" => Some(Outcome::Fail),
                "Error:" => Some(Outcome::Error),
                "Skipped:" => Some(Outcome::Skip),
                _ => None,
            };
            if let Some(outcome) = outcome {
                self.close_section();
                // The class still running at the summary is complete.
                let events = self.close_class();
                self.section = Some(Section {
                    outcome,
                    target: None,
                    text: String::new(),
                });
                let mut parsed = ParsedLine::events(events);
                parsed.console = Some(format!("{line}\n"));
                return parsed;
            }
        }
        if let Some(section) = self.section.as_mut() {
            if section.target.is_none() {
                let head = trimmed.split(" [").next().unwrap_or(trimmed);
                let head = head.strip_suffix(':').unwrap_or(head);
                if let Some((class, method)) = head.rsplit_once('#') {
                    let found = self.find_held(class, method);
                    if let Some(s) = self.section.as_mut() {
                        s.target = found;
                    }
                    if let Some((ci, ti)) = found {
                        if let Some(loc) = bracket_location(trimmed) {
                            let class = &mut self.held[ci];
                            if class.file.is_none() {
                                class.file = Some(loc.path.clone());
                            }
                            class.tests[ti].location = Some(loc);
                        }
                    }
                    return ParsedLine::console(line);
                }
            }
            let is_summary = trimmed.contains(" runs, ") && trimmed.contains(" assertions, ");
            if !is_summary {
                if let Some(s) = self.section.as_mut() {
                    s.text.push_str(trimmed);
                    s.text.push('\n');
                }
                return ParsedLine::console(line);
            }
        }
        if trimmed.contains(" runs, ") && trimmed.contains(" assertions, ") {
            let mut parsed = ParsedLine::events(self.flush());
            parsed.console = Some(format!("{line}\n"));
            return parsed;
        }
        if let Some((class, method, rest)) = start_line(trimmed) {
            let mut out = Vec::new();
            if self.current.as_ref().is_some_and(|c| c.name != class) {
                out.extend(self.close_class());
            }
            if self.current.is_none() {
                self.current = Some(Class {
                    name: class.to_string(),
                    ..Class::default()
                });
            }
            if let Some(c) = self.current.as_mut() {
                c.tests.push(Test {
                    method: method.to_string(),
                    seconds: None,
                    outcome: None,
                    output: String::new(),
                    message: None,
                    details: None,
                    location: None,
                });
            }
            match result_tail(rest) {
                Some((before, seconds, outcome)) => {
                    self.record(seconds, outcome, before);
                    let mut parsed = ParsedLine::events(out);
                    if !before.is_empty() {
                        parsed.console = Some(format!("{before}\n"));
                        parsed.attributed = true;
                    }
                    return parsed;
                }
                None => {
                    self.pending = true;
                    let mut parsed = ParsedLine::events(out);
                    if !rest.is_empty() {
                        if let Some(t) = self.current.as_mut().and_then(|c| c.tests.last_mut()) {
                            t.output.push_str(rest);
                            t.output.push('\n');
                        }
                        parsed.console = Some(format!("{rest}\n"));
                        parsed.attributed = true;
                    }
                    return parsed;
                }
            }
        }
        if self.pending {
            if let Some((before, seconds, outcome)) = result_tail(trimmed) {
                self.record(seconds, outcome, before);
                let mut parsed = ParsedLine::events(Vec::new());
                if !before.is_empty() {
                    parsed.console = Some(format!("{before}\n"));
                    parsed.attributed = true;
                }
                return parsed;
            }
            if let Some(t) = self.current.as_mut().and_then(|c| c.tests.last_mut()) {
                t.output.push_str(trimmed);
                t.output.push('\n');
            }
            let mut parsed = ParsedLine::console(line);
            parsed.attributed = true;
            return parsed;
        }
        ParsedLine::console(line)
    }

    fn finish(&mut self) -> Vec<TestEvent> {
        // A run that ended without its summary (interrupted): what is known.
        self.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, run_lines, tree_of};
    use crate::teamcity::TestStatus;

    #[test]
    fn classes_results_and_summary() {
        let mut p = MinitestParser::new();
        let events = run_lines(&mut p, &fixture("minitest-verbose.txt"));
        let tree = tree_of(&events);
        let rows: Vec<_> = tree
            .snapshot()
            .into_iter()
            .map(|n| (n.name, n.parent_id, n.status))
            .collect();
        assert_eq!(
            rows,
            [
                ("Test Results".to_string(), None, TestStatus::Failed),
                ("Shop::MoneyTest".into(), Some(0), TestStatus::Passed),
                ("test_formats".into(), Some(1), TestStatus::Passed),
                ("CartTest".into(), Some(0), TestStatus::Failed),
                ("test_boom".into(), Some(3), TestStatus::Failed),
                ("test_voucher".into(), Some(3), TestStatus::Ignored),
                ("test_adds_items".into(), Some(3), TestStatus::Passed),
                ("test_total".into(), Some(3), TestStatus::Failed),
            ]
        );
        let nodes = tree.snapshot();
        let total = &nodes[7];
        assert_eq!(total.expected.as_deref(), Some("1050"));
        assert_eq!(total.actual.as_deref(), Some("1049"));
        assert_eq!(total.message.as_deref(), Some("Expected: 1050"));
        assert!(events.iter().any(|e| matches!(e, TestEvent::TestOutput { name, text, .. } if name == "test_total" && text == "computing total\n")));
        let loc = total.location.as_ref().unwrap();
        assert!(loc.path.ends_with("test/cart_test.rb"), "{loc:?}");
        assert_eq!(
            (loc.line, loc.member.as_deref()),
            (Some(10), Some("test_total"))
        );
        let boom = &nodes[4];
        assert_eq!(boom.message.as_deref(), Some("ArgumentError: boom"));
        assert_eq!(boom.location.as_ref().unwrap().line, Some(18));
        assert_eq!(nodes[5].message.as_deref(), Some("vouchers are not ready"));
        // The class's file comes from its failures, so passing tests are located too.
        let adds = nodes[6].location.as_ref().unwrap();
        assert!(adds.path.ends_with("test/cart_test.rb"));
        assert_eq!(adds.member.as_deref(), Some("test_adds_items"));
    }

    #[test]
    fn passing_classes_are_reported_when_the_next_class_starts() {
        let mut p = MinitestParser::new();
        assert!(p.line("ATest#test_a = 0.00 s = .").events.is_empty());
        let next = p.line("BTest#test_b = 0.01 s = .");
        assert_eq!(next.events.len(), 4, "{:?}", next.events);
        let end = p.line("2 runs, 2 assertions, 0 failures, 0 errors, 0 skips");
        assert_eq!(end.events.len(), 4);
        assert!(p.finish().is_empty());
    }

    #[test]
    fn spec_names_and_tails() {
        assert_eq!(display_name("test_0001_adds items"), "adds items");
        assert_eq!(display_name("test_total"), "test_total");
        assert_eq!(
            start_line("Cart::#total#test_0002_sums = 0.00 s = ."),
            Some(("Cart::#total", "test_0002_sums", "0.00 s = ."))
        );
        assert_eq!(start_line("Finished in 0.01s"), None);
        assert_eq!(
            result_tail("hi0.25 s = E"),
            Some(("hi", 0.25, Outcome::Error))
        );
        assert_eq!(result_tail("plain text."), None);
    }
}
