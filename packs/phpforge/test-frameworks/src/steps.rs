//! Behat step definitions and the definitions a step uses (Ctrl+Click / Ctrl+B on a step of
//! a `.feature` file).
//!
//! Definitions are context methods with a pattern in a doc comment (`@Given /^I have (\d+)
//! apples?$/`, `@When I add :product to the basket`) or an attribute (`#[Given('…')]`,
//! `#[\Behat\Step\Then("…")]`). Patterns are regular expressions when they start with a
//! delimiter, otherwise turnip patterns: `:name` placeholders (a quoted string or a word),
//! optional parts `apple(s)` and alternatives `buy/purchase`. Like Behat 3, the keyword does
//! not matter for matching.

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

use crate::gherkin;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepDefinition {
    /// `Given`, `When` or `Then`.
    pub keyword: String,
    /// The pattern as written.
    pub pattern: String,
    /// Workspace-relative path of the context file.
    pub path: String,
    /// 0-based line and column of the method name.
    pub line: u32,
    pub column: u32,
    pub method: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub class: String,
}

const KEYWORDS: [&str; 3] = ["Given", "When", "Then"];

fn keyword_of(word: &str) -> Option<&'static str> {
    KEYWORDS.into_iter().find(|k| k.eq_ignore_ascii_case(word))
}

/// The value of a PHP string literal at the start of `s` (`'it''s'` rules: `\'` and `\\`).
fn php_string(s: &str) -> Option<String> {
    let mut chars = s.chars();
    let quote = chars.next().filter(|q| *q == '\'' || *q == '"')?;
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == quote {
            return Some(out);
        }
        if c == '\\' {
            match chars.next() {
                Some(n) if n == quote || n == '\\' => out.push(n),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => break,
            }
        } else {
            out.push(c);
        }
    }
    None
}

/// A step pattern on a doc comment or attribute line: (`keyword`, `pattern`).
fn pattern_on(line: &str) -> Option<(&'static str, String)> {
    let t = line
        .trim_start()
        .trim_start_matches(['/', '*'])
        .trim_start();
    if let Some(rest) = t.strip_prefix('@') {
        let word: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        let k = keyword_of(&word)?;
        let pattern = rest[word.len()..].trim();
        let pattern = pattern.strip_suffix("*/").unwrap_or(pattern).trim();
        return (!pattern.is_empty()).then(|| (k, pattern.to_string()));
    }
    let inner = t.strip_prefix("#[")?;
    let name_end = inner.find('(')?;
    let name = inner[..name_end].trim().rsplit('\\').next()?;
    let k = keyword_of(name)?;
    let args = inner[name_end + 1..].trim_start();
    let args = args
        .strip_prefix("pattern:")
        .map(str::trim_start)
        .unwrap_or(args);
    Some((k, php_string(args)?))
}

