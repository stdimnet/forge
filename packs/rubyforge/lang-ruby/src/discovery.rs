//! Tests in Ruby files, found with a tolerant line scanner (blocks nest by indentation, as
//! RuboCop-formatted code does):
//!
//! - RSpec (`*_spec.rb`): `RSpec.describe` / `describe` / `context` / `feature` (groups,
//!   also `fdescribe` / `xdescribe`) and `it` / `specify` / `example` / `scenario` (also
//!   `fit` / `xit` and `its(:attr)`); full descriptions join the group and example
//!   descriptions with spaces, like RSpec's `full_description`.
//! - Minitest (`*_test.rb`, `test_*.rb`): test classes (`< Minitest::Test`,
//!   `ActiveSupport::TestCase`, `Test::Unit::TestCase` and other `*Test` / `*TestCase`
//!   superclasses), `def test_*` methods and `test "name" do` blocks (as
//!   `test_name_with_underscores`), and Minitest::Spec `describe` / `it` blocks (methods
//!   `test_0001_<description>`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RubyTestKind {
    /// An RSpec example group or a Minitest class / spec `describe`.
    Group,
    /// An example or test method.
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubyTest {
    pub kind: RubyTestKind,
    /// 0-based line.
    pub line: u32,
    /// The description or method name as written (`adds items`, `test_total`).
    pub name: String,
    /// RSpec: the full description (`Cart #total sums items`); Minitest: the method
    /// (`test_total`, `test_0001_adds items`) or the class (`Shop::CartTest`).
    pub full_name: String,
    /// Minitest: the test class (`Shop::CartTest`, `Cart::#total` for specs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub class: String,
    /// Nesting depth (0 for top-level groups).
    pub depth: u32,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The description argument of a block call: `"text"`, `'text'`, `Constant::Name`,
/// `:symbol` or `"#method"`. Interpolation is kept as written.
pub fn describe_arg(rest: &str) -> Option<String> {
    let r = rest.trim().trim_start_matches('(').trim();
    let q = r.chars().next()?;
    if q == '"' || q == '\'' {
        let mut out = String::new();
        let mut escaped = false;
        for c in r[1..].chars() {
            if escaped {
                out.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                return Some(out);
            } else {
                out.push(c);
            }
        }
        return None;
    }
    if q == ':' {
        let s: String = r[1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '?' || *c == '!')
            .collect();
        return (!s.is_empty()).then_some(s);
    }
    if q.is_ascii_uppercase() {
        let s: String = r
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
            .collect();
        return Some(s.trim_end_matches(':').to_string());
    }
    None
}

/// `word rest` when the line starts with one of `words` followed by a space or `(`.
fn call<'a>(t: &'a str, words: &[&'static str]) -> Option<(&'static str, &'a str)> {
    words.iter().find_map(|w| {
        let rest = t.strip_prefix(w)?;
        (rest.starts_with(' ')
            || rest.starts_with('(')
            || rest.is_empty()
            || rest.starts_with(" do"))
        .then_some((*w, rest))
    })
}

const RSPEC_GROUPS: &[&str] = &[
    "RSpec.describe",
    "RSpec.context",
    "RSpec.feature",
    "describe",
    "context",
    "feature",
    "fdescribe",
    "fcontext",
    "xdescribe",
    "xcontext",
    "shared_examples",
    "shared_context",
];

const RSPEC_EXAMPLES: &[&str] = &[
    "it", "specify", "example", "scenario", "fit", "xit", "fspecify", "xspecify", "its",
];

/// Whether the line opens a block that ends with `end` at the same indentation.
fn opens_block(t: &str) -> bool {
    let code = t.split(" #").next().unwrap_or(t).trim_end();
    code.ends_with(" do") || code.contains(" do |") || code.ends_with(" do|")
}

/// RSpec examples and groups of a spec file.
pub fn discover_rspec(text: &str) -> Vec<RubyTest> {
    let mut out = Vec::new();
    // Open groups: (indent, description).
    let mut groups: Vec<(usize, String)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let ind = indent_of(raw);
        while groups
            .last()
            .is_some_and(|(gi, _)| ind <= *gi && !(t == "end" && ind == *gi))
        {
            groups.pop();
        }
        if t == "end" {
            if groups.last().is_some_and(|(gi, _)| *gi == ind) {
                groups.pop();
            }
            continue;
        }
        let full = |groups: &[(usize, String)], name: &str| {
            let mut parts: Vec<&str> = groups
                .iter()
                .map(|(_, d)| d.as_str())
                .filter(|d| !d.is_empty())
                .collect();
            if !name.is_empty() {
                parts.push(name);
            }
            parts.join(" ")
        };
        if let Some((word, rest)) = call(t, RSPEC_GROUPS) {
            if !opens_block(t) {
                continue;
            }
            let name = describe_arg(rest).unwrap_or_default();
            if word.starts_with("shared_") {
                // Shared groups run where they are included.
                groups.push((ind, String::new()));
                continue;
            }
            out.push(RubyTest {
                kind: RubyTestKind::Group,
                line: i as u32,
                full_name: full(&groups, &name),
                name: name.clone(),
                class: String::new(),
                depth: groups.iter().filter(|(_, d)| !d.is_empty()).count() as u32,
            });
            groups.push((ind, name));
            continue;
        }
        if let Some((word, rest)) = call(t, RSPEC_EXAMPLES) {
            if groups.is_empty() {
                continue;
            }
            if groups.iter().any(|(_, d)| d.is_empty()) {
                continue;
            }
            let mut name = describe_arg(rest).unwrap_or_default();
            if word == "its" {
                name = format!("{name} ");
            }
            // `it { is_expected.to … }` has a generated description: only the line runs it.
            out.push(RubyTest {
                kind: RubyTestKind::Test,
                line: i as u32,
                full_name: full(&groups, name.trim_end()),
                name: name.trim_end().to_string(),
                class: String::new(),
                depth: groups.len() as u32,
            });
            if opens_block(t) {
                groups.push((ind, String::new()));
                // Examples are leaves: the marker keeps `end` from closing the group.
                groups.pop();
            }
        }
    }
    out
}

