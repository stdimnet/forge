//! NUnit 3 result XML (`<test-run>`), which the Unity Test Framework writes with
//! `-runTests -testResults <file>` and the NUnit console runner with `--result`.
//!
//! - Every `TestFixture` suite becomes a suite named by its class; its location is the
//!   class's full name (`classname`), which a pack's location rules map to the source file
//!   (DotNetForge's Unity tests). Namespace, assembly and `TestSuite` levels are flattened
//!   away; parameterized methods (`ParameterizedMethod`) keep their rows as tests of the
//!   fixture (`Damage(10)`), located by their method so the rows count for its gutter icon.
//! - `Passed` passes; `Skipped` and `Inconclusive` are ignored (with the reason);
//!   `Failed` (also errors, `label="Error"`) fails, with the comparison of NUnit's
//!   `Expected:` / `But was:` and the first `in <file>:<line>` (Mono, `:line <n>` .NET) or
//!   Unity `(at <file>:<line>)` frame as the failure location.
//! - `duration` is in seconds; `<output>` becomes the test's output.

use crate::formats::trx::comparison;
use crate::junit::{parse_dom, Element};
use crate::testevents::{location, TestEvent};

/// Whether an XML document is an NUnit 3 result file.
pub fn is_nunit(xml: &str) -> bool {
    let head: String = xml.chars().take(400).collect();
    head.contains("<test-run")
}

fn child<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    el.children.iter().find(|c| c.name == name)
}

fn text_at(el: &Element, path: &[&str]) -> Option<String> {
    let mut cur = el;
    for p in path {
        cur = child(cur, p)?;
    }
    Some(cur.text.trim_end().to_string()).filter(|t| !t.trim().is_empty())
}

/// The first source frame of a Mono / .NET / Unity stack trace: `in /p/A.cs:25`,
/// `in /p/A.cs:line 25` or `(at Assets/A.cs:25)`.
pub fn frame_location(trace: &str) -> Option<(String, u32)> {
    for line in trace.lines() {
        let l = line.trim();
        let rest = if let Some(i) = l.rfind("(at ") {
            l[i + 4..].trim_end_matches(')')
        } else if let Some(i) = l.rfind(" in ") {
            &l[i + 4..]
        } else {
            continue;
        };
        let (file, n) = if let Some(j) = rest.rfind(":line ") {
            (&rest[..j], &rest[j + 6..])
        } else if let Some(j) = rest.rfind(':') {
            (&rest[..j], &rest[j + 1..])
        } else {
            continue;
        };
        if let Ok(n) = n.trim().parse::<u32>() {
            if !file.is_empty() && n > 0 {
                return Some((file.to_string(), n));
            }
        }
    }
    None
}

struct Fixture<'a> {
    class: String,
    cases: Vec<&'a Element>,
}

/// Collect the fixtures (and their test cases, parameterized rows included) below `el`.
fn collect<'a>(el: &'a Element, current: Option<usize>, out: &mut Vec<Fixture<'a>>) {
    for c in &el.children {
        match c.name.as_str() {
            "test-suite" => {
                let kind = c.attr("type").unwrap_or("");
                if kind == "TestFixture" || (kind == "SetUpFixture" && has_cases(c)) {
                    let class = c
                        .attr("classname")
                        .or_else(|| c.attr("fullname"))
                        .unwrap_or("")
                        .to_string();
                    out.push(Fixture {
                        class,
                        cases: Vec::new(),
                    });
                    let idx = out.len() - 1;
                    collect(c, Some(idx), out);
                } else {
                    // Assemblies, namespaces, parameterized methods and generic fixtures.
                    collect(c, current, out);
                }
            }
            "test-case" => {
                let idx = match current {
                    Some(i) => i,
                    None => {
                        let class = c.attr("classname").unwrap_or("").to_string();
                        match out.iter().position(|f| f.class == class) {
                            Some(i) => i,
                            None => {
                                out.push(Fixture {
                                    class,
                                    cases: Vec::new(),
                                });
                                out.len() - 1
                            }
                        }
                    }
                };
                out[idx].cases.push(c);
            }
            _ => {}
        }
    }
}

fn has_cases(el: &Element) -> bool {
    el.children
        .iter()
        .any(|c| c.name == "test-case" || (c.name == "test-suite" && has_cases(c)))
}

fn seconds_ms(v: &str) -> Option<u64> {
    let s: f64 = v.trim().parse().ok()?;
    Some((s * 1000.0).round() as u64)
}

fn with_newline(t: String) -> String {
    if t.ends_with('\n') {
        t
    } else {
        format!("{t}\n")
    }
}

