//! Tests in C++ sources, for the gutter ▶ icons: GoogleTest's `TEST`, `TEST_F`, `TEST_P`,
//! `TYPED_TEST` and `TYPED_TEST_P`, and Catch2's `TEST_CASE`, `TEST_CASE_METHOD`,
//! `SCENARIO`, `TEMPLATE_TEST_CASE` with their nested `SECTION`s (and BDD `GIVEN` / `WHEN` /
//! `THEN`). A small tokenizer skips comments and keeps string literals, so commented-out
//! tests are not found.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Framework {
    GoogleTest,
    Catch2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// `TEST`, `TEST_F`, `TEST_CASE`, `SCENARIO`.
    Test,
    /// `TEST_P`: instantiated with parameters (`Prefix/Suite.Test/0`).
    Parameterized,
    /// `TYPED_TEST`, `TYPED_TEST_P`, `TEMPLATE_TEST_CASE`.
    Typed,
    /// A Catch2 `SECTION` (or `GIVEN` / `WHEN` / `THEN`).
    Section,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CppTest {
    pub framework: Framework,
    pub kind: Kind,
    /// GoogleTest: the test suite (`PolygonTest`); Catch2: empty.
    pub suite: String,
    /// GoogleTest: the test (`AreaOfSquare`); Catch2: the test case or section name.
    pub name: String,
    /// Catch2: the test case and the sections down to this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
    /// Catch2 tags (`[math]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// 0-based line of the macro.
    pub line: u32,
}