/// `test "adds items to the cart"` → `test_adds_items_to_the_cart` (ActiveSupport).
pub fn test_method_name(description: &str) -> String {
    format!(
        "test_{}",
        description.split_whitespace().collect::<Vec<_>>().join("_")
    )
}

/// Superclasses whose subclasses are Minitest tests.
fn is_test_superclass(s: &str) -> bool {
    let s = s.trim();
    matches!(
        s,
        "Minitest::Test"
            | "MiniTest::Test"
            | "ActiveSupport::TestCase"
            | "Test::Unit::TestCase"
            | "Minitest::Spec"
    ) || s.ends_with("Test")
        || s.ends_with("TestCase")
        || s.ends_with("::Test")
}

/// Minitest classes, tests and spec blocks of a test file.
pub fn discover_minitest(text: &str) -> Vec<RubyTest> {
    let mut out = Vec::new();
    // Open scopes: (indent, kind, name). Kinds: "module", "class" (a test class),
    // "other" (a class that is not a test), "describe" (a spec group).
    let mut scopes: Vec<(usize, &'static str, String)> = Vec::new();
    // Spec examples numbered per describe block (Minitest::Spec's `test_0001_`).
    let mut counters: Vec<u32> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let ind = indent_of(raw);
        if t == "end" {
            while scopes.last().is_some_and(|(si, _, _)| *si >= ind) {
                let closes_here = scopes.last().is_some_and(|(si, _, _)| *si == ind);
                if scopes.pop().is_some_and(|(_, k, _)| k == "describe") {
                    counters.pop();
                }
                if closes_here {
                    break;
                }
            }
            continue;
        }
        while scopes.last().is_some_and(|(si, _, _)| ind <= *si) {
            if scopes.pop().is_some_and(|(_, k, _)| k == "describe") {
                counters.pop();
            }
        }
        let modules: Vec<&str> = scopes
            .iter()
            .filter(|(_, k, _)| *k == "module" || *k == "class" || *k == "other")
            .map(|(_, _, n)| n.as_str())
            .collect();
        if let Some(rest) = t.strip_prefix("module ") {
            let name = rest.split_whitespace().next().unwrap_or("").to_string();
            scopes.push((ind, "module", name));
            continue;
        }
        if let Some(rest) = t.strip_prefix("class ") {
            let (name, sup) = match rest.split_once('<') {
                Some((n, s)) => (
                    n.trim(),
                    Some(s.split(['#', ';']).next().unwrap_or("").trim()),
                ),
                None => (rest.split_whitespace().next().unwrap_or(""), None),
            };
            if name.starts_with("<<") {
                continue;
            }
            let mut full: Vec<&str> = modules.clone();
            full.push(name);
            let full = full.join("::");
            let is_test = sup.is_some_and(is_test_superclass);
            if is_test {
                out.push(RubyTest {
                    kind: RubyTestKind::Group,
                    line: i as u32,
                    name: name.to_string(),
                    full_name: full.clone(),
                    class: full.clone(),
                    depth: 0,
                });
            }
            scopes.push((
                ind,
                if is_test { "class" } else { "other" },
                name.to_string(),
            ));
            continue;
        }
        let class = scopes
            .iter()
            .rev()
            .find(|(_, k, _)| *k == "class")
            .map(|_| modules.join("::"));
        let describes: Vec<&str> = scopes
            .iter()
            .filter(|(_, k, _)| *k == "describe")
            .map(|(_, _, n)| n.as_str())
            .collect();
        if let Some((_, rest)) = call(t, &["describe"]) {
            if opens_block(t) {
                let name = describe_arg(rest).unwrap_or_default();
                let mut path: Vec<&str> = describes.clone();
                path.push(&name);
                // Minitest::Spec class names join nested describes with `::`.
                let spec_class = path.join("::");
                out.push(RubyTest {
                    kind: RubyTestKind::Group,
                    line: i as u32,
                    name: name.clone(),
                    full_name: spec_class.clone(),
                    class: spec_class,
                    depth: describes.len() as u32,
                });
                scopes.push((ind, "describe", name));
                counters.push(0);
            }
            continue;
        }
        if !describes.is_empty() {
            if let Some((_, rest)) = call(t, &["it", "specify"]) {
                let name = describe_arg(rest).unwrap_or_else(|| "anonymous".into());
                if let Some(n) = counters.last_mut() {
                    *n += 1;
                    out.push(RubyTest {
                        kind: RubyTestKind::Test,
                        line: i as u32,
                        full_name: format!("test_{:04}_{name}", n),
                        name,
                        class: describes.join("::"),
                        depth: describes.len() as u32,
                    });
                }
                if opens_block(t) {
                    // The example's own block.
                    scopes.push((ind, "block", String::new()));
                }
                continue;
            }
        }
        if let Some(class) = class.filter(|_| scopes.last().is_some_and(|(_, k, _)| *k == "class"))
        {
            if let Some(rest) = t.strip_prefix("def ") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '?' || *c == '!')
                    .collect();
                if name.starts_with("test_") {
                    out.push(RubyTest {
                        kind: RubyTestKind::Test,
                        line: i as u32,
                        full_name: name.clone(),
                        name,
                        class,
                        depth: 1,
                    });
                }
                if !t.ends_with("; end") && !t.ends_with(";end") {
                    scopes.push((ind, "block", String::new()));
                }
                continue;
            }
            if let Some((_, rest)) = call(t, &["test"]) {
                if let Some(d) = describe_arg(rest) {
                    out.push(RubyTest {
                        kind: RubyTestKind::Test,
                        line: i as u32,
                        full_name: test_method_name(&d),
                        name: d,
                        class,
                        depth: 1,
                    });
                }
                if opens_block(t) {
                    scopes.push((ind, "block", String::new()));
                }
                continue;
            }
        }
        let one_liner = t.ends_with("; end") || t.ends_with(";end");
        let opens = opens_block(t)
            || ["def ", "if ", "unless ", "case ", "while ", "until "]
                .iter()
                .any(|k| t.starts_with(k))
            || t == "begin";
        if opens && !one_liner {
            scopes.push((ind, "block", String::new()));
        }
    }
    out
}