/// The events of an NUnit 3 result document.
pub fn nunit_events(xml: &str) -> Result<Vec<TestEvent>, String> {
    let root = parse_dom(xml.trim_start_matches('\u{feff}'))?;
    if root.name != "test-run" {
        return Err("not an NUnit 3 result file".into());
    }
    let mut fixtures = Vec::new();
    collect(&root, None, &mut fixtures);
    fixtures.retain(|f| !f.cases.is_empty());
    let total: usize = fixtures.iter().map(|f| f.cases.len()).sum();
    let mut out = vec![TestEvent::Count {
        count: total as u32,
    }];
    for f in &fixtures {
        let suite = if f.class.is_empty() {
            "Tests".to_string()
        } else {
            f.class.rsplit('.').next().unwrap_or(&f.class).to_string()
        };
        out.push(TestEvent::SuiteStarted {
            name: suite.clone(),
            location: (!f.class.is_empty()).then(|| location(&f.class, None, None)),
        });
        for case in &f.cases {
            let name = case.attr("name").unwrap_or("test").to_string();
            let method = case
                .attr("methodname")
                .map(str::to_string)
                .unwrap_or_else(|| name.split('(').next().unwrap_or(&name).to_string());
            let loc = (!f.class.is_empty()).then(|| location(&f.class, None, Some(&method)));
            let result = case.attr("result").unwrap_or("");
            if matches!(result, "Skipped" | "Inconclusive") {
                let reason = text_at(case, &["reason", "message"])
                    .or_else(|| case.attr("label").map(str::to_string))
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty());
                out.push(TestEvent::TestIgnored {
                    name,
                    message: reason,
                    location: loc,
                });
                continue;
            }
            out.push(TestEvent::TestStarted {
                name: name.clone(),
                location: loc,
            });
            if let Some(t) = text_at(case, &["output"]) {
                out.push(TestEvent::TestOutput {
                    name: name.clone(),
                    text: with_newline(t),
                    stderr: false,
                });
            }
            if result != "Passed" {
                let message = text_at(case, &["failure", "message"]).unwrap_or_default();
                let trace = text_at(case, &["failure", "stack-trace"]);
                let cmp = comparison(&message);
                let label = case.attr("label").filter(|l| !l.is_empty());
                out.push(TestEvent::TestFailed {
                    name: name.clone(),
                    message: if message.trim().is_empty() {
                        format!("Test {}", label.unwrap_or("failed").to_ascii_lowercase())
                    } else {
                        message.trim().to_string()
                    },
                    details: trace.clone(),
                    expected: cmp.as_ref().map(|c| c.0.clone()),
                    actual: cmp.map(|c| c.1),
                    location: trace
                        .as_deref()
                        .and_then(frame_location)
                        .map(|(file, line)| location(&file, Some(line), Some(&method))),
                });
            }
            out.push(TestEvent::TestFinished {
                name,
                duration: case.attr("duration").and_then(seconds_ms),
            });
        }
        out.push(TestEvent::SuiteFinished { name: suite });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::document_events;
    use crate::formats::tests::{fixture, tree_with};
    use crate::junit::JUnitFlavor;
    use crate::teamcity::{LocationRule, TestStatus};

    #[test]
    fn unity_test_results() {
        let xml = fixture("unity-nunit3.xml");
        assert!(is_nunit(&xml));
        let events = nunit_events(&xml).unwrap();
        assert_eq!(events[0], TestEvent::Count { count: 6 });
        let rules = vec![LocationRule {
            pattern: r"^Shooter\.Tests\.PlayerTests$".into(),
            replacement: "Assets/Tests/EditMode/PlayerTests.cs".into(),
        }];
        let tree = tree_with(&events, &rules);
        let c = tree.counts();
        assert_eq!((c.total, c.passed, c.failed, c.ignored), (6, 3, 2, 1));
        let nodes = tree.snapshot();
        let suite = nodes.iter().find(|n| n.name == "PlayerTests").unwrap();
        assert_eq!(
            suite.location.as_ref().unwrap().path,
            "Assets/Tests/EditMode/PlayerTests.cs"
        );
        let health = nodes.iter().find(|n| n.name == "Health_Decreases").unwrap();
        assert_eq!(health.status, TestStatus::Failed);
        assert_eq!(health.expected.as_deref(), Some("5"));
        assert_eq!(health.actual.as_deref(), Some("3"));
        assert_eq!(health.duration, Some(12));
        let row = nodes.iter().find(|n| n.name == "Damage(10)").unwrap();
        assert_eq!(
            row.location.as_ref().unwrap().member.as_deref(),
            Some("Damage")
        );
        let skipped = nodes.iter().find(|n| n.name == "Respawn").unwrap();
        assert_eq!(skipped.status, TestStatus::Ignored);
        // A play mode fixture of another assembly.
        assert!(nodes.iter().any(|n| n.name == "SpawnerTests"));
        let sniffed = document_events(&xml, JUnitFlavor::Generic).unwrap();
        assert_eq!(sniffed, events);
    }

    #[test]
    fn stack_frames() {
        assert_eq!(
            frame_location("at A.B () [0x00010] in /p/Assets/A.cs:25 \n"),
            Some(("/p/Assets/A.cs".into(), 25))
        );
        assert_eq!(
            frame_location("A.B () (at Assets/Scripts/Player.cs:12)"),
            Some(("Assets/Scripts/Player.cs".into(), 12))
        );
        assert_eq!(
            frame_location("   at A.B() in C:\\p\\A.cs:line 7"),
            Some(("C:\\p\\A.cs".into(), 7))
        );
        assert_eq!(
            frame_location("at A.B () [0x00000] in <filename unknown>:0"),
            None
        );
        assert!(nunit_events("<testsuites/>").is_err());
        assert_eq!(
            nunit_events("<test-run><test-suite type=\"Assembly\"/></test-run>").unwrap(),
            vec![TestEvent::Count { count: 0 }]
        );
    }
}
