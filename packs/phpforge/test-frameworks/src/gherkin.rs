//! Gherkin `.feature` files as Behat reads them (English keywords): the feature, its
//! background, scenarios and scenario outlines with their steps, tags and example rows.
//! Lines are 1-based like Behat's `file:line` arguments.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// `Given`, `When`, `Then`, `And`, `But` or `*`.
    pub keyword: String,
    /// The step without its keyword.
    pub text: String,
    pub line: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub name: String,
    pub line: u32,
    /// `Scenario Outline` / `Scenario Template`.
    pub outline: bool,
    pub tags: Vec<String>,
    pub steps: Vec<Step>,
    /// Lines of the example rows (without the header row).
    pub examples: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub name: String,
    pub line: u32,
    pub tags: Vec<String>,
    pub background: Option<Scenario>,
    pub scenarios: Vec<Scenario>,
}

pub const STEP_KEYWORDS: [&str; 6] = ["Given", "When", "Then", "And", "But", "*"];

/// `Given I have 3 apples` → (`Given`, `I have 3 apples`).
pub fn split_step(line: &str) -> Option<(&str, &str)> {
    let t = line.trim_start();
    STEP_KEYWORDS.iter().find_map(|k| {
        let rest = t.strip_prefix(k)?;
        (rest.starts_with(' ') || (*k == "*" && rest.is_empty())).then(|| (*k, rest.trim()))
    })
}

fn heading<'a>(line: &'a str, keywords: &[&str]) -> Option<&'a str> {
    keywords.iter().find_map(|k| {
        line.strip_prefix(k)
            .and_then(|r| r.strip_prefix(':'))
            .map(str::trim)
    })
}

/// Parse a feature file. Unknown lines (descriptions, doc strings, comments) are skipped.
pub fn parse(text: &str) -> Feature {
    let mut feature = Feature::default();
    let mut tags: Vec<String> = Vec::new();
    // Where steps go: the background or the last scenario.
    let mut in_background = false;
    let mut in_examples = false;
    let mut header_seen = false;
    let mut doc_string: Option<&str> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = (i + 1) as u32;
        let t = raw.trim();
        if let Some(fence) = doc_string {
            if t.starts_with(fence) {
                doc_string = None;
            }
            continue;
        }
        if t.starts_with("\"\"\"") || t.starts_with("```") {
            doc_string = Some(if t.starts_with("```") {
                "```"
            } else {
                "\"\"\""
            });
            continue;
        }
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if t.starts_with('@') {
            tags.extend(
                t.split_whitespace()
                    .filter(|w| w.starts_with('@'))
                    .map(String::from),
            );
            continue;
        }
        if let Some(name) = heading(t, &["Feature", "Business Need", "Ability"]) {
            feature.name = name.into();
            feature.line = line;
            feature.tags = std::mem::take(&mut tags);
            continue;
        }
        if heading(t, &["Background"]).is_some() {
            feature.background = Some(Scenario {
                name: "Background".into(),
                line,
                ..Scenario::default()
            });
            in_background = true;
            in_examples = false;
            continue;
        }
        let outline = heading(t, &["Scenario Outline", "Scenario Template"]);
        let plain = outline
            .is_none()
            .then(|| heading(t, &["Scenario", "Example"]))
            .flatten();
        if let Some(name) = outline.or(plain) {
            feature.scenarios.push(Scenario {
                name: name.into(),
                line,
                outline: outline.is_some(),
                tags: std::mem::take(&mut tags),
                ..Scenario::default()
            });
            in_background = false;
            in_examples = false;
            continue;
        }
        if heading(t, &["Examples", "Scenarios"]).is_some() {
            in_examples = true;
            header_seen = false;
            tags.clear();
            continue;
        }
        if t.starts_with('|') {
            if in_examples {
                if header_seen {
                    if let Some(s) = feature.scenarios.last_mut() {
                        s.examples.push(line);
                    }
                } else {
                    header_seen = true;
                }
            }
            continue;
        }
        if let Some((keyword, rest)) = split_step(t) {
            let step = Step {
                keyword: keyword.into(),
                text: rest.into(),
                line,
            };
            let target = if in_background {
                feature.background.as_mut()
            } else {
                feature.scenarios.last_mut()
            };
            if let Some(s) = target {
                s.steps.push(step);
            }
        }
    }
    feature
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEATURE: &str = r#"@shop
Feature: Checkout
  In order to buy products
  As a customer

  Background:
    Given there is a "Lightsaber", which costs £5

  @smoke
  Scenario: Buying a single product
    When I add the "Lightsaber" to the basket
    Then I should have 1 product in the basket
    """
    Given inside a doc string
    """

  Scenario Outline: Buying <count> products
    When I add <count> products
    * the total should be <total>

    Examples:
      | count | total |
      | 2     | £10   |
      | 3     | £15   |
"#;

    #[test]
    fn parses_features_scenarios_outlines_and_steps() {
        let f = parse(FEATURE);
        assert_eq!((f.name.as_str(), f.line), ("Checkout", 2));
        assert_eq!(f.tags, ["@shop"]);
        let bg = f.background.unwrap();
        assert_eq!(
            bg.steps[0].text,
            "there is a \"Lightsaber\", which costs £5"
        );
        assert_eq!(f.scenarios.len(), 2);
        let s = &f.scenarios[0];
        assert_eq!(
            (s.name.as_str(), s.line, s.outline),
            ("Buying a single product", 10, false)
        );
        assert_eq!(s.tags, ["@smoke"]);
        let steps: Vec<_> = s
            .steps
            .iter()
            .map(|x| (x.keyword.as_str(), x.line))
            .collect();
        assert_eq!(steps, [("When", 11), ("Then", 12)]);
        let o = &f.scenarios[1];
        assert!(o.outline);
        assert_eq!(o.steps[1].keyword, "*");
        assert_eq!(o.examples, [23, 24]);
    }

    #[test]
    fn splits_steps() {
        assert_eq!(split_step("  And I pay"), Some(("And", "I pay")));
        assert_eq!(split_step("Andrew pays"), None);
        assert_eq!(split_step("Scenario: x"), None);
    }
}
