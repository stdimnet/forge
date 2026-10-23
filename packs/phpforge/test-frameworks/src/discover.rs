//! The tests of a file, for the gutter ▶ icons and Run Context Configuration
//! (Ctrl+Shift+F10), each with the temporary run configuration that runs it:
//!
//! | File | Targets |
//! | --- | --- |
//! | PHPUnit test class (`*Test`, `extends …TestCase`) | the class, `test*` / `@test` / `#[Test]` methods |
//! | Pest file (`test()`, `it()`, `describe()` calls) | every call and the rows of inline `->with([...])` datasets |
//! | Codeception `*Cest.php` / `*Cept.php` / `extends \Codeception\Test\Unit` | the class or file, public / `test*` methods |
//! | Behat `.feature` | the feature, every scenario and example row |
//! | PHPSpec `*Spec.php` (`extends ObjectBehavior`) | the spec, `it_*` / `its_*` examples |
//!
//! `key` names the test like the test tree of a run does (`<path>` for classes and files,
//! `<path>::<test>` for tests), so the gutter can show each test's last result.

use forge_runtools::Configuration;
use regex_lite::Regex;
use serde::Serialize;

use crate::frameworks::Framework;
use crate::gherkin;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetKind {
    Class,
    Method,
    File,
    Describe,
    Test,
    Dataset,
    Feature,
    Scenario,
    Example,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestTarget {
    /// 0-based line of the declaration.
    pub line: u32,
    pub kind: TargetKind,
    /// Shown as `Run '<label>'`.
    pub label: String,
    /// Result key (empty: no result is shown for it).
    pub key: String,
    pub configuration: Configuration,
}

/// The tests of a file and the configuration that runs the whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestFile {
    pub framework: Option<Framework>,
    pub targets: Vec<TestTarget>,
    /// Runs every test of the file (Run Context Configuration outside a test).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<Configuration>,
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn config(name: &str, framework: Framework, options: &[(&str, &str)]) -> Configuration {
    let mut c = Configuration::new(name, framework.id());
    for (k, v) in options {
        c = c.with_option(k, *v);
    }
    c
}

fn target(
    line: usize,
    kind: TargetKind,
    label: &str,
    key: String,
    configuration: Configuration,
) -> TestTarget {
    TestTarget {
        line: line as u32,
        kind,
        label: label.to_string(),
        key,
        configuration,
    }
}

// ---- PHP classes ------------------------------------------------------------------------

struct PhpClass<'a> {
    line: usize,
    name: &'a str,
    parent: &'a str,
    is_abstract: bool,
}

struct PhpMethod<'a> {
    line: usize,
    name: &'a str,
    public: bool,
    /// Doc comment and attributes directly above.
    before: String,
}

struct PhpFile<'a> {
    namespace: Option<&'a str>,
    uses: Vec<&'a str>,
    classes: Vec<(PhpClass<'a>, Vec<PhpMethod<'a>>)>,
}

