//! ScalaTest and MUnit suites of a Scala file, with their tests and the names the
//! frameworks report: `test("adds items")` (FunSuite, MUnit), `"A cart" should "add
//! items" in` and `it should …` (FlatSpec), `"A cart" when { "empty" should { "have no
//! items" in … } }` (WordSpec), `describe("A cart") { it("adds items") … }` (FunSpec) and
//! `"A cart" - { "adds items" in … }` (FreeSpec).

use serde::Serialize;

use crate::source::{ident, is_sym, parse, string, ScalaFile, Template, Tok};

/// The test framework of a suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Framework {
    ScalaTest,
    MUnit,
}

impl Framework {
    /// The run configuration type.
    pub fn type_id(self) -> &'static str {
        match self {
            Framework::ScalaTest => "scalatest",
            Framework::MUnit => "munit",
        }
    }
}

/// A test (or a group of tests: a `describe`, a WordSpec clause).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteTest {
    /// The full test name the framework reports (`A cart should add items`).
    pub name: String,
    /// What the gutter shows (`should add items`, `adds items`).
    pub label: String,
    pub line: u32,
    pub group: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suite {
    pub name: String,
    pub class: String,
    pub line: u32,
    pub framework: Framework,
    pub tests: Vec<SuiteTest>,
}

const SCALATEST_STYLES: [&str; 12] = [
    "AnyFunSuite",
    "AnyFlatSpec",
    "AnyWordSpec",
    "AnyFunSpec",
    "AnyFreeSpec",
    "AnyPropSpec",
    "AsyncFunSuite",
    "AsyncFlatSpec",
    "AsyncWordSpec",
    "AsyncFunSpec",
    "AsyncFreeSpec",
    "FixtureAnyFunSuite",
];
const SCALATEST_LEGACY: [&str; 5] = ["FunSuite", "FlatSpec", "WordSpec", "FunSpec", "FreeSpec"];
const MUNIT_SUITES: [&str; 3] = ["FunSuite", "CatsEffectSuite", "ScalaCheckSuite"];

fn framework_of(t: &Template, file: &ScalaFile) -> Option<Framework> {
    if t.kind != "class" && t.kind != "object" {
        return None;
    }
    let simple = |p: &String| p.rsplit('.').next().unwrap_or(p).to_string();
    for p in &t.parents {
        let s = simple(p);
        if p.starts_with("munit.")
            || (file.imports_from("munit") && MUNIT_SUITES.contains(&s.as_str()))
        {
            return Some(Framework::MUnit);
        }
        if p.starts_with("org.scalatest")
            || SCALATEST_STYLES.contains(&s.as_str())
            || (file.imports_from("org.scalatest") && SCALATEST_LEGACY.contains(&s.as_str()))
        {
            return Some(Framework::ScalaTest);
        }
    }
    // A project base suite (`class CartSpec extends UnitSpec`) in a file that imports a
    // framework.
    let base = t.parents.first().map(simple)?;
    if base.ends_with("Spec") || base.ends_with("Suite") || base.ends_with("Test") {
        if file.imports_from("munit") {
            return Some(Framework::MUnit);
        }
        if file.imports_from("org.scalatest") {
            return Some(Framework::ScalaTest);
        }
    }
    None
}

const VERBS: [&str; 4] = ["should", "must", "can", "when"];

