//! Kotest specs read from source: FunSpec (`test`, `context`), StringSpec (`"name" { }`),
//! DescribeSpec (`describe`, `context`, `it`) and BehaviorSpec (`given`, `when`, `and`,
//! `then`), declared in the constructor lambda (`class CartSpec : FunSpec({ … })`) or in
//! `init { }` blocks. Disabled variants (`xtest`, `xit`, `xdescribe`, …) and
//! `.config(…)` calls are recognized too.

use serde::{Deserialize, Serialize};

use crate::source::{parse, skip_balanced, KotlinFile, Tok, Token};

/// A Kotest spec style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpecStyle {
    FunSpec,
    StringSpec,
    DescribeSpec,
    BehaviorSpec,
}

impl SpecStyle {
    pub fn from_supertype(simple: &str) -> Option<Self> {
        Some(match simple {
            "FunSpec" => SpecStyle::FunSpec,
            "StringSpec" => SpecStyle::StringSpec,
            "DescribeSpec" => SpecStyle::DescribeSpec,
            "BehaviorSpec" => SpecStyle::BehaviorSpec,
            _ => return None,
        })
    }

    /// Whether a call names a container (`true`), a test (`false`) or neither (`None`).
    fn classify(self, call: &str) -> Option<bool> {
        let call = call
            .strip_prefix('x')
            .filter(|c| !c.is_empty())
            .unwrap_or(call);
        let lower = call.to_ascii_lowercase();
        match self {
            SpecStyle::FunSpec => match call {
                "context" => Some(true),
                "test" => Some(false),
                _ => None,
            },
            SpecStyle::StringSpec => None,
            SpecStyle::DescribeSpec => match call {
                "describe" | "context" => Some(true),
                "it" => Some(false),
                _ => None,
            },
            SpecStyle::BehaviorSpec => match lower.as_str() {
                "given" | "when" | "and" | "feature" => Some(true),
                "then" => Some(false),
                _ => None,
            },
        }
    }

    /// The prefix Kotest gives a container's name (`Given: `, `When: `, …).
    fn prefix(self, call: &str) -> &'static str {
        let call = call
            .strip_prefix('x')
            .filter(|c| !c.is_empty())
            .unwrap_or(call);
        match (self, call.to_ascii_lowercase().as_str()) {
            (SpecStyle::BehaviorSpec, "given") => "Given: ",
            (SpecStyle::BehaviorSpec, "when") => "When: ",
            (SpecStyle::BehaviorSpec, "and") => "And: ",
            (SpecStyle::BehaviorSpec, "then") => "Then: ",
            _ => "",
        }
    }
}

/// A test or container of a spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KotestTest {
    /// The name as Kotest reports it (`Then: it is empty` in BehaviorSpec).
    pub name: String,
    /// The names of the enclosing containers and this one.
    pub path: Vec<String>,
    pub line: u32,
    pub container: bool,
    /// `xtest`, `xit`, …
    #[serde(default)]
    pub disabled: bool,
}

impl KotestTest {
    /// The test path as Kotest's filters take it (`cart -- adds items`).
    pub fn filter_path(&self) -> String {
        self.path.join(" -- ")
    }
}

/// A spec class and its tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KotestSpec {
    pub name: String,
    /// Binary name (`com.acme.CartSpec`).
    pub class: String,
    pub line: u32,
    pub style: SpecStyle,
    pub tests: Vec<KotestTest>,
}

/// The Kotest specs of a file (empty without any).
pub fn discover_specs(text: &str) -> Vec<KotestSpec> {
    specs_of(&parse(text))
}

pub(crate) fn specs_of(file: &KotlinFile) -> Vec<KotestSpec> {
    let kotest = file.imports.iter().any(|i| i.starts_with("io.kotest"));
    let mut out = Vec::new();
    for c in &file.classes {
        if c.has_modifier("abstract") {
            continue;
        }
        let Some((style, args)) = c.supertypes.iter().find_map(|s| {
            let qualified_ok = kotest || s.name.starts_with("io.kotest.");
            let style = SpecStyle::from_supertype(s.simple()).filter(|_| qualified_ok)?;
            Some((style, s.arguments))
        }) else {
            continue;
        };
        let mut ranges: Vec<(usize, usize)> = args.into_iter().collect();
        ranges.extend(c.init_blocks.iter().copied());
        let mut tests = Vec::new();
        for (start, end) in ranges {
            walk(&file.tokens, start, end, style, &mut Vec::new(), &mut tests);
        }
        out.push(KotestSpec {
            name: c.name.clone(),
            class: c.binary_name.clone(),
            line: c.line,
            style,
            tests,
        });
    }
    out
}

/// After a test's name: `.config(…)` calls, then the index of its `{`.
fn body_after(toks: &[Token], mut i: usize, end: usize) -> Option<usize> {
    while i < end {
        if toks[i].is('{') {
            return Some(i);
        }
        if toks[i].is('.')
            && toks.get(i + 1).and_then(Token::keyword) == Some("config")
            && toks.get(i + 2).is_some_and(|t| t.is('('))
        {
            i = skip_balanced(toks, i + 2);
            continue;
        }
        return None;
    }
    None
}

