//! Test discovery from Python source for the gutter ▶ icons and Run Context Configuration,
//! following pytest's and unittest's default collection rules:
//!
//! - pytest: files `test_*.py` / `*_test.py`; module-level `test*` functions; `Test*`
//!   classes without `__init__` (nested ones too) and their `test*` methods;
//!   `@pytest.mark.parametrize` cases with pytest's ids (`ids=`, `pytest.param(id=…)`,
//!   derived from literals, `argname0` otherwise; stacked decorators multiply, the closest
//!   one first).
//! - unittest: `unittest.TestCase` subclasses (also `IsolatedAsyncioTestCase`, Django's and
//!   DRF's `*TestCase` and subclasses of other test cases of the file) and their `test*`
//!   methods, in any module.
//!
//! Every target carries the temporary configuration that runs it ([`crate::config`]).

use forge_runtools::Configuration;
use forge_testing::{TargetKind, TestFile, TestTarget};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::source::{bracket_contents, declarations, split_top_level, Decorator, DefKind};

/// Which runner builds the configurations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Runner {
    #[default]
    Pytest,
    Unittest,
}

impl Runner {
    pub fn type_id(self) -> &'static str {
        match self {
            Runner::Pytest => config::PYTEST,
            Runner::Unittest => config::UNITTEST,
        }
    }
}

/// A parametrized case of a test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Case {
    pub id: String,
    /// 0-based line of its value, when every case has a line of its own.
    pub line: Option<u32>,
}

/// A test class or function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub class: bool,
    pub name: String,
    pub line: u32,
    /// Enclosing test classes, outermost first.
    pub classes: Vec<String>,
    /// Belongs to a `unittest.TestCase`.
    pub unittest: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<Case>,
}

impl Item {
    /// `TestCart::test_rounds` (pytest node id after the file).
    pub fn node(&self) -> String {
        let mut parts = self.classes.clone();
        parts.push(self.name.clone());
        parts.join("::")
    }

    /// `TestCart.test_rounds` (unittest id after the module).
    pub fn dotted(&self) -> String {
        let mut parts = self.classes.clone();
        parts.push(self.name.clone());
        parts.join(".")
    }
}

/// The tests of a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub items: Vec<Item>,
    /// The module imports pytest.
    pub imports_pytest: bool,
}

impl Scan {
    /// Tests only pytest collects (functions, plain `Test*` classes).
    pub fn has_pytest_only(&self) -> bool {
        self.items.iter().any(|i| !i.unittest)
    }

    /// The runner the source suggests: pytest unless it only has unittest test cases.
    pub fn suggested_runner(&self) -> Runner {
        if self.imports_pytest || self.has_pytest_only() || self.items.is_empty() {
            Runner::Pytest
        } else {
            Runner::Unittest
        }
    }
}

/// `test_cart.py`, `cart_test.py`: files pytest collects by default.
pub fn is_pytest_file(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.ends_with(".py") && (name.starts_with("test_") || name.ends_with("_test.py"))
}

fn is_test_case_base(base: &str, known: &[String]) -> bool {
    let short = base.rsplit('.').next().unwrap_or(base);
    short.ends_with("TestCase") || known.iter().any(|k| k == short)
}