/// The tests in a suite's body.
fn tests_in(file: &ScalaFile, (start, end): (usize, usize)) -> Vec<SuiteTest> {
    let tokens = &file.tokens;
    let mut out: Vec<SuiteTest> = Vec::new();
    // (depth of the group's body, name prefix)
    let mut groups: Vec<(u32, String)> = Vec::new();
    let mut subject = String::new();
    let end = end.min(tokens.len());
    let mut i = start + 1;
    let prefixed = |groups: &[(u32, String)], name: &str| match groups.last() {
        Some((_, p)) => format!("{p} {name}"),
        None => name.to_string(),
    };
    while i < end {
        let t = &tokens[i];
        while groups.last().is_some_and(|(d, _)| t.depth < *d) {
            groups.pop();
        }
        let next = |k: usize| tokens.get(i + k);
        match &t.tok {
            // `test("name")`, `it("name")` (FunSpec), `property("name")`, `describe("x") {`
            Tok::Ident(w)
                if matches!(
                    w.as_str(),
                    "test" | "it" | "property" | "describe" | "ignore"
                ) && is_sym(next(1), '(')
                    && string(next(2)).is_some() =>
            {
                let name = string(next(2)).unwrap_or_default().to_string();
                if w == "describe" {
                    let full = prefixed(&groups, &name);
                    out.push(SuiteTest {
                        name: full.clone(),
                        label: name,
                        line: t.line,
                        group: true,
                    });
                    // The body after `)`.
                    let mut k = i + 3;
                    while k < end && !is_sym(tokens.get(k), '{') {
                        k += 1;
                    }
                    if k < end {
                        groups.push((tokens[k].depth + 1, full));
                    }
                    i = k + 1;
                    continue;
                }
                if w != "ignore" {
                    out.push(SuiteTest {
                        name: if w == "it" {
                            prefixed(&groups, &name)
                        } else {
                            name.clone()
                        },
                        label: name,
                        line: t.line,
                        group: false,
                    });
                }
                i += 3;
                continue;
            }
            // `it should "x" in`, `they must "y" in`
            Tok::Ident(w)
                if (w == "it" || w == "they")
                    && ident(next(1)).is_some_and(|v| VERBS.contains(&v))
                    && string(next(2)).is_some() =>
            {
                let verb = ident(next(1)).unwrap_or_default();
                let what = string(next(2)).unwrap_or_default();
                if ident(next(3)) == Some("in") {
                    let label = format!("{verb} {what}");
                    out.push(SuiteTest {
                        name: format!("{subject} {label}").trim().to_string(),
                        label,
                        line: t.line,
                        group: false,
                    });
                }
                i += 3;
                continue;
            }
            Tok::Str(s) => {
                let s = s.clone();
                // `"A cart" should "add items" in`
                if let (Some(verb), Some(what)) = (
                    ident(next(1)).filter(|v| VERBS.contains(v)),
                    string(next(2)),
                ) {
                    subject = s.clone();
                    if ident(next(3)) == Some("in") {
                        let label = format!("{verb} {what}");
                        out.push(SuiteTest {
                            name: format!("{s} {label}"),
                            label,
                            line: t.line,
                            group: false,
                        });
                    }
                    i += 3;
                    continue;
                }
                // `"A cart" when {`, `"empty" should {`, `"A cart" - {` (WordSpec, FreeSpec)
                let verb = ident(next(1)).filter(|v| VERBS.contains(v) || *v == "which");
                let dash = is_sym(next(1), '-');
                if (verb.is_some() || dash) && is_sym(next(2), '{') {
                    let label = match verb {
                        Some(v) => format!("{s} {v}"),
                        None => s.clone(),
                    };
                    let full = prefixed(&groups, &label);
                    out.push(SuiteTest {
                        name: full.clone(),
                        label: s.clone(),
                        line: t.line,
                        group: true,
                    });
                    groups.push((next(2).map_or(0, |b| b.depth + 1), full));
                    subject = s;
                    i += 3;
                    continue;
                }
                // `"have no items" in {`
                if ident(next(1)) == Some("in") {
                    out.push(SuiteTest {
                        name: prefixed(&groups, &s),
                        label: s,
                        line: t.line,
                        group: false,
                    });
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// The ScalaTest and MUnit suites of a scanned file.
pub fn suites_of(file: &ScalaFile) -> Vec<Suite> {
    file.templates
        .iter()
        .filter_map(|t| {
            let framework = framework_of(t, file)?;
            Some(Suite {
                name: t.name.clone(),
                class: t.class.clone(),
                line: t.line,
                framework,
                tests: t.body.map(|b| tests_in(file, b)).unwrap_or_default(),
            })
        })
        .collect()
}

/// The suites of a Scala file.
pub fn suites(text: &str) -> Vec<Suite> {
    suites_of(&parse(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &Suite) -> Vec<(&str, &str, u32)> {
        s.tests
            .iter()
            .map(|t| (t.name.as_str(), t.label.as_str(), t.line))
            .collect()
    }

    #[test]
    fn scalatest_styles() {
        let s = suites(include_str!("../tests/fixtures/CartSpec.scala"));
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].class, "com.acme.shop.CartSpec");
        assert_eq!(s[0].framework, Framework::ScalaTest);
        assert_eq!(
            names(&s[0]),
            [
                ("A Cart should add items", "should add items", 7),
                ("A Cart should sum the total", "should sum the total", 12),
                (
                    "A Cart must reject negative quantities",
                    "must reject negative quantities",
                    16
                ),
            ]
        );
        assert_eq!(
            names(&s[1]),
            [
                ("A Cart when", "A Cart", 23),
                ("A Cart when empty should", "empty", 24),
                (
                    "A Cart when empty should have no items",
                    "have no items",
                    25
                ),
            ]
        );
        assert!(s[1].tests[0].group);
        assert_eq!(
            names(&s[2]),
            [
                ("Checkout", "Checkout", 33),
                ("Checkout applies the discount", "applies the discount", 34),
                ("charges shipping", "charges shipping", 38),
            ]
        );
    }

    #[test]
    fn munit_and_funsuite() {
        let s = suites(include_str!("../tests/fixtures/PriceSuite.scala"));
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].framework, Framework::MUnit);
        assert_eq!(s[0].class, "com.acme.shop.PriceSuite");
        assert_eq!(
            names(&s[0]),
            [
                ("formats euros", "formats euros", 5),
                ("rounds half up", "rounds half up", 9)
            ]
        );
        let plain = "class Cart { def test(name: String) = name }";
        assert!(suites(plain).is_empty());
        let funsuite = "import org.scalatest.funsuite.AnyFunSuite\nclass A extends AnyFunSuite {\n  test(\"x\") {}\n}";
        assert_eq!(suites(funsuite)[0].tests[0].name, "x");
    }
}