fn walk(
    toks: &[Token],
    start: usize,
    end: usize,
    style: SpecStyle,
    path: &mut Vec<String>,
    out: &mut Vec<KotestTest>,
) {
    let end = end.min(toks.len());
    let mut i = start;
    while i < end {
        let t = &toks[i];
        // `test("name") {`, `` `when`("name") { ``
        if let Some(call) = t.ident() {
            let named = toks.get(i + 1).is_some_and(|t| t.is('('))
                && toks.get(i + 2).and_then(Token::string).is_some()
                && toks.get(i + 3).is_some_and(|t| t.is(')'));
            if named {
                if let Some(container) = style.classify(call) {
                    let raw = toks[i + 2].string().unwrap_or_default();
                    if let Some(open) = body_after(toks, i + 4, end) {
                        let close = skip_balanced(toks, open);
                        let name = format!("{}{raw}", style.prefix(call));
                        push(
                            out,
                            path,
                            &name,
                            t.line,
                            container,
                            call.starts_with('x') && call.len() > 1,
                        );
                        if container {
                            path.push(name);
                            walk(toks, open + 1, close.saturating_sub(1), style, path, out);
                            path.pop();
                        }
                        i = close;
                        continue;
                    }
                }
            }
        }
        // StringSpec: `"name" {` / `"name".config(…) {`
        if style == SpecStyle::StringSpec {
            if let Tok::Str(name) = &t.tok {
                let after_operator = i > start
                    && matches!(toks[i - 1].tok, Tok::Punct(c) if c != '{' && c != '}' && c != ')');
                if !after_operator {
                    if let Some(open) = body_after(toks, i + 1, end) {
                        let close = skip_balanced(toks, open);
                        push(out, path, name, t.line, false, false);
                        i = close;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
}

fn push(
    out: &mut Vec<KotestTest>,
    path: &[String],
    name: &str,
    line: u32,
    container: bool,
    disabled: bool,
) {
    let mut full = path.to_vec();
    full.push(name.to_string());
    out.push(KotestTest {
        name: name.to_string(),
        path: full,
        line,
        container,
        disabled,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(spec: &KotestSpec) -> Vec<(String, u32, bool)> {
        spec.tests
            .iter()
            .map(|t| (t.filter_path(), t.line, t.container))
            .collect()
    }

    #[test]
    fn fun_spec_in_constructor_lambda() {
        let src = r#"package com.acme.cart

import io.kotest.core.spec.style.FunSpec
import io.kotest.matchers.shouldBe

class CartSpec : FunSpec({
    test("adds items") {
        Cart().add("a").size shouldBe 1
    }
    context("totals") {
        test("applies the discount").config(enabled = true) {
            val label = "x { y"
        }
        xtest("rounds half up") { }
    }
})
"#;
        let specs = discover_specs(src);
        assert_eq!(specs.len(), 1);
        let s = &specs[0];
        assert_eq!(
            (s.class.as_str(), s.line, s.style),
            ("com.acme.cart.CartSpec", 5, SpecStyle::FunSpec)
        );
        assert_eq!(
            names(s),
            [
                ("adds items".into(), 6, false),
                ("totals".into(), 9, true),
                ("totals -- applies the discount".into(), 10, false),
                ("totals -- rounds half up".into(), 13, false),
            ]
        );
        assert!(s.tests[3].disabled);
    }

    #[test]
    fn string_describe_and_behavior_specs() {
        let src = r#"import io.kotest.core.spec.style.*

class Strings : StringSpec({
    "length is the number of characters" {
        "hello".length shouldBe 5
    }
    "trims".config(invocations = 3) { }
})

class Describe : DescribeSpec() {
    init {
        describe("a stack") {
            it("starts empty") { }
            context("after a push") { it("is not empty") { } }
        }
    }
}

class Behavior : BehaviorSpec({
    given("an empty cart") {
        `when`("an item is added") {
            then("the total is its price") { }
        }
    }
})

abstract class Base : FunSpec()
"#;
        let specs = discover_specs(src);
        assert_eq!(specs.len(), 3);
        assert_eq!(
            names(&specs[0]),
            [
                ("length is the number of characters".into(), 3, false),
                ("trims".into(), 6, false),
            ]
        );
        assert_eq!(
            names(&specs[1]),
            [
                ("a stack".into(), 11, true),
                ("a stack -- starts empty".into(), 12, false),
                ("a stack -- after a push".into(), 13, true),
                ("a stack -- after a push -- is not empty".into(), 13, false),
            ]
        );
        assert_eq!(
            names(&specs[2]).last().unwrap().0,
            "Given: an empty cart -- When: an item is added -- Then: the total is its price"
        );
        assert_eq!(
            specs[2].tests.last().unwrap().name,
            "Then: the total is its price"
        );
        assert!(
            discover_specs("class A : FunSpec({ test(\"x\") {} })").is_empty(),
            "not Kotest without its import"
        );
    }
}