fn scan_php(text: &str) -> PhpFile<'_> {
    let ns_re = Regex::new(r"^\s*namespace\s+([\w\\]+)\s*;").unwrap();
    let use_re = Regex::new(r"^\s*use\s+([\w\\]+)\s*;").unwrap();
    let class_re = Regex::new(
        r"^\s*((?:(?:final|abstract|readonly)\s+)*)class\s+(\w+)(?:\s+extends\s+([\\\w]+))?",
    )
    .unwrap();
    let method_re = Regex::new(
        r"^\s*((?:(?:public|protected|private|static|final|abstract)\s+)*)function\s+(\w+)\s*\(",
    )
    .unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let mut file = PhpFile {
        namespace: None,
        uses: Vec::new(),
        classes: Vec::new(),
    };
    for (i, line) in lines.iter().enumerate() {
        if let Some(c) = ns_re.captures(line) {
            file.namespace = Some(c.get(1).unwrap().as_str());
            continue;
        }
        if let Some(c) = use_re.captures(line) {
            file.uses.push(c.get(1).unwrap().as_str());
            continue;
        }
        if let Some(c) = class_re.captures(line) {
            file.classes.push((
                PhpClass {
                    line: i,
                    name: c.get(2).unwrap().as_str(),
                    parent: c.get(3).map_or("", |m| m.as_str()),
                    is_abstract: c.get(1).is_some_and(|m| m.as_str().contains("abstract")),
                },
                Vec::new(),
            ));
            continue;
        }
        let Some(m) = method_re.captures(line) else {
            continue;
        };
        let Some((_, methods)) = file.classes.last_mut() else {
            continue;
        };
        let modifiers = m.get(1).map_or("", |x| x.as_str());
        let mut j = i;
        while j > 0 {
            let t = lines[j - 1].trim_start();
            if t.starts_with("#[") || t.starts_with("/**") || t.starts_with('*') || t.is_empty() {
                j -= 1;
            } else {
                break;
            }
        }
        methods.push(PhpMethod {
            line: i,
            name: m.get(2).unwrap().as_str(),
            public: modifiers.contains("public")
                || !(modifiers.contains("protected") || modifiers.contains("private")),
            before: lines[j..i].join("\n"),
        });
    }
    file
}

impl PhpFile<'_> {
    fn fqcn(&self, name: &str) -> String {
        match self.namespace {
            Some(ns) => format!("{ns}\\{name}"),
            None => name.to_string(),
        }
    }

    /// The parent class with `use` imports resolved (`Unit` → `Codeception\Test\Unit`).
    fn parent_of(&self, class: &PhpClass) -> String {
        let p = class.parent.trim_start_matches('\\');
        if class.parent.starts_with('\\') || p.contains('\\') {
            return p.to_string();
        }
        self.uses
            .iter()
            .find(|u| u.rsplit('\\').next() == Some(p))
            .map(|u| u.to_string())
            .unwrap_or_else(|| p.to_string())
    }
}

fn phpunit(path: &str, php: &PhpFile) -> Vec<TestTarget> {
    let test_attr = Regex::new(r"#\[\s*(?:\\?PHPUnit\\Framework\\Attributes\\)?Test\s*\]").unwrap();
    let mut out = Vec::new();
    for (class, methods) in &php.classes {
        let is_test = class.name.ends_with("Test") || class.parent.ends_with("TestCase");
        if !is_test || class.is_abstract {
            continue;
        }
        let fqcn = php.fqcn(class.name);
        out.push(target(
            class.line,
            TargetKind::Class,
            class.name,
            path.to_string(),
            config(
                class.name,
                Framework::Phpunit,
                &[("scope", "class"), ("class", &fqcn), ("file", path)],
            ),
        ));
        for m in methods.iter().filter(|m| m.public) {
            let annotated = test_attr.is_match(&m.before) || m.before.contains("@test");
            if !m.name.starts_with("test") && !annotated {
                continue;
            }
            let name = format!("{}::{}", class.name, m.name);
            out.push(target(
                m.line,
                TargetKind::Method,
                m.name,
                format!("{path}::{}", m.name),
                config(
                    &name,
                    Framework::Phpunit,
                    &[
                        ("scope", "method"),
                        ("class", &fqcn),
                        ("method", m.name),
                        ("file", path),
                    ],
                ),
            ));
        }
    }
    out
}

