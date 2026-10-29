//! Visual Studio test results (TRX), which `dotnet test --logger trx` writes for xUnit,
//! NUnit and MSTest (VSTest and Microsoft.Testing.Platform).
//!
//! - `TestDefinitions/UnitTest/TestMethod` gives each test's class (`className`, nested
//!   classes as `Outer+Inner`) and method (`name`); `Results/UnitTestResult` its outcome,
//!   duration (`hh:mm:ss.fffffff`), output and error.
//! - Every class is a suite named by its class name; its location is the class's full
//!   name, which the .NET test provider's location rules map to the source file. Tests are
//!   named by their display name without the class (`Adds(quantity: 2)` for data rows) and
//!   located by their method, so data rows count for their method's gutter icon.
//! - `Passed` passes; `NotExecuted`, `Inconclusive`, `Pending`, `Disconnected` and
//!   `Warning` are ignored (with the skip reason); everything else fails. xUnit's
//!   `Expected:` / `Actual:`, NUnit's `Expected:` / `But was:`, MSTest's
//!   `Expected:<…>. Actual:<…>.` and FluentAssertions' `Expected … to be …, but found …`
//!   give the comparison; the first stack frame `in <file>:line <n>` the failure location
//!   (with the method as its member, so data rows keep counting for their method).

use crate::junit::{parse_dom, Element};
use crate::testevents::{location, TestEvent};

/// Whether an XML document is a TRX report.
pub fn is_trx(xml: &str) -> bool {
    let head: String = xml.chars().take(600).collect();
    head.contains("<TestRun") && (head.contains("TeamTest") || head.contains("runUser"))
        || head
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with("<TestRun")
}

struct Definition {
    class: String,
    method: String,
}

fn child<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    el.children.iter().find(|c| c.name == name)
}

fn text_of(el: &Element, path: &[&str]) -> Option<String> {
    let mut cur = el;
    for p in path {
        cur = child(cur, p)?;
    }
    Some(cur.text.clone()).filter(|t| !t.trim().is_empty())
}

/// `00:00:01.2345678` → milliseconds.
fn duration_ms(d: &str) -> Option<u64> {
    let mut parts = d.trim().split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.parse().ok()?;
    Some(((h * 3600.0 + m * 60.0 + s) * 1000.0).round() as u64)
}

fn after<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let i = text.find(key)?;
    Some(text[i + key.len()..].lines().next().unwrap_or("").trim())
}

/// Expected and actual values of an assertion message.
pub fn comparison(message: &str) -> Option<(String, String)> {
    // MSTest: `Assert.AreEqual failed. Expected:<3>. Actual:<2>.`
    if let (Some(e), Some(a)) = (message.find("Expected:<"), message.find("Actual:<")) {
        let take = |from: usize, key: &str| -> Option<String> {
            let rest = &message[from + key.len()..];
            rest.find(">.")
                .or_else(|| rest.rfind('>'))
                .map(|end| rest[..end].to_string())
        };
        if let (Some(x), Some(y)) = (take(e, "Expected:<"), take(a, "Actual:<")) {
            return Some((x, y));
        }
    }
    // xUnit (`Expected: 19.98` / `Actual:   19.97`) and NUnit (`Expected: 3` / `But was:  2`).
    if let Some(e) = after(message, "Expected:") {
        if let Some(a) = after(message, "Actual:").or_else(|| after(message, "But was:")) {
            return Some((e.to_string(), a.to_string()));
        }
    }
    // FluentAssertions: `Expected cart.Total to be 19.98M, but found 19.97M.`
    if let Some(i) = message.find(" to be ") {
        if message.starts_with("Expected") {
            let rest = &message[i + 7..];
            if let Some(j) = rest.find(", but found ") {
                let actual = rest[j + 12..]
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches('.');
                return Some((rest[..j].to_string(), actual.to_string()));
            }
        }
    }
    None
}

/// The first `in <file>:line <n>` of a .NET stack trace.
pub fn stack_location(trace: &str) -> Option<(String, u32)> {
    for line in trace.lines() {
        let l = line.trim();
        let Some(i) = l.rfind(" in ") else { continue };
        let rest = &l[i + 4..];
        let Some(j) = rest.rfind(":line ") else {
            continue;
        };
        if let Ok(n) = rest[j + 6..].trim().parse::<u32>() {
            return Some((rest[..j].to_string(), n));
        }
    }
    None
}