/// The tests of a Python module.
pub fn scan(path: &str, text: &str) -> Scan {
    let defs = declarations(text);
    let pytest_file = is_pytest_file(path);
    let imports_pytest = text.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("import pytest") || t.starts_with("from pytest") || t.contains("pytest.")
    });
    // Per def: Some(unittest?) when it is a test class.
    let mut test_class: Vec<Option<bool>> = vec![None; defs.len()];
    let mut unittest_names: Vec<String> = Vec::new();
    let mut items = Vec::new();
    for (i, d) in defs.iter().enumerate() {
        let parent_class = d.parent.and_then(|p| test_class[p].map(|u| (p, u)));
        // Only module-level code or test classes' bodies hold tests.
        let in_scope = d.parent.is_none() || parent_class.is_some();
        if !in_scope {
            continue;
        }
        let classes: Vec<String> = {
            let mut c = Vec::new();
            let mut p = d.parent;
            while let Some(x) = p {
                c.insert(0, defs[x].name.clone());
                p = defs[x].parent;
            }
            c
        };
        match d.kind {
            DefKind::Class => {
                let unittest = d
                    .bases
                    .iter()
                    .any(|b| is_test_case_base(b, &unittest_names));
                let has_init = defs.iter().any(|m| {
                    m.parent == Some(i) && m.kind == DefKind::Function && m.name == "__init__"
                });
                let pytest_class = pytest_file
                    && d.name.starts_with("Test")
                    && !has_init
                    && parent_class.is_none_or(|(_, u)| !u);
                if unittest {
                    unittest_names.push(d.name.clone());
                }
                if unittest || pytest_class {
                    test_class[i] = Some(unittest);
                    items.push(Item {
                        class: true,
                        name: d.name.clone(),
                        line: d.line,
                        classes,
                        unittest,
                        cases: Vec::new(),
                    });
                }
            }
            DefKind::Function => {
                if !d.name.starts_with("test") {
                    continue;
                }
                let unittest = parent_class.is_some_and(|(_, u)| u);
                if parent_class.is_none() && !pytest_file {
                    continue;
                }
                let cases = if unittest {
                    Vec::new()
                } else {
                    parametrize_cases(&d.decorators)
                };
                items.push(Item {
                    class: false,
                    name: d.name.clone(),
                    line: d.line,
                    classes,
                    unittest,
                    cases,
                });
            }
        }
    }
    // Classes without tests are no targets.
    let keep: Vec<bool> = items
        .iter()
        .map(|it| {
            !it.class || {
                let mut prefix = it.classes.clone();
                prefix.push(it.name.clone());
                items
                    .iter()
                    .any(|t| !t.class && t.classes.starts_with(&prefix))
            }
        })
        .collect();
    let items = items
        .into_iter()
        .zip(keep)
        .filter_map(|(it, k)| k.then_some(it))
        .collect();
    Scan {
        items,
        imports_pytest,
    }
}

fn offset_in(outer: &str, inner: &str) -> usize {
    (inner.as_ptr() as usize).saturating_sub(outer.as_ptr() as usize)
}

/// A Python string literal's value (`'a'`, `"b"`, `r"c"`, `"""d"""`).
pub fn string_literal(s: &str) -> Option<String> {
    let t = s.trim();
    let body = t.trim_start_matches(['r', 'R', 'u', 'U', 'b', 'B']);
    for q in ["\"\"\"", "'''", "\"", "'"] {
        if body.len() >= 2 * q.len() && body.starts_with(q) && body.ends_with(q) {
            let inner = &body[q.len()..body.len() - q.len()];
            return Some(
                inner
                    .replace("\\'", "'")
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\"),
            );
        }
    }
    None
}

/// pytest's id of one parameter value: strings and numbers as they are, else `argname<i>`.
fn value_id(value: &str, argname: &str, index: usize) -> String {
    let v = value.trim();
    if let Some(s) = string_literal(v) {
        return s;
    }
    let numeric = v
        .trim_start_matches('-')
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '_' || c == 'e')
        && v.trim_start_matches('-')
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit());
    if numeric || matches!(v, "True" | "False" | "None") {
        return v.to_string();
    }
    format!("{argname}{index}")
}

fn keyword<'a>(args: &[(usize, &'a str)], name: &str) -> Option<(usize, &'a str)> {
    args.iter().find_map(|(o, a)| {
        let t = a.trim_start();
        let rest = t.strip_prefix(name)?.trim_start().strip_prefix('=')?;
        (!rest.starts_with('=')).then(|| (o + (a.len() - rest.len()), rest))
    })
}

