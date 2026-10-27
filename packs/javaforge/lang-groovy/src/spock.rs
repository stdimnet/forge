//! Spock specifications: classes extending `spock.lang.Specification` (or a base spec of
//! the project) and their feature methods, like IntelliJ IDEA's Spock support.

use serde::Serialize;

use crate::source::{GroovyFile, TypeDecl};

/// A Spock feature method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub name: String,
    /// 0-based line.
    pub line: u32,
    /// `@Ignore`, `@PendingFeature` or `@IgnoreIf`.
    pub ignored: bool,
    /// Data-driven (`where:` block): one run per row.
    pub data_driven: bool,
}

/// A specification class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub name: String,
    /// Binary class name.
    pub class: String,
    pub line: u32,
    pub features: Vec<Feature>,
}

/// Fixture methods are not features.
const FIXTURES: [&str; 4] = ["setup", "cleanup", "setupSpec", "cleanupSpec"];

fn is_spec(t: &TypeDecl, file: &GroovyFile) -> bool {
    if t.kind != "class" {
        return false;
    }
    let Some(sup) = t.extends.as_deref() else {
        return false;
    };
    let simple = sup.rsplit('.').next().unwrap_or(sup);
    simple == "Specification"
        || sup.starts_with("spock.")
        // A base spec of the project (`class CartSpec extends BaseSpec`) in a file that
        // uses Spock.
        || (file.imports_from("spock.") && (simple.ends_with("Spec") || simple.ends_with("Specification")))
}

/// The specifications of a scanned file (`@Ignore`d ones are skipped).
pub fn specs_of(file: &GroovyFile) -> Vec<Spec> {
    file.types
        .iter()
        .filter(|t| is_spec(t, file) && !t.annotations.iter().any(|a| a == "Ignore"))
        .map(|t| Spec {
            name: t.name.clone(),
            class: t.class.clone(),
            line: t.line,
            features: t
                .methods
                .iter()
                .filter(|m| !m.is_static && !FIXTURES.contains(&m.name.as_str()))
                .filter(|m| m.quoted || !m.labels.is_empty())
                .map(|m| Feature {
                    name: m.name.clone(),
                    line: m.line,
                    ignored: m
                        .annotations
                        .iter()
                        .any(|a| matches!(a.as_str(), "Ignore" | "PendingFeature" | "IgnoreIf")),
                    data_driven: m.labels.iter().any(|l| l == "where"),
                })
                .collect(),
        })
        .collect()
}

/// The Spock specifications of a Groovy file.
pub fn specs(text: &str) -> Vec<Spec> {
    specs_of(&crate::source::parse(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_specs_and_features() {
        let src = include_str!("../tests/fixtures/CartSpec.groovy");
        let s = specs(src);
        assert_eq!(s.len(), 1);
        let spec = &s[0];
        assert_eq!(spec.class, "com.acme.cart.CartSpec");
        let features: Vec<(&str, bool, bool)> = spec
            .features
            .iter()
            .map(|f| (f.name.as_str(), f.ignored, f.data_driven))
            .collect();
        assert_eq!(
            features,
            [
                ("adds items", false, false),
                ("totals #items items to #total", false, true),
                ("removes items", true, false),
                ("rejectsNegativeQuantities", false, false),
            ]
        );
        // Helpers without blocks and fixtures are not features.
        assert!(!spec
            .features
            .iter()
            .any(|f| f.name == "setup" || f.name == "newCart"));
    }

    #[test]
    fn base_specs_and_plain_classes() {
        let base = "import spock.lang.Specification\nabstract class BaseSpec extends Specification {}\nclass OrderSpec extends BaseSpec {\n  def 'ships'() { expect: true }\n}\nclass Order {}\n";
        let names: Vec<String> = specs(base).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["BaseSpec", "OrderSpec"]);
        assert!(specs("class Order extends Base { def 'x'() {} }").is_empty());
    }
}
