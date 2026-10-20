//! TeamCity service messages (`phpunit --teamcity`, Pest, Jest reporters) turned into the
//! test tree of the Run tool window. The tree lives here; the UI receives changed nodes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::ansi::{AnsiParser, Style};
use crate::console::{ConsoleLine, ConsoleModel};
use crate::links::LinkContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceMessage {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
}

const PREFIX: &str = "##teamcity[";

/// Undo TeamCity escaping: `|'` `|n` `|r` `|[` `|]` `||` `|0xNNNN`.
pub fn unescape_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '|' {
            out.push(c);
            continue;
        }
        let Some(n) = chars.next() else {
            out.push('|');
            break;
        };
        match n {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            '0' if chars.peek() == Some(&'x') => {
                let rest: String = chars.clone().skip(1).take(4).collect();
                match u32::from_str_radix(&rest, 16)
                    .ok()
                    .filter(|_| rest.len() == 4)
                {
                    Some(code) => {
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        for _ in 0..5 {
                            chars.next();
                        }
                    }
                    None => out.push('0'),
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// Parse one line; `None` when it is not a service message.
pub fn parse_service_message(line: &str) -> Option<ServiceMessage> {
    let body = &line[line.find(PREFIX)? + PREFIX.len()..];
    let name_len = body.find(|c| !is_name_char(c)).unwrap_or(body.len());
    if name_len == 0 {
        return None;
    }
    let mut attrs = BTreeMap::new();
    let rest = &body[name_len..];
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // key='value' with `|` escapes inside the value
        let key_start = i;
        while i < b.len() && is_name_char(b[i] as char) {
            i += 1;
        }
        if i > key_start && rest[i..].starts_with("='") {
            let key = &rest[key_start..i];
            let mut j = i + 2;
            while j < b.len() && b[j] != b'\'' {
                j += if b[j] == b'|' { 2 } else { 1 };
            }
            if j < b.len() {
                attrs.insert(key.to_string(), unescape_value(&rest[i + 2..j]));
                i = j + 1;
                continue;
            }
            break;
        }
        i = if i > key_start { i } else { i + 1 };
    }
    Some(ServiceMessage {
        name: body[..name_len].to_string(),
        attrs,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Running,
    Passed,
    Failed,
    Ignored,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestNodeKind {
    Root,
    Suite,
    Test,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestLocation {
    /// File path as reported (absolute or relative).
    pub path: String,
    /// The path relative to the project root when it is inside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    /// Class or method name after `::`, used to find the line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// `php_qn:///abs/tests/UserTest.php::\Tests\UserTest::test_x`, `file:///…:12`, ...
pub fn parse_location_hint(hint: Option<&str>) -> Option<TestLocation> {
    let hint = hint.filter(|h| !h.is_empty())?;
    let rest = ["php_qn://", "file://", "pest_qn://", "python://"]
        .iter()
        .find_map(|p| hint.strip_prefix(p))
        .unwrap_or(hint);
    let mut parts = rest.split("::");
    let path = parts.next().unwrap_or_default();
    let mut loc = match path.rsplit_once(':') {
        Some((p, l)) if !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()) => TestLocation {
            path: p.to_string(),
            line: l.parse().ok(),
            ..TestLocation::default()
        },
        _ => TestLocation {
            path: path.to_string(),
            ..TestLocation::default()
        },
    };
    if let Some(member) = parts.last().filter(|m| !m.is_empty()) {
        let m = member.strip_prefix('\\').unwrap_or(member);
        let m = m.find(" with data set ").map_or(m, |i| &m[..i]);
        loc.member = Some(m.to_string());
    }
    Some(loc)
}

/// A node as the UI renders it. `status` of suites is derived from their children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestNode {
    pub id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<u32>,
    pub name: String,
    pub kind: TestNodeKind,
    pub status: TestStatus,
    /// Milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    /// "1 s 245 ms", empty without a duration.
    pub duration_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<TestLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    /// Console content of a test: its output plus the failure with expected/actual.
    pub console: Vec<ConsoleLine>,
}

#[derive(Debug, Clone, Default)]
struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    name: String,
    kind: Option<TestNodeKind>,
    status: Option<TestStatus>,
    duration: Option<u64>,
    location: Option<TestLocation>,
    message: Option<String>,
    details: Option<String>,
    expected: Option<String>,
    actual: Option<String>,
    output: String,
}

impl Node {
    fn kind(&self) -> TestNodeKind {
        self.kind.unwrap_or(TestNodeKind::Test)
    }
    fn status(&self) -> TestStatus {
        self.status.unwrap_or(TestStatus::Running)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestCounts {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
    pub running: u32,
}

/// The status line of the test runner: "Tests failed: 1, passed: 3 of 4 tests – 51 ms".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSummary {
    pub text: String,
    pub failed: bool,
    pub done: u32,
    pub total: u32,
    pub counts: TestCounts,
    /// Number of tests announced by `testCount`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_count: Option<u32>,
}

/// Builds and updates a test tree from service messages.
#[derive(Debug)]
pub struct TestTree {
    nodes: Vec<Node>,
    current: usize,
    running: HashMap<String, usize>,
    expected_count: Option<u32>,
    finished: bool,
    dirty: BTreeSet<usize>,
    links: LinkContext,
}

impl TestTree {
    pub fn new(root_name: &str, links: LinkContext) -> Self {
        let root = Node {
            name: root_name.to_string(),
            kind: Some(TestNodeKind::Root),
            ..Node::default()
        };
        TestTree {
            nodes: vec![root],
            current: 0,
            running: HashMap::new(),
            expected_count: None,
            finished: false,
            dirty: BTreeSet::from([0]),
            links,
        }
    }

    fn add(&mut self, name: &str, kind: TestNodeKind, parent: usize) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            parent: Some(parent),
            name: name.to_string(),
            kind: Some(kind),
            ..Node::default()
        });
        self.nodes[parent].children.push(id);
        self.touch(id);
        id
    }

    /// Mark a node and its ancestors (whose status derives from it) as changed.
    fn touch(&mut self, mut id: usize) {
        loop {
            self.dirty.insert(id);
            match self.nodes[id].parent {
                Some(p) => id = p,
                None => break,
            }
        }
    }

    fn location(&self, hint: Option<&String>) -> Option<TestLocation> {
        let mut loc = parse_location_hint(hint.map(String::as_str))?;
        loc.workspace_path = self.links.resolve(&loc.path);
        Some(loc)
    }

    /// Apply one message; returns whether the tree changed.
    pub fn apply(&mut self, msg: &ServiceMessage) -> bool {
        let a = &msg.attrs;
        let name = a.get("name").map(String::as_str).unwrap_or_default();
        match msg.name.as_str() {
            "testCount" => {
                self.expected_count = a
                    .get("count")
                    .and_then(|c| c.parse().ok())
                    .filter(|&c| c > 0);
                true
            }
            "testSuiteStarted" => {
                let id = self.add(name, TestNodeKind::Suite, self.current);
                self.nodes[id].location = self.location(a.get("locationHint"));
                self.current = id;
                true
            }
            "testSuiteFinished" => {
                if self.current != 0 {
                    let c = self.current;
                    self.nodes[c].status = Some(self.suite_status(c));
                    self.nodes[c].duration = self.sum_durations(c);
                    self.touch(c);
                    self.current = self.nodes[c].parent.unwrap_or(0);
                }
                true
            }
            "testStarted" => {
                let id = self.add(name, TestNodeKind::Test, self.current);
                self.nodes[id].location = self.location(a.get("locationHint"));
                self.running.insert(name.to_string(), id);
                true
            }
            "testFailed" => {
                let Some(t) = self.find(name) else {
                    return false;
                };
                let n = &mut self.nodes[t];
                n.status = Some(TestStatus::Failed);
                n.message = a.get("message").cloned();
                n.details = a.get("details").cloned();
                if a.contains_key("expected") || a.contains_key("actual") {
                    n.expected = a.get("expected").cloned();
                    n.actual = a.get("actual").cloned();
                }
                self.touch(t);
                true
            }
            "testIgnored" => {
                let t = match self.find(name) {
                    Some(t) => t,
                    None => self.add(name, TestNodeKind::Test, self.current),
                };
                self.nodes[t].status = Some(TestStatus::Ignored);
                self.nodes[t].message = a.get("message").cloned();
                self.touch(t);
                true
            }
            "testStdOut" | "testStdErr" => {
                let Some(t) = self.find(name) else {
                    return false;
                };
                self.nodes[t]
                    .output
                    .push_str(a.get("out").map(String::as_str).unwrap_or_default());
                self.touch(t);
                true
            }
            "testFinished" => {
                let Some(t) = self.find(name) else {
                    return false;
                };
                let n = &mut self.nodes[t];
                if n.status() == TestStatus::Running {
                    n.status = Some(TestStatus::Passed);
                }
                if let Some(d) = a.get("duration").and_then(|d| d.parse::<f64>().ok()) {
                    n.duration = Some(d.max(0.0).round() as u64);
                }
                self.running.remove(name);
                self.touch(t);
                true
            }
            _ => false,
        }
    }

    fn find(&self, name: &str) -> Option<usize> {
        if let Some(&id) = self.running.get(name) {
            return Some(id);
        }
        self.nodes[self.current]
            .children
            .iter()
            .rev()
            .copied()
            .find(|&c| self.nodes[c].kind() == TestNodeKind::Test && self.nodes[c].name == name)
    }

    /// The test that is running now (output is attributed to it).
    pub fn running_test(&self) -> Option<usize> {
        fn last(t: &TestTree, n: usize) -> Option<usize> {
            for &c in t.nodes[n].children.iter().rev() {
                let node = &t.nodes[c];
                if node.kind() == TestNodeKind::Test && node.status() == TestStatus::Running {
                    return Some(c);
                }
                if let Some(inner) = last(t, c) {
                    return Some(inner);
                }
            }
            None
        }
        last(self, 0)
    }

    /// Output printed while a test ran.
    pub fn add_output(&mut self, test: usize, text: &str) {
        if let Some(n) = self.nodes.get_mut(test) {
            n.output.push_str(text);
            self.touch(test);
        }
    }

    /// Close the tree when the process ends: unfinished tests become passed or failed by the
    /// exit code.
    pub fn finish(&mut self, exit_code: Option<i32>) {
        fn close(t: &mut TestTree, n: usize, ok: bool) {
            for c in t.nodes[n].children.clone() {
                close(t, c, ok);
            }
            let fallback = if ok {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            };
            if t.nodes[n].kind() == TestNodeKind::Test {
                if t.nodes[n].status() == TestStatus::Running {
                    t.nodes[n].status = Some(fallback);
                }
            } else {
                let status = if t.nodes[n].children.is_empty() {
                    fallback
                } else {
                    t.suite_status(n)
                };
                t.nodes[n].status = Some(status);
                t.nodes[n].duration = t.sum_durations(n);
            }
            t.dirty.insert(n);
        }
        close(self, 0, exit_code == Some(0));
        self.finished = true;
    }

    fn suite_status(&self, n: usize) -> TestStatus {
        let kids: Vec<TestStatus> = self.nodes[n]
            .children
            .iter()
            .map(|&c| {
                if self.nodes[c].kind() == TestNodeKind::Test {
                    self.nodes[c].status()
                } else {
                    self.suite_status(c)
                }
            })
            .collect();
        if kids.contains(&TestStatus::Failed) {
            TestStatus::Failed
        } else if kids.contains(&TestStatus::Running) {
            TestStatus::Running
        } else if !kids.is_empty() && kids.iter().all(|s| *s == TestStatus::Ignored) {
            TestStatus::Ignored
        } else {
            TestStatus::Passed
        }
    }

    /// Status shown for a node: suites with children derive it from them.
    fn display_status(&self, n: usize) -> TestStatus {
        let node = &self.nodes[n];
        if node.kind() == TestNodeKind::Test || node.children.is_empty() {
            node.status()
        } else {
            self.suite_status(n)
        }
    }

    fn sum_durations(&self, n: usize) -> Option<u64> {
        if self.nodes[n].kind() == TestNodeKind::Test {
            return self.nodes[n].duration;
        }
        let mut any = false;
        let mut sum = 0;
        for &c in &self.nodes[n].children {
            if let Some(d) = self.sum_durations(c) {
                sum += d;
                any = true;
            }
        }
        any.then_some(sum)
    }

    pub fn counts(&self) -> TestCounts {
        let mut c = TestCounts::default();
        for n in &self.nodes {
            if n.kind() != TestNodeKind::Test {
                continue;
            }
            c.total += 1;
            match n.status() {
                TestStatus::Passed => c.passed += 1,
                TestStatus::Failed => c.failed += 1,
                TestStatus::Ignored => c.ignored += 1,
                TestStatus::Running | TestStatus::Pending => c.running += 1,
            }
        }
        c
    }

    /// PHPStorm's status text: "Tests failed: 1, passed: 3 of 4 tests – 51 ms".
    pub fn summary(&self) -> TestSummary {
        let c = self.counts();
        let running = !self.finished;
        let prefix = if running {
            "Running tests"
        } else if c.failed > 0 {
            "Tests failed"
        } else if c.passed > 0 {
            "Tests passed"
        } else if c.ignored > 0 {
            "Tests ignored"
        } else {
            "No tests were found"
        };
        let mut parts = Vec::new();
        if c.failed > 0 {
            parts.push(if running {
                format!("failed: {}", c.failed)
            } else {
                c.failed.to_string()
            });
        }
        if c.passed > 0 {
            parts.push(if c.failed > 0 || running {
                format!("passed: {}", c.passed)
            } else {
                c.passed.to_string()
            });
        }
        if c.ignored > 0 {
            parts.push(if c.failed > 0 || c.passed > 0 || running {
                format!("ignored: {}", c.ignored)
            } else {
                c.ignored.to_string()
            });
        }
        let tail = if c.total > 0 {
            format!(
                " of {} test{}",
                c.total,
                if c.total == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        };
        let time = match self.nodes[0].duration {
            Some(d) if !running => format!(" – {}", format_duration(Some(d))),
            _ => String::new(),
        };
        let text = if parts.is_empty() {
            format!("{prefix}{}", if running { "…" } else { "" })
        } else {
            format!("{prefix}: {}{tail}{time}", parts.join(", "))
        };
        TestSummary {
            text,
            failed: c.failed > 0,
            done: c.passed + c.failed + c.ignored,
            total: c.total,
            counts: c,
            expected_count: self.expected_count,
        }
    }

    fn console_of(&self, n: usize) -> Vec<ConsoleLine> {
        let node = &self.nodes[n];
        let mut lines = Vec::new();
        if !node.output.is_empty() {
            let mut model = ConsoleModel::new(self.links.clone());
            let out = node.output.strip_suffix('\n').unwrap_or(&node.output);
            model
                .append(&AnsiParser::new().feed(out), None)
                .apply_to(&mut lines);
        }
        let danger = Style::fg("var(--danger)");
        let links = &self.links;
        let mut add = |text: &str, style: Option<&Style>| {
            for t in text.split('\n') {
                lines.push(ConsoleLine::plain(
                    t,
                    style.cloned().unwrap_or_default(),
                    links,
                ));
            }
        };
        if node.status() == TestStatus::Failed {
            if let Some(message) = &node.message {
                add(message, Some(&danger));
                if node.expected.is_some() || node.actual.is_some() {
                    add(
                        &format!("Expected :{}", node.expected.as_deref().unwrap_or_default()),
                        Some(&danger),
                    );
                    add(
                        &format!("Actual   :{}", node.actual.as_deref().unwrap_or_default()),
                        Some(&danger),
                    );
                }
            }
        }
        if node.status() == TestStatus::Ignored {
            if let Some(message) = node.message.as_deref().filter(|m| !m.is_empty()) {
                add(
                    &format!("Test ignored: {message}"),
                    Some(&Style::fg("var(--fg-muted)")),
                );
            }
        }
        if let Some(details) = &node.details {
            add("", None);
            add(details.trim(), Some(&danger));
        }
        lines
    }

    fn view(&self, n: usize) -> TestNode {
        let node = &self.nodes[n];
        TestNode {
            id: n as u32,
            parent_id: node.parent.map(|p| p as u32),
            name: node.name.clone(),
            kind: node.kind(),
            status: self.display_status(n),
            duration: node.duration,
            duration_text: format_duration(node.duration),
            location: node.location.clone(),
            message: node.message.clone(),
            details: node.details.clone(),
            expected: node.expected.clone(),
            actual: node.actual.clone(),
            console: if node.kind() == TestNodeKind::Test {
                self.console_of(n)
            } else {
                Vec::new()
            },
        }
    }

    /// Nodes changed since the last call, parents before children.
    pub fn take_updates(&mut self) -> Vec<TestNode> {
        let dirty = std::mem::take(&mut self.dirty);
        dirty.into_iter().map(|n| self.view(n)).collect()
    }

    /// Every node, parents before children.
    pub fn snapshot(&self) -> Vec<TestNode> {
        (0..self.nodes.len()).map(|n| self.view(n)).collect()
    }
}

/// "1 s 245 ms", "87 ms" like PHPStorm's test durations; empty without a duration.
pub fn format_duration(ms: Option<u64>) -> String {
    let Some(ms) = ms else {
        return String::new();
    };
    if ms < 1000 {
        return format!("{ms} ms");
    }
    let s = ms / 1000;
    let rest = ms % 1000;
    if s < 60 {
        return if rest > 0 {
            format!("{s} s {rest} ms")
        } else {
            format!("{s} s")
        };
    }
    format!("{} m {} s", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_service_messages_with_escapes() {
        let m = parse_service_message(
            "##teamcity[testFailed name='test_a' message='Failed asserting that |'a|' is |'b|'.' details=' /p/A.php:21|n' flowId='7']",
        )
        .unwrap();
        assert_eq!(m.name, "testFailed");
        assert_eq!(m.attrs["name"], "test_a");
        assert_eq!(m.attrs["message"], "Failed asserting that 'a' is 'b'.");
        assert_eq!(m.attrs["details"], " /p/A.php:21\n");
        assert_eq!(m.attrs["flowId"], "7");
        assert_eq!(m.attrs.len(), 4);
        assert!(parse_service_message("plain output").is_none());
        assert_eq!(unescape_value("|[x|]|||0x00e9"), "[x]|é");
    }

    #[test]
    fn parses_location_hints() {
        assert_eq!(
            parse_location_hint(Some(
                "php_qn:///p/tests/UserTest.php::\\Tests\\UserTest::test_a"
            )),
            Some(TestLocation {
                path: "/p/tests/UserTest.php".into(),
                member: Some("test_a".into()),
                ..TestLocation::default()
            })
        );
        assert_eq!(
            parse_location_hint(Some("file:///p/a.test.ts:12")),
            Some(TestLocation {
                path: "/p/a.test.ts".into(),
                line: Some(12),
                ..TestLocation::default()
            })
        );
        assert_eq!(
            parse_location_hint(Some("php_qn:///p/T.php::\\T::test_x with data set #1"))
                .unwrap()
                .member
                .as_deref(),
            Some("test_x")
        );
    }

    #[test]
    fn builds_a_tree_with_statuses_and_durations() {
        let mut b = TestTree::new("Test Results", LinkContext::new(Some("/p"), None));
        let lines = [
            "##teamcity[testCount count='3']",
            "##teamcity[testSuiteStarted name='Tests\\Unit\\UserTest' locationHint='php_qn:///p/UserTest.php::\\Tests\\Unit\\UserTest']",
            "##teamcity[testStarted name='test_ok' locationHint='php_qn:///p/UserTest.php::\\Tests\\Unit\\UserTest::test_ok']",
            "##teamcity[testFinished name='test_ok' duration='12']",
            "##teamcity[testStarted name='test_bad']",
            "##teamcity[testFailed name='test_bad' message='nope' type='comparisonFailure' expected='a' actual='b']",
            "##teamcity[testFinished name='test_bad' duration='3']",
            "##teamcity[testIgnored name='test_skip' message='later']",
            "##teamcity[testSuiteFinished name='Tests\\Unit\\UserTest']",
        ];
        for l in lines {
            b.apply(&parse_service_message(l).unwrap());
        }
        assert_eq!(
            b.summary().text,
            "Running tests: failed: 1, passed: 1, ignored: 1 of 3 tests"
        );
        b.finish(Some(1));
        let nodes = b.snapshot();
        let suite = &nodes[1];
        assert_eq!(b.summary().expected_count, Some(3));
        assert_eq!(suite.name, "Tests\\Unit\\UserTest");
        assert_eq!(suite.status, TestStatus::Failed);
        assert_eq!(suite.duration, Some(15));
        assert_eq!(
            suite.location.as_ref().unwrap().workspace_path.as_deref(),
            Some("UserTest.php")
        );
        let kids: Vec<_> = nodes[2..]
            .iter()
            .map(|n| (n.name.as_str(), n.status, n.parent_id))
            .collect();
        assert_eq!(
            kids,
            [
                ("test_ok", TestStatus::Passed, Some(1)),
                ("test_bad", TestStatus::Failed, Some(1)),
                ("test_skip", TestStatus::Ignored, Some(1))
            ]
        );
        assert_eq!(
            (nodes[3].expected.as_deref(), nodes[3].actual.as_deref()),
            (Some("a"), Some("b"))
        );
        let console: Vec<String> = nodes[3].console.iter().map(ConsoleLine::text).collect();
        assert_eq!(console, ["nope", "Expected :a", "Actual   :b"]);
        let c = b.counts();
        assert_eq!((c.total, c.passed, c.failed, c.ignored), (3, 1, 1, 1));
        assert_eq!(nodes[0].status, TestStatus::Failed);
        assert_eq!(
            b.summary().text,
            "Tests failed: 1, passed: 1, ignored: 1 of 3 tests – 15 ms"
        );
    }

    #[test]
    fn reports_only_changed_nodes() {
        let mut b = TestTree::new("Test Results", LinkContext::default());
        b.apply(&parse_service_message("##teamcity[testSuiteStarted name='S']").unwrap());
        assert_eq!(b.take_updates().len(), 2);
        b.apply(&parse_service_message("##teamcity[testStarted name='t']").unwrap());
        let ids: Vec<u32> = b.take_updates().iter().map(|n| n.id).collect();
        assert_eq!(ids, [0, 1, 2]);
        assert!(b.take_updates().is_empty());
    }

    #[test]
    fn formats_durations_like_phpstorm() {
        assert_eq!(format_duration(Some(87)), "87 ms");
        assert_eq!(format_duration(Some(1245)), "1 s 245 ms");
        assert_eq!(format_duration(Some(2000)), "2 s");
        assert_eq!(format_duration(Some(61_000)), "1 m 1 s");
        assert_eq!(format_duration(None), "");
    }
}