/// The cases of one `parametrize` decorator: (id, line) each.
fn one_parametrize(d: &Decorator) -> Option<Vec<(String, u32)>> {
    let text = &d.line.text;
    let expr = d.expr();
    let call = ["pytest.mark.parametrize", "mark.parametrize", "parametrize"]
        .iter()
        .find_map(|p| expr.strip_prefix(p))?
        .trim_start();
    if !call.starts_with('(') {
        return None;
    }
    let call_off = offset_in(text, call);
    let inner = bracket_contents(call, 0)?;
    let inner_off = call_off + 1;
    let args = split_top_level(inner);
    let positional: Vec<&(usize, &str)> = args
        .iter()
        .filter(|(_, a)| {
            let t = a.trim_start();
            !(t.split_once('=').is_some_and(|(k, v)| {
                k.trim().chars().all(|c| c.is_alphanumeric() || c == '_') && !v.starts_with('=')
            }))
        })
        .collect();
    let names_src = positional
        .first()
        .map(|(_, a)| *a)
        .or_else(|| keyword(&args, "argnames").map(|(_, a)| a))?;
    let argnames: Vec<String> = match string_literal(names_src) {
        Some(s) => s
            .split(',')
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect(),
        None => {
            let t = names_src.trim();
            bracket_contents(t, 0)
                .map(|i| {
                    split_top_level(i)
                        .into_iter()
                        .filter_map(|(_, n)| string_literal(n))
                        .collect()
                })
                .unwrap_or_default()
        }
    };
    let (values_off, values_src) = positional
        .get(1)
        .map(|(o, a)| (*o, *a))
        .or_else(|| keyword(&args, "argvalues"))?;
    let vt = values_src.trim_start();
    let vt_off = inner_off + values_off + (values_src.len() - vt.len());
    let list = bracket_contents(vt.trim_end(), 0)?;
    let elements = split_top_level(list);
    let explicit_ids: Option<Vec<String>> = keyword(&args, "ids").and_then(|(_, v)| {
        let v = v.trim();
        let inner = bracket_contents(v, 0)?;
        Some(
            split_top_level(inner)
                .into_iter()
                .map(|(_, s)| string_literal(s).unwrap_or_default())
                .collect(),
        )
    });
    let mut out = Vec::new();
    for (index, (off, el)) in elements.iter().enumerate() {
        let trimmed = el.trim_start();
        let line = d
            .line
            .line_at(vt_off + 1 + off + (el.len() - trimmed.len()));
        let el = trimmed.trim_end();
        let mut values: Vec<&str> = vec![el];
        let mut id = None;
        if let Some(p) = el.strip_prefix("pytest.param").map(str::trim_start) {
            if let Some(args) = bracket_contents(p, 0) {
                let parts = split_top_level(args);
                id = keyword(&parts, "id").and_then(|(_, v)| string_literal(v));
                values = parts
                    .iter()
                    .filter(|(_, a)| !a.contains('=') || string_literal(a).is_some())
                    .map(|(_, a)| a.trim())
                    .collect();
            }
        } else if argnames.len() > 1 && (el.starts_with('(') || el.starts_with('[')) {
            if let Some(i) = bracket_contents(el, 0) {
                values = split_top_level(i)
                    .into_iter()
                    .map(|(_, v)| v.trim())
                    .collect();
            }
        }
        let id = explicit_ids
            .as_ref()
            .and_then(|ids| ids.get(index).cloned())
            .filter(|s| !s.is_empty())
            .or(id)
            .unwrap_or_else(|| {
                values
                    .iter()
                    .enumerate()
                    .map(|(k, v)| value_id(v, argnames.get(k).map_or("arg", String::as_str), index))
                    .collect::<Vec<_>>()
                    .join("-")
            });
        out.push((id, line));
    }
    // Duplicate ids get their index appended, like pytest does.
    let dupes: Vec<String> = out
        .iter()
        .filter(|(id, _)| out.iter().filter(|(o, _)| o == id).count() > 1)
        .map(|(id, _)| id.clone())
        .collect();
    let mut counters: std::collections::HashMap<String, usize> = Default::default();
    for (id, _) in out.iter_mut() {
        if dupes.contains(id) {
            let n = counters.entry(id.clone()).or_default();
            *id = format!("{id}{n}");
            *n += 1;
        }
    }
    Some(out)
}

