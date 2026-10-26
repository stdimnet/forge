//! Tests, benchmarks, fuzz tests and examples of a `_test.go` file, as `go test` finds them:
//! top-level `func TestXxx(t *testing.T)`, `func BenchmarkXxx(b *testing.B)`,
//! `func FuzzXxx(f *testing.F)` and `func ExampleXxx()`, plus subtests and sub-benchmarks
//! started with a string literal name (`t.Run("rounds", func(t *testing.T) { … })`),
//! nested to any depth. Names are the ones `go test -run` matches: subtests joined with `/`,
//! spaces replaced by `_`.

use serde::{Deserialize, Serialize};

use crate::scan::{matching, tokens, Tok, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoTestKind {
    Test,
    Benchmark,
    Fuzz,
    Example,
}

impl GoTestKind {
    /// The `testing` type of the function's parameter.
    fn param_type(self) -> Option<&'static str> {
        match self {
            GoTestKind::Test => Some("T"),
            GoTestKind::Benchmark => Some("B"),
            GoTestKind::Fuzz => Some("F"),
            GoTestKind::Example => None,
        }
    }

    fn of_name(name: &str) -> Option<GoTestKind> {
        let (kind, rest) = if let Some(r) = name.strip_prefix("Test") {
            (GoTestKind::Test, r)
        } else if let Some(r) = name.strip_prefix("Benchmark") {
            (GoTestKind::Benchmark, r)
        } else if let Some(r) = name.strip_prefix("Fuzz") {
            (GoTestKind::Fuzz, r)
        } else if let Some(r) = name.strip_prefix("Example") {
            (GoTestKind::Example, r)
        } else {
            return None;
        };
        // `Testing` is not a test: the rest must not start with a lower-case letter.
        if rest.chars().next().is_some_and(|c| c.is_lowercase()) {
            return None;
        }
        if kind == GoTestKind::Test && name == "TestMain" {
            return None;
        }
        Some(kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoTest {
    /// The full name `go test -run` matches (`TestCart/adds_items`).
    pub name: String,
    /// The function's or subtest's own name (`adds items` as written).
    pub label: String,
    pub kind: GoTestKind,
    /// 0-based line of the declaration or `Run` call.
    pub line: u32,
    /// Nesting depth: 0 for functions.
    pub depth: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoTestFile {
    /// The `package` clause (`cart` or `cart_test`).
    pub package: String,
    /// Functions first come in source order, each followed by its subtests.
    pub tests: Vec<GoTest>,
}

/// `go test`'s rewrite of a subtest name: spaces become underscores, non-printable
/// characters are escaped.
pub fn subtest_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_whitespace() {
            out.push('_');
        } else if c.is_control() {
            out.push_str(&c.escape_unicode().to_string());
        } else {
            out.push(c);
        }
    }
    out
}

/// `name *testing.T` at `i` (the token after `(`): the parameter name.
fn testing_param<'a>(t: &[Token<'a>], i: usize, ty: &str) -> Option<&'a str> {
    let name = t.get(i)?.ident()?;
    let star = t.get(i + 1)?;
    let pkg = t.get(i + 2)?;
    let dot = t.get(i + 3)?;
    let typ = t.get(i + 4)?;
    let close = t.get(i + 5)?;
    (star.is_punct('*')
        && pkg.is_ident("testing")
        && dot.is_punct('.')
        && typ.is_ident(ty)
        && close.is_punct(')'))
    .then_some(name)
}