/// The step definitions of a PHP file.
pub fn definitions(path: &str, text: &str) -> Vec<StepDefinition> {
    let class_re = Regex::new(r"^\s*(?:(?:final|abstract|readonly)\s+)*class\s+(\w+)").unwrap();
    let fn_re = Regex::new(r"function\s+(\w+)\s*\(").unwrap();
    let mut out = Vec::new();
    let mut pending: Vec<(&'static str, String)> = Vec::new();
    let mut class = String::new();
    for (i, line) in text.lines().enumerate() {
        if let Some(c) = class_re.captures(line) {
            class = c[1].to_string();
            pending.clear();
            continue;
        }
        if let Some(p) = pattern_on(line) {
            pending.push(p);
            continue;
        }
        if let Some(m) = fn_re.captures(line) {
            let name = m.get(1).unwrap();
            for (keyword, pattern) in pending.drain(..) {
                out.push(StepDefinition {
                    keyword: keyword.into(),
                    pattern,
                    path: path.into(),
                    line: i as u32,
                    column: line[..name.start()].chars().count() as u32,
                    method: name.as_str().into(),
                    class: class.clone(),
                });
            }
        }
    }
    out
}

/// Escape regular expression syntax.
fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A turnip pattern as a regular expression.
fn turnip_regex(pattern: &str) -> String {
    let placeholder = r#"(?:"[^"]*"|'[^']*'|-?[\w.,]+)"#;
    let mut out = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    // The plain words just written, for `word/other` alternatives.
    let mut word_start: Option<usize> = None;
    while i < chars.len() {
        let c = chars[i];
        if c == ':'
            && chars
                .get(i + 1)
                .is_some_and(|n| n.is_alphanumeric() || *n == '_')
        {
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push_str(placeholder);
            word_start = None;
            continue;
        }
        if c == '(' {
            if let Some(end) = chars[i + 1..].iter().position(|x| *x == ')') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push_str(&format!("(?:{})?", escape(&inner)));
                i += end + 2;
                word_start = None;
                continue;
            }
        }
        if c == '/' {
            if let Some(start) = word_start {
                let next: String = chars[i + 1..]
                    .iter()
                    .take_while(|x| x.is_alphanumeric() || **x == '_')
                    .collect();
                if !next.is_empty() {
                    let first = out.split_off(start);
                    out.push_str(&format!("(?:{first}|{})", escape(&next)));
                    i += 1 + next.chars().count();
                    word_start = None;
                    continue;
                }
            }
        }
        if c.is_alphanumeric() || c == '_' {
            if word_start.is_none() {
                word_start = Some(out.len());
            }
        } else {
            word_start = None;
        }
        out.push_str(&escape(&c.to_string()));
        i += 1;
    }
    out.push('$');
    out
}

/// A `/regex/flags` pattern as a regular expression for `regex-lite`.
fn delimited_regex(pattern: &str) -> Option<String> {
    let delim = pattern.chars().next()?;
    if delim.is_alphanumeric() || delim.is_whitespace() || delim == '\\' {
        return None;
    }
    let end = pattern.rfind(delim).filter(|e| *e > 0)?;
    let body = &pattern[delim.len_utf8()..end];
    let flags: String = pattern[end + delim.len_utf8()..]
        .chars()
        .filter(|f| matches!(f, 'i' | 'm' | 's' | 'x'))
        .collect();
    Some(if flags.is_empty() {
        body.to_string()
    } else {
        format!("(?{flags}){body}")
    })
}

/// The regular expression of a step pattern.
pub fn pattern_regex(pattern: &str) -> String {
    let p = pattern.trim();
    let delimited = p.starts_with(['/', '#', '~', '!', '@', '%', '|']);
    match delimited.then(|| delimited_regex(p)).flatten() {
        Some(r) => r,
        None => turnip_regex(p),
    }
}

/// Whether a definition matches a step (its text without the keyword).
pub fn matches(def: &StepDefinition, step: &str) -> bool {
    Regex::new(&pattern_regex(&def.pattern)).is_ok_and(|r| r.is_match(step.trim()))
}

