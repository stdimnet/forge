//! JUnit XML reports (Codeception `--xml`, Behat `--format=junit`, PHPSpec `--format=junit`,
//! PHPUnit `--log-junit`) turned into the TeamCity service messages that build the test
//! tree, so a report read after the run shows the same tree as a streamed TeamCity run.
//!
//! - `<testsuite>` elements become suites; test cases of one class inside a suite that is not
//!   that class (Codeception's suites) get a suite per class.
//! - `<failure>` and `<error>` fail a test, `<skipped>` (and the `status` attribute of Behat
//!   and PHPSpec: `skipped`, `pending`, `undefined`, `broken`) ignore or fail it.
//! - Locations come from the `file`/`line` attributes; PHPSpec reports only the spec class,
//!   so its file is derived (`spec\Acme\MarkdownSpec` → `spec/Acme/MarkdownSpec.php`) and the
//!   example name is the method (`it converts text` → `it_converts_text`).

use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use serde::{Deserialize, Serialize};

use crate::teamcity::ServiceMessage;

/// Which tool wrote the report; it decides how locations are derived.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JUnitFlavor {
    /// PHPUnit and Codeception: `class`, `file` and `line` attributes.
    #[default]
    Generic,
    /// Behat: suites are features, test cases are scenarios.
    Behat,
    /// PHPSpec: `classname` is the spec class, names are example descriptions.
    PhpSpec,
}

#[derive(Debug, Clone, Default)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Element>,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn attr_nonempty(&self, name: &str) -> Option<&str> {
        self.attr(name).map(str::trim).filter(|v| !v.is_empty())
    }
}

fn element_from(start: &BytesStart<'_>) -> Result<Element, String> {
    let mut el = Element {
        name: start.name().as_ref().to_string(),
        ..Element::default()
    };
    for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?;
        el.attrs
            .push((attr.key.as_ref().to_string(), value.into_owned()));
    }
    Ok(el)
}

fn entity(name: &str) -> Option<char> {
    Some(match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "apos" => '\'',
        "quot" => '"',
        _ => return None,
    })
}