fn codeception(path: &str, php: &PhpFile) -> Vec<TestTarget> {
    let mut out = Vec::new();
    let file_name = base_name(path);
    if file_name.ends_with("Cept.php") {
        let name = file_name.trim_end_matches(".php");
        out.push(target(
            0,
            TargetKind::File,
            name,
            path.to_string(),
            config(
                name,
                Framework::Codeception,
                &[("scope", "file"), ("file", path)],
            ),
        ));
        return out;
    }
    let cest = file_name.ends_with("Cest.php");
    for (class, methods) in &php.classes {
        if class.is_abstract {
            continue;
        }
        out.push(target(
            class.line,
            TargetKind::Class,
            class.name,
            path.to_string(),
            config(
                class.name,
                Framework::Codeception,
                &[("scope", "file"), ("file", path)],
            ),
        ));
        for m in methods.iter().filter(|m| m.public) {
            let test = if cest {
                !m.name.starts_with('_')
            } else {
                m.name.starts_with("test") || m.before.contains("@test")
            };
            if !test {
                continue;
            }
            let name = format!("{}:{}", class.name, m.name);
            out.push(target(
                m.line,
                TargetKind::Method,
                m.name,
                format!("{path}::{}", m.name),
                config(
                    &name,
                    Framework::Codeception,
                    &[("scope", "method"), ("file", path), ("method", m.name)],
                ),
            ));
        }
    }
    out
}

fn phpspec(path: &str, php: &PhpFile) -> Vec<TestTarget> {
    let mut out = Vec::new();
    for (class, methods) in &php.classes {
        if !class.name.ends_with("Spec") || class.is_abstract {
            continue;
        }
        out.push(target(
            class.line,
            TargetKind::Class,
            class.name,
            path.to_string(),
            config(
                class.name,
                Framework::Phpspec,
                &[("scope", "file"), ("file", path)],
            ),
        ));
        for m in methods.iter().filter(|m| m.public) {
            if !(m.name.starts_with("it_") || m.name.starts_with("its_")) {
                continue;
            }
            let line = (m.line + 1).to_string();
            let name = format!("{}::{}", class.name, m.name);
            out.push(target(
                m.line,
                TargetKind::Method,
                m.name,
                format!("{path}::{}", m.name),
                config(
                    &name,
                    Framework::Phpspec,
                    &[
                        ("scope", "method"),
                        ("file", path),
                        ("line", &line),
                        ("method", m.name),
                    ],
                ),
            ));
        }
    }
    out
}

// ---- Pest -------------------------------------------------------------------------------

/// A `test()` / `it()` / `describe()` call or a dataset row.
struct PestCall {
    line: usize,
    func: String,
    description: String,
    /// Parenthesis depth inside the call's argument list.
    depth: i32,
}

/// Depth of `(` and `[` at the start of every line, and the calls found, ignoring strings
/// and comments.
fn scan_pest(text: &str) -> (Vec<PestCall>, Vec<(usize, i32, String)>) {
    let call_re = Regex::new(r#"^(test|it|describe)\s*\(\s*(['"])"#).unwrap();
    let chars: Vec<char> = text.chars().collect();
    let mut calls = Vec::new();
    // Dataset rows: (line, bracket depth, row text) for lines inside `->with([`.
    let mut rows = Vec::new();
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut line = 0usize;
    let mut line_start = true;
    // Bracket depth of the open `->with([` arrays.
    let mut with_arrays: Vec<i32> = Vec::new();
    let mut i = 0;
    let rest = |i: usize| -> String { chars[i..chars.len().min(i + 200)].iter().collect() };
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            line_start = true;
            i += 1;
            continue;
        }
        if line_start && !c.is_whitespace() {
            line_start = false;
            if let Some(&depth) = with_arrays.last() {
                if bracket == depth + 1 && c != ']' {
                    let text: String = chars[i..].iter().take_while(|x| **x != '\n').collect();
                    rows.push((line, bracket, text.trim().trim_end_matches(',').to_string()));
                }
            }
        }
        // Comments.
        if c == '/' && chars.get(i + 1) == Some(&'/') || c == '#' && chars.get(i + 1) != Some(&'[')
        {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '\'' || c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' {
                    i += 1;
                } else if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        let prev_ident = i > 0
            && (chars[i - 1].is_alphanumeric()
                || chars[i - 1] == '_'
                || chars[i - 1] == '$'
                || chars[i - 1] == '>'
                || chars[i - 1] == ':');
        if !prev_ident && (c == 't' || c == 'i' || c == 'd') {
            let r = rest(i);
            if let Some(m) = call_re.captures(&r) {
                let quote = m.get(2).unwrap().as_str().chars().next().unwrap();
                let start = m.get(0).unwrap().end();
                let desc: String = r[start..]
                    .chars()
                    .scan(false, |esc, ch| {
                        if *esc {
                            *esc = false;
                            return Some(Some(ch));
                        }
                        if ch == '\\' {
                            *esc = true;
                            return Some(None);
                        }
                        (ch != quote).then_some(Some(ch))
                    })
                    .flatten()
                    .collect();
                calls.push(PestCall {
                    line,
                    func: m.get(1).unwrap().as_str().to_string(),
                    description: desc,
                    depth: paren + 1,
                });
            }
        }
        if c == '-' && rest(i).starts_with("->with(") {
            let after: String = rest(i + 7);
            if after.trim_start().starts_with('[') {
                with_arrays.push(bracket);
            }
        }
        match c {
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => {
                bracket -= 1;
                if with_arrays.last() == Some(&bracket) {
                    with_arrays.pop();
                }
            }
            _ => {}
        }
        i += 1;
    }
    (calls, rows)
}