/// The parametrized cases of a test function (none when it is not parametrized).
pub fn parametrize_cases(decorators: &[Decorator]) -> Vec<Case> {
    let lists: Vec<Vec<(String, u32)>> = decorators.iter().filter_map(one_parametrize).collect();
    match lists.len() {
        0 => Vec::new(),
        1 => {
            let list = &lists[0];
            let mut lines: Vec<u32> = list.iter().map(|(_, l)| *l).collect();
            lines.dedup();
            let own_lines = lines.len() == list.len();
            list.iter()
                .map(|(id, l)| Case {
                    id: id.clone(),
                    line: own_lines.then_some(*l),
                })
                .collect()
        }
        _ => {
            // The closest decorator is applied first: its ids come first.
            let mut ids: Vec<String> = vec![String::new()];
            for list in lists.iter().rev() {
                let mut next = Vec::new();
                for prefix in &ids {
                    for (id, _) in list {
                        next.push(if prefix.is_empty() {
                            id.clone()
                        } else {
                            format!("{prefix}-{id}")
                        });
                        if next.len() >= 200 {
                            break;
                        }
                    }
                }
                ids = next;
            }
            ids.into_iter().map(|id| Case { id, line: None }).collect()
        }
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn module_stem(path: &str) -> &str {
    file_name(path).trim_end_matches(".py")
}

/// The gutter targets of a scanned module for `runner`. Tests the runner cannot run (plain
/// functions for unittest) are left out.
pub fn test_file(path: &str, scan: &Scan, runner: Runner) -> TestFile {
    let items: Vec<&Item> = scan
        .items
        .iter()
        .filter(|i| runner == Runner::Pytest || i.unittest)
        .collect();
    if items.is_empty() {
        return TestFile::default();
    }
    let stem = module_stem(path);
    let mut targets = Vec::new();
    for it in items {
        let qualified = format!("{stem}.{}", it.dotted());
        let cfg = |test: &str, label: &str| -> Configuration {
            match runner {
                Runner::Pytest => config::pytest_test(path, test, &format!("pytest for {label}")),
                Runner::Unittest => {
                    config::unittest_test(path, &it.dotted(), &format!("Unittests for {label}"))
                }
            }
        };
        if it.class {
            targets.push(TestTarget {
                line: it.line,
                kind: TargetKind::Class,
                label: it.name.clone(),
                key: path.to_string(),
                configuration: cfg(&it.node(), &qualified),
            });
            continue;
        }
        targets.push(TestTarget {
            line: it.line,
            kind: TargetKind::Method,
            label: it.name.clone(),
            key: format!("{path}::{}", it.name),
            configuration: cfg(&it.node(), &qualified),
        });
        for case in &it.cases {
            let Some(line) = case.line.filter(|l| *l != it.line) else {
                continue;
            };
            let node = format!("{}[{}]", it.node(), case.id);
            targets.push(TestTarget {
                line,
                kind: TargetKind::Dataset,
                label: format!("{}[{}]", it.name, case.id),
                key: format!("{path}::{}[{}]", it.name, case.id),
                configuration: config::pytest_test(
                    path,
                    &node,
                    &format!("pytest for {qualified}[{}]", case.id),
                ),
            });
        }
    }
    let file = match runner {
        Runner::Pytest => config::pytest_file(path, &format!("pytest in {}", file_name(path))),
        Runner::Unittest => {
            config::unittest_file(path, &format!("Unittests in {}", file_name(path)))
        }
    };
    TestFile {
        framework: Some(runner.type_id().to_string()),
        targets,
        file: Some(file),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CART: &str = r#"import pytest
from shop.cart import Cart, round_cents


def test_empty_cart_total():
    assert Cart().total() == 0


@pytest.mark.parametrize(
    ("price", "expected"),
    [
        (1.1781, 1.18),
        pytest.param(2.005, 2.01, id="half-up"),
        (-1, -1),
    ],
)
def test_round_cents(price, expected):
    assert round_cents(price) == expected


class TestCart:
    @pytest.mark.parametrize("qty", [1, 2], ids=["one", "two"])
    def test_add(self, qty):
        ...

    def helper(self):
        ...

    class TestVouchers:
        async def test_apply(self):
            ...


class TestNotCollected:
    def __init__(self):
        ...

    def test_never(self):
        ...


def helper():
    def test_inner():
        ...
"#;

    #[test]
    fn pytest_functions_classes_and_cases() {
        let s = scan("tests/test_cart.py", CART);
        let names: Vec<_> = s.items.iter().map(|i| i.node()).collect();
        assert_eq!(
            names,
            [
                "test_empty_cart_total",
                "test_round_cents",
                "TestCart",
                "TestCart::test_add",
                "TestCart::TestVouchers",
                "TestCart::TestVouchers::test_apply"
            ]
        );
        let round = &s.items[1];
        let ids: Vec<_> = round.cases.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["1.1781-1.18", "half-up", "-1--1"]);
        assert_eq!(round.cases[0].line, Some(11));
        assert_eq!(round.cases[2].line, Some(13));
        let add = &s.items[3];
        assert_eq!(
            add.cases.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert_eq!(add.cases[0].line, None, "cases on the decorator's line");
        assert!(s.imports_pytest && s.has_pytest_only());
        assert_eq!(s.suggested_runner(), Runner::Pytest);
    }

    #[test]
    fn targets_carry_their_configurations() {
        let s = scan("tests/test_cart.py", CART);
        let f = test_file("tests/test_cart.py", &s, Runner::Pytest);
        assert_eq!(f.framework.as_deref(), Some("pytest"));
        let file = f.file.as_ref().unwrap();
        assert_eq!(file.name, "pytest in test_cart.py");
        assert_eq!(file.option("scope"), "file");
        let t = |label: &str| f.targets.iter().find(|t| t.label == label).unwrap();
        let round = t("test_round_cents");
        assert_eq!(round.line, 16);
        assert_eq!(round.key, "tests/test_cart.py::test_round_cents");
        assert_eq!(round.configuration.option("test"), "test_round_cents");
        assert_eq!(
            round.configuration.name,
            "pytest for test_cart.test_round_cents"
        );
        let case = t("test_round_cents[half-up]");
        assert_eq!(case.kind, TargetKind::Dataset);
        assert_eq!(case.line, 12);
        assert_eq!(
            case.configuration.option("test"),
            "test_round_cents[half-up]"
        );
        let class = t("TestCart");
        assert_eq!(class.kind, TargetKind::Class);
        assert_eq!(class.key, "tests/test_cart.py");
        assert_eq!(class.configuration.option("scope"), "test");
        assert_eq!(class.configuration.option("test"), "TestCart");
        let nested = t("test_apply");
        assert_eq!(
            nested.configuration.option("test"),
            "TestCart::TestVouchers::test_apply"
        );
        assert!(test_file("tests/test_cart.py", &s, Runner::Unittest).is_empty());
    }

    #[test]
    fn unittest_cases_in_any_module() {
        let src = "import unittest\n\nclass Base(unittest.TestCase):\n    def test_a(self): pass\n\nclass More(Base):\n    def test_b(self): pass\n    def helper(self): pass\n\nclass Plain:\n    def test_c(self): pass\n\ndef test_d(): pass\n";
        let s = scan("shop/checks.py", src);
        let names: Vec<_> = s.items.iter().map(|i| i.dotted()).collect();
        assert_eq!(names, ["Base", "Base.test_a", "More", "More.test_b"]);
        assert!(s.items.iter().all(|i| i.unittest));
        assert_eq!(s.suggested_runner(), Runner::Unittest);
        let f = test_file("shop/checks.py", &s, Runner::Unittest);
        assert_eq!(f.framework.as_deref(), Some("unittest"));
        let b = f.targets.iter().find(|t| t.label == "test_b").unwrap();
        assert_eq!(b.configuration.type_id, "unittest");
        assert_eq!(b.configuration.option("test"), "More.test_b");
        assert_eq!(b.configuration.name, "Unittests for checks.More.test_b");
        // pytest runs unittest cases too.
        let p = test_file("shop/checks.py", &s, Runner::Pytest);
        assert_eq!(p.targets.len(), 4);
        assert!(scan("shop/models.py", "def test_x(): pass\n")
            .items
            .is_empty());
    }

    #[test]
    fn stacked_parametrize_multiplies() {
        let src = "import pytest\n@pytest.mark.parametrize('x', [0, 1])\n@pytest.mark.parametrize('y', [2, 3])\ndef test_foo(x, y): pass\n\n@pytest.mark.parametrize('v', ['a', 'a', obj])\ndef test_dupes(v): pass\n";
        let s = scan("test_m.py", src);
        let ids: Vec<_> = s.items[0].cases.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["2-0", "2-1", "3-0", "3-1"]);
        let ids: Vec<_> = s.items[1].cases.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a0", "a1", "v2"]);
        assert!(is_pytest_file("a/b_test.py") && !is_pytest_file("a/conftest.py"));
        assert_eq!(string_literal("r'a\\'b'").as_deref(), Some("a'b"));
    }
}