/// The events of a TRX document.
pub fn trx_events(xml: &str) -> Result<Vec<TestEvent>, String> {
    let root = parse_dom(xml.trim_start_matches('\u{feff}'))?;
    if root.name != "TestRun" {
        return Err("not a TRX report".into());
    }
    let mut defs: Vec<(String, Definition)> = Vec::new();
    if let Some(d) = child(&root, "TestDefinitions") {
        for t in d.children.iter().filter(|c| c.name == "UnitTest") {
            let id = t.attr("id").unwrap_or("").to_string();
            let m = child(t, "TestMethod");
            defs.push((
                id,
                Definition {
                    class: m
                        .and_then(|m| m.attr("className"))
                        .unwrap_or("")
                        .to_string(),
                    method: m
                        .and_then(|m| m.attr("name"))
                        .or_else(|| t.attr("name"))
                        .unwrap_or("")
                        .to_string(),
                },
            ));
        }
    }
    struct Row<'a> {
        class: String,
        method: String,
        name: String,
        result: &'a Element,
    }
    let mut rows: Vec<Row> = Vec::new();
    if let Some(results) = child(&root, "Results") {
        let mut stack: Vec<&Element> = results.children.iter().collect();
        stack.reverse();
        while let Some(r) = stack.pop() {
            if r.name != "UnitTestResult" {
                continue;
            }
            // Data-driven MSTest results nest their rows in `InnerResults`.
            if let Some(inner) = child(r, "InnerResults") {
                let mut kids: Vec<&Element> = inner.children.iter().collect();
                kids.reverse();
                stack.extend(kids);
                continue;
            }
            let test_id = r.attr("testId").unwrap_or("");
            let display = r.attr("testName").unwrap_or("").to_string();
            let def = defs.iter().find(|(id, _)| id == test_id).map(|(_, d)| d);
            let class = def.map(|d| d.class.clone()).unwrap_or_default();
            let method = def.map(|d| d.method.clone()).unwrap_or_default();
            // `Ns.Class.Method(args)` → `Method(args)`.
            let name = if !class.is_empty() && display.starts_with(&format!("{class}.")) {
                display[class.len() + 1..].to_string()
            } else {
                display
            };
            rows.push(Row {
                class,
                method,
                name: if name.is_empty() { "test".into() } else { name },
                result: r,
            });
        }
    }
    let mut out = vec![TestEvent::Count {
        count: rows.len() as u32,
    }];
    let mut classes: Vec<String> = Vec::new();
    for r in &rows {
        if !classes.contains(&r.class) {
            classes.push(r.class.clone());
        }
    }
    for class in &classes {
        let suite = if class.is_empty() {
            "Tests".to_string()
        } else {
            class.rsplit('.').next().unwrap_or(class).to_string()
        };
        out.push(TestEvent::SuiteStarted {
            name: suite.clone(),
            location: (!class.is_empty()).then(|| location(class, None, None)),
        });
        for r in rows.iter().filter(|r| &r.class == class) {
            let res = r.result;
            let outcome = res.attr("outcome").unwrap_or("").to_string();
            let output = child(res, "Output");
            let message = output
                .and_then(|o| text_of(o, &["ErrorInfo", "Message"]))
                .unwrap_or_default();
            let trace = output.and_then(|o| text_of(o, &["ErrorInfo", "StackTrace"]));
            let loc = (!r.class.is_empty()).then(|| location(&r.class, None, Some(&r.method)));
            match outcome.as_str() {
                "NotExecuted" | "Inconclusive" | "Pending" | "Disconnected" | "Warning" => {
                    out.push(TestEvent::TestIgnored {
                        name: r.name.clone(),
                        message: Some(message.trim().to_string()).filter(|m| !m.is_empty()),
                        location: loc,
                    });
                    continue;
                }
                _ => {}
            }
            out.push(TestEvent::TestStarted {
                name: r.name.clone(),
                location: loc,
            });
            if let Some(o) = output {
                if let Some(t) = text_of(o, &["StdOut"]) {
                    out.push(TestEvent::TestOutput {
                        name: r.name.clone(),
                        text: if t.ends_with('\n') {
                            t
                        } else {
                            format!("{t}\n")
                        },
                        stderr: false,
                    });
                }
                if let Some(t) = text_of(o, &["StdErr"]) {
                    out.push(TestEvent::TestOutput {
                        name: r.name.clone(),
                        text: if t.ends_with('\n') {
                            t
                        } else {
                            format!("{t}\n")
                        },
                        stderr: true,
                    });
                }
            }
            if outcome != "Passed" {
                let cmp = comparison(&message);
                out.push(TestEvent::TestFailed {
                    name: r.name.clone(),
                    message: if message.trim().is_empty() {
                        format!(
                            "Test {}",
                            if outcome.is_empty() {
                                "failed"
                            } else {
                                &outcome
                            }
                        )
                    } else {
                        message.trim().to_string()
                    },
                    details: trace.clone().map(|t| t.trim_end().to_string()),
                    expected: cmp.as_ref().map(|c| c.0.clone()),
                    actual: cmp.map(|c| c.1),
                    location: trace
                        .as_deref()
                        .and_then(stack_location)
                        .map(|(f, l)| location(&f, Some(l), Some(&r.method))),
                });
            }
            out.push(TestEvent::TestFinished {
                name: r.name.clone(),
                duration: res.attr("duration").and_then(duration_ms),
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
    fn xunit_report() {
        let xml = fixture("xunit.trx");
        assert!(is_trx(&xml));
        let events = trx_events(&xml).unwrap();
        assert_eq!(events[0], TestEvent::Count { count: 6 });
        let rules = vec![LocationRule {
            pattern: r"^Bookstore\.Tests\.CartTests(\+.*)?$".into(),
            replacement: "tests/Bookstore.Tests/CartTests.cs".into(),
        }];
        let tree = tree_with(&events, &rules);
        let c = tree.counts();
        assert_eq!((c.total, c.passed, c.failed, c.ignored), (6, 3, 2, 1));
        let nodes = tree.snapshot();
        let suite = nodes.iter().find(|n| n.name == "CartTests").unwrap();
        assert_eq!(
            suite.location.as_ref().unwrap().path,
            "tests/Bookstore.Tests/CartTests.cs"
        );
        let row = nodes
            .iter()
            .find(|n| n.name == "Add_RejectsNonPositiveQuantities(quantity: 0)")
            .unwrap();
        assert_eq!(row.status, TestStatus::Failed);
        assert_eq!(
            row.location.as_ref().unwrap().member.as_deref(),
            Some("Add_RejectsNonPositiveQuantities")
        );
        let total = nodes.iter().find(|n| n.name == "Total_AddsLines").unwrap();
        assert_eq!(total.status, TestStatus::Failed);
        assert_eq!(total.expected.as_deref(), Some("19.98"));
        assert_eq!(total.actual.as_deref(), Some("19.97"));
        assert_eq!(total.duration, Some(12));
        assert!(nodes
            .iter()
            .any(|n| n.name == "WhenEmpty+HasNoLines" || n.name == "HasNoLines"));
        // Through the sniffing reader of report files.
        let sniffed = document_events(&xml, JUnitFlavor::Generic).unwrap();
        assert_eq!(sniffed, events);
    }

    #[test]
    fn mstest_inner_results_and_messages() {
        let events = trx_events(&fixture("mstest.trx")).unwrap();
        let tree = tree_with(&events, &[]);
        let c = tree.counts();
        assert_eq!(
            (c.total, c.passed, c.failed, c.ignored),
            (3, 1, 1, 1),
            "{events:#?}"
        );
        assert_eq!(
            comparison("Assert.AreEqual failed. Expected:<3>. Actual:<2>. "),
            Some(("3".into(), "2".into()))
        );
        assert_eq!(
            comparison("  Expected: 3\n  But was:  2\n"),
            Some(("3".into(), "2".into()))
        );
        assert_eq!(
            comparison("Expected cart.Total to be 19.98M, but found 19.97M."),
            Some(("19.98M".into(), "19.97M".into()))
        );
        assert_eq!(comparison("boom"), None);
        assert_eq!(
            stack_location("   at A.B() in /src/A.cs:line 12\n   at C()"),
            Some(("/src/A.cs".into(), 12))
        );
        assert_eq!(duration_ms("00:01:00.5"), Some(60500));
        assert!(trx_events("<TestRun><Results/></TestRun>").unwrap().len() == 1);
        assert!(trx_events("<testsuites/>").is_err());
    }
}
