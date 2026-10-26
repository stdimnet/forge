//! TeamCity service messages (`phpunit --teamcity`, Pest, Jest reporters) turned into the
//! test tree of the Run tool window. The tree lives here; the UI receives changed nodes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::ansi::{AnsiParser, Style};
use crate::console::{ConsoleLine, ConsoleModel};
use crate::links::LinkContext;
use crate::testevents::TestEvent;

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
    // `php_qn://`, `file://`, `pest_qn://`, `python://`, `go_qn://`, … (Windows drive
    // letters such as `C:/x` have no `//` after the colon).
    let rest = match hint.find("://") {
        Some(i)
            if i > 1
                && hint[..i]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') =>
        {
            &hint[i + 3..]
        }
        _ => hint,
    };
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
    rules: Vec<(regex_lite::Regex, String)>,
    /// The file of a single-file run: tests reported without a location belong to it.
    default_file: Option<String>,
}

/// Rewrites a reported location path before it is resolved in the project (a test
/// framework's "location hints back to files"): `pattern` is a regular expression,
/// `replacement` may use `$1`, `${name}`. The first matching rule wins.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationRule {
    pub pattern: String,
    pub replacement: String,
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
            rules: Vec::new(),
            default_file: None,
        }
    }

    /// Tests reported without a location get `file` (with their name as member), so the
    /// results of a single-file run reach the file's gutter icons.
    pub fn with_default_file(mut self, file: Option<String>) -> Self {
        self.default_file = file.filter(|f| !f.trim().is_empty());
        self
    }

    fn default_location(&self, name: &str) -> Option<TestLocation> {
        let file = self.default_file.as_ref()?;
        self.location(Some(&TestLocation {
            path: file.clone(),
            member: Some(name.to_string()),
            ..TestLocation::default()
        }))
    }

    /// Location rules applied to reported paths (invalid patterns are ignored).
    pub fn with_location_rules(mut self, rules: &[LocationRule]) -> Self {
        self.rules = rules
            .iter()
            .filter_map(|r| {
                regex_lite::Regex::new(&r.pattern)
                    .ok()
                    .map(|re| (re, r.replacement.clone()))
            })
            .collect();
        self
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

    /// A reported location with the location rules applied and its project path resolved.
    fn location(&self, loc: Option<&TestLocation>) -> Option<TestLocation> {
        let mut loc = loc?.clone();
        if let Some((re, to)) = self.rules.iter().find(|(re, _)| re.is_match(&loc.path)) {
            loc.path = re.replace(&loc.path, to.as_str()).into_owned();
        }
        loc.workspace_path = self.links.resolve(&loc.path);
        Some(loc)
    }

    /// Apply one TeamCity message; returns whether the tree changed.
    pub fn apply(&mut self, msg: &ServiceMessage) -> bool {
        match TestEvent::from_service_message(msg) {
            Some(e) => self.apply_event(&e),
            None => false,
        }
    }

    /// Apply one event; returns whether the tree changed.
    pub fn apply_event(&mut self, event: &TestEvent) -> bool {
        match event {
            TestEvent::Count { count } => {
                self.expected_count = Some(*count).filter(|&c| c > 0);
                true
            }
            TestEvent::SuiteStarted { name, location } => {
                let id = self.add(name, TestNodeKind::Suite, self.current);
                self.nodes[id].location = self.location(location.as_ref());
                self.current = id;
                true
            }
            TestEvent::SuiteFinished { .. } => {
                if self.current != 0 {
                    let c = self.current;
                    self.nodes[c].status = Some(self.suite_status(c));
                    self.nodes[c].duration = self.sum_durations(c);
                    self.touch(c);
                    self.current = self.nodes[c].parent.unwrap_or(0);
                }
                true
            }
            TestEvent::TestStarted { name, location } => {
                let id = self.add(name, TestNodeKind::Test, self.current);
                self.nodes[id].location = self
                    .location(location.as_ref())
                    .or_else(|| self.default_location(name));
                self.running.insert(name.to_string(), id);
                true
            }
            TestEvent::TestFailed {
                name,
                message,
                details,
                expected,
                actual,
                location,
            } => {
                let Some(t) = self.find(name) else {
                    return false;
                };
                // A failure spot (a stack frame) is more precise than a location without a line.
                if self.nodes[t]
                    .location
                    .as_ref()
                    .map_or(true, |l| l.line.is_none())
                {
                    if let Some(loc) = self.location(location.as_ref()) {
                        self.nodes[t].location = Some(loc);
                    }
                }
                let n = &mut self.nodes[t];
                n.status = Some(TestStatus::Failed);
                n.message = Some(message.clone());
                n.details = details.clone();
                if expected.is_some() || actual.is_some() {
                    n.expected = expected.clone();
                    n.actual = actual.clone();
                }
                self.touch(t);
                true
            }
            TestEvent::TestIgnored {
                name,
                message,
                location,
            } => {
                let t = match self.find(name) {
                    Some(t) => t,
                    None => {
                        let t = self.add(name, TestNodeKind::Test, self.current);
                        self.nodes[t].location = self
                            .location(location.as_ref())
                            .or_else(|| self.default_location(name));
                        t
                    }
                };
                self.nodes[t].status = Some(TestStatus::Ignored);
                self.nodes[t].message = message.clone();
                self.touch(t);
                true
            }
            TestEvent::TestOutput { name, text, .. } => {
                let Some(t) = self.find(name) else {
                    return false;
                };
                self.nodes[t].output.push_str(text);
                self.touch(t);
                true
            }
            TestEvent::TestFinished { name, duration } => {
                let Some(t) = self.find(name) else {
                    return false;
                };
                let n = &mut self.nodes[t];
                if n.status() == TestStatus::Running {
                    n.status = Some(TestStatus::Passed);
                }
                if let Some(d) = duration {
                    n.duration = Some(*d);
                }
                self.running.remove(name.as_str());
                self.touch(t);
                true
            }
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

/// A node of a finished tree as the test history stores it: only what the tree needs to be
/// shown again (console lines are rebuilt from `output` and the failure).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredNode {
    pub name: String,
    pub kind: TestNodeKind,
    pub status: TestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<StoredNode>,
}

impl TestTree {
    /// The tree in its stored form (the root node with everything below it).
    pub fn export(&self) -> StoredNode {
        fn node(t: &TestTree, n: usize) -> StoredNode {
            let x = &t.nodes[n];
            StoredNode {
                name: x.name.clone(),
                kind: x.kind(),
                status: t.display_status(n),
                duration: x.duration,
                location: x.location.clone(),
                message: x.message.clone(),
                details: x.details.clone(),
                expected: x.expected.clone(),
                actual: x.actual.clone(),
                output: x.output.clone(),
                children: x.children.iter().map(|&c| node(t, c)).collect(),
            }
        }
        node(self, 0)
    }

    /// A finished tree from its stored form (Test History).
    pub fn import(root: &StoredNode, links: LinkContext) -> Self {
        let mut tree = TestTree::new(&root.name, links);
        fn add(t: &mut TestTree, s: &StoredNode, parent: Option<usize>) {
            let id = match parent {
                Some(p) => t.add(&s.name, s.kind, p),
                None => 0,
            };
            let n = &mut t.nodes[id];
            n.kind = Some(s.kind);
            n.status = Some(s.status);
            n.duration = s.duration;
            n.location = s.location.clone();
            n.message = s.message.clone();
            n.details = s.details.clone();
            n.expected = s.expected.clone();
            n.actual = s.actual.clone();
            n.output = s.output.clone();
            for c in &s.children {
                add(t, c, Some(id));
            }
        }
        add(&mut tree, root, None);
        tree.finished = true;
        tree
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
    fn exported_trees_import_with_the_same_nodes() {
        let mut b = TestTree::new("Test Results", LinkContext::new(Some("/p"), None));
        for l in [
            "##teamcity[testSuiteStarted name='S' locationHint='php_qn:///p/STest.php::\\S']",
            "##teamcity[testStarted name='a']",
            "##teamcity[testStdOut name='a' out='hello|n']",
            "##teamcity[testFinished name='a' duration='5']",
            "##teamcity[testStarted name='b']",
            "##teamcity[testFailed name='b' message='nope' expected='1' actual='2']",
            "##teamcity[testFinished name='b' duration='7']",
            "##teamcity[testSuiteFinished name='S']",
        ] {
            b.apply(&parse_service_message(l).unwrap());
        }
        b.finish(Some(1));
        let stored = b.export();
        let json = serde_json::to_string(&stored).unwrap();
        let back: StoredNode = serde_json::from_str(&json).unwrap();
        let again = TestTree::import(&back, LinkContext::new(Some("/p"), None));
        assert_eq!(again.snapshot(), b.snapshot());
        assert_eq!(again.summary(), b.summary());
    }

    #[test]
    fn location_rules_and_default_files() {
        let mut t = TestTree::new("Test Results", LinkContext::new(Some("/p"), None))
            .with_location_rules(&[LocationRule {
                pattern: "^example\\.com/shop/".into(),
                replacement: "".into(),
            }])
            .with_default_file(Some("spec/cart_spec.lua".into()));
        t.apply_event(&TestEvent::TestStarted {
            name: "Cart adds".into(),
            location: None,
        });
        t.apply_event(&TestEvent::TestStarted {
            name: "TestA".into(),
            location: Some(TestLocation {
                path: "example.com/shop/cart".into(),
                ..TestLocation::default()
            }),
        });
        let nodes = t.snapshot();
        let loc = nodes[1].location.as_ref().unwrap();
        assert_eq!(
            (loc.workspace_path.as_deref(), loc.member.as_deref()),
            (Some("spec/cart_spec.lua"), Some("Cart adds"))
        );
        assert_eq!(
            nodes[2]
                .location
                .as_ref()
                .unwrap()
                .workspace_path
                .as_deref(),
            Some("cart")
        );
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