/// Whether a PHP file is a Pest test file (calls `test()`, `it()` or `describe()` outside a
/// class).
fn is_pest(text: &str) -> bool {
    let re = Regex::new(r#"(?m)^(test|it|describe)\s*\(\s*['"]"#).unwrap();
    re.is_match(text)
        && !Regex::new(r"(?m)^\s*(?:final\s+|abstract\s+)*class\s+\w+")
            .unwrap()
            .is_match(text)
}

/// Pest's PHPUnit method name for a description (`Str::evaluable`).
pub fn pest_evaluable(description: &str) -> String {
    let code = format!(
        "__pest_evaluable_{}",
        description.replace('_', "__").replace(' ', "_")
    );
    code.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn pest(path: &str, text: &str) -> Vec<TestTarget> {
    let (calls, rows) = scan_pest(text);
    let mut out = Vec::new();
    // Open describes: (description, depth of their argument list).
    let mut describes: Vec<(String, i32)> = Vec::new();
    let mut tests: Vec<(usize, String)> = Vec::new();
    for call in &calls {
        while describes.last().is_some_and(|(_, d)| call.depth <= *d) {
            describes.pop();
        }
        let own = match call.func.as_str() {
            "it" => format!("it {}", call.description),
            _ => call.description.clone(),
        };
        let name = describes
            .iter()
            .map(|(d, _)| format!("`{d}` → "))
            .chain(std::iter::once(own.clone()))
            .collect::<String>();
        let (kind, key) = if call.func == "describe" {
            describes.push((call.description.clone(), call.depth));
            (TargetKind::Describe, String::new())
        } else {
            tests.push((call.line, name.clone()));
            (TargetKind::Test, format!("{path}::{name}"))
        };
        let filter_name = if call.func == "describe" {
            format!("`{}`", call.description)
        } else {
            name.clone()
        };
        out.push(target(
            call.line,
            kind,
            &own,
            key,
            config(
                &name,
                Framework::Pest,
                &[("scope", "test"), ("file", path), ("test", &filter_name)],
            ),
        ));
    }
    for (line, _, row) in rows {
        // The test the dataset belongs to: the last one declared above it.
        let Some((_, test)) = tests.iter().rev().find(|(l, _)| *l < line) else {
            continue;
        };
        let value = match row.split_once("=>") {
            Some((key, _)) => key.trim().trim_matches(['\'', '"']).to_string(),
            None => row.trim().to_string(),
        };
        if value.is_empty() {
            continue;
        }
        let label = format!("{test} with {value}");
        out.push(target(
            line,
            TargetKind::Dataset,
            &label,
            String::new(),
            config(
                &label,
                Framework::Pest,
                &[
                    ("scope", "test"),
                    ("file", path),
                    ("test", test),
                    ("dataset", &value),
                ],
            ),
        ));
    }
    out.sort_by_key(|t| t.line);
    out
}

// ---- Behat ------------------------------------------------------------------------------

fn behat(path: &str, text: &str) -> Vec<TestTarget> {
    let feature = gherkin::parse(text);
    let mut out = Vec::new();
    let feature_name = if feature.name.is_empty() {
        base_name(path).trim_end_matches(".feature").to_string()
    } else {
        feature.name.clone()
    };
    if feature.line > 0 {
        out.push(target(
            feature.line as usize - 1,
            TargetKind::Feature,
            &feature_name,
            path.to_string(),
            config(
                &feature_name,
                Framework::Behat,
                &[("scope", "file"), ("file", path)],
            ),
        ));
    }
    for s in &feature.scenarios {
        let line = s.line.to_string();
        out.push(target(
            s.line as usize - 1,
            TargetKind::Scenario,
            &s.name,
            format!("{path}::{}", s.name),
            config(
                &format!("{feature_name}: {}", s.name),
                Framework::Behat,
                &[("scope", "scenario"), ("file", path), ("line", &line)],
            ),
        ));
        for (n, row) in s.examples.iter().enumerate() {
            let label = format!("{} #{}", s.name, n + 1);
            let line = row.to_string();
            out.push(target(
                *row as usize - 1,
                TargetKind::Example,
                &label,
                String::new(),
                config(
                    &format!("{feature_name}: {label}"),
                    Framework::Behat,
                    &[("scope", "scenario"), ("file", path), ("line", &line)],
                ),
            ));
        }
    }
    out
}

/// The tests of a file (`path` workspace-relative, `text` its editor content).
pub fn discover(path: &str, text: &str) -> TestFile {
    let name = base_name(path);
    let lower = name.to_ascii_lowercase();
    let file_config = |f: Framework| {
        Some(config(
            name.trim_end_matches(".php"),
            f,
            &[("scope", "file"), ("file", path)],
        ))
    };
    if lower.ends_with(".feature") {
        return TestFile {
            framework: Some(Framework::Behat),
            targets: behat(path, text),
            file: file_config(Framework::Behat),
        };
    }
    if !lower.ends_with(".php") {
        return TestFile {
            framework: None,
            targets: Vec::new(),
            file: None,
        };
    }
    if is_pest(text) {
        let targets = pest(path, text);
        return TestFile {
            framework: Some(Framework::Pest),
            file: (!targets.is_empty())
                .then(|| file_config(Framework::Pest))
                .flatten(),
            targets,
        };
    }
    let php = scan_php(text);
    let parents: Vec<String> = php.classes.iter().map(|(c, _)| php.parent_of(c)).collect();
    let (framework, targets) = if name.ends_with("Cest.php")
        || name.ends_with("Cept.php")
        || parents.iter().any(|p| p.starts_with("Codeception\\"))
    {
        (Framework::Codeception, codeception(path, &php))
    } else if name.ends_with("Spec.php") || parents.iter().any(|p| p.ends_with("ObjectBehavior")) {
        (Framework::Phpspec, phpspec(path, &php))
    } else {
        (Framework::Phpunit, phpunit(path, &php))
    };
    let file = match framework {
        _ if targets.is_empty() => None,
        // PHPUnit runs a file through its class.
        Framework::Phpunit => None,
        f => file_config(f),
    };
    TestFile {
        framework: (!targets.is_empty()).then_some(framework),
        targets,
        file,
    }
}

/// The target at a 0-based line: the last declaration at or above it.
pub fn target_at(file: &TestFile, line: u32) -> Option<&TestTarget> {
    file.targets
        .iter()
        .rfind(|t| t.line <= line && t.kind != TargetKind::Dataset && t.kind != TargetKind::Example)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(file: &TestFile) -> Vec<(u32, TargetKind, String, String)> {
        file.targets
            .iter()
            .map(|t| (t.line, t.kind, t.label.clone(), t.key.clone()))
            .collect()
    }

    #[test]
    fn phpunit_classes_and_methods() {
        let text = "<?php\nnamespace Tests\\Unit;\n\nuse PHPUnit\\Framework\\Attributes\\Test;\n\nfinal class UserTest extends TestCase\n{\n    public function test_full_name(): void {}\n\n    #[Test]\n    public function email_is_normalized(): void {}\n\n    private function helper() {}\n    public function setUp(): void {}\n}\n";
        let f = discover("tests/Unit/UserTest.php", text);
        assert_eq!(f.framework, Some(Framework::Phpunit));
        assert_eq!(
            summary(&f),
            [
                (
                    5,
                    TargetKind::Class,
                    "UserTest".into(),
                    "tests/Unit/UserTest.php".into()
                ),
                (
                    7,
                    TargetKind::Method,
                    "test_full_name".into(),
                    "tests/Unit/UserTest.php::test_full_name".into()
                ),
                (
                    10,
                    TargetKind::Method,
                    "email_is_normalized".into(),
                    "tests/Unit/UserTest.php::email_is_normalized".into()
                ),
            ]
        );
        let m = &f.targets[1].configuration;
        assert_eq!(
            (m.name.as_str(), m.type_id.as_str()),
            ("UserTest::test_full_name", "phpunit")
        );
        assert_eq!(m.option("class"), "Tests\\Unit\\UserTest");
        assert_eq!(m.option("scope"), "method");
        assert!(f.file.is_none());
        assert!(discover("app/User.php", "<?php class User {}")
            .targets
            .is_empty());
    }

    const PEST: &str = r#"<?php

use App\Cart;

beforeEach(function () {
    $this->cart = new Cart();
});

test('starts empty', function () {
    expect($this->cart->count())->toBe(0); // it('not a test')
});

describe('Cart', function () {
    it('adds items', function (int $n) {
        expect(true)->toBeTrue();
    })->with([
        'one' => [1],
        'two' => [2],
    ]);

    describe('totals', function () {
        it("sums \"prices\"", fn () => expect(1)->toBe(1));
    });
});

it('has emails', function (string $email) {
})->with([
    'enunomaduro@gmail.com',
    'other@example.com',
]);
"#;

    #[test]
    fn pest_tests_describes_and_dataset_rows() {
        let f = discover("tests/Unit/CartTest.php", PEST);
        assert_eq!(f.framework, Some(Framework::Pest));
        let p = "tests/Unit/CartTest.php";
        assert_eq!(
            summary(&f),
            [
                (
                    8,
                    TargetKind::Test,
                    "starts empty".into(),
                    format!("{p}::starts empty")
                ),
                (12, TargetKind::Describe, "Cart".into(), String::new()),
                (
                    13,
                    TargetKind::Test,
                    "it adds items".into(),
                    format!("{p}::`Cart` → it adds items")
                ),
                (
                    16,
                    TargetKind::Dataset,
                    "`Cart` → it adds items with one".into(),
                    String::new()
                ),
                (
                    17,
                    TargetKind::Dataset,
                    "`Cart` → it adds items with two".into(),
                    String::new()
                ),
                (20, TargetKind::Describe, "totals".into(), String::new()),
                (
                    21,
                    TargetKind::Test,
                    "it sums \"prices\"".into(),
                    format!("{p}::`Cart` → `totals` → it sums \"prices\"")
                ),
                (
                    25,
                    TargetKind::Test,
                    "it has emails".into(),
                    format!("{p}::it has emails")
                ),
                (
                    27,
                    TargetKind::Dataset,
                    "it has emails with 'enunomaduro@gmail.com'".into(),
                    String::new()
                ),
                (
                    28,
                    TargetKind::Dataset,
                    "it has emails with 'other@example.com'".into(),
                    String::new()
                ),
            ]
        );
        let describe = &f.targets[1].configuration;
        assert_eq!(describe.option("test"), "`Cart`");
        assert_eq!(f.targets[3].configuration.option("dataset"), "one");
        assert_eq!(f.file.as_ref().unwrap().option("scope"), "file");
        assert_eq!(target_at(&f, 18).unwrap().label, "it adds items");
        assert_eq!(
            pest_evaluable("`Cart` → it adds_items"),
            "__pest_evaluable__Cart__→_it_adds__items"
        );
    }

    #[test]
    fn codeception_cests_cepts_and_unit_tests() {
        let cest = "<?php\nnamespace Tests\\Acceptance;\n\nclass LoginCest\n{\n    public function _before(AcceptanceTester $I) {}\n\n    public function tryToLogin(AcceptanceTester $I) {}\n    protected function helper() {}\n}\n";
        let f = discover("tests/Acceptance/LoginCest.php", cest);
        assert_eq!(f.framework, Some(Framework::Codeception));
        assert_eq!(f.targets.len(), 2);
        let m = &f.targets[1].configuration;
        assert_eq!(
            (m.name.as_str(), m.option("method")),
            ("LoginCest:tryToLogin", "tryToLogin")
        );
        let cept = discover(
            "tests/Acceptance/SignupCept.php",
            "<?php\n$I = new AcceptanceTester($scenario);\n",
        );
        assert_eq!(
            summary(&cept),
            [(
                0,
                TargetKind::File,
                "SignupCept".into(),
                "tests/Acceptance/SignupCept.php".into()
            )]
        );
        let unit = "<?php\nuse Codeception\\Test\\Unit;\n\nclass CartTest extends Unit\n{\n    public function testTotal() {}\n}\n";
        let u = discover("tests/Unit/CartTest.php", unit);
        assert_eq!(u.framework, Some(Framework::Codeception));
        assert_eq!(u.targets[1].configuration.type_id, "codeception");
    }

    #[test]
    fn behat_features_scenarios_and_examples() {
        let text = "Feature: Checkout\n\n  Scenario: Buying one\n    Given a product\n\n  Scenario Outline: Buying <n>\n    When I buy <n>\n\n    Examples:\n      | n |\n      | 1 |\n      | 2 |\n";
        let f = discover("features/checkout.feature", text);
        let p = "features/checkout.feature";
        assert_eq!(
            summary(&f),
            [
                (0, TargetKind::Feature, "Checkout".into(), p.to_string()),
                (
                    2,
                    TargetKind::Scenario,
                    "Buying one".into(),
                    format!("{p}::Buying one")
                ),
                (
                    5,
                    TargetKind::Scenario,
                    "Buying <n>".into(),
                    format!("{p}::Buying <n>")
                ),
                (
                    10,
                    TargetKind::Example,
                    "Buying <n> #1".into(),
                    String::new()
                ),
                (
                    11,
                    TargetKind::Example,
                    "Buying <n> #2".into(),
                    String::new()
                ),
            ]
        );
        let s = &f.targets[1].configuration;
        assert_eq!(
            (s.name.as_str(), s.option("line")),
            ("Checkout: Buying one", "3")
        );
        assert_eq!(f.targets[4].configuration.option("line"), "12");
    }

    #[test]
    fn phpspec_examples() {
        let text = "<?php\n\nnamespace spec\\Acme;\n\nuse PhpSpec\\ObjectBehavior;\n\nclass MarkdownSpec extends ObjectBehavior\n{\n    function it_is_initializable() {}\n\n    function its_title_is_empty() {}\n\n    function let() {}\n}\n";
        let f = discover("spec/Acme/MarkdownSpec.php", text);
        assert_eq!(f.framework, Some(Framework::Phpspec));
        let names: Vec<_> = f.targets.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            names,
            ["MarkdownSpec", "it_is_initializable", "its_title_is_empty"]
        );
        let c = &f.targets[1].configuration;
        assert_eq!(
            (c.option("line"), c.option("method")),
            ("9", "it_is_initializable")
        );
        assert_eq!(
            f.targets[1].key,
            "spec/Acme/MarkdownSpec.php::it_is_initializable"
        );
    }
}