impl CppTest {
    /// `PolygonTest.AreaOfSquare`, `median/even count`.
    pub fn full_name(&self) -> String {
        match self.framework {
            Framework::GoogleTest => format!("{}.{}", self.suite, self.name),
            Framework::Catch2 => self.path.join("/"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovered {
    pub framework: Option<Framework>,
    pub tests: Vec<CppTest>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Punct(char),
}

/// Tokens with their byte offsets; comments and preprocessor-irrelevant text are skipped.
fn tokenize(text: &str) -> Vec<(Tok, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        // Raw strings: R"delim(...)delim".
        if c == b'R' && b.get(i + 1) == Some(&b'"') {
            let start = i;
            let open = match text[i + 2..].find('(') {
                Some(o) => i + 2 + o,
                None => break,
            };
            let delim = &text[i + 2..open];
            let close = format!("){delim}\"");
            match text[open + 1..].find(&close) {
                Some(e) => {
                    out.push((Tok::Str(text[open + 1..open + 1 + e].to_string()), start));
                    i = open + 1 + e + close.len();
                }
                None => break,
            }
            continue;
        }
        if c == b'"' || c == b'\'' {
            let start = i;
            let mut s = String::new();
            i += 1;
            while i < b.len() && b[i] != c && b[i] != b'\n' {
                if b[i] == b'\\' && i + 1 < b.len() {
                    let e = b[i + 1];
                    s.push(match e {
                        b'n' => '\n',
                        b't' => '\t',
                        other => other as char,
                    });
                    i += 2;
                    continue;
                }
                // Keep UTF-8 sequences whole.
                let ch_len = text[i..].chars().next().map(char::len_utf8).unwrap_or(1);
                s.push_str(&text[i..i + ch_len]);
                i += ch_len;
            }
            i += 1;
            if c == b'"' {
                out.push((Tok::Str(s), start));
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push((Tok::Ident(text[start..i].to_string()), start));
            continue;
        }
        if matches!(c, b'(' | b')' | b'{' | b'}' | b',' | b';' | b'<' | b'>') {
            out.push((Tok::Punct(c as char), i));
        }
        i += if c.is_ascii() {
            1
        } else {
            text[i..].chars().next().map(char::len_utf8).unwrap_or(1)
        };
    }
    out
}

const GTEST: &[&str] = &[
    "TEST",
    "TEST_F",
    "TEST_P",
    "TYPED_TEST",
    "TYPED_TEST_P",
    "GTEST_TEST",
];
const CATCH_CASES: &[&str] = &[
    "TEST_CASE",
    "TEST_CASE_METHOD",
    "SCENARIO",
    "TEMPLATE_TEST_CASE",
    "TEMPLATE_PRODUCT_TEST_CASE",
    "TEMPLATE_LIST_TEST_CASE",
];
const CATCH_SECTIONS: &[(&str, &str)] = &[
    ("SECTION", ""),
    ("GIVEN", "Given: "),
    ("AND_GIVEN", "And given: "),
    ("WHEN", "When: "),
    ("AND_WHEN", "And when: "),
    ("THEN", "Then: "),
    ("AND_THEN", "And: "),
];

/// The arguments of a macro call starting at the `(` token `open`: each a list of tokens;
/// returns the index after the closing `)`.
fn args(toks: &[(Tok, usize)], open: usize) -> (Vec<Vec<Tok>>, usize) {
    let mut out: Vec<Vec<Tok>> = vec![Vec::new()];
    let mut depth = 0;
    let mut i = open;
    while i < toks.len() {
        match &toks[i].0 {
            Tok::Punct('(') => {
                depth += 1;
                if depth > 1 {
                    out.last_mut().unwrap().push(toks[i].0.clone());
                }
            }
            Tok::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    return (out, i + 1);
                }
                out.last_mut().unwrap().push(toks[i].0.clone());
            }
            Tok::Punct(',') if depth == 1 => out.push(Vec::new()),
            t => out.last_mut().unwrap().push(t.clone()),
        }
        i += 1;
    }
    (out, i)
}

fn ident_arg(a: Option<&Vec<Tok>>) -> Option<String> {
    match a?.as_slice() {
        [Tok::Ident(s)] => Some(s.clone()),
        _ => None,
    }
}

/// A string argument: concatenated literals (`"a" "b"`).
fn str_arg(a: Option<&Vec<Tok>>) -> Option<String> {
    let a = a?;
    if a.is_empty() || !a.iter().all(|t| matches!(t, Tok::Str(_))) {
        return None;
    }
    Some(
        a.iter()
            .map(|t| match t {
                Tok::Str(s) => s.as_str(),
                _ => "",
            })
            .collect(),
    )
}

/// `[math][!mayfail]` → `["math", "!mayfail"]`.
fn tags(s: &str) -> Vec<String> {
    s.split('[')
        .filter_map(|t| t.split(']').next())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Which framework a file includes.
pub fn included_framework(text: &str) -> Option<Framework> {
    for line in text.lines() {
        let l = line.trim_start();
        if !l.starts_with('#') || !l.contains("include") {
            continue;
        }
        if l.contains("gtest/") || l.contains("gmock/") {
            return Some(Framework::GoogleTest);
        }
        if l.contains("catch2/") || l.contains("catch.hpp") || l.contains("catch_amalgamated") {
            return Some(Framework::Catch2);
        }
    }
    None
}

/// The tests of a C++ source.
pub fn discover(text: &str) -> Discovered {
    let toks = tokenize(text);
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_of = |pos: usize| (line_starts.partition_point(|&s| s <= pos) - 1) as u32;
    let included = included_framework(text);
    let mut tests: Vec<CppTest> = Vec::new();
    let mut depth: i32 = 0;
    // Open Catch2 scopes: (name, body depth); the first is the test case.
    let mut scopes: Vec<(String, i32)> = Vec::new();
    // A macro whose body `{` comes next: its name.
    let mut pending: Option<String> = None;
    let mut i = 0;
    while i < toks.len() {
        let (tok, pos) = &toks[i];
        match tok {
            Tok::Punct('{') => {
                depth += 1;
                if let Some(name) = pending.take() {
                    scopes.push((name, depth));
                }
            }
            Tok::Punct('}') => {
                while scopes.last().is_some_and(|(_, d)| *d >= depth) {
                    scopes.pop();
                }
                depth -= 1;
            }
            Tok::Punct(';') => pending = None,
            Tok::Ident(id) if matches!(toks.get(i + 1), Some((Tok::Punct('('), _))) => {
                let id = id.as_str();
                let line = line_of(*pos);
                if GTEST.contains(&id) && included != Some(Framework::Catch2) {
                    let (a, next) = args(&toks, i + 1);
                    if let (Some(suite), Some(name), 2) =
                        (ident_arg(a.first()), ident_arg(a.get(1)), a.len())
                    {
                        let kind = match id {
                            "TEST_P" => Kind::Parameterized,
                            "TYPED_TEST" | "TYPED_TEST_P" => Kind::Typed,
                            _ => Kind::Test,
                        };
                        tests.push(CppTest {
                            framework: Framework::GoogleTest,
                            kind,
                            suite,
                            name,
                            path: Vec::new(),
                            tags: Vec::new(),
                            line,
                        });
                        i = next;
                        continue;
                    }
                }
                if CATCH_CASES.contains(&id) && included != Some(Framework::GoogleTest) {
                    let (a, next) = args(&toks, i + 1);
                    let (name_at, prefix) = match id {
                        "TEST_CASE_METHOD" => (1, ""),
                        "SCENARIO" => (0, "Scenario: "),
                        _ => (0, ""),
                    };
                    if let Some(name) = str_arg(a.get(name_at)) {
                        let name = format!("{prefix}{name}");
                        tests.push(CppTest {
                            framework: Framework::Catch2,
                            kind: if id.starts_with("TEMPLATE") {
                                Kind::Typed
                            } else {
                                Kind::Test
                            },
                            suite: String::new(),
                            name: name.clone(),
                            path: vec![name.clone()],
                            tags: str_arg(a.get(name_at + 1))
                                .map(|t| tags(&t))
                                .unwrap_or_default(),
                            line,
                        });
                        scopes.clear();
                        pending = Some(name);
                        i = next;
                        continue;
                    }
                }
                if let Some((_, prefix)) = CATCH_SECTIONS.iter().find(|(m, _)| *m == id) {
                    if !scopes.is_empty() {
                        let (a, next) = args(&toks, i + 1);
                        if let Some(name) = str_arg(a.first()) {
                            let name = format!("{prefix}{name}");
                            let mut path: Vec<String> =
                                scopes.iter().map(|(n, _)| n.clone()).collect();
                            path.push(name.clone());
                            tests.push(CppTest {
                                framework: Framework::Catch2,
                                kind: Kind::Section,
                                suite: String::new(),
                                name: name.clone(),
                                path,
                                tags: Vec::new(),
                                line,
                            });
                            pending = Some(name);
                            i = next;
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    let framework = included.or_else(|| tests.first().map(|t| t.framework));
    Discovered { framework, tests }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GTEST_FILE: &str = r#"#include <gtest/gtest.h>
#include "shapes.hpp"

// TEST(Commented, Out) {}
/* TEST(Block, Comment) {} */
class PolygonTest : public ::testing::Test {
 protected:
  std::vector<shapes::Point> square_{{0, 0}, {2, 0}, {2, 2}, {0, 2}};
};

TEST_F(PolygonTest, AreaOfSquare) { EXPECT_DOUBLE_EQ(shapes::area(square_), 4.0); }

TEST(Shapes, EmptyPolygonHasNoArea) {
  EXPECT_EQ(shapes::area({}), 0.0);
}

class ScaleTest : public ::testing::TestWithParam<int> {};
TEST_P(ScaleTest, KeepsRatio) { SUCCEED(); }
INSTANTIATE_TEST_SUITE_P(Scaled, ScaleTest, ::testing::Values(2, 3));

template <typename T> class Typed : public ::testing::Test {};
TYPED_TEST_SUITE(Typed, ::testing::Types<int, float>);
TYPED_TEST(Typed, Zero) { EXPECT_EQ(TypeParam{}, TypeParam(0)); }
"#;

    #[test]
    fn googletest_macros() {
        let d = discover(GTEST_FILE);
        assert_eq!(d.framework, Some(Framework::GoogleTest));
        let found: Vec<(String, Kind, u32)> = d
            .tests
            .iter()
            .map(|t| (t.full_name(), t.kind, t.line))
            .collect();
        assert_eq!(
            found,
            [
                ("PolygonTest.AreaOfSquare".into(), Kind::Test, 10),
                ("Shapes.EmptyPolygonHasNoArea".into(), Kind::Test, 12),
                ("ScaleTest.KeepsRatio".into(), Kind::Parameterized, 17),
                ("Typed.Zero".into(), Kind::Typed, 22),
            ]
        );
    }

    const CATCH_FILE: &str = r#"#include <catch2/catch_test_macros.hpp>
#include "stats.hpp"

TEST_CASE("mean of a vector", "[stats][fast]") {
  REQUIRE(mean({1, 2, 3}) == 2);
}

TEST_CASE("median") {
  std::vector<double> v;
  SECTION("odd count") {
    v = {3, 1, 2};
    REQUIRE(median(v) == 2);
  }
  SECTION("even count") {
    v = {1, 2, 3, 4};
    SECTION("sorted") { REQUIRE(median(v) == 2.5); }
  }
}

SCENARIO("vectors can be resized") {
  GIVEN("a vector with items") {
    WHEN("the size is increased") {
      THEN("the capacity grows") {}
    }
  }
}

TEST_CASE_METHOD(Fixture, "uses a fixture", "[fixture]") {}
const char* s = "TEST_CASE(\"not a test\")";
"#;

    #[test]
    fn catch2_cases_and_sections() {
        let d = discover(CATCH_FILE);
        assert_eq!(d.framework, Some(Framework::Catch2));
        let found: Vec<(String, Kind, u32)> = d
            .tests
            .iter()
            .map(|t| (t.full_name(), t.kind, t.line))
            .collect();
        assert_eq!(
            found,
            [
                ("mean of a vector".into(), Kind::Test, 3),
                ("median".into(), Kind::Test, 7),
                ("median/odd count".into(), Kind::Section, 9),
                ("median/even count".into(), Kind::Section, 13),
                ("median/even count/sorted".into(), Kind::Section, 15),
                ("Scenario: vectors can be resized".into(), Kind::Test, 19),
                ("Scenario: vectors can be resized/Given: a vector with items".into(), Kind::Section, 20),
                (
                    "Scenario: vectors can be resized/Given: a vector with items/When: the size is increased".into(),
                    Kind::Section,
                    21
                ),
                (
                    "Scenario: vectors can be resized/Given: a vector with items/When: the size is increased/Then: the capacity grows".into(),
                    Kind::Section,
                    22
                ),
                ("uses a fixture".into(), Kind::Test, 27),
            ]
        );
        assert_eq!(d.tests[0].tags, ["stats", "fast"]);
        assert_eq!(d.tests[9].tags, ["fixture"]);
    }

    #[test]
    fn without_includes_the_macros_decide() {
        let d = discover("TEST(A, B) {}\n");
        assert_eq!(d.framework, Some(Framework::GoogleTest));
        let d = discover("TEST_CASE(\"x\") {}\n");
        assert_eq!(d.framework, Some(Framework::Catch2));
        let d = discover("int main() { return 0; }\n");
        assert_eq!(d, Discovered::default());
        // Raw strings and character literals do not confuse the tokenizer.
        let d = discover("auto s = R\"x(TEST(A, B) {})x\"; char c = '{';\nTEST(C, D) {}\n");
        assert_eq!(d.tests.len(), 1);
        assert_eq!(d.tests[0].full_name(), "C.D");
        assert_eq!(d.tests[0].line, 1);
    }
}