/// Whether a path is an RSpec spec file.
pub fn is_spec_file(path: &str) -> bool {
    let p = path.replace('\\', "/");
    p.ends_with("_spec.rb")
}

/// Whether a path is a Minitest / test-unit test file.
pub fn is_test_file(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let name = p.rsplit('/').next().unwrap_or(&p);
    name.ends_with(".rb") && (name.ends_with("_test.rb") || name.starts_with("test_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(tests: &[RubyTest]) -> Vec<(u32, RubyTestKind, &str, u32)> {
        tests
            .iter()
            .map(|t| (t.line, t.kind, t.full_name.as_str(), t.depth))
            .collect()
    }

    #[test]
    fn rspec_groups_and_examples() {
        let text = r##"require "spec_helper"

RSpec.describe Shop::Cart do
  let(:cart) { described_class.new }

  it "adds items" do
    cart.add(:apple)
    expect(cart.size).to eq(1)
  end

  describe "#total" do
    it "sums the items" do
      expect(cart.total).to eq(0)
    end

    context 'with a voucher' do
      it { is_expected.to be_valid }
      xit "applies the voucher"
    end
  end

  shared_examples "a container" do
    it "is empty at first"
  end
end
"##;
        use RubyTestKind::*;
        assert_eq!(
            rows(&discover_rspec(text)),
            [
                (2, Group, "Shop::Cart", 0),
                (5, Test, "Shop::Cart adds items", 1),
                (10, Group, "Shop::Cart #total", 1),
                (11, Test, "Shop::Cart #total sums the items", 2),
                (15, Group, "Shop::Cart #total with a voucher", 2),
                (16, Test, "Shop::Cart #total with a voucher", 3),
                (
                    17,
                    Test,
                    "Shop::Cart #total with a voucher applies the voucher",
                    3
                ),
            ]
        );
    }

    #[test]
    fn minitest_classes_methods_and_specs() {
        let text = r##"require "test_helper"

module Shop
  class CartTest < Minitest::Test
    def setup
      @cart = Cart.new
    end

    def test_adds_items
      assert_equal 1, @cart.add(:apple).size
    end

    test "applies the voucher" do
      assert true
    end

    private

    def helper; end
  end

  class Helper
    def test_not_a_test; end
  end
end

describe "Money" do
  describe "#format" do
    it "formats cents" do
      _(Money.new(5).format).must_equal "0.05"
    end
    it "rounds"
  end
end
"##;
        let found = discover_minitest(text);
        use RubyTestKind::*;
        assert_eq!(
            rows(&found),
            [
                (3, Group, "Shop::CartTest", 0),
                (8, Test, "test_adds_items", 1),
                (12, Test, "test_applies_the_voucher", 1),
                (26, Group, "Money", 0),
                (27, Group, "Money::#format", 1),
                (28, Test, "test_0001_formats cents", 2),
                (31, Test, "test_0002_rounds", 2),
            ]
        );
        assert_eq!(found[1].class, "Shop::CartTest");
        assert_eq!(found[5].class, "Money::#format");
        assert_eq!(found[5].name, "formats cents");
    }

    #[test]
    fn file_kinds() {
        assert!(is_spec_file("spec/models/cart_spec.rb"));
        assert!(is_test_file("test/cart_test.rb") && is_test_file("test/test_cart.rb"));
        assert!(!is_test_file("lib/cart.rb") && !is_spec_file("lib/cart.rb"));
        assert_eq!(describe_arg("(\"a \\\"b\\\"\")"), Some("a \"b\"".into()));
        assert_eq!(describe_arg(" :valid?"), Some("valid?".into()));
    }
}