fn parse_dom(xml: &str) -> Result<Element, String> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let done = |stack: &mut Vec<Element>, root: &mut Option<Element>, el: Element| match stack
        .last_mut()
    {
        Some(parent) => parent.children.push(el),
        None => {
            root.get_or_insert(el);
        }
    };
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(s) => stack.push(element_from(&s)?),
            Event::Empty(s) => {
                let el = element_from(&s)?;
                done(&mut stack, &mut root, el);
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("unbalanced end tag")?;
                done(&mut stack, &mut root, el);
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            Event::CData(c) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&c.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    let ch = r
                        .resolve_char_ref()
                        .map_err(|e| e.to_string())?
                        .or_else(|| entity(&r));
                    match ch {
                        Some(ch) => top.text.push(ch),
                        None => {
                            top.text.push('&');
                            top.text.push_str(&r);
                            top.text.push(';');
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("unexpected end of the report".into());
    }
    root.ok_or_else(|| "empty report".into())
}

fn msg(name: &str, attrs: &[(&str, String)]) -> ServiceMessage {
    ServiceMessage {
        name: name.to_string(),
        attrs: attrs
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// `"0.0123"` seconds → `"12"` milliseconds.
fn millis(time: Option<&str>) -> String {
    time.and_then(|t| t.trim().parse::<f64>().ok())
        .map(|s| ((s * 1000.0).max(0.0).round() as u64).to_string())
        .unwrap_or_default()
}

/// `it converts plain text` → `it_converts_plain_text`.
pub fn phpspec_method(example: &str) -> String {
    example.trim().replace(' ', "_")
}

/// `spec\Acme\MarkdownSpec` → `spec/Acme/MarkdownSpec.php`.
pub fn phpspec_file(class: &str) -> String {
    format!("{}.php", class.trim_start_matches('\\').replace('\\', "/"))
}

/// The expected and actual values of a PHPUnit failure diff (`--- Expected` / `+++ Actual`).
fn comparison(text: &str) -> Option<(String, String)> {
    let start = text.find("--- Expected")?;
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for line in text[start..].lines().skip(1) {
        if line.starts_with("+++") || line.starts_with("@@") {
            continue;
        }
        if let Some(e) = line.strip_prefix('-') {
            expected.push(e);
        } else if let Some(a) = line.strip_prefix('+') {
            actual.push(a);
        } else if let Some(both) = line.strip_prefix(' ') {
            expected.push(both);
            actual.push(both);
        } else {
            break;
        }
    }
    (!expected.is_empty() || !actual.is_empty()).then(|| (expected.join("\n"), actual.join("\n")))
}

struct Converter {
    flavor: JUnitFlavor,
    out: Vec<ServiceMessage>,
}

impl Converter {
    fn location(&self, case: &Element, suite_file: Option<&str>) -> String {
        let class = case
            .attr_nonempty("class")
            .or(case.attr_nonempty("classname"));
        let name = case.attr("name").unwrap_or_default();
        match self.flavor {
            JUnitFlavor::PhpSpec => match class {
                Some(c) => format!(
                    "php_qn://{}::\\{}::{}",
                    phpspec_file(c),
                    c.trim_start_matches('\\'),
                    phpspec_method(name)
                ),
                None => String::new(),
            },
            JUnitFlavor::Behat => match case.attr_nonempty("file").or(suite_file) {
                Some(f) => format!("file://{f}::{name}"),
                None => String::new(),
            },
            JUnitFlavor::Generic => {
                let Some(file) = case.attr_nonempty("file").or(suite_file) else {
                    return String::new();
                };
                if let Some(line) = case.attr_nonempty("line") {
                    return format!("file://{file}:{line}");
                }
                match class {
                    Some(c) => format!("php_qn://{file}::\\{}::{name}", c.trim_start_matches('\\')),
                    None => format!("file://{file}::{name}"),
                }
            }
        }
    }

    fn case(&mut self, case: &Element, suite_file: Option<&str>) {
        let name = case.attr("name").unwrap_or("test").to_string();
        let hint = self.location(case, suite_file);
        let duration = millis(case.attr("time"));
        let status = case.attr("status").unwrap_or_default();
        let problem = case
            .children
            .iter()
            .find(|c| c.name == "failure" || c.name == "error");
        let skipped = case.children.iter().find(|c| c.name == "skipped");
        if (skipped.is_some() || matches!(status, "skipped" | "pending")) && problem.is_none() {
            let message = skipped
                .and_then(|s| {
                    s.attr_nonempty("message")
                        .map(str::to_string)
                        .or_else(|| Some(s.text.trim().to_string()).filter(|t| !t.is_empty()))
                })
                .unwrap_or_else(|| status.to_string());
            self.out.push(msg(
                "testIgnored",
                &[("name", name), ("message", message), ("locationHint", hint)],
            ));
            return;
        }
        self.out.push(msg(
            "testStarted",
            &[("name", name.clone()), ("locationHint", hint)],
        ));
        let failed_status = matches!(status, "failed" | "broken" | "undefined" | "error");
        if problem.is_some() || failed_status {
            let text = problem
                .map(|p| p.text.trim().to_string())
                .unwrap_or_default();
            // The first line of PHPUnit / Codeception failures names the test.
            let class = case
                .attr_nonempty("class")
                .or(case.attr_nonempty("classname"))
                .unwrap_or("\0");
            let message = problem
                .and_then(|p| p.attr_nonempty("message").map(str::to_string))
                .or_else(|| {
                    text.lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty() && !l.contains("::") && !l.contains(class))
                        .map(str::to_string)
                })
                .or_else(|| text.lines().next().map(str::to_string))
                .unwrap_or_else(|| format!("Test {status}"));
            let mut attrs = vec![
                ("name", name.clone()),
                ("message", message.clone()),
                (
                    "details",
                    if text == message {
                        String::new()
                    } else {
                        text.clone()
                    },
                ),
            ];
            if let Some((expected, actual)) = comparison(&text) {
                attrs.push(("type", "comparisonFailure".into()));
                attrs.push(("expected", expected));
                attrs.push(("actual", actual));
            }
            self.out.push(msg("testFailed", &attrs));
        }
        let out = case
            .children
            .iter()
            .filter(|c| c.name == "system-out")
            .map(|c| c.text.as_str())
            .collect::<String>();
        if !out.trim().is_empty() {
            self.out
                .push(msg("testStdOut", &[("name", name.clone()), ("out", out)]));
        }
        self.out.push(msg(
            "testFinished",
            &[("name", name), ("duration", duration)],
        ));
    }

    fn suite(&mut self, suite: &Element) {
        let name = suite.attr("name").unwrap_or_default().to_string();
        let file = suite.attr_nonempty("file").map(str::to_string);
        let hint = match (&file, self.flavor) {
            (Some(f), JUnitFlavor::Behat) => format!("file://{f}"),
            (Some(f), _) => format!("php_qn://{f}::\\{}", name.trim_start_matches('\\')),
            (None, _) => String::new(),
        };
        let named = !name.is_empty();
        if named {
            self.out.push(msg(
                "testSuiteStarted",
                &[("name", name.clone()), ("locationHint", hint)],
            ));
        }
        // Codeception lists the cases of every class directly in the suite: group them.
        let mut class_suite: Option<String> = None;
        for child in &suite.children {
            match child.name.as_str() {
                "testsuite" => {
                    if let Some(c) = class_suite.take() {
                        self.out.push(msg("testSuiteFinished", &[("name", c)]));
                    }
                    self.suite(child);
                }
                "testcase" => {
                    let class = match self.flavor {
                        JUnitFlavor::Generic => child
                            .attr_nonempty("class")
                            .or(child.attr_nonempty("classname"))
                            .map(|c| c.trim_start_matches('\\').to_string())
                            .filter(|c| {
                                let short = c.rsplit('\\').next().unwrap_or(c);
                                *c != name && short != name
                            }),
                        _ => None,
                    };
                    if class != class_suite {
                        if let Some(c) = class_suite.take() {
                            self.out.push(msg("testSuiteFinished", &[("name", c)]));
                        }
                        if let Some(c) = &class {
                            let hint = child
                                .attr_nonempty("file")
                                .map(|f| format!("php_qn://{f}::\\{c}"))
                                .unwrap_or_default();
                            self.out.push(msg(
                                "testSuiteStarted",
                                &[("name", c.clone()), ("locationHint", hint)],
                            ));
                        }
                        class_suite = class;
                    }
                    self.case(child, file.as_deref());
                }
                _ => {}
            }
        }
        if let Some(c) = class_suite {
            self.out.push(msg("testSuiteFinished", &[("name", c)]));
        }
        if named {
            self.out.push(msg("testSuiteFinished", &[("name", name)]));
        }
    }
}

/// Service messages for a JUnit XML report, in tree order.
pub fn junit_messages(xml: &str, flavor: JUnitFlavor) -> Result<Vec<ServiceMessage>, String> {
    let root = parse_dom(xml)?;
    let mut c = Converter {
        flavor,
        out: Vec::new(),
    };
    match root.name.as_str() {
        "testsuites" => {
            let count = root
                .attr("tests")
                .and_then(|t| t.trim().parse::<u32>().ok());
            if let Some(n) = count {
                c.out.push(msg("testCount", &[("count", n.to_string())]));
            }
            for s in root.children.iter().filter(|s| s.name == "testsuite") {
                c.suite(s);
            }
        }
        "testsuite" => c.suite(&root),
        other => return Err(format!("not a JUnit report: <{other}>")),
    }
    Ok(c.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::LinkContext;
    use crate::teamcity::{TestStatus, TestTree};

    fn tree(xml: &str, flavor: JUnitFlavor) -> TestTree {
        let mut t = TestTree::new("Test Results", LinkContext::new(Some("/p"), None));
        for m in junit_messages(xml, flavor).unwrap() {
            t.apply(&m);
        }
        t.finish(Some(1));
        t
    }

    const CODECEPTION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="Unit" tests="2" assertions="2" errors="0" failures="1" skipped="0" time="0.012">
    <testcase name="testFullName" class="Tests\Unit\UserTest" file="/p/tests/Unit/UserTest.php" line="12" assertions="1" time="0.004"/>
    <testcase name="testDisplayName" class="Tests\Unit\UserTest" file="/p/tests/Unit/UserTest.php" line="20" assertions="1" time="0.008">
      <failure type="PHPUnit\Framework\ExpectationFailedException">Tests\Unit\UserTest::testDisplayName
Failed asserting that two strings are identical.
--- Expected
+++ Actual
@@ @@
-'Amazing Grace'
+'Grace'

/p/tests/Unit/UserTest.php:22</failure>
    </testcase>
  </testsuite>
  <testsuite name="Acceptance" tests="1" time="0.5">
    <testcase name="tryToLogin" class="LoginCest" file="/p/tests/Acceptance/LoginCest.php" feature="try to login" time="0.5"/>
    <testcase name="tryToLogout" class="LoginCest" file="/p/tests/Acceptance/LoginCest.php" time="0.1"><skipped message="Not ready"/></testcase>
  </testsuite>
</testsuites>"#;

    #[test]
    fn codeception_reports_group_cases_by_class() {
        let t = tree(CODECEPTION, JUnitFlavor::Generic);
        let nodes = t.snapshot();
        let names: Vec<(&str, Option<u32>)> = nodes
            .iter()
            .map(|n| (n.name.as_str(), n.parent_id))
            .collect();
        assert_eq!(
            names,
            [
                ("Test Results", None),
                ("Unit", Some(0)),
                ("Tests\\Unit\\UserTest", Some(1)),
                ("testFullName", Some(2)),
                ("testDisplayName", Some(2)),
                ("Acceptance", Some(0)),
                ("LoginCest", Some(5)),
                ("tryToLogin", Some(6)),
                ("tryToLogout", Some(6)),
            ]
        );
        let bad = &nodes[4];
        assert_eq!(bad.status, TestStatus::Failed);
        assert_eq!(
            bad.message.as_deref(),
            Some("Failed asserting that two strings are identical.")
        );
        assert_eq!(bad.expected.as_deref(), Some("'Amazing Grace'"));
        assert_eq!(bad.actual.as_deref(), Some("'Grace'"));
        let loc = bad.location.as_ref().unwrap();
        assert_eq!(
            loc.workspace_path.as_deref(),
            Some("tests/Unit/UserTest.php")
        );
        assert_eq!(loc.line, Some(20));
        assert_eq!(nodes[3].duration, Some(4));
        assert_eq!(nodes[8].status, TestStatus::Ignored);
        assert_eq!(nodes[8].message.as_deref(), Some("Not ready"));
        assert_eq!(
            t.summary().text,
            "Tests failed: 1, passed: 2, ignored: 1 of 4 tests – 512 ms"
        );
    }

    #[test]
    fn behat_reports_scenarios_per_feature() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="default">
  <testsuite name="Checkout" tests="3" skipped="0" failures="1" errors="0" time="0.004" file="features/checkout.feature">
    <testcase name="Buying a single product" classname="Checkout" status="passed" time="0.001"></testcase>
    <testcase name="Buying two products" classname="Checkout" status="failed" time="0.002">
      <failure message="Then I should have 2 products in the basket: Failed asserting that 1 matches expected 2."></failure>
    </testcase>
    <testcase name="Paying with a voucher" classname="Checkout" status="undefined" time="0">
      <error message="And I redeem the voucher &quot;SUMMER&quot;" type="undefined"></error>
    </testcase>
  </testsuite>
</testsuites>"#;
        let t = tree(xml, JUnitFlavor::Behat);
        let nodes = t.snapshot();
        assert_eq!(nodes[1].name, "Checkout");
        assert_eq!(
            nodes[1]
                .location
                .as_ref()
                .unwrap()
                .workspace_path
                .as_deref(),
            Some("features/checkout.feature")
        );
        assert_eq!(nodes[2].status, TestStatus::Passed);
        assert_eq!(nodes[3].status, TestStatus::Failed);
        assert_eq!(
            nodes[3].message.as_deref(),
            Some("Then I should have 2 products in the basket: Failed asserting that 1 matches expected 2.")
        );
        let loc = nodes[3].location.as_ref().unwrap();
        assert_eq!(loc.member.as_deref(), Some("Buying two products"));
        assert_eq!(nodes[4].status, TestStatus::Failed);
        assert_eq!(
            nodes[4].message.as_deref(),
            Some("And I redeem the voucher \"SUMMER\"")
        );
    }

    #[test]
    fn phpspec_reports_derive_spec_files_and_methods() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<testsuites time="0.02" tests="3" failures="1" errors="0">
  <testsuite name="Acme\Markdown" time="0.01" tests="3" failures="1" errors="0" skipped="1">
    <testcase name="it converts plain text to html paragraphs" time="0.003" classname="spec\Acme\MarkdownSpec" status="passed" />
    <testcase name="it escapes html" time="0.002" classname="spec\Acme\MarkdownSpec" status="failed">
      <failure type="PhpSpec\Exception\Example\FailureException" message="expected &quot;&lt;p&gt;&amp;lt;b&amp;gt;&lt;/p&gt;&quot;, but got &quot;&lt;p&gt;&lt;b&gt;&lt;/p&gt;&quot;." />
    </testcase>
    <testcase name="it supports tables" time="0" classname="spec\Acme\MarkdownSpec" status="pending"><skipped><![CDATA[ todo: write pending example ]]></skipped></testcase>
  </testsuite>
</testsuites>"#;
        let t = tree(xml, JUnitFlavor::PhpSpec);
        let nodes = t.snapshot();
        assert_eq!(t.summary().expected_count, Some(3));
        assert_eq!(nodes[1].name, "Acme\\Markdown");
        let loc = nodes[3].location.as_ref().unwrap();
        assert_eq!(loc.path, "spec/Acme/MarkdownSpec.php");
        assert_eq!(loc.member.as_deref(), Some("it_escapes_html"));
        assert_eq!(
            nodes[3].message.as_deref(),
            Some("expected \"<p>&lt;b&gt;</p>\", but got \"<p><b></p>\".")
        );
        assert_eq!(nodes[4].status, TestStatus::Ignored);
        assert_eq!(
            nodes[4].message.as_deref(),
            Some("todo: write pending example")
        );
    }

    #[test]
    fn rejects_other_documents() {
        assert!(junit_messages("<html/>", JUnitFlavor::Generic).is_err());
        assert!(junit_messages("<testsuites><testsuite>", JUnitFlavor::Generic).is_err());
        assert!(junit_messages("", JUnitFlavor::Generic).is_err());
    }
}