/// The step on a line of a feature file, with outline placeholders (`<count>`) replaced by
/// the first example row.
pub fn step_at(feature_text: &str, line: u32) -> Option<String> {
    let raw = feature_text.lines().nth(line as usize)?;
    let (_, text) = gherkin::split_step(raw)?;
    if !text.contains('<') {
        return Some(text.to_string());
    }
    let lines: Vec<&str> = feature_text.lines().collect();
    let feature = gherkin::parse(feature_text);
    let outline = feature
        .scenarios
        .iter()
        .find(|s| s.steps.iter().any(|st| st.line == line + 1))?;
    let row = outline.examples.first()?;
    let cells = |l: u32| -> Vec<String> {
        lines
            .get(l as usize - 1)
            .map(|t| {
                t.trim()
                    .trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    let values = cells(*row);
    let header = cells(row - 1);
    let mut text = text.to_string();
    for (h, v) in header.iter().zip(values) {
        text = text.replace(&format!("<{h}>"), &v);
    }
    Some(text)
}

/// The definitions the step on `line` (0-based) of a feature file uses.
pub fn step_targets(feature_text: &str, line: u32, defs: &[StepDefinition]) -> Vec<StepDefinition> {
    let Some(step) = step_at(feature_text, line) else {
        return Vec::new();
    };
    defs.iter().filter(|d| matches(d, &step)).cloned().collect()
}

/// Whether a PHP file may contain step definitions (cheap check before parsing).
pub fn may_define_steps(text: &str) -> bool {
    ["@Given", "@When", "@Then", "Given(", "When(", "Then("]
        .iter()
        .any(|m| text.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT: &str = r#"<?php

namespace App\Tests\Behat;

use Behat\Behat\Context\Context;
use Behat\Step\Then;

final class FeatureContext implements Context
{
    /**
     * @Given there is a(n) :product, which costs £:price
     */
    public function thereIsAWhichCostsPs(string $product, int $price): void {}

    /**
     * @When /^I add the "([^"]*)" to the basket$/i
     */
    public function iAddTheToTheBasket($product) {}

    #[Then('I should have :count product(s) in the basket')]
    public function iShouldHaveProductsInTheBasket(int $count) {}

    #[\Behat\Step\When('I buy/purchase :count items')]
    public function iBuyItems($count) {}
}
"#;

    fn defs() -> Vec<StepDefinition> {
        definitions("features/bootstrap/FeatureContext.php", CONTEXT)
    }

    #[test]
    fn finds_annotations_and_attributes() {
        let d = defs();
        let found: Vec<_> = d
            .iter()
            .map(|x| (x.keyword.as_str(), x.method.as_str(), x.line, x.column))
            .collect();
        assert_eq!(
            found,
            [
                ("Given", "thereIsAWhichCostsPs", 12, 20),
                ("When", "iAddTheToTheBasket", 17, 20),
                ("Then", "iShouldHaveProductsInTheBasket", 20, 20),
                ("When", "iBuyItems", 23, 20),
            ]
        );
        assert_eq!(d[0].class, "FeatureContext");
        assert_eq!(
            d[2].pattern,
            "I should have :count product(s) in the basket"
        );
        assert!(may_define_steps(CONTEXT));
        assert!(!may_define_steps("<?php class A {}"));
    }

    #[test]
    fn matches_turnip_and_regex_patterns() {
        let d = defs();
        let hit = |step: &str| -> Vec<String> {
            d.iter()
                .filter(|x| matches(x, step))
                .map(|x| x.method.clone())
                .collect()
        };
        assert_eq!(
            hit("there is a \"Sith Lord Lightsaber\", which costs £5"),
            ["thereIsAWhichCostsPs"]
        );
        assert_eq!(
            hit("there is an 'Apple', which costs £1"),
            ["thereIsAWhichCostsPs"]
        );
        assert_eq!(
            hit("I ADD the \"Lightsaber\" to the basket"),
            ["iAddTheToTheBasket"]
        );
        assert_eq!(
            hit("I should have 1 product in the basket"),
            ["iShouldHaveProductsInTheBasket"]
        );
        assert_eq!(
            hit("I should have 2 products in the basket"),
            ["iShouldHaveProductsInTheBasket"]
        );
        assert_eq!(hit("I purchase 3 items"), ["iBuyItems"]);
        assert_eq!(hit("I buy 3 items"), ["iBuyItems"]);
        assert!(hit("I should have products in the basket").is_empty());
    }

    #[test]
    fn finds_the_definitions_of_a_step_line_with_outline_values() {
        let feature = "Feature: F\n  Scenario Outline: S\n    Then I should have <n> products in the basket\n\n    Examples:\n      | n |\n      | 3 |\n";
        assert_eq!(
            step_at(feature, 2).as_deref(),
            Some("I should have 3 products in the basket")
        );
        let t = step_targets(feature, 2, &defs());
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].method, "iShouldHaveProductsInTheBasket");
        assert!(step_targets(feature, 1, &defs()).is_empty());
    }

    #[test]
    fn converts_patterns() {
        assert_eq!(pattern_regex("/^I have (\\d+)$/i"), "(?i)^I have (\\d+)$");
        assert_eq!(pattern_regex("#^a/b$#"), "^a/b$");
        assert_eq!(
            php_string(r"'it\'s \\ here' rest").as_deref(),
            Some(r"it's \ here")
        );
    }
}