/// Subtests started in `t[from..to]` with `param.Run("name", func(x *testing.T) {…})`.
fn subtests(
    t: &[Token<'_>],
    from: usize,
    to: usize,
    param: &str,
    ty: &str,
    parent: &str,
    kind: GoTestKind,
    depth: u32,
    out: &mut Vec<GoTest>,
) {
    let mut i = from;
    while i < to {
        let is_run = t[i].is_ident(param)
            && t.get(i + 1).is_some_and(|x| x.is_punct('.'))
            && t.get(i + 2).is_some_and(|x| x.is_ident("Run"))
            && t.get(i + 3).is_some_and(|x| x.is_punct('('));
        if !is_run {
            i += 1;
            continue;
        }
        let call_end = matching(t, i + 3).min(to);
        let Some(Tok::Str(name)) = t.get(i + 4).map(|x| &x.tok) else {
            // A computed name (table-driven tests): not discoverable.
            i = call_end + 1;
            continue;
        };
        if !t.get(i + 5).is_some_and(|x| x.is_punct(',')) {
            i = call_end + 1;
            continue;
        }
        let full = format!("{parent}/{}", subtest_name(name));
        out.push(GoTest {
            name: full.clone(),
            label: name.clone(),
            kind,
            line: t[i].line,
            depth,
        });
        // The closure: `func ( x * testing . T ) {`.
        if t.get(i + 6).is_some_and(|x| x.is_ident("func"))
            && t.get(i + 7).is_some_and(|x| x.is_punct('('))
        {
            if let Some(inner) = testing_param(t, i + 8, ty) {
                let open = i + 14;
                if t.get(open).is_some_and(|x| x.is_punct('{')) {
                    let close = matching(t, open).min(call_end);
                    subtests(t, open + 1, close, inner, ty, &full, kind, depth + 1, out);
                }
            }
        }
        i = call_end + 1;
    }
}

/// Discover the tests of a Go test file.
pub fn discover(text: &str) -> GoTestFile {
    let t = tokens(text);
    let mut file = GoTestFile::default();
    let mut depth = 0i32;
    let mut i = 0;
    while i < t.len() {
        let tok = &t[i];
        if tok.is_punct('{') || tok.is_punct('(') {
            depth += 1;
        } else if tok.is_punct('}') || tok.is_punct(')') {
            depth -= 1;
        }
        if depth == 0 && tok.is_ident("package") && file.package.is_empty() {
            if let Some(name) = t.get(i + 1).and_then(Token::ident) {
                file.package = name.to_string();
            }
        }
        // `func Name(` at the top level (methods have a receiver: `func (`).
        let top_func = depth == 0
            && tok.is_ident("func")
            && t.get(i + 2).is_some_and(|x| x.is_punct('('));
        let Some(name) = top_func.then(|| t.get(i + 1).and_then(Token::ident)).flatten() else {
            i += 1;
            continue;
        };
        let Some(kind) = GoTestKind::of_name(name) else {
            i += 1;
            continue;
        };
        let line = tok.line;
        let params_end = matching(&t, i + 2);
        let param = match kind.param_type() {
            Some(ty) => match testing_param(&t, i + 3, ty) {
                Some(p) => Some((p, ty)),
                None => {
                    i = params_end + 1;
                    continue;
                }
            },
            // Examples take no parameters.
            None if params_end == i + 3 => None,
            None => {
                i = params_end + 1;
                continue;
            }
        };
        file.tests.push(GoTest {
            name: name.to_string(),
            label: name.to_string(),
            kind,
            line,
            depth: 0,
        });
        // The body: the first `{` after the parameters (results may follow).
        let Some(open) = (params_end + 1..t.len()).find(|&k| t[k].is_punct('{')) else {
            break;
        };
        let close = matching(&t, open);
        if let Some((p, ty)) = param {
            if kind != GoTestKind::Fuzz {
                subtests(&t, open + 1, close, p, ty, name, kind, 1, &mut file.tests);
            }
        }
        i = close + 1;
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const CART_TEST: &str = r#"package cart

import (
	"fmt"
	"testing"
)

func TestMain(m *testing.M) { m.Run() }

func TestAdd(t *testing.T) {
	c := New()
	t.Run("adds items", func(t *testing.T) {
		t.Run(`keeps order`, func(t *testing.T) {})
	})
	t.Run("rounds", func(st *testing.T) {
		st.Parallel()
	})
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {})
	}
}

func Testing(t *testing.T) {}
func helper(t *testing.T) {}

func (s *suite) TestMethod(t *testing.T) {}

func BenchmarkTotal(b *testing.B) {
	b.Run("small", func(b *testing.B) {})
}

func FuzzParse(f *testing.F) {
	f.Fuzz(func(t *testing.T, s string) { t.Run("x", func(t *testing.T) {}) })
}

func ExampleCart_Total() {
	fmt.Println("x")
	// Output: x
}

func ExampleBad(t *testing.T) {}
"#;

    #[test]
    fn finds_functions_and_literal_subtests() {
        let f = discover(CART_TEST);
        assert_eq!(f.package, "cart");
        let rows: Vec<_> = f
            .tests
            .iter()
            .map(|t| (t.name.as_str(), t.kind, t.line, t.depth))
            .collect();
        assert_eq!(
            rows,
            [
                ("TestAdd", GoTestKind::Test, 9, 0),
                ("TestAdd/adds_items", GoTestKind::Test, 11, 1),
                ("TestAdd/adds_items/keeps_order", GoTestKind::Test, 12, 2),
                ("TestAdd/rounds", GoTestKind::Test, 14, 1),
                ("BenchmarkTotal", GoTestKind::Benchmark, 27, 0),
                ("BenchmarkTotal/small", GoTestKind::Benchmark, 28, 1),
                ("FuzzParse", GoTestKind::Fuzz, 31, 0),
                ("ExampleCart_Total", GoTestKind::Example, 35, 0),
            ]
        );
        assert_eq!(f.tests[1].label, "adds items");
    }

    #[test]
    fn subtest_names_follow_go_test() {
        assert_eq!(subtest_name("half up 2"), "half_up_2");
        assert_eq!(subtest_name("a/b"), "a/b");
    }
}
